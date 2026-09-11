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
    /// Extra font features (OpenType), e.g. ["ss01", "zero"]. An entry here
    /// wins over `ligatures` for the `calt` feature.
    pub features: Vec<String>,
    /// Render contextual ligatures (`calt`: `=>`, `->`, `!=`) when the font has them.
    pub ligatures: bool,
    /// Fallback families tried, in order, for glyphs `family` lacks (Nerd Font
    /// icons, CJK, symbols) before the system fallback. Families that are not
    /// installed are skipped.
    pub fallbacks: Vec<String>,
}

impl FontConfig {
    /// The Nerd Font symbol-only fonts, which most prompt themes rely on.
    pub fn default_fallbacks() -> Vec<String> {
        vec!["Symbols Nerd Font Mono".into(), "Symbols Nerd Font".into()]
    }
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family: Some("Lilex".into()),
            size: Some(15.0),
            weight: "normal".into(),
            features: Vec::new(),
            ligatures: true,
            fallbacks: Self::default_fallbacks(),
        }
    }
}
