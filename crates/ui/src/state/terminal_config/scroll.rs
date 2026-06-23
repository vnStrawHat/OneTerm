//! Nhóm Scroll: multiplier, alternate scroll.

use serde::{Deserialize, Serialize};

/// Nhóm Scroll: multiplier, alternate scroll.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrollConfig {
    /// Scroll multiplier cho mouse wheel (1.0 = default, 3.0 = nhanh 3x).
    #[serde(default = "default_scroll_multiplier")]
    pub multiplier: f32,
    /// Alternate scroll: trong alt-screen (vim/less/htop), mouse wheel
    /// gửi arrow keys thay vì scroll scrollback.
    #[serde(default = "default_true")]
    pub alternate_scroll: bool,
}

impl Default for ScrollConfig {
    fn default() -> Self {
        Self {
            multiplier: default_scroll_multiplier(),
            alternate_scroll: true,
        }
    }
}

fn default_scroll_multiplier() -> f32 {
    1.0
}

pub(super) fn default_true() -> bool {
    true
}
