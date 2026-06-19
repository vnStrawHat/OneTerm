//! Tiện ích màu & ký tự cho render terminal.
//!
//! Tham chiếu: Zed `crates/terminal_view/src/terminal_element.rs`
//! (`is_decorative_character`, `is_app_chosen_exact_color`) +
//! `crates/terminal/src/terminal.rs` (`is_default_background_color`).
//! Làm việc với raw `vte::ansi::Color` (không bọc thêm layer như Zed).

use alacritty_terminal::vte::ansi::{Color, NamedColor};

/// Ký tự trang trí (box-drawing, block, geometric, powerline) — giữ nguyên
/// màu chính xác, KHÔNG điều chỉnh contrast (vì cần khớp nền kề cạnh).
///
/// Sửa lỗi zed#34234: icon thường (git, folder…) bị loại trừ để vẫn đọc được.
pub fn is_decorative_character(ch: char) -> bool {
    matches!(
        ch as u32,
        // Box Drawing & Block Elements
        0x2500..=0x257F // └ ┐ ─ │ …
        | 0x2580..=0x259F // ▀ ▄ █ ░ ▒ ▓ …
        | 0x25A0..=0x25FF // ■ ▶ ● … (tam giác/tròn separator)

        // Powerline separator symbols (Private Use Area)
        | 0xE0B0..=0xE0B7 // tam giác + nửa tròn
        | 0xE0B8..=0xE0BF // tam giác góc
        | 0xE0C0..=0xE0CA // flame / pixelated / ice
        | 0xE0CC..=0xE0D1 // honeycomb / lego
        | 0xE0D2..=0xE0D7 // trapezoid / inverted triangle
    )
}

/// App đã tự chọn màu fg chính xác, KHÔNG muốn điều chỉnh contrast:
/// - 24-bit true color `\e[38;2;R;G;Bm` → `Color::Spec(_)`.
/// - 256-color palette `\e[38;5;Nm` với `N >= 16` (cube 6×6×6 ở 16..=231 +
///   grayscale 24 bước ở 232..=255).
///
/// Index 0..=15 (ANSI 16 màu theme) vẫn qua contrast adjustment vì có thể
/// đụng nền theme.
pub fn is_app_chosen_exact_color(fg: &Color) -> bool {
    match fg {
        Color::Spec(_) => true,
        Color::Indexed(n) => *n >= 16,
        Color::Named(_) => false,
    }
}

/// Màu nền mặc định của terminal (`Color::Named(NamedColor::Background)`).
/// Dùng để skip vẽ rect nền (để nền element cha xuyên qua).
pub fn is_default_background_color(bg: &Color) -> bool {
    matches!(bg, Color::Named(NamedColor::Background))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::vte::ansi::Rgb;

    #[test]
    fn spec_is_app_chosen() {
        assert!(is_app_chosen_exact_color(&Color::Spec(Rgb { r: 1, g: 2, b: 3 })));
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
        assert!(!is_app_chosen_exact_color(&Color::Named(NamedColor::Foreground)));
    }

    #[test]
    fn default_bg_is_named_background() {
        assert!(is_default_background_color(&Color::Named(NamedColor::Background)));
        assert!(!is_default_background_color(&Color::Named(NamedColor::Foreground)));
        assert!(!is_default_background_color(&Color::Spec(Rgb { r: 0, g: 0, b: 0 })));
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
        // Icon thường (git/folder) không phải decorative → vẫn qua contrast.
        assert!(!is_decorative_character('\u{F1D3}')); // Devicons git-ish (Nerd Font PUA khác range)
    }
}