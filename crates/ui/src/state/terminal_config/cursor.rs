//! Nhóm Cursor: shape, color, blink.

use serde::{Deserialize, Serialize};

/// Nhóm Cursor: shape, color, blink.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorConfig {
    /// Hình dáng: "block" | "bar" | "underline".
    #[serde(default = "default_cursor_shape")]
    pub shape: String,
    /// Màu con trỏ (null = theme caret, "#RRGGBB" để override).
    #[serde(default)]
    pub color: Option<String>,
    /// Có nhấp nháy khi focus không.
    #[serde(default = "default_true")]
    pub blink: bool,
}

impl Default for CursorConfig {
    fn default() -> Self {
        Self {
            shape: default_cursor_shape(),
            color: None,
            blink: true,
        }
    }
}

fn default_cursor_shape() -> String {
    "block".into()
}

pub(super) fn default_true() -> bool {
    true
}
