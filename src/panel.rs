//! The kind of panel the frames are shown as: what each pixel looks like,
//! not how many there are (that is `size`).

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PanelType {
    /// Plain square pixels touching each other, as an image editor shows
    /// them.
    #[default]
    PixelArt,
    /// An LED matrix of 3 mm pitch built from SMD2121 LEDs.
    #[serde(rename = "p3-smd2121")]
    P3Smd2121,
    /// An LED matrix of 1.875 mm pitch built from SMD1515 LEDs. Once
    /// misnamed SMD1515, the name settings files of then still use.
    #[serde(rename = "p1.875-smd1515", alias = "p1.875-smd1415")]
    P1875Smd1515,
}

/// What one LED of a panel looks like, face-on, in millimetres from the
/// centre of its cell, x to the right and y down. The shader draws from
/// these; they are here so that a panel type is one place.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Led {
    /// From one LED's centre to the next.
    pub pitch: f32,
    /// The package, width and height, and the rounding of its corners.
    pub package: [f32; 2],
    pub corner: f32,
    /// The window over the dies, width and height, and the rounding of
    /// its corners: a circle when the rounding is half of each side.
    pub window: [f32; 2],
    pub window_corner: f32,
    /// A die's width and height, and where the red, green and blue ones
    /// sit.
    pub die: [f32; 2],
    pub red: [f32; 2],
    pub green: [f32; 2],
    pub blue: [f32; 2],
    /// How far light from a die spreads through the window, and how
    /// bright that is beside the die itself.
    pub spread: f32,
    pub glow: f32,
    /// How much a diffuser in the window blurs the dies into their
    /// spread light: 0 for clear, 1 for only the spread light seen.
    pub diffuser: f32,
    /// The halo, when it is on: how far a die's light reaches across the
    /// package and board, and how bright that is at the die.
    pub halo_spread: f32,
    pub halo: f32,
}

/// What "Brighter" multiplies the LEDs' own light by. A real LED's dies
/// are far brighter than a screen's white; on a screen the light of a
/// small die is cut off at white and the panel looks dark. This puts some
/// of it back, a fully lit LED then giving about a quarter of the light
/// of a plain white pixel instead of a sixteenth.
pub const BRIGHTER: f32 = 4.0;

/// SMD2121: a black package 2.1 mm square, a round clear lens 1.6 mm
/// across, dies 0.3 mm: red top left, green right, blue bottom left.
const SMD2121: Led = Led {
    pitch: 3.0,
    package: [2.1, 2.1],
    corner: 0.15,
    window: [1.6, 1.6],
    window_corner: 0.8,
    die: [0.3, 0.3],
    red: [-0.32, -0.3],
    green: [0.34, 0.0],
    blue: [-0.32, 0.3],
    spread: 0.5,
    glow: 0.85,
    diffuser: 0.0,
    halo_spread: 0.75,
    halo: 0.3,
};

/// SMD1515, from NationStar's drawings (NH-B1515RGBA-HF and -GF,
/// REESTAR RS-1515MBAM, which agree): a black package 1.6 mm wide and
/// 1.5 mm tall with square corners; a rounded-square cup about 1.3 by
/// 1.22 mm at the top, filled with black matte epoxy with a diffuser;
/// dies about 0.15 by 0.11 mm in a column down the middle, blue 0.25 mm
/// above the centre, green on it, red 0.24 mm below. The spread and
/// diffuser are judged, not measured: the diffuser blurs the dies into
/// one soft spot that fills much of the cup.
const SMD1515: Led = Led {
    pitch: 1.875,
    package: [1.6, 1.5],
    corner: 0.04,
    window: [1.3, 1.22],
    window_corner: 0.3,
    die: [0.15, 0.11],
    red: [0.0, 0.24],
    green: [0.0, 0.0],
    blue: [0.0, -0.25],
    spread: 0.6,
    glow: 0.75,
    diffuser: 0.7,
    halo_spread: 0.47,
    halo: 0.3,
};

