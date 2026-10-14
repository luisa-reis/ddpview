//! The picture: the panel, as large as fits. The frame is kept in a
//! texture (`Picture`), drawn either by a sprite as plain square pixels or
//! by a rectangle with the LED material (`led.rs`), as the panel type
//! says. Both are placed alike; one is shown.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::PrimaryWindow;
use ddpview::panel::PanelType;
use ddpview::size::Size;

use crate::led::{LedMaterial, Shape};
use crate::session::Session;

/// The frame as a texture, one texel a pixel, the size of the panel.
#[derive(Resource)]
pub struct Picture(pub Handle<Image>);

/// The part of the window the panel may fill, in logical pixels from the
/// window's top left. The controls set it to what they leave free.
#[derive(Resource, Default)]
pub struct Room {
    pub min: Vec2,
    pub max: Vec2,
}

/// Where the panel was put, for anything drawn in relation to it.
#[derive(Resource, Default)]
pub struct Placed {
    /// Screen pixels to a panel pixel.
    pub scale: f32,
}

#[derive(Component)]
struct PanelSprite;

#[derive(Component)]
struct PanelLeds;

pub fn plugin(app: &mut App) {
    app.init_resource::<Room>()
        .init_resource::<Placed>()
        .add_systems(Startup, setup)
        .add_systems(Update, (paint, show, place));
}

/// A panel's pixels as a texture. The bytes are sRGB, as a sender means
/// them to be seen, so they reach the screen unchanged.
fn image(size: Size, rgba: Vec<u8>) -> Image {
    Image::new(
        Extent3d {
            width: size.width,
            height: size.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
}

fn black(size: Size) -> Vec<u8> {
    [0, 0, 0, 255].repeat(size.pixels())
}

fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<LedMaterial>>,
    session: Res<Session>,
) {
    let picture = images.add(image(session.size, black(session.size)));
    commands.spawn(Camera2d);
    commands.spawn((Sprite::from_image(picture.clone()), PanelSprite));
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(1.0, 1.0))),
        MeshMaterial2d(materials.add(LedMaterial {
            shape: shape(&session),
            picture: picture.clone(),
        })),
        PanelLeds,
    ));
    commands.insert_resource(Picture(picture));
}

fn dimensions(size: Size) -> Vec2 {
    Vec2::new(size.width as f32, size.height as f32)
}

/// The LED material's shape for the session's panel. Pixel Art has no
/// LED and does not show the material; it is given one all the same.
fn shape(session: &Session) -> Shape {
    let led = session
        .panel
        .led()
        .or(PanelType::P3Smd2121.led())
        .expect("an LED panel type");
    Shape::new(dimensions(session.size), led)
}

/// Shows the drawing the panel type asks for, and hides the other.
fn show(
    session: Res<Session>,
    mut sprite: Single<&mut Visibility, (With<PanelSprite>, Without<PanelLeds>)>,
    mut leds: Single<&mut Visibility, With<PanelLeds>>,
) {
    let pixel_art = session.panel == PanelType::PixelArt;
    let shown = |on| {
        if on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        }
    };
    sprite.set_if_neq(shown(pixel_art));
    leds.set_if_neq(shown(!pixel_art));
}

/// Copies the frame into the texture when there is a new one or the panel
/// changed size or type. The frame is cut to the panel or padded with black.
fn paint(
    mut session: ResMut<Session>,
    picture: Res<Picture>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<LedMaterial>>,
    leds: Single<&MeshMaterial2d<LedMaterial>>,
) {
    if !session.redraw {
        return;
    }
    session.bypass_change_detection().redraw = false;
    let size = session.size;
    let rgba = match &session.frame {
        Some(frame) => frame.rgba(size.pixels()),
        None => black(size),
    };
    if let Some(mut picture) = images.get_mut(&picture.0) {
        *picture = image(size, rgba);
    }
    // Touched even when nothing about it changed, so that the material
    // is prepared again with the new texture.
    if let Some(mut material) = materials.get_mut(&leds.0) {
        material.shape = shape(&session);
    }
}

/// Sizes and places the sprite in the room it has: centred, each pixel a
/// whole number of screen pixels when the panel fits, and starting on a
/// screen pixel so that none is cut in two.
fn place(
    session: Res<Session>,
    room: Res<Room>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut placed: ResMut<Placed>,
    sprite: Single<(&mut Sprite, &mut Transform), With<PanelSprite>>,
    leds: Single<&mut Transform, (With<PanelLeds>, Without<PanelSprite>)>,
) {
    let factor = window.scale_factor();
    let size = session.size;
    // In screen (physical) pixels from here on.
    let min = (room.min * factor).ceil();
    let max = (room.max * factor).floor();
    let space = (max - min).max(Vec2::ZERO);
    let scale = size.scale_to_fit((space.x as u32, space.y as u32));
    let drawn = dimensions(size) * scale;
    let corner = min + ((space - drawn) / 2.0).floor();
    // And back to the camera's: logical pixels from the window's centre,
    // y upwards.
    let centre = (corner + drawn / 2.0) / factor;
    let window_size = Vec2::new(window.width(), window.height());
    let (mut sprite, mut transform) = sprite.into_inner();
    sprite.custom_size = Some(drawn / factor);
    transform.translation = Vec3::new(
        centre.x - window_size.x / 2.0,
        window_size.y / 2.0 - centre.y,
        0.0,
    );
    // The LEDs' rectangle is 1 by 1: its scale is its size.
    let mut leds = leds.into_inner();
    leds.translation = transform.translation;
    leds.scale = (drawn / factor).extend(1.0);
    placed.scale = scale;
}
