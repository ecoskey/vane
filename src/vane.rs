use std::time::Duration;

use bevy_app::{App, Plugin};
use bevy_ecs::{component::Component, event::EntityEvent};
use bevy_math::Vec3;
use bevy_transform::components::Transform;
use smallvec::SmallVec;

use crate::{activity::Activity, field::FlowVector, flow::FlowLayers};

pub struct VanePlugin;

impl Plugin for VanePlugin {
    fn build(&self, app: &mut App) {
        todo!()
    }
}

#[derive(Component, Default, Debug)]
#[component(immutable)]
#[require(FlowLayers::all(), Transform, VaneData, Activity)]
#[non_exhaustive]
pub enum Vane {
    #[default]
    Point,
}

pub struct VaneUpdate {
    pub timestamp: Duration,
    pub latency: Duration,
}

#[derive(Component, Default)]
pub struct VaneData {
    samples: SmallVec<[VaneSample; 1]>,
    last_update: Option<VaneUpdate>,
}

pub struct VaneSample {
    pub flow: FlowVector,
    pub position: Vec3,
}

// RENDER WORLD LOGIC ----------------------------------------------------------

// LOGIC FOR VANES
// 1. Make big list
