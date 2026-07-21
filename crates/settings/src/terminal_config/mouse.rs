//! Mouse group: mouse wheel and right-click context-menu behavior.

use serde::{Deserialize, Serialize};

/// Mouse group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MouseConfig {
    /// Show OneTerm's context menu on right click.
    ///
    /// Default `true`: right click opens the terminal panel context menu.
    /// Disable this to let CLI apps receive right click directly.
    #[serde(default = "default_true")]
    pub show_context_menu: bool,
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn right_click_context_menu_defaults_to_enabled() {
        assert!(MouseConfig::default().show_context_menu);
        let config: MouseConfig = serde_json::from_str("{}").unwrap();
        assert!(config.show_context_menu);
    }

    #[test]
    fn right_click_context_menu_can_be_disabled_explicitly() {
        let config: MouseConfig =
            serde_json::from_str(r#"{ "show_context_menu": false }"#).unwrap();
        assert!(!config.show_context_menu);
    }
}

impl Default for MouseConfig {
    fn default() -> Self {
        Self {
            show_context_menu: default_true(),
        }
    }
}
