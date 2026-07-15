//! Colors group: override foreground, background, cursor, selection,
//! the 16 ANSI colors, and min contrast.

use serde::{Deserialize, Serialize};

/// Colors group: override foreground, background, cursor, selection,
/// the 16 ANSI colors, and min contrast.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorsConfig {
    /// Override foreground (null = theme foreground, "#RRGGBB" to override).
    #[serde(default = "default_color_foreground")]
    pub foreground: Option<String>,
    /// Override background.
    #[serde(default)]
    pub background: Option<String>,
    /// Override cursor color.
    #[serde(default)]
    pub cursor: Option<String>,
    /// Override selection highlight color (null = auto dark/light).
    #[serde(default = "default_color_selection")]
    pub selection: Option<String>,
    /// Override gutter text color (timestamp + line number). null = dim foreground.
    #[serde(default)]
    pub gutter_fg: Option<String>,
    /// Override gutter background color. null = same as terminal background.
    #[serde(default)]
    pub gutter_bg: Option<String>,
    /// Override clock text color [HH:MM:SS]. null = gutter_fg.
    #[serde(default)]
    pub clock_fg: Option<String>,
    /// Override line number color. null = gutter_fg.
    #[serde(default = "default_color_line_number_fg")]
    pub line_number_fg: Option<String>,
    /// Minimum contrast threshold (WCAG, 0.0 = off).
    #[serde(default = "default_min_contrast")]
    pub min_contrast: f32,
    /// Override the 16 ANSI colors (up to 16; missing ones use the default).
    /// E.g.: ["#000000", "#cc0000", ...]
    #[serde(default)]
    pub ansi: Vec<String>,
}

impl Default for ColorsConfig {
    fn default() -> Self {
        Self {
            foreground: default_color_foreground(),
            background: None,
            cursor: None,
            selection: default_color_selection(),
            gutter_fg: None,
            gutter_bg: None,
            clock_fg: None,
            line_number_fg: default_color_line_number_fg(),
            min_contrast: default_min_contrast(),
            ansi: Vec::new(),
        }
    }
}

fn default_color_foreground() -> Option<String> {
    Some("#efefef".into())
}

fn default_color_selection() -> Option<String> {
    Some("#343b48".into())
}

fn default_color_line_number_fg() -> Option<String> {
    Some("#2b7f99".into())
}

fn default_min_contrast() -> f32 {
    0.0
}
