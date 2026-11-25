//! Colors group: override foreground, background, cursor, selection,
//! the 16 ANSI colors, and min contrast.

use serde::{Deserialize, Serialize};

/// Colors group: override foreground, background, cursor, selection,
/// the 16 ANSI colors, and min contrast.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorsConfig {
    /// Override foreground (null = theme foreground, "#RRGGBB" to override).
    pub foreground: Option<String>,
    /// Override background.
    pub background: Option<String>,
    /// Override cursor color.
    pub cursor: Option<String>,
    /// Override selection highlight color (null = auto dark/light).
    pub selection: Option<String>,
    /// Override gutter text color (timestamp + line number). null = dim foreground.
    pub gutter_fg: Option<String>,
    /// Override gutter background color. null = same as terminal background.
    pub gutter_bg: Option<String>,
    /// Override clock text color [HH:MM:SS]. null = gutter_fg.
    pub clock_fg: Option<String>,
    /// Override line number color. null = gutter_fg.
    pub line_number_fg: Option<String>,
    /// Minimum contrast threshold (WCAG, 0.0 = off).
    pub min_contrast: f32,
    /// Override the 16 ANSI colors (up to 16; missing ones use the default).
    /// E.g.: ["#000000", "#cc0000", ...]
    pub ansi: Vec<String>,
}

impl Default for ColorsConfig {
    fn default() -> Self {
        Self {
            foreground: Some("#efefef".into()),
            background: None,
            cursor: None,
            selection: Some("#343b48".into()),
            gutter_fg: None,
            gutter_bg: None,
            clock_fg: None,
            line_number_fg: Some("#2b7f99".into()),
            min_contrast: 0.0,
            ansi: Vec::new(),
        }
    }
}
