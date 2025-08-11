#![allow(clippy::type_complexity)]

use bevy_app::{PluginGroup, PluginGroupBuilder};

use crate::{flow::FlowPlugin, vane::VanePlugin};

pub mod flow;
pub mod vane;

pub struct VanePlugins;

impl PluginGroup for VanePlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(FlowPlugin)
            .add(VanePlugin)
    }
}
