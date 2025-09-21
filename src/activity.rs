use bevy_app::{App, Plugin, PostUpdate};
use bevy_camera::primitives::Aabb;
use bevy_ecs::{
    component::Component,
    entity::Entity,
    query::With,
    schedule::{IntoScheduleConfigs, SystemSet},
    system::{Commands, Query},
    world::Ref,
};
use bevy_math::{UVec3, Vec3, Vec3A};
use bevy_reflect::Reflect;
use bevy_render::sync_world::SyncToRenderWorld;
use bevy_transform::components::{GlobalTransform, Transform};
use core::ops::{Mul, Sub};

pub struct ActivityPlugin;

impl Plugin for ActivityPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<ActiveRegion>()
            .register_type::<Activity>()
            .add_systems(
                PostUpdate,
                (update_active_region_aabbs, update_activities).chain(),
            );
    }
}

#[derive(Component, Reflect)]
#[require(SyncToRenderWorld)]
#[require(Transform, Aabb, ActiveEntities)]
pub struct ActiveRegion;

#[derive(Component, Default)]
struct ActiveEntities(Vec<Entity>);

#[derive(Component, Default, Reflect)]
#[require(Aabb)]
pub enum Activity {
    Awake,
    #[default]
    Asleep,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash, SystemSet)]
struct ActivityUpdateSystems;

fn update_active_region_aabbs(
    mut active_regions: Query<(Ref<GlobalTransform>, &mut Aabb), With<ActiveRegion>>,
) {
    for (transform, mut aabb) in active_regions.iter_mut() {
        let (scale, _, translation) = transform.to_scale_rotation_translation();
        let center = translation.to_vec3a();
        let center_changed = center
            .sub(aabb.center)
            .abs()
            .cmpgt(Vec3A::splat(f32::EPSILON))
            .any();
        let half_extents = scale.to_vec3a().mul(0.5);
        let half_extents_changed = half_extents
            .sub(aabb.half_extents)
            .abs()
            .cmpgt(Vec3A::splat(f32::EPSILON))
            .any();
        if center_changed || half_extents_changed {
            *aabb = Aabb {
                center,
                half_extents,
            };
        }
    }
}

fn update_activities(
    mut commands: Commands,
    mut active_regions: Query<(&Aabb, &mut ActiveEntities), With<ActiveRegion>>,
    tracked_entities: Query<&Aabb, With<Activity>>,
) {
}
