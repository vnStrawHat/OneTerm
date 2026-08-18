//! Completion group: the `completion` block in `terminal.json`.
//!
//! Controls the terminal auto-completion overlay + in-session history. Defaults
//! are chosen so the feature is **on and safe** out of the box
//! (docs/auto-completion/06 §2). Each struct is `#[serde(default)]` so an old
//! `terminal.json` without a `completion` block — or with only some of its
//! fields — loads the rest from `Default`, the same pattern as `SecurityConfig`.

use serde::{Deserialize, Serialize};

/// Per-source enable toggles (docs 06 §2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompletionSources {
    /// In-session command history (`memory` source).
    pub memory: bool,
    /// Hand-authored bundled catalogs (`manual` source).
    pub manual: bool,
    /// Script-generated bundled catalogs (`external` source).
    pub external: bool,
}

impl Default for CompletionSources {
    fn default() -> Self {
        Self {
            memory: true,
            manual: true,
            external: true,
        }
    }
}

/// The `completion` config group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompletionConfig {
    /// Master on/off for auto-completion.
    pub enabled: bool,
    /// Whether `Tab` accepts the selection (else `Tab` → shell).
    pub accept_tab: bool,
    /// Per-family in-session history capacity (`0` disables + clears history).
    pub max_history: usize,
    /// Minimum chars before command suggestions appear.
    pub min_prefix_len: usize,
    /// Rows shown in the overlay before scrolling.
    pub max_visible_items: usize,
    /// Per-source toggles.
    pub sources: CompletionSources,
    /// Allow fuzzy (subsequence) matches as a secondary pass.
    pub fuzzy: bool,
    /// In subcommand trees, also offer ancestor options (ranked lower).
    pub inherit_ancestor_options: bool,
    /// Suppress inside the alternate screen (TUIs).
    pub disable_in_alt_screen: bool,
    /// Only show inside the OSC 133 command-input region.
    pub require_prompt_region: bool,
    /// Let `Cmd`/`PowerShell` also suggest coreutils+linux commands.
    pub windows_allow_coreutils: bool,
    /// Override shell detection: `null | "cmd" | "powershell" | "unix"`.
    pub force_family: Option<String>,
    /// Strip secret values before recording history (keep `true`).
    pub redact_sensitive: bool,
}

impl Default for CompletionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            accept_tab: true,
            max_history: 500,
            min_prefix_len: 1,
            max_visible_items: 8,
            sources: CompletionSources::default(),
            fuzzy: true,
            inherit_ancestor_options: true,
            disable_in_alt_screen: true,
            require_prompt_region: true,
            windows_allow_coreutils: false,
            force_family: None,
            redact_sensitive: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_on_and_safe() {
        let c = CompletionConfig::default();
        assert!(c.enabled);
        assert!(c.accept_tab);
        assert_eq!(c.max_history, 500);
        assert_eq!(c.min_prefix_len, 1);
        assert_eq!(c.max_visible_items, 8);
        assert!(c.sources.memory && c.sources.manual && c.sources.external);
        assert!(c.fuzzy);
        assert!(c.inherit_ancestor_options);
        assert!(c.disable_in_alt_screen);
        assert!(c.require_prompt_region);
        assert!(!c.windows_allow_coreutils);
        assert_eq!(c.force_family, None);
        assert!(c.redact_sensitive);
    }

    #[test]
    fn old_config_without_completion_block_uses_defaults() {
        // An empty object → every field falls back to its serde default.
        let c: CompletionConfig = serde_json::from_str("{}").unwrap();
        assert!(c.enabled);
        assert_eq!(c.max_history, 500);
        assert!(c.sources.external);
    }

    #[test]
    fn partial_config_fills_missing_defaults() {
        let c: CompletionConfig =
            serde_json::from_str(r#"{ "enabled": false, "max_history": 0 }"#).unwrap();
        assert!(!c.enabled);
        assert_eq!(c.max_history, 0);
        // Untouched fields keep defaults.
        assert!(c.accept_tab);
        assert_eq!(c.max_visible_items, 8);
        assert!(c.sources.memory);
    }

    #[test]
    fn force_family_parses() {
        let c: CompletionConfig = serde_json::from_str(r#"{ "force_family": "unix" }"#).unwrap();
        assert_eq!(c.force_family.as_deref(), Some("unix"));
    }
}
