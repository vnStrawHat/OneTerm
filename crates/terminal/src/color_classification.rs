//! Character utilities for terminal rendering.
//!
//! `US-0085` deleted the two colour predicates that lived here
//! (`is_app_chosen_exact_color`, `is_default_background_color`): they were the
//! last readers of the fork's `Color` in this crate and had no caller outside
//! their own tests. The view asks the same two questions of its own
//! `render::frame::Color`, which is the type the row planner actually holds.

/// Decorative characters (box-drawing, block, geometric, powerline) — keep their
/// exact color, do NOT adjust contrast (they must match the adjacent background).
///
/// Regular icons (git, folder…) are excluded so they stay readable.
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

#[cfg(test)]
mod tests {
    use super::*;

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
