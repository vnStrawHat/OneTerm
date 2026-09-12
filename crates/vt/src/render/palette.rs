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

/// The reference's `DIM_FACTOR`, as integer arithmetic: two thirds.
const DIM_NUMERATOR: u16 = 2;
const DIM_DENOMINATOR: u16 = 3;

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
            NamedColor::DimForeground => self.dim_foreground.unwrap_or(dim(self.foreground)),
            NamedColor::DimBlack => dim(self.indexed[0]),
            NamedColor::DimRed => dim(self.indexed[1]),
            NamedColor::DimGreen => dim(self.indexed[2]),
            NamedColor::DimYellow => dim(self.indexed[3]),
            NamedColor::DimBlue => dim(self.indexed[4]),
            NamedColor::DimMagenta => dim(self.indexed[5]),
            NamedColor::DimCyan => dim(self.indexed[6]),
            NamedColor::DimWhite => dim(self.indexed[7]),
        }
    }
}

impl Default for Palette {
    fn default() -> Palette {
        Palette::new()
    }
}

fn dim(color: Rgb) -> Rgb {
    let scale = |channel: u8| (u16::from(channel) * DIM_NUMERATOR / DIM_DENOMINATOR) as u8;
    Rgb {
        r: scale(color.r),
        g: scale(color.g),
        b: scale(color.b),
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
