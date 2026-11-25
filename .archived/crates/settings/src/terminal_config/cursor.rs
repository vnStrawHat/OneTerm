//! Cursor group: shape, color, blink.

use serde::{Deserialize, Serialize};

/// Cursor group: shape, color, blink.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CursorConfig {
    /// Shape: "block" | "bar" | "underline".
    pub shape: String,
    /// Cursor color (null = theme caret, "#RRGGBB" to override).
    pub color: Option<String>,
    /// Whether the cursor blinks when focused.
    pub blink: bool,
}

impl Default for CursorConfig {
    fn default() -> Self {
        Self {
            shape: "block".into(),
            color: None,
            blink: true,
        }
    }
}
