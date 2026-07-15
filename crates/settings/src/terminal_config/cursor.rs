//! Cursor group: shape, color, blink.

use serde::{Deserialize, Serialize};

/// Cursor group: shape, color, blink.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorConfig {
    /// Shape: "block" | "bar" | "underline".
    #[serde(default = "default_cursor_shape")]
    pub shape: String,
    /// Cursor color (null = theme caret, "#RRGGBB" to override).
    #[serde(default)]
    pub color: Option<String>,
    /// Whether the cursor blinks when focused.
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
