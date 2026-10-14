// A panel of SMD2121 LEDs at 3 mm pitch, seen face-on. Each panel pixel
// is one LED: a black 2.1 mm package with a round lens, and under it a
// red, a green and a blue die, each lit by its own channel of the frame.
// Lengths are in millimetres, measured from the centre of the LED's cell,
// x to the right and y down.

import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;
@if(SRGB_OUTPUT)
import bevy_render::color_operations::linear_to_srgb;

struct Led {
    // The panel's size in pixels.
    size: vec2<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> led: Led;
// The frame, one texel a pixel. Read with textureLoad: no filtering.
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var picture: texture_2d<f32>;

const PITCH: f32 = 3.0;
// Half the package's side (2.1 mm) and the rounding of its corners.
const PACKAGE: f32 = 1.05;
const CORNER: f32 = 0.15;
// The radius of the clear lens over the dies.
const LENS: f32 = 0.8;
// Half a die's side, and where each sits: red top left, green right,
// blue bottom left.
const DIE: f32 = 0.15;
const RED_AT: vec2<f32> = vec2(-0.32, -0.3);
const GREEN_AT: vec2<f32> = vec2(0.34, 0.0);
const BLUE_AT: vec2<f32> = vec2(-0.32, 0.3);
// How far light from a die spreads through the lens, and how bright the
// spread is beside the die itself.
const SPREAD: f32 = 0.5;
const GLOW: f32 = 0.85;

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

// 1 inside a rounded square of half-side `half`, 0 outside.
fn rounded_square(p: vec2<f32>, half: f32, corner: f32) -> f32 {
    let q = abs(p) - vec2(half - corner);
    let outside = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - corner;
    return select(0.0, 1.0, outside <= 0.0);
}

fn square(p: vec2<f32>, half: f32) -> f32 {
    return select(0.0, 1.0, max(abs(p.x), abs(p.y)) <= half);
}

fn spread(p: vec2<f32>) -> f32 {
    return exp(-dot(p, p) / (SPREAD * SPREAD));
}

// The light leaving one point of the panel. `cell` is in pixels from the
// panel's top left.
fn shade(cell: vec2<f32>) -> vec3<f32> {
    if any(cell < vec2(0.0)) || any(cell >= led.size) {
        return BOARD;
    }
    let pixel = vec2<i32>(floor(cell));
    let colour = textureLoad(picture, pixel, 0).rgb;
    let p = (fract(cell) - 0.5) * PITCH;
    if rounded_square(p, PACKAGE, CORNER) == 0.0 {
        return BOARD;
    }
    if length(p) > LENS {
        return FACE;
    }
    let r = RED_AT - p;
    let g = GREEN_AT - p;
    let b = BLUE_AT - p;
    let dies = vec3(
        square(r, DIE),
        square(g, DIE),
        square(b, DIE),
    );
    let glow = GLOW * vec3(spread(r), spread(g), spread(b));
    return CUP + colour * max(dies, glow);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let cell = in.uv * led.size;
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
