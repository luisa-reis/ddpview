//! A diffuser in front of an LED panel: a sheet of black (tinted) acrylic
//! with a matte face, some distance in front of the LEDs. What it shows
//! is where each LED's light lands on its face; this works out how that
//! light is spread, once, for the shader to add up over the LEDs near
//! each point.
//!
//! The light of one LED, taken as a Lambertian emitter (as a display LED
//! of 110 to 120 degrees nearly is), crosses the gap, refracts into the
//! acrylic, loses what the surface reflects, and is absorbed by the tint
//! along its path: light going in at a slant travels further through the
//! sheet and is dimmed more, which keeps each LED's spot tighter than
//! the geometry alone would. The matte face scatters what reaches it, so
//! the panel looks like the light falling on that face: no image of the
//! LEDs behind it, and the board and packages hidden in the dark tint.

use std::f32::consts::PI;
use std::ops::RangeInclusive;

use crate::panel::Led;

/// Points in the spread profile the shader looks up.
pub const PROFILE: usize = 64;
/// The most LEDs out from the one under a point that the shader adds up,
/// in each direction: 33 by 33 at most. Light beyond is left out.
pub const MAX_REACH: u32 = 16;

/// Acrylic (PMMA).
const REFRACTIVE_INDEX: f32 = 1.49;
/// How much light 3 mm of the black sheet lets through, straight on,
/// leaving the surfaces aside: black acrylic made to have LEDs shine
/// through it passes a few tens of percent at 3 mm. A judgement, not a
/// measurement of a particular sheet.
const TINT_AT_3MM: f32 = 0.25;
/// The share of each LED's light that the radius a profile covers holds.
const COVERED: f32 = 0.995;
/// Slant angles the light is followed at.
const RAYS: usize = 4000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Diffuser {
    /// The sheet's thickness, and the gap between the LEDs and it, in
    /// millimetres.
    pub thickness: f32,
    pub distance: f32,
}

impl Diffuser {
    pub const DEFAULT: Diffuser = Diffuser {
        thickness: 3.0,
        distance: 0.0,
    };
    pub const THICKNESS: RangeInclusive<f32> = 0.5..=10.0;
    pub const DISTANCE: RangeInclusive<f32> = 0.0..=20.0;

    pub fn fits(&self) -> bool {
        Self::THICKNESS.contains(&self.thickness) && Self::DISTANCE.contains(&self.distance)
    }

    /// How the light of one of `led`'s LEDs lands on the sheet's face.
    pub fn spread(&self, led: &Led) -> Spread {
        let rays = self.rays();
        // The LED is not a point: its light leaves a patch of it, taken
        // as the round Gaussian its spread through the window describes.
        let sigma = led.spread / 2f32.sqrt();
        let total: f32 = rays.iter().map(|ray| ray.power).sum();
        let mut sorted = rays.clone();
        sorted.sort_by(|a, b| a.radius.total_cmp(&b.radius));
        let mut held = 0.0;
        let mut radius = sorted.last().map_or(0.0, |ray| ray.radius);
        for ray in &sorted {
            held += ray.power;
            if held >= COVERED * total {
                radius = ray.radius;
                break;
            }
        }
        let radius = (radius + 3.0 * sigma).min(MAX_REACH as f32 * led.pitch);
        let mut profile = [0.0; PROFILE];
        for (j, value) in profile.iter_mut().enumerate() {
            let at = radius * Spread::place(j);
            *value = rays
                .iter()
                .map(|ray| ray.power * ring(at, ray.radius, sigma))
                .sum();
        }
        Spread { radius, profile }
    }

