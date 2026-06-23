//! Nhóm Font: family, fallback, size, weight, features.

use serde::{Deserialize, Serialize};

/// Nhóm Font: family, fallback, size, weight, features.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontConfig {
    /// Font family chính (null = dùng theme mono font).
    #[serde(default = "default_font_family")]
    pub family: Option<String>,
    /// Font fallback stack (rỗng = platform defaults).
    #[serde(default)]
    pub fallback_fonts: Vec<String>,
    /// Font size in px (null = dùng theme mono font size).
    #[serde(default = "default_font_size")]
    pub size: Option<f32>,
    /// Font weight: "thin" | "extra_light" | "light" | "normal" | "medium"
    /// | "semibold" | "bold" | "extra_bold" | "black".
    #[serde(default = "default_weight")]
    pub weight: String,
    /// Font features (OpenType): vd ["calt", "liga"] → bật ligatures.
    /// Terminal mặc định tắt calt; thêm "calt" vào list để bật lại.
    #[serde(default)]
    pub features: Vec<String>,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family: default_font_family(),
            fallback_fonts: Vec::new(),
            size: default_font_size(),
            weight: default_weight(),
            features: Vec::new(),
        }
    }
}

fn default_font_family() -> Option<String> {
    Some("Cascadia Mono".into())
}

fn default_font_size() -> Option<f32> {
    Some(15.0)
}

fn default_weight() -> String {
    "normal".into()
}
