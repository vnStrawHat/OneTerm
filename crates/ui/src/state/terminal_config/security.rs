//! Security group: gates for privacy/security-sensitive terminal features.

use serde::{Deserialize, Serialize};

/// Security group.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Allow programs to read the system clipboard via OSC 52 (`52;c;?`).
    ///
    /// Default `false`: reading is refused because it exposes the local
    /// clipboard to programs running in the terminal — including remote ones
    /// over SSH. Writing to the clipboard (OSC 52 set) is always allowed.
    #[serde(default = "default_false")]
    pub allow_clipboard_read: bool,
}

fn default_false() -> bool {
    false
}
