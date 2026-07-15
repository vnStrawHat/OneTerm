//! Font weight parsing.

use gpui::FontWeight;

/// Parse a font weight from a string → FontWeight.
pub fn parse_weight(s: &str) -> FontWeight {
    match s.to_ascii_lowercase().as_str() {
        "thin" => FontWeight::THIN,
        "extra_light" | "extralight" => FontWeight::EXTRA_LIGHT,
        "light" => FontWeight::LIGHT,
        "normal" | "regular" => FontWeight::NORMAL,
        "medium" => FontWeight::MEDIUM,
        "semibold" => FontWeight::SEMIBOLD,
        "bold" => FontWeight::BOLD,
        "extra_bold" | "extrabold" => FontWeight::EXTRA_BOLD,
        "black" => FontWeight::BLACK,
        _ => FontWeight::default(),
    }
}
