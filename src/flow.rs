use bevy_app::{App, Plugin};
use bevy_asset::{AsAssetId, AssetId, Handle};
use bevy_camera::primitives::Aabb;
use bevy_ecs::{
    component::Component,
    entity::Entity,
    error::BevyError,
    query::{Changed, Has, With},
    system::{Query, Res},
};
use bevy_math::{Quat, Vec3, Vec4, Vec4Swizzles};
use bevy_time::Time;
use bevy_transform::components::{GlobalTransform, Transform};

use crate::field::FlowField;

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
pub(crate) struct InheritedVelocity {
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
