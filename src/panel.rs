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
    /// An LED matrix of 3 mm pitch built from SMD2121 LEDs: black 2.1 mm
    /// packages, each with a red, a green and a blue die, on a dark board.
    #[serde(rename = "p3-smd2121")]
    P3Smd2121,
}

impl PanelType {
    /// In the order the controls list them.
    pub const ALL: [PanelType; 2] = [PanelType::PixelArt, PanelType::P3Smd2121];

    /// The distance from one pixel's centre to the next, in millimetres;
    /// `None` for a panel with no physical size.
    pub fn pitch_mm(self) -> Option<f32> {
        match self {
            PanelType::PixelArt => None,
            PanelType::P3Smd2121 => Some(3.0),
        }
    }
}

impl fmt::Display for PanelType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            PanelType::PixelArt => "Pixel Art",
            PanelType::P3Smd2121 => "P3 SMD2121 LED",
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
    }
}
