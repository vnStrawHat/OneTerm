//! Color & character utilities for terminal rendering.
//!
//! Reference: Zed `crates/terminal_view/src/terminal_element.rs`
//! (`is_decorative_character`, `is_app_chosen_exact_color`) +
//! `crates/terminal/src/terminal.rs` (`is_default_background_color`).
//! Works with raw `vte::ansi::Color` (no extra wrapping layer like Zed).

use alacritty_terminal::vte::ansi::{Color, NamedColor};

/// Decorative characters (box-drawing, block, geometric, powerline) — keep their
/// exact color, do NOT adjust contrast (they must match the adjacent background).
///
/// Fixes zed#34234: regular icons (git, folder…) are excluded so they stay readable.
pub fn is_decorative_character(ch: char) -> bool {
    matches!(
        ch as u32,
        // Box Drawing & Block Elements
        0x2500..=0x257F // └ ┐ ─ │ …
        | 0x2580..=0x259F // ▀ ▄ █ ░ ▒ ▓ …
        | 0x25A0..=0x25FF // ■ ▶ ● … (triangle/circle separators)

        // Powerline separator symbols (Private Use Area)
        | 0xE0B0..=0xE0B7 // triangles + half circles
        | 0xE0B8..=0xE0BF // angled triangles
        | 0xE0C0..=0xE0CA // flame / pixelated / ice
        | 0xE0CC..=0xE0D1 // honeycomb / lego
        | 0xE0D2..=0xE0D7 // trapezoid / inverted triangle
    )
}

/// The app already chose an exact fg color and we do NOT want to adjust contrast:
/// - 24-bit true color `\e[38;2;R;G;Bm` → `Color::Spec(_)`.
/// - 256-color palette `\e[38;5;Nm` with `N >= 16` (6×6×6 cube at 16..=231 +
///   24-step grayscale at 232..=255).
///
/// Index 0..=15 (the ANSI 16-color theme) still goes through contrast adjustment
/// because it may clash with the theme background.
pub fn is_app_chosen_exact_color(fg: &Color) -> bool {
    match fg {
        Color::Spec(_) => true,
        Color::Indexed(n) => *n >= 16,
        Color::Named(_) => false,
    }
}

/// The terminal's default background color (`Color::Named(NamedColor::Background)`).
/// Used to skip drawing the background rect (letting the parent element's background show through).
pub fn is_default_background_color(bg: &Color) -> bool {
    matches!(bg, Color::Named(NamedColor::Background))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::vte::ansi::Rgb;

    #[test]
    fn spec_is_app_chosen() {
        assert!(is_app_chosen_exact_color(&Color::Spec(Rgb {
            r: 1,
            g: 2,
            b: 3
        })));
    }

    #[test]
    fn indexed_high_is_app_chosen() {
        assert!(is_app_chosen_exact_color(&Color::Indexed(16)));
        assert!(is_app_chosen_exact_color(&Color::Indexed(231)));
        assert!(is_app_chosen_exact_color(&Color::Indexed(255)));
    }

    #[test]
    fn indexed_low_is_not_app_chosen() {
        assert!(!is_app_chosen_exact_color(&Color::Indexed(0)));
        assert!(!is_app_chosen_exact_color(&Color::Indexed(15)));
    }

    #[test]
    fn named_is_not_app_chosen() {
        assert!(!is_app_chosen_exact_color(&Color::Named(NamedColor::Red)));
        assert!(!is_app_chosen_exact_color(&Color::Named(
            NamedColor::Foreground
        )));
    }

    #[test]
    fn default_bg_is_named_background() {
        assert!(is_default_background_color(&Color::Named(
            NamedColor::Background
        )));
        assert!(!is_default_background_color(&Color::Named(
            NamedColor::Foreground
        )));
        assert!(!is_default_background_color(&Color::Spec(Rgb {
            r: 0,
            g: 0,
            b: 0
        })));
    }

    #[test]
    fn box_chars_are_decorative() {
        assert!(is_decorative_character('─'));
        assert!(is_decorative_character('└'));
        assert!(is_decorative_character('█'));
        assert!(is_decorative_character('▀'));
        assert!(is_decorative_character('■'));
        assert!(is_decorative_character('▶'));
    }

    #[test]
    fn powerline_chars_are_decorative() {
        assert!(is_decorative_character('\u{E0B0}'));
        assert!(is_decorative_character('\u{E0B4}'));
        assert!(is_decorative_character('\u{E0D2}'));
    }

    #[test]
    fn regular_chars_not_decorative() {
        assert!(!is_decorative_character('a'));
        assert!(!is_decorative_character(' '));
        assert!(!is_decorative_character('$'));
        // Regular icons (git/folder) are not decorative → still go through contrast.
        assert!(!is_decorative_character('\u{F1D3}')); // Devicons git-ish (Nerd Font PUA, different range)
    }
}
