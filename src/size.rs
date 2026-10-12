//! A panel's size, and the guess at it. DDP carries a run of bytes and
//! says nothing of rows, so the width and height come from the user or
//! from the number of pixels in a frame.

use std::fmt;

/// The longest side a panel may have.
pub const MAX_SIDE: u32 = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

/// Sizes panels are commonly made in or tiled to, smallest first: LED
/// matrices of 8 and 16, and HUB75 panels of 32 and 64 alone or side by
/// side.
pub const PRESETS: [Size; 11] = [
    Size::new(8, 8),
    Size::new(16, 16),
    Size::new(32, 8),
    Size::new(32, 16),
    Size::new(32, 32),
    Size::new(64, 32),
    Size::new(64, 64),
    Size::new(128, 32),
    Size::new(128, 64),
    Size::new(192, 64),
    Size::new(128, 128),
];

impl Size {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub const fn pixels(self) -> usize {
        self.width as usize * self.height as usize
    }

    /// Whether both sides are between 1 and `MAX_SIDE`.
    pub const fn fits(self) -> bool {
        self.width >= 1 && self.width <= MAX_SIDE && self.height >= 1 && self.height <= MAX_SIDE
    }

    /// "WxH", such as "128x64".
    pub fn parse(text: &str) -> Option<Self> {
        let (w, h) = text.split_once(['x', 'X'])?;
        let size = Self::new(w.trim().parse().ok()?, h.trim().parse().ok()?);
        size.fits().then_some(size)
    }

    /// How many screen pixels each panel pixel gets in a space of `room`:
    /// the largest whole number that fits, so that every pixel is the same
    /// square, or, when the panel is bigger than the space, the fraction
    /// that makes it fit.
    pub fn scale_to_fit(self, room: (u32, u32)) -> f32 {
        let whole = (room.0 / self.width).min(room.1 / self.height);
        if whole >= 1 {
            return whole as f32;
        }
        (room.0 as f32 / self.width as f32).min(room.1 as f32 / self.height as f32)
    }

    /// How far from square: the long side over the short.
    fn aspect(self) -> f64 {
        f64::from(self.width.max(self.height)) / f64::from(self.width.min(self.height))
    }
}

impl fmt::Display for Size {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

/// A size for a frame of some number of pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Guess {
    pub size: Size,
    /// Pixels of the panel the frame does not reach, drawn black. Zero
    /// when the count is a tidy rectangle; otherwise the size was rounded
    /// up and is likely wrong.
    pub unused: usize,
}

