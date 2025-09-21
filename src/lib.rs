#![allow(clippy::type_complexity)]

use bevy_app::{PluginGroup, PluginGroupBuilder};

use crate::{
    activity::ActivityPlugin, culling::CullingPlugin, field::FlowFieldPlugin, flow::FlowPlugin,
    vane::VanePlugin,
};

pub mod activity;
pub mod culling;
pub mod field;
pub mod flow;
pub mod measure;
pub mod vane;

mod render;

pub struct VanePlugins;

impl PluginGroup for VanePlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(ActivityPlugin)
            .add(CullingPlugin)
            .add(FlowFieldPlugin)
            .add(FlowPlugin)
            .add(MeasurePlugin)
            .add(VanePlugin)
    }
}

pub mod prelude {
    pub use crate::{
        activity::{ActiveRegion, Activity},
        field::{FlowField, FlowFieldGenerator as _, uniform_flow_field},
        flow::{Flow, FlowInfluence, FlowLayers, InheritAngularVelocity, InheritLinearVelocity},
        measure::{Measure, Measured, Trigger, measures},
        vane::Vane,
    };
}
