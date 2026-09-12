//! Phase 2 of the hand-off: named and indexed colours become pixels.
//!
//! Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/damage-and-render-state.md`
//! section "Resolved values, never ids" (R-14).
//!
//! This is the one part of building a frame that genuinely needs no engine
//! state, which is why it runs **outside the lock**
//! ([`crate::render::RenderState::map_colors`]). The palette itself is the
//! embedder's: the theme's sixteen, the OSC override table on top, versioned by
//! the palette epoch the render state carries so a theme change forces a full
//! rebuild rather than a half-mapped frame.

use crate::cell::{Color, NamedColor, Rgb};

// A dim colour is a 50 % mix with the **background**, which is what OneTerm
// paints today (`crates/terminal/src/palette.rs`, `TerminalPalette::dim`) — not
// a fraction toward black, which would visibly change every dim cell at the
// seam. Integer arithmetic, rounding half up, matches that function's `round()`.

/// Every colour a cell can name, resolved.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Palette {
    /// The 256-entry table: sixteen ANSI, the 6x6x6 cube, twenty-four greys.
    pub indexed: [Rgb; 256],
    pub foreground: Rgb,
    pub background: Rgb,
    pub cursor: Rgb,
    /// `None` falls back to the plain foreground, which is what the current
    /// renderer does.
    pub bright_foreground: Option<Rgb>,
    /// `None` dims the plain foreground.
    pub dim_foreground: Option<Rgb>,
}

impl Palette {
    /// The xterm defaults, so a terminal renders before any theme is applied.
    pub fn new() -> Palette {
        Palette {
            indexed: default_indexed(),
            foreground: Rgb {
                r: 0xd8,
                g: 0xd8,
                b: 0xd8,
            },
            background: Rgb {
                r: 0x18,
                g: 0x18,
                b: 0x18,
            },
            cursor: Rgb {
                r: 0xd8,
                g: 0xd8,
                b: 0xd8,
            },
            bright_foreground: None,
            dim_foreground: None,
        }
    }

    /// Resolve one colour. Already-resolved `Rgb` passes straight through, which
    /// is what makes [`crate::render::RenderState::map_colors`] idempotent.
    pub fn resolve(&self, color: Color) -> Rgb {
        match color {
            Color::Rgb(rgb) => rgb,
            Color::Palette(index) => self.indexed[index as usize],
            Color::Named(named) => self.named(named),
        }
    }

    fn named(&self, named: NamedColor) -> Rgb {
        match named {
            NamedColor::Black => self.indexed[0],
            NamedColor::Red => self.indexed[1],
            NamedColor::Green => self.indexed[2],
            NamedColor::Yellow => self.indexed[3],
            NamedColor::Blue => self.indexed[4],
            NamedColor::Magenta => self.indexed[5],
            NamedColor::Cyan => self.indexed[6],
            NamedColor::White => self.indexed[7],
            NamedColor::BrightBlack => self.indexed[8],
            NamedColor::BrightRed => self.indexed[9],
            NamedColor::BrightGreen => self.indexed[10],
            NamedColor::BrightYellow => self.indexed[11],
            NamedColor::BrightBlue => self.indexed[12],
            NamedColor::BrightMagenta => self.indexed[13],
            NamedColor::BrightCyan => self.indexed[14],
            NamedColor::BrightWhite => self.indexed[15],
            NamedColor::Foreground => self.foreground,
            NamedColor::Background => self.background,
            NamedColor::Cursor => self.cursor,
            NamedColor::BrightForeground => self.bright_foreground.unwrap_or(self.foreground),
            NamedColor::DimForeground => self.dim_foreground.unwrap_or(self.dim(self.foreground)),
            NamedColor::DimBlack => self.dim(self.indexed[0]),
            NamedColor::DimRed => self.dim(self.indexed[1]),
            NamedColor::DimGreen => self.dim(self.indexed[2]),
            NamedColor::DimYellow => self.dim(self.indexed[3]),
            NamedColor::DimBlue => self.dim(self.indexed[4]),
            NamedColor::DimMagenta => self.dim(self.indexed[5]),
            NamedColor::DimCyan => self.dim(self.indexed[6]),
            NamedColor::DimWhite => self.dim(self.indexed[7]),
        }
    }

    /// SGR 2 against this palette's own background.
    ///
    /// Public because the embedder's theme owns the background: an adapter that
    /// swaps `background` gets the matching dim colours for free, and one that
    /// needs different dim colours entirely sets `indexed` and
    /// `dim_foreground` instead.
    pub fn dim(&self, color: Rgb) -> Rgb {
        let mix = |channel: u8, background: u8| {
            (u16::from(channel) + u16::from(background)).div_ceil(2) as u8
        };
        Rgb {
            r: mix(color.r, self.background.r),
            g: mix(color.g, self.background.g),
            b: mix(color.b, self.background.b),
        }
    }
}

impl Default for Palette {
    fn default() -> Palette {
        Palette::new()
    }
}

fn default_indexed() -> [Rgb; 256] {
    const BASE: [(u8, u8, u8); 16] = [
        (0x00, 0x00, 0x00),
        (0xcd, 0x00, 0x00),
        (0x00, 0xcd, 0x00),
        (0xcd, 0xcd, 0x00),
        (0x00, 0x00, 0xee),
        (0xcd, 0x00, 0xcd),
        (0x00, 0xcd, 0xcd),
        (0xe5, 0xe5, 0xe5),
        (0x7f, 0x7f, 0x7f),
        (0xff, 0x00, 0x00),
        (0x00, 0xff, 0x00),
        (0xff, 0xff, 0x00),
        (0x5c, 0x5c, 0xff),
        (0xff, 0x00, 0xff),
        (0x00, 0xff, 0xff),
        (0xff, 0xff, 0xff),
    ];
    const CUBE: [u8; 6] = [0, 0x5f, 0x87, 0xaf, 0xd7, 0xff];

    let mut table = [Rgb { r: 0, g: 0, b: 0 }; 256];
    for (slot, &(r, g, b)) in table.iter_mut().zip(BASE.iter()) {
        *slot = Rgb { r, g, b };
    }
    for index in 0..216usize {
        table[16 + index] = Rgb {
            r: CUBE[index / 36],
            g: CUBE[(index / 6) % 6],
            b: CUBE[index % 6],
        };
    }
    for index in 0..24usize {
        let level = 8 + index as u8 * 10;
        table[232 + index] = Rgb {
            r: level,
            g: level,
            b: level,
        };
    }
    table
}
