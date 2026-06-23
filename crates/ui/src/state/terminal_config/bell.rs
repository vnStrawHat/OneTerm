//! Nhóm Bell: enable/disable bell indicator.

use serde::{Deserialize, Serialize};

/// Nhóm Bell: enable/disable bell indicator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BellConfig {
    /// Bật/tắt bell indicator (🔔 khi nhận \x07).
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for BellConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

fn default_true() -> bool {
    true
}
