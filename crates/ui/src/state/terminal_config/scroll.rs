//! Nhóm Scroll: multiplier, alternate scroll, scrollback history.

use serde::{Deserialize, Serialize};

/// Nhóm Scroll: multiplier, alternate scroll, scrollback history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrollConfig {
    /// Scroll multiplier cho mouse wheel (1.0 = default, 3.0 = nhanh 3x).
    #[serde(default = "default_scroll_multiplier")]
    pub multiplier: f32,
    /// Alternate scroll: trong alt-screen (vim/less/htop), mouse wheel
    /// gửi arrow keys thay vì scroll scrollback.
    #[serde(default = "default_true")]
    pub alternate_scroll: bool,
    /// Số dòng scrollback history tối đa (default 10000).
    /// Tổng dòng trong gutter = scrollback_history + viewport lines.
    /// Tăng giá trị này để gutter line number có thể lên cao hơn.
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
