//! Security group: gates for privacy/security-sensitive terminal features.

use serde::{Deserialize, Serialize};

/// Security group.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SecurityConfig {
    /// Allow programs to read the system clipboard via OSC 52 (`52;c;?`).
    ///
    /// Default `false`: reading is refused because it exposes the local
    /// clipboard to programs running in the terminal — including remote ones
    /// over SSH. Writing to the clipboard (OSC 52 set) is always allowed.
    pub allow_clipboard_read: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase0_clipboard_read_permission_defaults_to_denied() {
        assert!(!SecurityConfig::default().allow_clipboard_read);
        let config: SecurityConfig = serde_json::from_str("{}").unwrap();
        assert!(!config.allow_clipboard_read);
    }

    #[test]
    fn phase0_clipboard_read_permission_can_be_enabled_explicitly() {
        let config: SecurityConfig =
            serde_json::from_str(r#"{ "allow_clipboard_read": true }"#).unwrap();
        assert!(config.allow_clipboard_read);
    }
}
