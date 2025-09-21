use bevy_app::{App, Plugin};
use bevy_asset::AssetId;
use bevy_camera::primitives::Aabb;
use bevy_ecs::{
    component::Component,
    entity::Entity,
    query::{Has, With},
    system::Query,
};
use bevy_math::Vec3;
use bevy_render::{Extract, sync_world::MainEntityHashMap};
use bevy_transform::components::GlobalTransform;

use crate::{
    field::FlowField,
    flow::{
        Flow, FlowInfluence, FlowLayers, InheritAngularVelocity, InheritLinearVelocity,
        InheritedVelocity,
    },
};

pub struct VaneRenderPlugin;

impl Plugin for VaneRenderPlugin {
    fn build(&self, app: &mut App) {
        todo!()
    }
}

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
    regions: Extract<Query<&Contains, With<ActiveRegion>>>,
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
    regions: Query<(&ExtractedRegion, &mut ExtractedRegionFields), With<ActiveRegion>>,
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