    /// One LED's light followed through the gap and the sheet at slant
    /// angles from straight on to grazing: where on the face each slant
    /// lands, and the share of the LED's light that arrives there.
    fn rays(&self) -> Vec<Ray> {
        let absorbed = -TINT_AT_3MM.ln() / 3.0;
        let step = PI / 2.0 / RAYS as f32;
        (0..RAYS)
            .map(|i| {
                let slant = (i as f32 + 0.5) * step;
                let inside = (slant.sin() / REFRACTIVE_INDEX).asin();
                // A Lambertian emitter puts 2 cos sin of its light into
                // each slant; all of it, from straight on to grazing.
                let share = 2.0 * slant.cos() * slant.sin() * step;
                let path = self.thickness / inside.cos();
                Ray {
                    radius: self.distance * slant.tan() + self.thickness * inside.tan(),
                    power: share * fresnel(slant, inside) * (-absorbed * path).exp(),
                }
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy)]
struct Ray {
    radius: f32,
    power: f32,
}

/// How the light of one LED lies on the diffuser's face: light per square
/// millimetre against the distance from the LED, for an LED giving out 1.
/// A whole panel lit evenly gives `transmitted` per LED's cell.
#[derive(Debug, Clone, PartialEq)]
pub struct Spread {
    /// How far out the profile goes, in millimetres; beyond, nothing.
    pub radius: f32,
    /// The light at `radius * place(j)`, for each `j`.
    pub profile: [f32; PROFILE],
}

impl Spread {
    /// Where point `j` of the profile is, as a share of its radius:
    /// closer together near the LED, where the light changes most.
    pub fn place(j: usize) -> f32 {
        let u = j as f32 / (PROFILE - 1) as f32;
        u * u
    }

    /// The light at `at` millimetres from the LED.
    pub fn at(&self, at: f32) -> f32 {
        if at >= self.radius {
            return 0.0;
        }
        let u = (at / self.radius).sqrt() * (PROFILE - 1) as f32;
        let i = (u.floor() as usize).min(PROFILE - 2);
        let f = u - i as f32;
        self.profile[i] * (1.0 - f) + self.profile[i + 1] * f
    }

    /// The share of an LED's light that comes out of the face: what an
    /// evenly lit panel gives, relative to its LEDs.
    pub fn transmitted(&self) -> f32 {
        // Rings between profile points, in fine steps.
        let steps = 4000;
        let dr = self.radius / steps as f32;
        (0..steps)
            .map(|k| {
                let r = (k as f32 + 0.5) * dr;
                self.at(r) * 2.0 * PI * r * dr
            })
            .sum()
    }

    /// How many LEDs out from the one under a point the light reaches.
    pub fn reach(&self, pitch: f32) -> u32 {
        ((self.radius / pitch).ceil() as u32).clamp(1, MAX_REACH)
    }
}

/// The share of light that goes into the sheet at `slant` (refracted to
/// `inside`) rather than being reflected: Fresnel's equations, for light
/// of no particular polarisation.
fn fresnel(slant: f32, inside: f32) -> f32 {
    let (ci, ct) = (slant.cos(), inside.cos());
    let n = REFRACTIVE_INDEX;
    let s = ((ci - n * ct) / (ci + n * ct)).powi(2);
    let p = ((ct - n * ci) / (ct + n * ci)).powi(2);
    1.0 - (s + p) / 2.0
}

/// Light per square millimetre at `at` from the centre of a ring of light
/// of radius `radius` carrying 1, blurred by a round Gaussian of `sigma`.
fn ring(at: f32, radius: f32, sigma: f32) -> f32 {
    let s2 = sigma * sigma;
    (-(at - radius).powi(2) / (2.0 * s2)).exp() * scaled_i0(at * radius / s2) / (2.0 * PI * s2)
}

/// The Bessel function I0 times e^-x, which stays small where I0 itself
/// would overflow (Abramowitz and Stegun 9.8.1 and 9.8.2).
fn scaled_i0(x: f32) -> f32 {
    let x = x.abs();
    if x < 3.75 {
        let t = (x / 3.75).powi(2);
        let i0 = 1.0
            + t * (3.515_623
                + t * (3.089_942_4
                    + t * (1.206_749_2 + t * (0.265_973_2 + t * (0.036_076_8 + t * 0.004_581_3)))));
        i0 * (-x).exp()
    } else {
        let t = 3.75 / x;
        (0.398_942_3
            + t * (0.013_285_92
                + t * (0.002_253_19
                    + t * (-0.001_575_65
                        + t * (0.009_162_81
                            + t * (-0.020_577_06
                                + t * (0.026_355_37 + t * (-0.016_476_33 + t * 0.003_923_77))))))))
            / x.sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::PanelType;

    fn led() -> Led {
        PanelType::P1875Smd1515.led().unwrap()
    }

    fn spread(thickness: f32, distance: f32) -> Spread {
        Diffuser {
            thickness,
            distance,
        }
        .spread(&led())
    }

    /// Where the light has fallen to half of what it is over the LED.
    fn half_width(spread: &Spread) -> f32 {
        let peak = spread.at(0.0);
        let mut r = 0.0;
        while spread.at(r) > peak / 2.0 {
            r += 0.01;
        }
        r
    }

    #[test]
    fn the_default_is_3mm_against_the_leds() {
        assert_eq!(
            Diffuser::DEFAULT,
            Diffuser {
                thickness: 3.0,
                distance: 0.0
            }
        );
        assert!(Diffuser::DEFAULT.fits());
        assert!(!Diffuser {
            thickness: 0.0,
            distance: 0.0
        }
        .fits());
        assert!(!Diffuser {
            thickness: 3.0,
            distance: -1.0
        }
        .fits());
    }

    #[test]
    fn a_ring_blurred_still_carries_all_its_light() {
        for (radius, sigma) in [(0.0, 0.3), (1.0, 0.3), (2.0, 0.5), (8.0, 0.4)] {
            let dr = 0.002;
            let carried: f32 = (0..10_000)
                .map(|k| {
                    let r = (k as f32 + 0.5) * dr;
                    ring(r, radius, sigma) * 2.0 * PI * r * dr
                })
                .sum();
            assert!((carried - 1.0).abs() < 0.01, "{radius} {sigma}: {carried}");
        }
    }

    #[test]
    fn three_mm_lets_through_a_share_like_black_acrylic() {
        let through = spread(3.0, 0.0).transmitted();
        // The tint's 25% straight on, less what slanting paths and the
        // surface take.
        assert!((0.12..0.22).contains(&through), "{through}");
    }

    #[test]
    fn thicker_is_darker_and_wider() {
        let (thin, thick) = (spread(1.0, 0.0), spread(6.0, 0.0));
        assert!(thick.transmitted() < thin.transmitted() / 2.0);
        assert!(half_width(&thick) > half_width(&thin));
    }

    #[test]
    fn a_gap_spreads_the_light_but_takes_little_of_it() {
        let (near, far) = (spread(3.0, 0.0), spread(3.0, 5.0));
        assert!(half_width(&far) > 2.0 * half_width(&near));
        // Slanting light crosses the sheet at the angle refraction gives
        // it, whatever the gap: only the reach of the profile cuts some.
        assert!(far.transmitted() > 0.8 * near.transmitted());
    }

    #[test]
    fn the_light_falls_away_from_the_led() {
        let s = spread(3.0, 0.0);
        let mut last = f32::INFINITY;
        for j in 0..PROFILE {
            let here = s.at(s.radius * Spread::place(j) * 0.999);
            assert!(here <= last * 1.0001, "{j}");
            last = here;
        }
        assert_eq!(s.at(s.radius), 0.0);
        assert!(s.reach(led().pitch) >= 1);
        assert!(spread(10.0, 20.0).reach(led().pitch) <= MAX_REACH);
    }
}
