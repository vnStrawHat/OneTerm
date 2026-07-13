//! Plain color mirror types — same field layout as `gpui::Hsla`/`gpui::Rgba`.
//!
//! This crate must not depend on GPUI. These structs mirror the GPUI color
//! types so the `ui` crate can convert with a trivial field copy (see
//! `ui::views::terminal::highlight::bridge`).

/// HSLA color (hue, saturation, lightness, alpha) — mirrors `gpui::Hsla`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Hsla {
    /// Hue (0.0..=360.0).
    pub h: f32,
    /// Saturation (0.0..=1.0).
    pub s: f32,
    /// Lightness (0.0..=1.0).
    pub l: f32,
    /// Alpha (0.0..=1.0).
    pub a: f32,
}

impl Hsla {
    /// Create an HSLA color.
    pub const fn new(h: f32, s: f32, l: f32, a: f32) -> Self {
        Self { h, s, l, a }
    }
}

/// RGBA color (8-bit channels) — mirrors `gpui::Rgba`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// Convert a `#RRGGBB` or `#RGB` hex string to `Hsla` (alpha = 1.0).
///
/// Returns `None` for invalid strings. This is the same parser used by the
/// `ui` crate's `parse_hex_color`, duplicated here so the pure crate can parse
/// the default semantic style asset without a GPUI dependency.
pub fn parse_hex(hex: &str) -> Option<Hsla> {
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    let (r, g, b) = match hex.len() {
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            (r, g, b)
        }
        3 => {
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
            (r, g, b)
        }
        _ => return None,
    };
    Some(rgb_to_hsla(r, g, b, 255))
}

/// Convert 8-bit RGB(A) to `Hsla` (same algorithm as gpui's `Rgba::to_hsla`).
pub fn rgb_to_hsla(r: u8, g: u8, b: u8, a: u8) -> Hsla {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;
    let a = a as f32 / 255.0;

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;

    if (max - min).abs() < f32::EPSILON {
        return Hsla::new(0.0, 0.0, l, a);
    }

    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };

    let h = match (max, min) {
        _ if max == r => (g - b) / d + if g < b { 6.0 } else { 0.0 },
        _ if max == g => (b - r) / d + 2.0,
        _ => (r - g) / d + 4.0,
    };
    Hsla::new(h * 60.0, s, l, a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_6digit() {
        let c = parse_hex("#F92672").unwrap();
        assert_eq!(c.a, 1.0);
        // Red-dominant → hue near 338°.
        assert!((c.h - 338.0).abs() < 2.0);
    }

    #[test]
    fn parse_hex_3digit() {
        let c = parse_hex("#f00").unwrap();
        assert!((c.h - 0.0).abs() < 1.0);
        assert!((c.s - 1.0).abs() < 0.01);
    }

    #[test]
    fn parse_hex_invalid() {
        assert_eq!(parse_hex("xyz"), None);
        assert_eq!(parse_hex("#12"), None);
    }

    #[test]
    fn rgb_to_hsla_white() {
        let c = rgb_to_hsla(255, 255, 255, 255);
        assert!((c.l - 1.0).abs() < 0.01);
        assert!((c.s - 0.0).abs() < 0.01);
    }
}
