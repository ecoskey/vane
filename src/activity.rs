use bevy_app::{App, Plugin, PostUpdate};
use bevy_camera::primitives::Aabb;
use bevy_ecs::{
    component::Component,
    entity::Entity,
    error::BevyError,
    event::EntityEvent,
    query::{Has, With},
    schedule::{IntoScheduleConfigs, SystemSet},
    system::{Command, Local, Query, SystemParam, SystemState},
    world::{Ref, World},
};
use bevy_math::Vec3A;
use bevy_reflect::Reflect;
use bevy_render::sync_world::SyncToRenderWorld;
use bevy_transform::components::{GlobalTransform, Transform};
use bevy_utils::Parallel;
use core::ops::{Mul, Sub};

pub struct ActivityPlugin;

impl Plugin for ActivityPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<ActiveRegion>()
            .register_type::<TrackActivity>()
            .register_type::<Active>()
            .register_type::<Activate>()
            .register_type::<Deactivate>()
            .register_type::<SetActive>()
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
pub struct TrackActivity;

#[derive(Component, Default, Reflect)]
pub struct Active;

#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash, SystemSet)]
pub struct ActivitySystems;

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

#[derive(EntityEvent, Reflect)]
pub struct Activate {
    pub entity: Entity,
}

#[derive(EntityEvent, Reflect)]
pub struct Deactivate {
    pub entity: Entity,
}

#[derive(EntityEvent, Reflect)]
pub struct SetActive {
    pub entity: Entity,
    pub active: bool,
}

struct SetActiveMany {
    entities: Vec<Entity>,
    active: bool,
}

impl Command for SetActiveMany {
    fn apply(self, world: &mut World) -> () {
        for entity in self.entities.iter().copied() {
            let Ok(mut entity_mut) = world.get_entity_mut(entity) else {
                continue;
            };

            let is_active = entity_mut.contains::<Active>();
            if is_active == self.active {
                continue;
            }
            if self.active {
                entity_mut.insert(Active);
            } else {
                entity_mut.remove::<Active>();
            }

            world.trigger(SetActive {
                entity,
                active: self.active,
            });

            if self.active {
                world.trigger(Activate { entity });
            } else {
                world.trigger(Deactivate { entity });
            }
        }
    }
}

#[derive(SystemParam)]
struct UpdateActivitiesParams<'w, 's> {
    active_regions: Query<'w, 's, (&'static Aabb, &'static mut ActiveEntities), With<ActiveRegion>>,
    tracked_entities: Query<'w, 's, (Entity, &'static Aabb, Has<Active>), With<TrackActivity>>,
}

fn update_activities(
    world: &mut World,
    params: &mut SystemState<UpdateActivitiesParams>,
    mut activated: Local<Parallel<Vec<Entity>>>,
    mut insert_active_batch: Local<Vec<(Entity, Active)>>,
    mut deactivated: Local<Parallel<Vec<Entity>>>,
) -> Result<(), BevyError> {
    let mut params = params.get_mut(world);

    params
        .active_regions
        .iter_mut()
        .for_each(|(_, mut active_entities)| active_entities.0.clear());

    fn aabbs_intersect(a: Aabb, b: Aabb) -> bool {
        (a.min().cmplt(b.max())).all() || (b.min().cmplt(a.max())).all()
    }

    //TODO: par_iter
    params
        .tracked_entities
        .iter_mut()
        .for_each(|(entity, entity_aabb, was_active)| {
            let mut is_active = false;
            for (region_aabb, mut active_entities) in params.active_regions.iter_mut() {
                let intersects_region = aabbs_intersect(*entity_aabb, *region_aabb);
                is_active |= intersects_region;

                if is_active {
                    active_entities.0.push(entity);
                }

                if is_active != was_active {
                    if is_active {
                        activated.scope(|entities| entities.push(entity));
                    } else {
                        deactivated.scope(|entities| entities.push(entity));
                    }
                }
            }
        });

    activated.drain().for_each(|entity| {
        world.trigger(Activate { entity });
        world.trigger(SetActive {
            entity,
            active: true,
        });
        insert_active_batch.push((entity, Active));
    });
    world.try_insert_batch_if_new(insert_active_batch.drain(..))?;

    deactivated.drain().for_each(|entity| {
        world.trigger(Deactivate { entity });
        world.trigger(SetActive {
            entity,
            active: false,
        });
        if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
            entity_mut.remove::<Active>();
        }
    });

    Ok(())
}
