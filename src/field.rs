use std::{
    iter::Sum,
    ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign},
};

use atomicow::CowArc;
use bevy_app::{App, Plugin};
use bevy_asset::{Asset, AssetApp, AssetId};
use bevy_derive::Deref;
use bevy_ecs::{
    resource::Resource,
    system::{Commands, Res, SystemParamItem},
};
use bevy_math::{Affine3A, Mat4, UVec3, Vec3, Vec4, VectorSpace};
use bevy_reflect::TypePath;
use bevy_render::{
    RenderApp, RenderStartup,
    render_asset::{PrepareAssetError, RenderAsset, RenderAssetPlugin},
    render_resource::{
        AddressMode, Extent3d, FilterMode, Origin3d, Sampler, SamplerDescriptor,
        TexelCopyBufferLayout, TexelCopyTextureInfo, Texture, TextureAspect, TextureDescriptor,
        TextureDimension, TextureFormat, TextureUsages, TextureView, TextureViewDescriptor,
    },
    renderer::{RenderDevice, RenderQueue},
};
use bevy_transform::components::Transform;
use bytemuck::{Pod, Zeroable};
use half::{f16, slice::HalfFloatSliceExt};
use variadics_please::all_tuples;

pub struct FlowFieldPlugin;

impl Plugin for FlowFieldPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<FlowField>()
            .add_plugins(RenderAssetPlugin::<GpuFlowField>::default());

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.add_systems(RenderStartup, init_flow_field_sampler);
        }
    }
}

// FIELD ASSET TYPE ------------------------------------------------------------

#[derive(TypePath, Asset, Clone)]
pub struct FlowField {
    label: Option<CowArc<'static, str>>,
    size: UVec3,
    texels: Box<[RawFlowVector]>,
}

impl FlowField {
    #[inline]
    pub fn zeroed(size: UVec3) -> Self {
        let texels =
            vec![[f16::from_f32(0.0); 4]; (size.x * size.y * size.z) as usize].into_boxed_slice();
        Self {
            label: None,
            size,
            texels,
        }
    }

    #[inline]
    pub fn from_gen(size: UVec3, generator: impl FlowFieldGenerator) -> Self {
        let mut field = Self::zeroed(size);
        field.modify().fill_from_gen(generator);
        field
    }

    #[inline]
    pub fn with_label(self, label: impl Into<CowArc<'static, str>>) -> Self {
        Self {
            label: Some(label.into()),
            ..self
        }
    }

    #[inline]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    #[inline]
    pub fn size(&self) -> UVec3 {
        self.size
    }

    #[inline]
    pub fn modify(&mut self) -> FlowFieldGuard<'_> {
        let mut scratch = vec![FlowVector::ZERO; self.texels.len()];
        let texels_raw: &[f16] = bytemuck::cast_slice(&self.texels);
        let scratch_raw: &mut [f32] = bytemuck::cast_slice_mut(&mut scratch);
        texels_raw.convert_to_f32_slice(scratch_raw);

        FlowFieldGuard {
            size: self.size,
            texels: &mut self.texels,
            scratch: scratch.into_boxed_slice(),
        }
    }
}

type RawFlowVector = [f16; 4];

pub struct FlowFieldGuard<'a> {
    size: UVec3,
    texels: &'a mut [RawFlowVector],
    scratch: Box<[FlowVector]>,
}

impl<'a> FlowFieldGuard<'a> {
    #[inline]
    fn coords_to_index(&self, coords: UVec3) -> u32 {
        self.size.x * self.size.y * coords.z + self.size.x * coords.y + self.size.x
    }

    #[inline]
    pub fn get(&self, coords: UVec3) -> FlowVector {
        let index = self.coords_to_index(coords);
        self.scratch[index as usize]
    }

    #[inline]
    pub fn get_mut(&mut self, coords: UVec3) -> &mut FlowVector {
        let index = self.coords_to_index(coords);
        &mut self.scratch[index as usize]
    }

    #[inline]
    pub fn set(&mut self, coords: UVec3, flow_vector: FlowVector) {
        let index = self.coords_to_index(coords);
        self.scratch[index as usize] = flow_vector;
    }

    pub fn fill_from_gen(&mut self, mut generator: impl FlowFieldGenerator) {
        for x in 0..self.size.x {
            for y in 0..self.size.y {
                for z in 0..self.size.z {
                    let pos = UVec3::new(x, y, z);
                    let pos_vec3 = pos.as_vec3() + Vec3::splat(0.5) - self.size.as_vec3().div(2.0);
                    let vector = generator.generate(pos_vec3);
                    self.set(pos, vector)
                }
            }
        }
    }
}

impl<'a> Drop for FlowFieldGuard<'a> {
    fn drop(&mut self) {
        let scratch_slice: &[f32] = bytemuck::cast_slice(&self.scratch);
        let texels_slice: &mut [f16] = bytemuck::cast_slice_mut(self.texels);
        texels_slice.convert_from_f32_slice(scratch_slice);
    }
}

