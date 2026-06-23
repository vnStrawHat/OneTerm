//! Nhóm Colors: override foreground, background, cursor, selection,
//! ANSI 16 colors, min contrast.

use serde::{Deserialize, Serialize};

/// Nhóm Colors: override foreground, background, cursor, selection,
/// ANSI 16 colors, min contrast.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorsConfig {
    /// Override foreground (null = theme foreground, "#RRGGBB" để override).
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
    /// Ngưỡng contrast tối thiểu (WCAG, 0.0 = tắt).
    #[serde(default = "default_min_contrast")]
    pub min_contrast: f32,
    /// Override ANSI 16 colors (tối đa 16, thiếu = dùng default).
    /// Vd: ["#000000", "#cc0000", ...]
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
