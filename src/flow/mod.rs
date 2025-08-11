use std::{collections::HashMap, ops::Deref};

use bevy_app::{App, Plugin, PreStartup};
use bevy_asset::{AsAssetId, AssetId, Handle};
use bevy_camera::primitives::Aabb;
use bevy_ecs::{
    component::Component,
    entity::{Entity, EntityHashMap},
    error::BevyError,
    lifecycle::Add,
    observer::On,
    query::{AnyOf, Changed, Has, QueryItem, With},
    relationship::RelationshipTarget,
    system::{Commands, Query, Res, lifetimeless::Read},
};
use bevy_math::{Affine3, Mat3, Quat, Vec3, Vec4, Vec4Swizzles, bounding::Aabb3d};
use bevy_render::{
    Extract,
    render_asset::RenderAssets,
    render_resource::{BindGroup, BufferUsages, RawBufferVec, TextureView},
    renderer::{RenderDevice, RenderQueue},
    sync_world::{MainEntityHashMap, SyncToRenderWorld},
};
use bevy_time::Time;
use bevy_transform::components::{GlobalTransform, Transform};

mod field;
pub use field::*;
use tracing::warn;

pub struct FlowPlugin;

impl Plugin for FlowPlugin {
    fn build(&self, app: &mut App) {}
}

#[derive(Component)]
#[require(FlowInfluence, FlowLayers::layer(0), Transform, Aabb)]
#[repr(transparent)]
pub struct Flow {
    pub field: Handle<FlowField>,
}

impl AsAssetId for Flow {
    type Asset = FlowField;

    fn as_asset_id(&self) -> AssetId<Self::Asset> {
        self.field.id()
    }
}

#[derive(Component, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(transparent)]
pub struct FlowInfluence(pub f32);

impl Default for FlowInfluence {
    fn default() -> Self {
        Self(1.0)
    }
}

#[derive(Component, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(transparent)]
pub struct FlowLayers(u32);

pub type Layer = u8;

impl FlowLayers {
    #[inline]
    pub fn all() -> Self {
        Self(!0)
    }

    #[inline]
    pub fn none() -> Self {
        Self(0)
    }

    #[inline]
    pub fn layer(layer: Layer) -> Self {
        Self(1 << layer)
    }

    #[inline]
    pub fn from_layers(layers: impl IntoIterator<Item = Layer>) {
        let mut raw_layers = 0;
        layers.into_iter().for_each(|n| raw_layers |= 1 << n);
        Self(raw_layers);
    }

    #[inline]
    pub fn with(mut self, layer: Layer) -> Self {
        self.0 |= 1 << layer;
        self
    }

    #[inline]
    pub fn without(mut self, layer: Layer) -> Self {
        self.0 &= !(1 << layer);
        self
    }

    #[inline]
    pub fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
}

impl Default for FlowLayers {
    fn default() -> Self {
        Self::none()
    }
}

fn warn_flow_region_missing(
    ev: On<Add, Flow>,
    flows: Query<Option<&InRegion>>,
    regions: Query<(), With<Region>>,
) -> Result<(), BevyError> {
    if let Some(InRegion(region)) = flows.get(ev.entity())? {
        let is_region = regions.get(*region).is_ok();
        if !is_region {
            warn!(
                "Region missing: Flow ({}) has a related region ({}), but which has no Region component",
                ev.entity(),
                *region,
            );
        };
    } else {
        warn!(
            "Region missing: Flow ({}) has no related region. Make sure to insert an InRegion component, or this Flow will not function.",
            ev.entity(),
        );
    }
    Ok(())
}

const CORNERS: [Vec3; 8] = [
    Vec3::new(-0.5, -0.5, -0.5),
    Vec3::new(0.5, -0.5, -0.5),
    Vec3::new(-0.5, 0.5, -0.5),
    Vec3::new(-0.5, -0.5, 0.5),
    Vec3::new(0.5, 0.5, -0.5),
    Vec3::new(-0.5, 0.5, 0.5),
    Vec3::new(0.5, -0.5, 0.5),
    Vec3::new(0.5, 0.5, 0.5),
];

#[derive(Debug, thiserror::Error)]
#[error("Failed to construct Aabb: Flow ({entity:?}) has an invalid scale: {scale:?}")]
struct InvalidFlowScaleError {
    entity: Entity,
    scale: Vec3,
}

