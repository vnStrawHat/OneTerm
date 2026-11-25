//! Hex color parsing + serialization for `TerminalSettings`.
//!
//! `parse_hex_color` converts a user-facing `"#RRGGBB"` string (from
//! `terminal.json`) into `Hsla`. `hsla_to_hex` is the inverse — used when
//! persisting the live settings back to the config file.

use gpui::{Hsla, Rgba};

/// Parse "#RRGGBB" → Hsla. Returns None if parsing fails.
pub fn parse_hex_color(s: &str) -> Option<Hsla> {
    let s = s.trim_start_matches('#');
    // `len()` counts bytes; without the ASCII guard a 6-byte multi-byte string
    // would pass the length check and the byte slices below could panic on a
    // non-char boundary.
    if !s.is_ascii() || s.len() != 6 {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_color_rejects_non_ascii_six_byte_input() {
        // Six bytes but two/four chars: slicing at byte 2 lands inside a
        // multi-byte char and used to panic (CORR-23). Reachable from a
        // user-edited terminal.json at startup.
        assert!(parse_hex_color("€€").is_none());
        assert!(parse_hex_color("#€€").is_none());
        assert!(parse_hex_color("aéaé").is_none());
        // Six ASCII-boundary-aligned bytes must still be rejected as non-hex.
        assert!(parse_hex_color("ééé").is_none());
    }

    #[test]
    fn parse_hex_color_rejects_wrong_length_and_non_hex() {
        assert!(parse_hex_color("").is_none());
        assert!(parse_hex_color("#12345").is_none());
        assert!(parse_hex_color("#1234567").is_none());
        assert!(parse_hex_color("#GGGGGG").is_none());
    }

    #[test]
    fn hex_roundtrip_through_hsla() {
        for hex in ["#000000", "#FFFFFF", "#1E90FF", "#7F7F7F", "#ABCDEF"] {
            let color = parse_hex_color(hex).expect("valid hex");
            assert_eq!(hsla_to_hex(color), hex, "roundtrip of {hex}");
        }
        // Lowercase input is accepted and re-serialized in uppercase.
        let color = parse_hex_color("#abcdef").expect("valid hex");
        assert_eq!(hsla_to_hex(color), "#ABCDEF");
    }
}
