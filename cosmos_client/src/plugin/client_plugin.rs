use bevy::app::PluginGroupBuilder;
use bevy::audio::AudioPlugin;
use bevy::gltf::GltfPlugin;
use bevy::pbr::PbrPlugin;
use bevy::prelude::{AnimationPlugin, GilrsPlugin};
use bevy::sprite::SpritePlugin;
use bevy::text::TextPlugin;
use bevy::ui::UiPlugin;
use bevy::winit::WinitPlugin;
use crate::PluginGroup;

#[derive(Default)]
pub struct ClientPluginGroup;

impl PluginGroup for ClientPluginGroup {
    fn build(&mut self, group: &mut PluginGroupBuilder) {
        group.add(WinitPlugin::default());
        group.add(SpritePlugin::default());
        group.add(TextPlugin::default());
        group.add(UiPlugin::default());
        group.add(PbrPlugin::default());

        // NOTE: Load this after renderer initialization so that it knows about the supported
        // compressed texture formats
        group.add(GltfPlugin::default());
        group.add(AudioPlugin::default());
        group.add(GilrsPlugin::default());
        group.add(AnimationPlugin::default());
    }
}