/// Guesses a panel's size from the number of pixels in a frame. A count
/// can be many rectangles (4096 is 64x64 and 128x32), so this takes the
/// likeliest, in this order:
///
/// 1. a preset with exactly that many pixels, the squarest if several;
/// 2. a square;
/// 3. a rectangle whose sides are both multiples of 8, wider than tall and
///    no more than 8 to 1, the squarest there is (48x40 for 1920);
/// 4. failing all those, the count is not a tidy rectangle: the first of
///    32x32, 64x32, 64x64, 96x64, 96x96, ... that holds it, the rest of
///    the panel left black (96x64 for 4099).
///
/// Sides are multiples of 8 in step 3 because panels are; without that,
/// 1000 pixels would be a confident 40x25. `None` for no pixels, or more
/// than the largest panel.
pub fn guess(pixels: usize) -> Option<Guess> {
    if pixels == 0 || pixels > Size::new(MAX_SIDE, MAX_SIDE).pixels() {
        return None;
    }
    let squarest = |sizes: &mut dyn Iterator<Item = Size>| {
        sizes.min_by(|a, b| a.aspect().total_cmp(&b.aspect()))
    };
    let exact = |size| Some(Guess { size, unused: 0 });

    if let Some(preset) = squarest(&mut PRESETS.into_iter().filter(|p| p.pixels() == pixels)) {
        return exact(preset);
    }
    let root = pixels.isqrt();
    if root * root == pixels {
        return exact(Size::new(root as u32, root as u32));
    }
    let eights = (8..=root)
        .step_by(8)
        .filter(|h| pixels.is_multiple_of(*h))
        .filter_map(|h| {
            let w = pixels / h;
            (w.is_multiple_of(8) && w <= 8 * h && w <= MAX_SIDE as usize)
                .then(|| Size::new(w as u32, h as u32))
        });
    if let Some(size) = squarest(&mut eights.into_iter()) {
        return exact(size);
    }
    let steps = (1..=MAX_SIDE / 32)
        .flat_map(|n| [Size::new(n * 32, n * 32), Size::new((n + 1) * 32, n * 32)]);
    steps
        .filter(|size| size.fits())
        .find(|size| size.pixels() >= pixels)
        .map(|size| Guess {
            size,
            unused: size.pixels() - pixels,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guessed(pixels: usize) -> (String, usize) {
        let g = guess(pixels).expect("a guess");
        assert_eq!(g.size.pixels(), pixels + g.unused, "{pixels} pixels");
        (g.size.to_string(), g.unused)
    }

    #[test]
    fn presets_are_guessed_exactly() {
        for preset in PRESETS {
            let (size, unused) = guessed(preset.pixels());
            assert_eq!(unused, 0, "{preset}");
            assert_eq!(Size::parse(&size).unwrap().pixels(), preset.pixels());
        }
        assert_eq!(guessed(4096).0, "64x64", "not 128x32");
        assert_eq!(guessed(2048).0, "64x32");
        assert_eq!(guessed(1024).0, "32x32");
        assert_eq!(guessed(256).0, "16x16", "not 32x8");
        assert_eq!(guessed(8192).0, "128x64");
        assert_eq!(guessed(16384).0, "128x128");
        assert_eq!(guessed(12288).0, "192x64");
    }

    #[test]
    fn squares_and_rectangles_of_eights() {
        assert_eq!(guessed(100), ("10x10".into(), 0));
        assert_eq!(guessed(48 * 48), ("48x48".into(), 0));
        assert_eq!(guessed(1920), ("48x40".into(), 0), "not 60x32 or 120x16");
        assert_eq!(
            guessed(96 * 48),
            ("72x64".into(), 0),
            "the squarest, not 96x48"
        );
        assert_eq!(guessed(256 * 64), ("128x128".into(), 0), "the square first");
        assert_eq!(
            guessed(64 * 8),
            ("32x16".into(), 0),
            "a preset before a strip"
        );
        assert_eq!(guessed(72 * 8), ("24x24".into(), 0));
        assert_eq!(guessed(80 * 8), ("40x16".into(), 0));
        assert_eq!(guessed(64 * 8 + 64), ("24x24".into(), 0));
    }

    #[test]
    fn an_untidy_count_is_rounded_up_in_steps_of_32() {
        assert_eq!(guessed(4099), ("96x64".into(), 96 * 64 - 4099), "a prime");
        assert_eq!(guessed(1000), ("32x32".into(), 24), "40x25 is not a panel");
        assert_eq!(guessed(1500), ("64x32".into(), 548));
        assert_eq!(guessed(1), ("1x1".into(), 0), "one pixel is a square");
        assert_eq!(guessed(60), ("32x32".into(), 964), "a strip of 60");
        assert_eq!(guessed(2049), ("64x64".into(), 2047));
        assert_eq!(guessed(6145), ("96x96".into(), 96 * 96 - 6145));
        // 9 to 1 is too long and thin to be step 3's.
        assert_eq!(guessed(72 * 8 * 8 + 8), ("96x64".into(), 96 * 64 - 4616));
    }

    #[test]
    fn no_guess_for_nothing_or_too_much() {
        assert_eq!(guess(0), None);
        assert_eq!(guessed(1024 * 1024), ("1024x1024".into(), 0));
        assert_eq!(guess(1024 * 1024 + 1), None);
        assert_eq!(guessed(1024 * 1024 - 1).0, "1024x1024");
    }

    #[test]
    fn a_panel_is_scaled_by_whole_pixels_when_it_fits() {
        let panel = Size::new(64, 32);
        assert_eq!(panel.scale_to_fit((640, 640)), 10.0);
        assert_eq!(
            panel.scale_to_fit((700, 330)),
            10.0,
            "the tighter side decides"
        );
        assert_eq!(
            panel.scale_to_fit((700, 319)),
            9.0,
            "never a part of a pixel"
        );
        assert_eq!(panel.scale_to_fit((64, 32)), 1.0);
        assert_eq!(panel.scale_to_fit((32, 32)), 0.5, "too big: whatever fits");
        assert_eq!(panel.scale_to_fit((0, 0)), 0.0);
    }

    #[test]
    fn sizes_are_read_and_written_as_w_x_h() {
        assert_eq!(Size::parse("128x64"), Some(Size::new(128, 64)));
        assert_eq!(Size::parse("64X32"), Some(Size::new(64, 32)));
        assert_eq!(Size::new(64, 32).to_string(), "64x32");
        for bad in [
            "", "64", "64x", "x64", "0x64", "64x0", "1025x8", "-1x8", "8x8x8",
        ] {
            assert_eq!(Size::parse(bad), None, "{bad:?}");
        }
    }
}
