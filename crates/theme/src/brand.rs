//! OneTerm brand colours — identity tints that stay the same in every theme.
//!
//! Per-theme colours come from `cx.theme()`; the values here are the few that
//! must not follow the theme (the app icon tint, the "OneTerm" wordmark), so
//! callers share one definition instead of repeating a hex literal.

use gpui::{Rgba, rgb};

/// The OneTerm logo cyan (`#58c4dc`), used to tint the title-bar icon, the
/// About page mark, and the empty-Space placeholder.
pub const BRAND_ACCENT_HEX: u32 = 0x58c4dc;

/// [`BRAND_ACCENT_HEX`] as a gpui colour.
pub fn brand_accent() -> Rgba {
    rgb(BRAND_ACCENT_HEX)
}
