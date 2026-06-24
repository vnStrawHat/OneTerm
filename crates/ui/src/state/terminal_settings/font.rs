//! Font defaults + weight parsing.

use gpui::{FontWeight, SharedString};

/// Platform-specific default font fallback stack for the terminal.
pub fn default_terminal_font_fallbacks() -> Vec<SharedString> {
    #[cfg(target_os = "windows")]
    {
        vec![
            "Cascadia Mono".into(),
            "Cascadia Code".into(),
            "DejaVu Sans Mono".into(),
            "Lucida Console".into(),
            "Courier New".into(),
            "MS Gothic".into(),
            "NSimSun".into(),
        ]
    }
    #[cfg(target_os = "macos")]
    {
        vec![
            "Menlo".into(),
            "Monaco".into(),
            "Courier New".into(),
            "Apple Symbols".into(),
        ]
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        vec![
            "DejaVu Sans Mono".into(),
            "Noto Sans Mono".into(),
            "Ubuntu Mono".into(),
            "Liberation Mono".into(),
            "Courier New".into(),
        ]
    }
}

/// Parse font weight từ string → FontWeight.
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
