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

/// Layout group: line height, cell width, padding, gutter, tab title.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutConfig {
    /// Line height multiplier (1.2 = 120% of font size).
    #[serde(default = "default_line_height")]
    pub line_height: f32,
    /// Cell width override in px (null = auto from the advance width of '0',
    /// like Windows Terminal / the CSS ch unit).
    #[serde(default = "default_cell_width")]
    pub cell_width: Option<f32>,
    /// Padding around the terminal content (px).
    #[serde(default)]
    pub padding: PaddingConfig,
    /// Enable/disable the gutter (timestamp + line number on the left of the terminal).
    #[serde(default = "default_show_gutter")]
    pub show_gutter: bool,
    /// Auto-hide the Right Dock when the active tab is a Local Shell.
    #[serde(default)]
    pub auto_hide_right_dock_on_local: bool,
    /// How the terminal tab title is determined (static label vs OSC 0/2).
    #[serde(default)]
    pub tab_title: TabTitleMode,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            line_height: default_line_height(),
            cell_width: default_cell_width(),
            padding: PaddingConfig::default(),
            show_gutter: default_show_gutter(),
            auto_hide_right_dock_on_local: false,
            tab_title: TabTitleMode::default(),
        }
    }
}

fn default_line_height() -> f32 {
    1.2
}

fn default_cell_width() -> Option<f32> {
    None // auto: measure the advance width of '0' (CSS ch unit, like Windows Terminal)
}

fn default_show_gutter() -> bool {
    true
}

/// Padding on all 4 sides (px).
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
