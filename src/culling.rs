use bevy_app::{App, Plugin};
use bevy_ecs::component::Component;
use bevy_math::{UVec3, Vec3};
use bevy_reflect::Reflect;

use crate::prelude::ActiveRegion;

pub struct CullingPlugin;

impl Plugin for CullingPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<CullingResolution>()
            .register_required_components::<ActiveRegion, CullingResolution>();
    }
}

#[derive(Component, Reflect)]
pub enum CullingResolution {
    Fixed(UVec3),
    ApproxVoxelSize(Vec3),
}

impl Default for CullingResolution {
    fn default() -> Self {
        Self::ApproxVoxelSize(Vec3::splat(1.0))
    }
}

// TODO: clusters and such.
