use bevy_app::{PluginGroup, PluginGroupBuilder};

pub struct VanePlugins;

impl PluginGroup for VanePlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
    }
}
