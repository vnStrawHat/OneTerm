//! Hex color parsing + serialization for `TerminalSettings`.
//!
//! `parse_hex_color` converts a user-facing `"#RRGGBB"` string (from
//! `terminal.json`) into `Hsla`. `hsla_to_hex` is the inverse — used when
//! persisting the live settings back to the config file.

use gpui::{Hsla, Rgba};

/// Parse "#RRGGBB" → Hsla. Returns None if parsing fails.
pub fn parse_hex_color(s: &str) -> Option<Hsla> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some(
        Rgba {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: 1.0,
        }
        .into(),
    )
}

/// Convert an `Hsla` color back to a `"#RRGGBB"` string (alpha dropped — the
/// config format does not store alpha). The inverse of [`parse_hex_color`].
pub fn hsla_to_hex(c: Hsla) -> String {
    let rgba = Rgba::from(c);
    let r = (rgba.r * 255.0).round().clamp(0.0, 255.0) as u8;
    let g = (rgba.g * 255.0).round().clamp(0.0, 255.0) as u8;
    let b = (rgba.b * 255.0).round().clamp(0.0, 255.0) as u8;
    format!("#{:02X}{:02X}{:02X}", r, g, b)
}
