use std::{ops::Range, time::Duration};

use bevy_app::{App, Plugin};
use bevy_ecs::{
    component::Component,
    entity::Entity,
    event::{EntityEvent, Event, Trigger},
};
use bevy_math::Vec3;
use bevy_transform::components::Transform;
use smallvec::SmallVec;

use crate::{activity::TrackActivity, field::FlowVector, flow::FlowLayers};

pub struct VanePlugin;

impl Plugin for VanePlugin {
    fn build(&self, app: &mut App) {
        todo!()
    }
}

#[derive(Component, Default, Debug)]
#[component(immutable)]
#[require(FlowLayers::all(), Transform, VaneData, TrackActivity)]
#[non_exhaustive]
pub enum Vane {
    #[default]
    Point,
}

#[derive(Event)]
pub struct UpdateManyVanes {
    pub timestamp: Duration,
    pub latency: Duration,
    pub ranges: Box<[(Entity, Range<u32>)]>,
    pub samples: Box<[FlowVector]>,
}

#[derive(EntityEvent)]
pub struct UpdateVane<'ev> {
    pub timestamp: Duration,
    pub latency: Duration,
    #[event_target]
    pub vane: Entity,
    pub samples: &'ev [FlowVector],
}

#[derive(Component, Default)]
pub struct VaneData {
    samples: SmallVec<[VaneSample; 1]>,
    last_update: Option<Duration>,
}

pub struct VaneSample {
    pub flow: FlowVector,
    pub position: Vec3,
}
