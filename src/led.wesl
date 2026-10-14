// A panel of LEDs seen face-on. Each panel pixel is one LED: a black
// package with a round lens, and under it a red, a green and a blue die,
// each lit by its own channel of the frame. What the LED looks like comes
// from the panel type (`Shape`). Lengths are in millimetres, measured
// from the centre of the LED's cell, x to the right and y down.

import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
@if(SRGB_OUTPUT)
import bevy_render::color_operations::linear_to_srgb;

// As `Shape` in led.rs, field for field. Sizes are halves.
struct Shape {
    // The panel's size in pixels.
    size: vec2<f32>,
    pitch: f32,
    corner: f32,
    // The package (`package` is a word WESL keeps for itself).
    body: vec2<f32>,
    lens: f32,
    die: f32,
    red: vec2<f32>,
    green: vec2<f32>,
    blue: vec2<f32>,
    // How far light from a die spreads through the lens, and how bright
    // the spread is beside the die itself.
    spread: f32,
    glow: f32,
    // The halo: how far light from a die reaches across the package and
    // board, and how bright it is at the die; 0 when the halo is off.
    halo_spread: f32,
    halo: f32,
    // What the LEDs' own light is multiplied by: 1, or more when they
    // are made brighter.
    gain: f32,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> shape: Shape;
// The frame, one texel a pixel. Read with textureLoad: no filtering.
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var picture: texture_2d<f32>;

// Unlit surfaces, linear: the board between packages, the package's
// face, and the lens, a little lighter for the cup below it.
const BOARD: vec3<f32> = vec3(0.003, 0.003, 0.0035);
const FACE: vec3<f32> = vec3(0.012, 0.012, 0.013);
const CUP: vec3<f32> = vec3(0.022, 0.022, 0.024);

// Samples along each side of a screen pixel: 16 in all. A die is a few
// tenths of a millimetre, often less than a screen pixel; averaging
// samples gives it the share of the pixel it covers instead of a
// flicker between all and nothing.
const SAMPLES: i32 = 4;

// 1 inside a rounded rectangle of half-sides `half`, 0 outside.
fn rounded_rectangle(p: vec2<f32>, half: vec2<f32>, corner: f32) -> f32 {
    let q = abs(p) - (half - vec2(corner));
    let outside = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - corner;
    return select(0.0, 1.0, outside <= 0.0);
}

fn square(p: vec2<f32>, half: f32) -> f32 {
    return select(0.0, 1.0, max(abs(p.x), abs(p.y)) <= half);
}

fn spread(p: vec2<f32>) -> f32 {
    return exp(-dot(p, p) / (shape.spread * shape.spread));
}

fn halo(p: vec2<f32>) -> f32 {
    return exp(-dot(p, p) / (shape.halo_spread * shape.halo_spread));
}

// Light past what the screen can show, scaled down as a whole rather
// than cut off channel by channel: an orange made brighter stays orange
// instead of turning yellow.
fn within_white(light: vec3<f32>) -> vec3<f32> {
    return light / max(1.0, max(light.r, max(light.g, light.b)));
}

// The light leaving one point of the panel. `cell` is in pixels from the
// panel's top left.
fn shade(cell: vec2<f32>) -> vec3<f32> {
    if any(cell < vec2(0.0)) || any(cell >= shape.size) {
        return BOARD;
    }
    let pixel = vec2<i32>(floor(cell));
    let colour = textureLoad(picture, pixel, 0).rgb;
    let p = (fract(cell) - 0.5) * shape.pitch;
    let r = shape.red - p;
    let g = shape.green - p;
    let b = shape.blue - p;
    // The halo reaches over everything in the cell, lens or not. Light
    // from a neighbouring LED's halo is left out: by the edge of the cell
    // there is little of it.
    var lit = shape.halo * vec3(halo(r), halo(g), halo(b));
    var surface = BOARD;
    if rounded_rectangle(p, shape.body, shape.corner) == 1.0 {
        surface = FACE;
        if length(p) <= shape.lens {
            surface = CUP;
            let dies = vec3(
                square(r, shape.die),
                square(g, shape.die),
                square(b, shape.die),
            );
            let glow = shape.glow * vec3(spread(r), spread(g), spread(b));
            lit += max(dies, glow);
        }
    }
    return within_white(surface + shape.gain * colour * lit);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let cell = in.uv * shape.size;
    // This screen pixel's extent, in pixels of the panel.
    let across = dpdx(cell);
    let down = dpdy(cell);
    var light = vec3(0.0);
    for (var i = 0; i < SAMPLES; i++) {
        for (var j = 0; j < SAMPLES; j++) {
            let at = (vec2(f32(i), f32(j)) + 0.5) / f32(SAMPLES) - 0.5;
            light += shade(cell + at.x * across + at.y * down);
        }
    }
    var out = vec4(light / f32(SAMPLES * SAMPLES), 1.0);
@if(SRGB_OUTPUT)
    out = vec4(linear_to_srgb(out.rgb), out.a);
    return out;
}