fn update_flow_aabbs(
    vanes: Query<(Entity, &GlobalTransform, &mut Aabb), (With<Flow>, Changed<GlobalTransform>)>,
) -> Result<(), BevyError> {
    for (entity, transform, mut aabb) in vanes {
        let scale = transform.scale();
        let corners = CORNERS
            .iter()
            .map(|point| transform.transform_point(point * scale));
        *aabb = Aabb::enclosing(corners).ok_or(InvalidFlowScaleError { entity, scale })?;
    }
    Ok(())
}

#[derive(Copy, Clone, Component)]
#[require(InheritedVelocity)]
pub struct InheritLinearVelocity;

#[derive(Copy, Clone, Component)]
#[require(InheritedVelocity)]
pub struct InheritAngularVelocity;

#[derive(Component, Default)]
struct InheritedVelocity {
    previous_transform: Option<GlobalTransform>,
    linear_velocity: Vec3,
    angular_velocity: Vec3,
}

fn update_velocity(
    query: Query<(
        &GlobalTransform,
        Has<InheritLinearVelocity>,
        Has<InheritAngularVelocity>,
        &mut InheritedVelocity,
    )>,
    time: Res<Time>,
) {
    for (transform, inherit_linear, inherit_angular, mut inherited_motion) in query {
        let prev_srt = inherited_motion
            .previous_transform
            .as_ref()
            .map(|tf| tf.to_scale_rotation_translation());
        let (_, rotation, translation) = transform.to_scale_rotation_translation();

        let linear_velocity = prev_srt
            .as_ref()
            .filter(|_| inherit_linear)
            .map(|(_, _, prev_translation)| (translation - *prev_translation) / time.delta_secs())
            .unwrap_or(Vec3::ZERO);

        let angular_velocity = prev_srt
            .as_ref()
            .filter(|_| inherit_angular)
            .map(|(_, prev_rotation, _)| {
                angular_velocity_between(*prev_rotation, rotation, time.delta_secs())
            })
            .unwrap_or(Vec3::ZERO);

        *inherited_motion = InheritedVelocity {
            previous_transform: Some(*transform),
            linear_velocity,
            angular_velocity,
        }
    }
}

// See: https://mariogc.com/post/angular-velocity-quaternions/#the-angular-velocities
fn angular_velocity_between(q1: Quat, q2: Quat, dt_secs: f32) -> Vec3 {
    let q2v = Vec4::from_array(q2.to_array());
    (2.0 / dt_secs)
        * (q1.w * q2v.xyz()
            + q2.x * q2v.wzy() * Vec3::new(-1.0, 1.0, -1.0)
            + q2.y * q2v.zwx() * Vec3::new(-1.0, -1.0, 1.0)
            + q2.z * q2v.yxw() * Vec3::new(1.0, -1.0, -1.0))
}

#[derive(Component, Default)]
#[require(SyncToRenderWorld)]
pub struct Region;

#[derive(Component)]
#[relationship(relationship_target = Contains)]
pub struct InRegion(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = InRegion)]
pub struct Contains(Vec<Entity>);

// RENDER WORLD LOGIC ----------------------------------------------------------

#[derive(Component)]
pub struct ExtractedFlow {
    pub transform: GlobalTransform,
    pub aabb: Aabb,
    pub field_id: AssetId<FlowField>,
    pub flags: FlowFlags,
    pub layers: FlowLayers,
    pub influence: FlowInfluence,
    pub linear_velocity: Vec3,
    pub angular_velocity: Vec3,
}

#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct FlowFlags(u32);

bitflags::bitflags! {
    impl FlowFlags: u32 {
        const INHERIT_LINEAR_VELOCITY = 1 << 0;
        const INHERIT_ANGULAR_VELOCITY = 1 << 1;
    }
}

#[derive(Component, Default)]
pub struct ExtractedRegion {
    flows: MainEntityHashMap<ExtractedFlow>,
}