impl PanelType {
    /// In the order the controls list them.
    pub const ALL: [PanelType; 3] = [
        PanelType::PixelArt,
        PanelType::P3Smd2121,
        PanelType::P1875Smd1515,
    ];

    /// What its LEDs look like; `None` for plain pixels.
    pub fn led(self) -> Option<Led> {
        match self {
            PanelType::PixelArt => None,
            PanelType::P3Smd2121 => Some(SMD2121),
            PanelType::P1875Smd1515 => Some(SMD1515),
        }
    }

    /// The distance from one pixel's centre to the next, in millimetres;
    /// `None` for a panel with no physical size.
    pub fn pitch_mm(self) -> Option<f32> {
        self.led().map(|led| led.pitch)
    }
}

impl fmt::Display for PanelType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            PanelType::PixelArt => "Pixel Art",
            PanelType::P3Smd2121 => "P3 SMD2121 LED",
            PanelType::P1875Smd1515 => "P1.875 SMD1515 LED",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct In {
        panel: PanelType,
    }

    #[test]
    fn panel_types_are_written_as_short_names() {
        for (panel, text) in [
            (PanelType::PixelArt, "panel = \"pixel-art\"\n"),
            (PanelType::P3Smd2121, "panel = \"p3-smd2121\"\n"),
            (PanelType::P1875Smd1515, "panel = \"p1.875-smd1515\"\n"),
        ] {
            assert_eq!(toml::to_string(&In { panel }).unwrap(), text);
            assert_eq!(toml::from_str::<In>(text).unwrap(), In { panel });
        }
        assert_eq!(
            toml::from_str::<In>("panel = \"p1.875-smd1415\"\n").unwrap(),
            In {
                panel: PanelType::P1875Smd1515
            },
            "the old, wrong name still reads"
        );
    }

    #[test]
    fn pixel_art_is_the_default_and_listed_first() {
        assert_eq!(PanelType::default(), PanelType::PixelArt);
        assert_eq!(PanelType::ALL[0], PanelType::PixelArt);
        assert_eq!(PanelType::P3Smd2121.to_string(), "P3 SMD2121 LED");
        assert_eq!(PanelType::P3Smd2121.pitch_mm(), Some(3.0));
        assert_eq!(PanelType::P1875Smd1515.pitch_mm(), Some(1.875));
        assert_eq!(PanelType::PixelArt.pitch_mm(), None);
    }

    /// As the shader's `rounded_rectangle`.
    fn in_rounded_rectangle(p: [f32; 2], half: [f32; 2], corner: f32) -> bool {
        let q = [
            p[0].abs() - (half[0] - corner),
            p[1].abs() - (half[1] - corner),
        ];
        let outside = q[0].max(0.0).hypot(q[1].max(0.0)) + q[0].max(q[1]).min(0.0) - corner;
        outside <= 0.0
    }

    #[test]
    fn every_led_fits_in_its_cell() {
        for panel in PanelType::ALL {
            let Some(led) = panel.led() else { continue };
            let (w, h) = (led.package[0], led.package[1]);
            assert!(w < led.pitch && h < led.pitch, "{panel}: a gap between");
            assert!(
                led.window[0] <= w && led.window[1] <= h,
                "{panel}: the window on the face"
            );
            assert!(
                2.0 * led.window_corner <= led.window[0].min(led.window[1]),
                "{panel}: the window's corners fit it"
            );
            assert!(
                2.0 * led.halo_spread < led.pitch,
                "{panel}: the halo has faded by the edge of the cell"
            );
            let half = [led.window[0] / 2.0, led.window[1] / 2.0];
            for at in [led.red, led.green, led.blue] {
                for (sx, sy) in [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
                    let corner = [at[0] + sx * led.die[0] / 2.0, at[1] + sy * led.die[1] / 2.0];
                    assert!(
                        in_rounded_rectangle(corner, half, led.window_corner),
                        "{panel}: dies under the window"
                    );
                }
            }
        }
    }
}
