//! Bell group: enable/disable the bell indicator.

use serde::{Deserialize, Serialize};

/// Bell group: enable/disable the bell indicator.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BellConfig {
    /// Enable/disable the bell indicator (🔔 on receiving \x07).
    pub enabled: bool,
}

impl Default for BellConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}
