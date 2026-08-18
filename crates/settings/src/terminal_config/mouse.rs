//! Mouse group: mouse wheel and right-click context-menu behavior.

use serde::{Deserialize, Serialize};

/// Mouse group.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MouseConfig {
    /// Show OneTerm's context menu on right click.
    ///
    /// Default `true`: right click opens the terminal panel context menu.
    /// Disable this to let CLI apps receive right click directly.
    pub show_context_menu: bool,
    /// Copy the selection to the system clipboard as soon as the mouse button
    /// is released (X11 primary-selection habit).
    ///
    /// Default `true` (the historical behaviour). Disable it to keep the
    /// clipboard untouched until an explicit Copy (SEC-10).
    pub copy_on_select: bool,
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
    fn copy_on_select_defaults_to_enabled_and_can_be_disabled() {
        assert!(MouseConfig::default().copy_on_select);
        let config: MouseConfig = serde_json::from_str("{}").unwrap();
        assert!(config.copy_on_select);
        let config: MouseConfig = serde_json::from_str(r#"{ "copy_on_select": false }"#).unwrap();
        assert!(!config.copy_on_select);
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
            show_context_menu: true,
            copy_on_select: true,
        }
    }
}
