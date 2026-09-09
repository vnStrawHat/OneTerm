//! ANSI palette, VteRgb ↔ Rgba conversion, and the per-palette `Color` → `Hsla` table.

use alacritty_terminal::vte::ansi::Rgb as VteRgb;
use gpui::{Hsla, Rgba};
use oneterm_terminal::{TerminalPalette, resolve_color};

use crate::render::frame::{Color as FrameColor, Fnv1a};

/// Fixed ANSI 16-color palette (the Tango color scheme).
pub(crate) const ANSI_16: [VteRgb; 16] = [
    VteRgb {
        r: 0x00,
        g: 0x00,
        b: 0x00,
    }, // 0 black
    VteRgb {
        r: 0xcc,
        g: 0x00,
        b: 0x00,
    }, // 1 red
    VteRgb {
        r: 0x4e,
        g: 0x9a,
        b: 0x06,
    }, // 2 green
    VteRgb {
        r: 0xc4,
        g: 0xa0,
        b: 0x00,
    }, // 3 yellow
    VteRgb {
        r: 0x34,
        g: 0x65,
        b: 0xa4,
    }, // 4 blue
    VteRgb {
        r: 0x75,
        g: 0x50,
        b: 0x7b,
    }, // 5 magenta
    VteRgb {
        r: 0x06,
        g: 0x98,
        b: 0x9a,
    }, // 6 cyan
    VteRgb {
        r: 0xd3,
        g: 0xd7,
        b: 0xcf,
    }, // 7 white
    VteRgb {
        r: 0x55,
        g: 0x57,
        b: 0x53,
    }, // 8 bright black
    VteRgb {
        r: 0xef,
        g: 0x29,
        b: 0x29,
    }, // 9 bright red
    VteRgb {
        r: 0x8a,
        g: 0xe2,
        b: 0x34,
    }, // 10 bright green
    VteRgb {
        r: 0xfc,
        g: 0xe9,
        b: 0x4f,
    }, // 11 bright yellow
    VteRgb {
        r: 0x72,
        g: 0x9f,
        b: 0xcf,
    }, // 12 bright blue
    VteRgb {
        r: 0xad,
        g: 0x7f,
        b: 0xa8,
    }, // 13 bright magenta
    VteRgb {
        r: 0x34,
        g: 0xe2,
        b: 0xe2,
    }, // 14 bright cyan
    VteRgb {
        r: 0xee,
        g: 0xee,
        b: 0xec,
    }, // 15 bright white
];

/// `vte::ansi::Rgb` (u8) → `gpui::Rgba` (0..1, alpha 1).
pub(crate) fn rgba_from_vte(c: VteRgb) -> Rgba {
    Rgba {
        r: c.r as f32 / 255.0,
        g: c.g as f32 / 255.0,
        b: c.b as f32 / 255.0,
        a: 1.0,
    }
}

/// `gpui::Rgba` (0..1) → `vte::ansi::Rgb` (u8).
pub(crate) fn vte_from_rgba(c: Rgba) -> VteRgb {
    VteRgb {
        r: (c.r * 255.0).round().clamp(0.0, 255.0) as u8,
        g: (c.g * 255.0).round().clamp(0.0, 255.0) as u8,
        b: (c.b * 255.0).round().clamp(0.0, 255.0) as u8,
    }
}

/// `vte::ansi::Rgb` → `gpui::Hsla` (via `Rgba`).
pub(crate) fn hsla_from_vte(c: VteRgb) -> gpui::Hsla {
    gpui::Hsla::from(rgba_from_vte(c))
}

/// Every non-truecolor [`FrameColor`] resolved to `Hsla` once per palette, so
/// row planning maps a cell color in O(1) instead of converting RGB -> HSL per
/// cell (the conversion is branchy and runs for every cell of a dirty row).
///
/// Layout: 0..256 indexed, then foreground, background, cursor, the eight dim
/// ANSI colors, bright foreground and dim foreground.
#[derive(Clone, Copy)]
pub(crate) struct ColorTable {
    entries: [Hsla; Self::LEN],
    /// FNV-1a over the palette bytes; part of the plan-cache style key.
    pub(crate) hash: u64,
}

impl ColorTable {
    const FOREGROUND: usize = 256;
    const BACKGROUND: usize = 257;
    const CURSOR: usize = 258;
    const DIM_ANSI: usize = 259;
    const BRIGHT_FOREGROUND: usize = 267;
    const DIM_FOREGROUND: usize = 268;
    const LEN: usize = 269;

    pub(crate) fn from_palette(palette: &TerminalPalette) -> Self {
        let mut entries = [Hsla::default(); Self::LEN];
        for (i, entry) in entries.iter_mut().enumerate() {
            let color = match i {
                0..=255 => FrameColor::Indexed(i as u8),
                Self::FOREGROUND => FrameColor::Foreground,
                Self::BACKGROUND => FrameColor::Background,
                Self::CURSOR => FrameColor::Cursor,
                Self::DIM_ANSI..Self::BRIGHT_FOREGROUND => {
                    FrameColor::DimAnsi((i - Self::DIM_ANSI) as u8)
                }
                Self::BRIGHT_FOREGROUND => FrameColor::BrightForeground,
                _ => FrameColor::DimForeground,
            };
            *entry = hsla_from_vte(resolve_color(&color.to_vte(), palette));
        }
        let mut h = Fnv1a::new();
        for rgb in [palette.foreground, palette.background, palette.cursor]
            .iter()
            .chain(palette.ansi.iter())
        {
            h.write(&[rgb.r, rgb.g, rgb.b]);
        }
        for (i, slot) in palette.indexed.iter().enumerate() {
            if let Some(rgb) = slot {
                h.write_u16(i as u16);
                h.write(&[rgb.r, rgb.g, rgb.b]);
            }
        }
        Self {
            entries,
            hash: h.finish(),
        }
    }

    #[inline]
    pub(crate) fn color(&self, c: FrameColor) -> Hsla {
        let index = match c {
            FrameColor::Indexed(n) | FrameColor::Ansi(n) => usize::from(n),
            FrameColor::Foreground => Self::FOREGROUND,
            FrameColor::Background => Self::BACKGROUND,
            FrameColor::Cursor => Self::CURSOR,
            FrameColor::DimAnsi(n) => Self::DIM_ANSI + usize::from(n.min(7)),
            FrameColor::BrightForeground => Self::BRIGHT_FOREGROUND,
            FrameColor::DimForeground => Self::DIM_FOREGROUND,
            FrameColor::Rgb(r, g, b) => {
                return Hsla::from(rgba_from_vte(VteRgb { r, g, b }));
            }
        };
        self.entries[index]
    }
}
