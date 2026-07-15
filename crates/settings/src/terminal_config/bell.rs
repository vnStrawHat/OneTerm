//! Bell group: enable/disable the bell indicator.

use serde::{Deserialize, Serialize};

/// Bell group: enable/disable the bell indicator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BellConfig {
    /// Enable/disable the bell indicator (🔔 on receiving \x07).
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
