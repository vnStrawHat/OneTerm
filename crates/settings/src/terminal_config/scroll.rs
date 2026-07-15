//! Scroll group: multiplier, alternate scroll, scrollback history.

use serde::{Deserialize, Serialize};

/// Scroll group: multiplier, alternate scroll, scrollback history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrollConfig {
    /// Scroll multiplier for the mouse wheel (1.0 = default, 3.0 = 3x faster).
    #[serde(default = "default_scroll_multiplier")]
    pub multiplier: f32,
    /// Alternate scroll: in alt-screen (vim/less/htop), the mouse wheel sends
    /// arrow keys instead of scrolling the scrollback.
    #[serde(default = "default_true")]
    pub alternate_scroll: bool,
    /// Maximum number of scrollback history lines (default 10000).
    /// Total lines in the gutter = scrollback_history + viewport lines.
    /// Increase this so the gutter line number can go higher.
    #[serde(default = "default_scrollback_history")]
    pub scrollback_history: usize,
}

impl Default for ScrollConfig {
    fn default() -> Self {
        Self {
            multiplier: default_scroll_multiplier(),
            alternate_scroll: true,
            scrollback_history: default_scrollback_history(),
        }
    }
}

fn default_scroll_multiplier() -> f32 {
    1.0
}

fn default_scrollback_history() -> usize {
    10_000
}

pub(super) fn default_true() -> bool {
    true
}
