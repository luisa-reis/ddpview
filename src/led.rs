//! The LED look: a material that draws each pixel of the frame texture as
//! an SMD2121 LED, in the shader beside this file (`led.wesl`).

use bevy::asset::embedded_asset;
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use bevy::sprite_render::{Material2d, Material2dPlugin};

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct LedMaterial {
    /// The panel's size in pixels.
    #[uniform(0)]
    pub size: Vec2,
    /// The frame, the same texture the plain pixels are drawn from.
    #[texture(1)]
    pub picture: Handle<Image>,
}

impl Material2d for LedMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://ddpview/led.wesl".into()
    }
}

pub fn plugin(app: &mut App) {
    embedded_asset!(app, "led.wesl");
    app.add_plugins(Material2dPlugin::<LedMaterial>::default());
}
