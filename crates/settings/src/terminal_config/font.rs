//! Font group: family, fallback, size, weight, features.

use serde::{Deserialize, Serialize};

/// Font group: family, fallback, size, weight, features.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FontConfig {
    /// Primary font family (null = use the theme mono font).
    pub family: Option<String>,
    /// Font size in px (null = use the theme mono font size).
    pub size: Option<f32>,
    /// Font weight: "thin" | "extra_light" | "light" | "normal" | "medium"
    /// | "semibold" | "bold" | "extra_bold" | "black".
    pub weight: String,
    /// Font features (OpenType): e.g. ["calt", "liga"] → enable ligatures.
    /// The terminal disables calt by default; add "calt" to the list to re-enable it.
    pub features: Vec<String>,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family: Some("Lilex".into()),
            size: Some(15.0),
            weight: "normal".into(),
            features: Vec::new(),
        }
    }
}
