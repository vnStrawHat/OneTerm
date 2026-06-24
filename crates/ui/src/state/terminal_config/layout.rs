//! Nhóm Layout: line height, cell width, padding.

use serde::{Deserialize, Serialize};

/// Nhóm Layout: line height, cell width, padding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutConfig {
    /// Line height multiplier (1.2 = 120% font size).
    #[serde(default = "default_line_height")]
    pub line_height: f32,
    /// Cell width override in px (null = auto từ advance width của '0',
    /// giống Windows Terminal / CSS ch unit).
    #[serde(default = "default_cell_width")]
    pub cell_width: Option<f32>,
    /// Padding quanh terminal content (px).
    #[serde(default)]
    pub padding: PaddingConfig,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            line_height: default_line_height(),
            cell_width: default_cell_width(),
            padding: PaddingConfig::default(),
        }
    }
}

fn default_line_height() -> f32 {
    1.2
}

fn default_cell_width() -> Option<f32> {
    None // auto: đo advance width của '0' (CSS ch unit, giống Windows Terminal)
}

/// Padding 4 phía (px).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PaddingConfig {
    #[serde(default)]
    pub top: f32,
    #[serde(default = "default_padding_right")]
    pub right: f32,
    #[serde(default)]
    pub bottom: f32,
    #[serde(default = "default_padding_left")]
    pub left: f32,
}

impl Default for PaddingConfig {
    fn default() -> Self {
        Self {
            top: 0.0,
            right: default_padding_right(),
            bottom: 0.0,
            left: default_padding_left(),
        }
    }
}

fn default_padding_right() -> f32 {
    5.0
}

fn default_padding_left() -> f32 {
    10.0
}
