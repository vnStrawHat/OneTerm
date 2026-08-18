//! Scroll group: multiplier, alternate scroll, scrollback history.

use serde::{Deserialize, Serialize};

/// Scroll group: multiplier, alternate scroll, scrollback history.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScrollConfig {
    /// Scroll multiplier for the mouse wheel (1.0 = default, 3.0 = 3x faster).
    pub multiplier: f32,
    /// Alternate scroll: in alt-screen (vim/less/htop), the mouse wheel sends
    /// arrow keys instead of scrolling the scrollback.
    pub alternate_scroll: bool,
    /// Maximum number of scrollback history lines (default 10000).
    /// Total lines in the gutter = scrollback_history + viewport lines.
    /// Increase this so the gutter line number can go higher.
    pub scrollback_history: usize,
}

impl Default for ScrollConfig {
    fn default() -> Self {
        Self {
            multiplier: 1.0,
            alternate_scroll: true,
            scrollback_history: 10_000,
        }
    }
}
