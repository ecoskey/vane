use bevy_app::{PluginGroup, PluginGroupBuilder};

pub struct VanePlugins;

impl PluginGroup for VanePlugins {
    fn build(self) -> PluginGroupBuilder {
        let mut plugin_group = PluginGroupBuilder::start::<Self>();
        plugin_group
    }
}