pub struct GpuFlowField {
    label: Option<CowArc<'static, str>>,
    size: UVec3,
    texture: Texture,
    texture_view: TextureView,
}

impl GpuFlowField {
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub fn size(&self) -> UVec3 {
        self.size
    }

    pub fn texture(&self) -> &Texture {
        &self.texture
    }

    pub fn texture_view(&self) -> &TextureView {
        &self.texture_view
    }
}

impl RenderAsset for GpuFlowField {
    type SourceAsset = FlowField;

    type Param = (Res<'static, RenderDevice>, Res<'static, RenderQueue>);

    fn prepare_asset(
        source_asset: Self::SourceAsset,
        _asset_id: AssetId<Self::SourceAsset>,
        (render_device, render_queue): &mut SystemParamItem<Self::Param>,
        previous_asset: Option<&Self>,
    ) -> Result<Self, PrepareAssetError<Self::SourceAsset>> {
        let texture_extent = Extent3d {
            width: source_asset.size.x,
            height: source_asset.size.y,
            depth_or_array_layers: source_asset.size.z,
        };

        let (texture, texture_view) = previous_asset
            .filter(|prev| prev.size == source_asset.size)
            .map(|prev| (prev.texture.clone(), prev.texture_view.clone()))
            .unwrap_or_else(|| {
                let texture = render_device.create_texture(&TextureDescriptor {
                    label: source_asset.label.as_deref(),
                    size: texture_extent,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: TextureDimension::D3,
                    format: TextureFormat::Rgba16Float,
                    usage: TextureUsages::COPY_SRC
                        | TextureUsages::COPY_DST
                        | TextureUsages::TEXTURE_BINDING
                        | TextureUsages::STORAGE_BINDING,
                    view_formats: &[],
                });
                let texture_view_label = source_asset
                    .label
                    .as_ref()
                    .map(|label| format!("{label}_view"));
                let texture_view = texture.create_view(&TextureViewDescriptor {
                    label: texture_view_label.as_deref(),
                    ..Default::default()
                });
                (texture, texture_view)
            });

        const BYTES_PER_RAW_TEXEL: u32 = 8;

        // TODO: partial writes
        render_queue.write_texture(
            TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            bytemuck::cast_slice(&source_asset.texels),
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(source_asset.size.x * BYTES_PER_RAW_TEXEL),
                rows_per_image: Some(source_asset.size.y),
            },
            texture_extent,
        );

        Ok(GpuFlowField {
            label: source_asset.label.clone(),
            size: source_asset.size,
            texture,
            texture_view,
        })
    }
}

#[derive(Resource, Deref)]
pub struct FlowFieldSampler(Sampler);

pub(super) fn init_flow_field_sampler(render_device: Res<RenderDevice>, mut commands: Commands) {
    let sampler = render_device.create_sampler(&SamplerDescriptor {
        label: Some("flow_field_sampler"),
        address_mode_u: AddressMode::ClampToEdge, //TODO: correct behavior?
        address_mode_v: AddressMode::ClampToEdge, // should flow determine address mode?
        address_mode_w: AddressMode::ClampToEdge,
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        mipmap_filter: FilterMode::Linear,
        ..Default::default()
    });

    commands.insert_resource(FlowFieldSampler(sampler));
}

// FIELD VECTOR TYPE -----------------------------------------------------------

// TODO: link to module docs regarding flow layering
// TODO: revise. expand on units and what they mean.

/// Describes the flow of a fluid at a given location.
///
/// A `FlowVector` is comprised of two parts:
/// - momentum density (units: kg m/s m^-3)
/// - density          (units: kg     m^-3)
///
/// This may seem like an odd choice, compared to just tracking the fluid velocity,
/// but it provides a fuller view of how the fluid is acting, and it's necessary
/// because of the way [`vane`](crate) layers fluid flows. Adding one velocity
/// to another velocity makes no sense in practice: what if the amount of fluid
/// flowing in direction A is much greater than that in direction B? Tracking momentum
/// also makes it much more natural to calculate the force a fluid exerts on objects
/// and surfaces.
#[derive(Copy, Clone, PartialEq, Debug, Default, Pod, Zeroable)]
#[repr(transparent)]
pub struct FlowVector(Vec4);

impl FlowVector {
    /// Returns a [`FlowVector`] with the given `momentum_density` and `density`
    #[inline]
    pub fn new(momentum_density: Vec3, density: f32) -> Self {
        Self(momentum_density.extend(density))
    }

    /// Creates a [`FlowVector`] given the fluid's velocity and density.
    ///
    /// [`FlowVector::new`] should be preferred in most cases since it
    /// better represents how the fluid will interact with objects.
    pub fn from_velocity(velocity: Vec3, density: f32) -> Self {
        Self::new(velocity * density, density)
    }

    /// Returns the momentum density of the fluid flow
    ///
    /// units: kg m/s m^-3
    #[inline]
    pub fn momentum_density(self) -> Vec3 {
        self.0.truncate()
    }

    /// Returns the density of the fluid flow
    ///
    /// units: kg m^-3
    #[inline]
    pub fn density(self) -> f32 {
        self.0.w
    }

