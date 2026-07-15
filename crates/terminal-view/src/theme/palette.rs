//! ANSI palette + VteRgb ↔ Rgba conversion.

use alacritty_terminal::vte::ansi::Rgb as VteRgb;
use gpui::Rgba;

/// Fixed ANSI 16-color palette (GNOME/Tango default).
pub const ANSI_16: [VteRgb; 16] = [
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
pub fn rgba_from_vte(c: VteRgb) -> Rgba {
    Rgba {
        r: c.r as f32 / 255.0,
        g: c.g as f32 / 255.0,
        b: c.b as f32 / 255.0,
        a: 1.0,
    }
}

/// `gpui::Rgba` (0..1) → `vte::ansi::Rgb` (u8).
pub fn vte_from_rgba(c: Rgba) -> VteRgb {
    VteRgb {
        r: (c.r * 255.0).round().clamp(0.0, 255.0) as u8,
        g: (c.g * 255.0).round().clamp(0.0, 255.0) as u8,
        b: (c.b * 255.0).round().clamp(0.0, 255.0) as u8,
    }
}

/// `vte::ansi::Rgb` → `gpui::Hsla` (qua `Rgba`).
pub fn hsla_from_vte(c: VteRgb) -> gpui::Hsla {
    gpui::Hsla::from(rgba_from_vte(c))
}