fn extract_regions(
    regions: Extract<Query<&Contains, With<Region>>>,
    flows: Extract<
        Query<(
            Entity,
            &Flow,
            &FlowLayers,
            &FlowInfluence,
            &GlobalTransform,
            &Aabb,
            Has<InheritLinearVelocity>,
            Has<InheritAngularVelocity>,
            Option<&InheritedVelocity>,
        )>,
    >,
) {
    //TODO: better extraction logic:
    // despawn removed/disabled main world regions
    //
    for region in &regions {
        for (
            entity,
            flow,
            layers,
            influence,
            transform,
            aabb,
            inherit_linear_velocity,
            inherit_angular_velocity,
            inherited_velocity,
        ) in region.iter().filter_map(|flow| flows.get(flow).ok())
        {
            let mut flags = FlowFlags::empty();

            if inherit_linear_velocity {
                flags |= FlowFlags::INHERIT_LINEAR_VELOCITY;
            }

            if inherit_angular_velocity {
                flags |= FlowFlags::INHERIT_ANGULAR_VELOCITY;
            }

            let extracted_flow = ExtractedFlow {
                transform: *transform,
                aabb: *aabb,
                field_id: flow.as_asset_id(),
                flags,
                layers: *layers,
                influence: *influence,
                linear_velocity: inherited_velocity
                    .as_ref()
                    .map(|inherited_velocity| inherited_velocity.linear_velocity)
                    .unwrap_or(Vec3::ZERO),
                angular_velocity: inherited_velocity
                    .as_ref()
                    .map(|inherited_velocity| inherited_velocity.angular_velocity)
                    .unwrap_or(Vec3::ZERO),
            };

            //TODO: actually extract lol
        }
    }

    todo!()
}

#[derive(Component, Default)]
struct ExtractedRegionFields {
    indices: HashMap<AssetId<FlowField>, u32>,
    field_textures: Vec<TextureView>,
}

#[derive(Debug, thiserror::Error)]
#[error("FlowField missing with id: {id:?}")]
pub struct FlowFieldMissingError {
    pub id: AssetId<FlowField>,
}

impl ExtractedRegionFields {
    fn insert(
        &mut self,
        fields: &RenderAssets<GpuFlowField>,
        field_id: AssetId<FlowField>,
    ) -> Result<u32, FlowFieldMissingError> {
        let field = fields
            .get(field_id)
            .ok_or(FlowFieldMissingError { id: field_id })?;
        let index = self.indices.get(&field_id).copied().unwrap_or_else(|| {
            let index = self.field_textures.len();
            self.field_textures
                .insert(index, field.texture_view().clone());
            self.indices.insert(field_id, index as u32);
            index as u32
        });
        Ok(index)
    }

    fn clear(&mut self) {
        self.indices.clear();
        self.field_textures.clear();
    }
}

fn prepare_flow_field_indices(
    regions: Query<(&ExtractedRegion, &mut ExtractedRegionFields), With<Region>>,
    fields: Res<RenderAssets<GpuFlowField>>,
) -> Result<(), BevyError> {
    for (flows, mut field_indices) in regions {
        field_indices.clear();
        for flow in flows.flows.values() {
            field_indices.insert(fields.as_ref(), flow.field_id)?;
        }
    }
    Ok(())
}

#[derive(Component)]
pub struct RegionUniforms(RawBufferVec<GpuFlow>);

impl Default for RegionUniforms {
    fn default() -> Self {
        Self(RawBufferVec::new(
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        ))
    }
}

fn prepare_flow_uniforms(
    regions: Query<(
        &ExtractedRegion,
        &ExtractedRegionFields,
        &mut RegionUniforms,
    )>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
) -> Result<(), BevyError> {
    for (region, indices, mut uniforms) in regions {
        uniforms.0.clear();
        for flow in region.flows.values() {
            let field_index = *indices
                .indices
                .get(&flow.field_id)
                .ok_or(FlowFieldMissingError { id: flow.field_id })?;

            let (scale, rotation, translation) = flow.transform.to_scale_rotation_translation();

            let gpu_flow = GpuFlow {
                translation,
                field_index,
                rotation,
                scale,
                flags: flow.flags,
                linear_velocity: flow.linear_velocity,
                layers: flow.layers,
                angular_velocity: flow.angular_velocity,
                influence: flow.influence,
            };
            uniforms.0.push(gpu_flow);
        }
        uniforms
            .0
            .write_buffer(render_device.as_ref(), render_queue.as_ref());
    }
    Ok(())
}

#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct GpuFlow {
    translation: Vec3,
    field_index: u32,
    rotation: Quat,
    scale: Vec3,
    flags: FlowFlags,
    linear_velocity: Vec3,
    layers: FlowLayers,
    angular_velocity: Vec3,
    influence: FlowInfluence,
}

// TODO:
// extract flows into arrays per-region
// - how to assign indices? Need stability + robustness. C.R.U.D.
// - create binding arrays
// VANES:
// - associate with region
// - extract to gpu
// - run compute shader to do pre-cull + sampling
// - readback to cpu with channel
// - quadratic averaging + variance?
// PROXIES:
// - need to design main-world api