    /// Returns the velocity of the fluid flow. This is equivalent to
    /// `momentum_density` / `density`.
    ///
    /// units: m/s
    #[inline]
    pub fn velocity(self) -> Vec3 {
        self.momentum_density() / self.density()
    }
}

impl From<Vec4> for FlowVector {
    #[inline]
    fn from(value: Vec4) -> Self {
        Self(value)
    }
}

impl From<FlowVector> for Vec4 {
    #[inline]
    fn from(value: FlowVector) -> Self {
        value.0
    }
}

impl Neg for FlowVector {
    type Output = Self;

    #[inline]
    fn neg(self) -> Self::Output {
        Self(-self.0)
    }
}

impl Add for FlowVector {
    type Output = Self;

    #[inline]
    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl AddAssign for FlowVector {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl Sum for FlowVector {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(FlowVector::ZERO, |a, b| a + b)
    }
}

impl<'a> Sum<&'a FlowVector> for FlowVector {
    fn sum<I: Iterator<Item = &'a FlowVector>>(iter: I) -> Self {
        iter.fold(FlowVector::ZERO, |a, b| a + *b)
    }
}

impl Sub for FlowVector {
    type Output = Self;

    #[inline]
    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl SubAssign for FlowVector {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

impl Mul<f32> for FlowVector {
    type Output = Self;

    #[inline]
    fn mul(self, rhs: f32) -> Self::Output {
        Self(self.0 * rhs)
    }
}

impl MulAssign<f32> for FlowVector {
    #[inline]
    fn mul_assign(&mut self, rhs: f32) {
        self.0 *= rhs;
    }
}

impl Div<f32> for FlowVector {
    type Output = Self;

    #[inline]
    fn div(self, rhs: f32) -> Self::Output {
        Self(self.0 / rhs)
    }
}

impl DivAssign<f32> for FlowVector {
    #[inline]
    fn div_assign(&mut self, rhs: f32) {
        self.0 /= rhs;
    }
}

impl VectorSpace for FlowVector {
    type Scalar = f32;

    const ZERO: Self = Self(Vec4::ZERO);
}

// FIELD GENERATORS ------------------------------------------------------------

pub trait FlowFieldGenerator: Sized {
    fn generate(&mut self, position: Vec3) -> FlowVector;

    #[inline]
    fn transformed(self, transform: Transform) -> impl FlowFieldGenerator {
        Transformed {
            inner: self,
            world_to_local: transform.compute_affine().inverse(),
        }
    }

    #[inline]
    fn amplified(self, multiplier: f32) -> impl FlowFieldGenerator {
        Amplified {
            inner: self,
            multiplier,
        }
    }
}

macro_rules! impl_flow_field_generator {
    ($(($T:ident, $t:ident)),*) => {
        #[expect(
            clippy::allow_attributes,
            reason = "This is in a macro; as such, the below lints may not always apply."
        )]
        #[allow(
            unused_variables,
            reason = "Some invocations of this macro may trigger the `unused_variables` lint, where others won't."
        )]
        #[allow(
            unused_mut,
            reason = "Some invocations of this macro may trigger the `unused_mut` lint, where others won't."
        )]
        #[allow(
            clippy::unused_unit,
            reason = "Zero-length tuples won't have anything to wrap."
        )]
        impl <$($T: FlowFieldGenerator),*> FlowFieldGenerator for ($($T,)*) {
            #[inline]
            fn generate(&mut self, position: Vec3) -> FlowVector {
                let ($($t,)*) = self;
                let mut vector = FlowVector::ZERO;
                $(vector += <$T as FlowFieldGenerator>::generate($t, position);)*
                vector
            }
        }
    };
}

all_tuples!(impl_flow_field_generator, 0, 16, T, t);

impl<F: FnMut(Vec3) -> FlowVector> FlowFieldGenerator for F {
    #[inline]
    fn generate(&mut self, position: Vec3) -> FlowVector {
        self(position)
    }
}

struct Transformed<T: FlowFieldGenerator> {
    inner: T,
    world_to_local: Affine3A,
}

impl<T: FlowFieldGenerator> FlowFieldGenerator for Transformed<T> {
    #[inline]
    fn generate(&mut self, position: Vec3) -> FlowVector {
        self.inner
            .generate(self.world_to_local.transform_point3(position))
    }
}

struct Amplified<T: FlowFieldGenerator> {
    inner: T,
    multiplier: f32,
}

impl<T: FlowFieldGenerator> FlowFieldGenerator for Amplified<T> {
    #[inline]
    fn generate(&mut self, position: Vec3) -> FlowVector {
        self.inner.generate(position) * self.multiplier
    }
}

struct Uniform(FlowVector);

impl FlowFieldGenerator for Uniform {
    #[inline]
    fn generate(&mut self, _position: Vec3) -> FlowVector {
        self.0
    }
}

#[inline]
pub fn uniform_flow_field(value: FlowVector) -> impl FlowFieldGenerator {
    Uniform(value)
}
