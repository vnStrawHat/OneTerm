//! Layout group: line height, cell width, padding.

use serde::{Deserialize, Serialize};

/// How the terminal tab title is determined.
///
/// - `Default` — always show the static label ("Terminal" for local shells,
///   the SSH session label for remote sessions).
/// - `Osc` — use the live OSC 0/2 window title set by the shell (e.g. the
///   running command / cwd), falling back to the static label when the shell
///   hasn't set one. Long executable paths are shortened to their basename
///   (e.g. `C:\Windows\system32\cmd.exe` → `cmd.exe`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TabTitleMode {
    /// Static label: "Terminal" (local) / SSH session label.
    #[default]
    Default,
    /// Live OSC 0/2 window title from the shell.
    Osc,
}

/// Semantic highlighting mode for plain-text terminal output.
///
/// - `Auto` — on (uses OSC 133 row roles when available, regex fallback otherwise).
/// - `On`   — always on.
/// - `Off`  — disabled (URL highlighting still works — it is always-on).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SemanticHighlightingMode {
    /// On — uses OSC 133 row roles when available, regex fallback otherwise.
    #[default]
    Auto,
    /// Always on.
    On,
    /// Disabled (URL highlighting still works — always-on).
    Off,
}

/// Layout group: line height, cell width, padding, gutter, tab title.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutConfig {
    /// Line height multiplier (1.2 = 120% of font size).
    pub line_height: f32,
    /// Cell width override in px (null = auto from the advance width of '0',
    /// like Windows Terminal / the CSS ch unit).
    pub cell_width: Option<f32>,
    /// Padding around the terminal content (px).
    pub padding: PaddingConfig,
    /// Enable/disable the gutter (timestamp + line number on the left of the terminal).
    pub show_gutter: bool,
    /// How the terminal tab title is determined (static label vs OSC 0/2).
    pub tab_title: TabTitleMode,
    /// Semantic highlighting mode for plain-text terminal output.
    pub semantic_highlighting: SemanticHighlightingMode,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            line_height: 1.2,
            // Auto: measure the advance width of '0' (CSS ch unit, like Windows Terminal).
            cell_width: None,
            padding: PaddingConfig::default(),
            show_gutter: false,
            tab_title: TabTitleMode::default(),
            semantic_highlighting: SemanticHighlightingMode::Auto,
        }
    }
}

/// Padding on all 4 sides (px).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct PaddingConfig {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Default for PaddingConfig {
    fn default() -> Self {
        Self {
            top: 0.0,
            right: 5.0,
            bottom: 0.0,
            left: 10.0,
        }
    }
}
