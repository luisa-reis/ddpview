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
    /// An LED matrix of 1.875 mm pitch built from SMD1415 LEDs.
    #[serde(rename = "p1.875-smd1415")]
    P1875Smd1415,
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
    /// The radius of the clear lens over the dies.
    pub lens: f32,
    /// A die's side, and where the red, green and blue ones sit.
    pub die: f32,
    pub red: [f32; 2],
    pub green: [f32; 2],
    pub blue: [f32; 2],
    /// How far light from a die spreads through the lens, and how bright
    /// that is beside the die itself.
    pub spread: f32,
    pub glow: f32,
}

/// SMD2121: a black package 2.1 mm square, the lens 1.6 mm across, dies
/// 0.3 mm: red top left, green right, blue bottom left.
const SMD2121: Led = Led {
    pitch: 3.0,
    package: [2.1, 2.1],
    corner: 0.15,
    lens: 0.8,
    die: 0.3,
    red: [-0.32, -0.3],
    green: [0.34, 0.0],
    blue: [-0.32, 0.3],
    spread: 0.5,
    glow: 0.85,
};

/// SMD1415: a black package 1.4 mm wide and 1.5 mm tall, its insides
/// those of the SMD2121 at two thirds, the dies a little less shrunk:
/// a die is much the same size whatever package it is in.
const SMD1415: Led = Led {
    pitch: 1.875,
    package: [1.4, 1.5],
    corner: 0.1,
    lens: 0.55,
    die: 0.24,
    red: [-0.21, -0.2],
    green: [0.22, 0.0],
    blue: [-0.21, 0.2],
    spread: 0.33,
    glow: 0.85,
};

impl PanelType {
    /// In the order the controls list them.
    pub const ALL: [PanelType; 3] = [
        PanelType::PixelArt,
        PanelType::P3Smd2121,
        PanelType::P1875Smd1415,
    ];

    /// What its LEDs look like; `None` for plain pixels.
    pub fn led(self) -> Option<Led> {
        match self {
            PanelType::PixelArt => None,
            PanelType::P3Smd2121 => Some(SMD2121),
            PanelType::P1875Smd1415 => Some(SMD1415),
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
            PanelType::P1875Smd1415 => "P1.875 SMD1415 LED",
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
            (PanelType::P1875Smd1415, "panel = \"p1.875-smd1415\"\n"),
        ] {
            assert_eq!(toml::to_string(&In { panel }).unwrap(), text);
            assert_eq!(toml::from_str::<In>(text).unwrap(), In { panel });
        }
    }

    #[test]
    fn pixel_art_is_the_default_and_listed_first() {
        assert_eq!(PanelType::default(), PanelType::PixelArt);
        assert_eq!(PanelType::ALL[0], PanelType::PixelArt);
        assert_eq!(PanelType::P3Smd2121.to_string(), "P3 SMD2121 LED");
        assert_eq!(PanelType::P3Smd2121.pitch_mm(), Some(3.0));
        assert_eq!(PanelType::P1875Smd1415.pitch_mm(), Some(1.875));
        assert_eq!(PanelType::PixelArt.pitch_mm(), None);
    }

    #[test]
    fn every_led_fits_in_its_cell() {
        for panel in PanelType::ALL {
            let Some(led) = panel.led() else { continue };
            let (w, h) = (led.package[0], led.package[1]);
            assert!(w < led.pitch && h < led.pitch, "{panel}: a gap between");
            assert!(2.0 * led.lens <= w.min(h), "{panel}: the lens on the face");
            for at in [led.red, led.green, led.blue] {
                let reach = at[0].hypot(at[1]) + led.die / 2.0 * 2f32.sqrt();
                assert!(reach <= led.lens, "{panel}: dies under the lens");
            }
        }
    }
}
