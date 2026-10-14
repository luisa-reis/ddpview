//! The LED look: a material that draws each pixel of the frame texture as
//! an LED, in the shader beside this file (`led.wesl`). What the LED looks
//! like comes from the panel type (`ddpview::panel::Led`).

use bevy::asset::embedded_asset;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{Material2d, Material2dPlugin};
use ddpview::panel::{Led, BRIGHTER};

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct LedMaterial {
    #[uniform(0)]
    pub shape: Shape,
    /// The frame, the same texture the plain pixels are drawn from.
    #[texture(1)]
    pub picture: Handle<Image>,
}

/// The panel's size and its LED, as the shader's `Shape` has them. Half
/// sizes, as the shader measures from the centre.
#[derive(Clone, Copy, ShaderType)]
pub struct Shape {
    /// The panel's size in pixels.
    pub size: Vec2,
    pub pitch: f32,
    pub corner: f32,
    /// The package; `body` because `package` is a WESL keyword.
    pub body: Vec2,
    pub lens: f32,
    pub die: f32,
    pub red: Vec2,
    pub green: Vec2,
    pub blue: Vec2,
    pub spread: f32,
    pub glow: f32,
    pub halo_spread: f32,
    /// 0 with the halo off.
    pub halo: f32,
    /// 1, or `BRIGHTER` when the LEDs are made brighter.
    pub gain: f32,
}

impl Shape {
    pub fn new(size: Vec2, led: Led, halo: bool, brighter: bool) -> Self {
        Self {
            size,
            pitch: led.pitch,
            corner: led.corner,
            body: Vec2::from(led.package) / 2.0,
            lens: led.lens,
            die: led.die / 2.0,
            red: led.red.into(),
            green: led.green.into(),
            blue: led.blue.into(),
            spread: led.spread,
            glow: led.glow,
            halo_spread: led.halo_spread,
            halo: if halo { led.halo } else { 0.0 },
            gain: if brighter { BRIGHTER } else { 1.0 },
        }
    }
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
