//! Terminal config JSON — load/save `terminal.json`.
//!
//! The config file is organized into logical groups: font, cursor, layout, shell,
//! scroll, mouse, bell, colors. If the file does not exist → create a default file so the
//! user can see the available options.
//!
//! Path: `target/terminal.json` (debug) / `~/.OneTerm/terminal.json` (release).

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use oneterm_core::{
    LocalShellConfig, atomic_write, config_dir, migrate_json_value, quarantine_file,
    set_schema_version,
};

pub mod bell;
pub mod colors;
pub mod completion;
pub mod cursor;
pub mod font;
pub mod layout;
pub mod mouse;
pub mod scroll;
pub mod security;

pub use bell::BellConfig;
pub use colors::ColorsConfig;
pub use completion::{CompletionConfig, CompletionSources};
pub use cursor::CursorConfig;
pub use font::FontConfig;
pub use layout::{LayoutConfig, PaddingConfig, SemanticHighlightingMode, TabTitleMode};
pub use mouse::MouseConfig;
pub use scroll::ScrollConfig;
pub use security::SecurityConfig;

// File path is resolved at runtime via config_dir().join("terminal.json") —
// debug → target/, release → ~/.OneTerm/ (see oneterm_core::config_dir).

// ── Top-level config ─────────────────────────────────────────────────

/// The full terminal config — parsed from `terminal.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TerminalConfig {
    #[serde(default)]
    pub font: FontConfig,
    #[serde(default)]
    pub cursor: CursorConfig,
    #[serde(default)]
    pub layout: LayoutConfig,
    #[serde(default)]
    pub shell: LocalShellConfig,
    #[serde(default)]
    pub scroll: ScrollConfig,
    #[serde(default)]
    pub mouse: MouseConfig,
    #[serde(default)]
    pub bell: BellConfig,
    #[serde(default)]
    pub colors: ColorsConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub completion: CompletionConfig,
}

const CURRENT_SCHEMA_VERSION: u32 = 1;

// ── Load / Save ─────────────────────────────────────────────────────

/// Strip `//` line comments and `/* */` block comments from a JSON string.
/// Standard JSON does not support comments, but users often add notes.
/// Must handle this carefully so `//` inside string values is not stripped.
fn strip_json_comments(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let mut in_string = false;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            result.push(c);
            if c == '\\' {
                // Escape char — push next char blindly.
                if i + 1 < chars.len() {
                    result.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
            } else if c == '"' {
                in_string = false;
            }
            i += 1;
        } else if c == '"' {
            in_string = true;
            result.push(c);
            i += 1;
        } else if c == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            // Line comment — skip to end of line.
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            // Block comment — skip to */.
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2; // skip */
        } else {
            result.push(c);
            i += 1;
        }
    }
    result
}

impl TerminalConfig {
    fn parse_document(raw: &str) -> std::io::Result<Self> {
        let value: Value = serde_json::from_str(raw).map_err(std::io::Error::other)?;
        let value = migrate_json_value(
            value,
            CURRENT_SCHEMA_VERSION,
            "terminal.json",
            |_, value| {
                if !value.is_object() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "terminal.json schema must be an object",
                    ));
                }
                Ok(value)
            },
        )?;
        serde_json::from_value(value).map_err(std::io::Error::other)
    }

    fn serialize_document(&self) -> std::io::Result<String> {
        let mut value = serde_json::to_value(self).map_err(std::io::Error::other)?;
        set_schema_version(&mut value, CURRENT_SCHEMA_VERSION)?;
        serde_json::to_string_pretty(&value).map_err(std::io::Error::other)
    }
    /// Load the config from file. If the file does not exist → create a default + return default.
    /// Supports `//` and `/* */` comments in the JSON.
    pub fn load() -> Self {
        Self::load_from(&config_dir().join("terminal.json"))
    }

    /// Load the config from an explicit path for deterministic callers and tests.
    pub fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(&path) {
            Ok(raw) => {
                let json = strip_json_comments(&raw);
                match Self::parse_document(&json) {
                    Ok(cfg) => cfg,
                    Err(e) => {
                        log::error!("terminal.json parse error: {e} — using defaults");
                        if let Err(quarantine_error) = quarantine_file(&path) {
                            log::warn!("failed to quarantine terminal.json: {quarantine_error}");
                        }
                        Self::default()
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // A missing file is the only read failure that safely selects defaults.
                let cfg = Self::default();
                match cfg.serialize_document() {
                    Ok(json) => match atomic_write(&path, json.as_bytes()) {
                        Ok(()) => log::info!("Created default terminal.json at {path:?}"),
                        Err(write_error) => log::warn!(
                            "failed to create default terminal.json at {path:?}: {write_error}"
                        ),
                    },
                    Err(serialize_error) => {
                        log::warn!("failed to serialize default terminal.json: {serialize_error}")
                    }
                }
                cfg
            }
            Err(error) => {
                log::error!("failed to read terminal.json: {error}; using defaults");
                Self::default()
            }
        }
    }

    /// Save the config to `terminal.json` (pretty-printed, no comments).
    pub fn save(&self) -> std::io::Result<()> {
        self.save_to(&config_dir().join("terminal.json"))
    }

    /// Save the config to an explicit path for deterministic callers and tests.
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        let json = self.serialize_document()?;
        atomic_write(&path, json.as_bytes())?;
        log::info!("Saved terminal.json to {path:?}");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_line_comments() {
        let input = r#"{ "a": 1, // this is a comment
 "b": 2 }"#;
        let stripped = strip_json_comments(input);
        let parsed: serde_json::Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(parsed["a"], 1);
        assert_eq!(parsed["b"], 2);
    }

    #[test]
    fn strip_block_comments() {
        let input = r#"{ "a": 1 /* block */, "b": 2 }"#;
        let stripped = strip_json_comments(input);
        let parsed: serde_json::Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(parsed["a"], 1);
        assert_eq!(parsed["b"], 2);
    }

    #[test]
    fn strip_comments_preserves_strings_with_slashes() {
        let input = r#"{ "url": "https://example.com", "a": 1 // comment
 }"#;
        let stripped = strip_json_comments(input);
        let parsed: serde_json::Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(parsed["url"], "https://example.com");
        assert_eq!(parsed["a"], 1);
    }

    #[test]
    fn strip_comments_preserves_escaped_quotes() {
        let input = r#"{ "a": "say \"hi\" // not a comment", "b": 2 // real comment
 }"#;
        let stripped = strip_json_comments(input);
        let parsed: serde_json::Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(parsed["a"], "say \"hi\" // not a comment");
        assert_eq!(parsed["b"], 2);
    }

    #[test]
    fn default_config_serializes_all_groups() {
        let cfg = TerminalConfig::default();
        let json = serde_json::to_string_pretty(&cfg).unwrap();
        // Verify all groups present.
        assert!(json.contains("\"font\""));
        assert!(json.contains("\"cursor\""));
        assert!(json.contains("\"layout\""));
        assert!(json.contains("\"shell\""));
        assert!(json.contains("\"scroll\""));
        assert!(json.contains("\"bell\""));
        assert!(json.contains("\"colors\""));
    }

    #[test]
    fn empty_json_uses_defaults() {
        let json = "{}";
        let cfg: TerminalConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.cursor.shape, "block");
        assert_eq!(cfg.font.family.as_deref(), Some("Lilex"));
        assert_eq!(cfg.font.size, Some(15.0));
        assert_eq!(cfg.layout.line_height, 1.2);
        assert_eq!(cfg.layout.cell_width, None);
        assert_eq!(cfg.layout.padding.right, 5.0);
        assert_eq!(cfg.layout.padding.left, 10.0);
        assert!(cfg.bell.enabled);
        assert_eq!(cfg.scroll.multiplier, 1.0);
        assert_eq!(cfg.scroll.scrollback_history, 10_000);
        assert_eq!(cfg.colors.foreground.as_deref(), Some("#efefef"));
        assert_eq!(cfg.colors.selection.as_deref(), Some("#343b48"));
        assert_eq!(cfg.colors.min_contrast, 0.0);
        assert_eq!(cfg.colors.line_number_fg.as_deref(), Some("#2b7f99"));
    }

    #[test]
    fn custom_config_parses() {
        let json = r##"{
            "font": {
                "family": "JetBrains Mono",
                "size": 15.0,
                "weight": "bold",
                "features": ["calt", "liga"]
            },
            "cursor": {
                "shape": "bar",
                "color": "#ff0000",
                "blink": false
            },
            "layout": {
                "line_height": 1.5,
                "cell_width": 8.0,
                "padding": { "top": 4.0, "right": 8.0, "bottom": 4.0, "left": 8.0 }
            },
            "scroll": {
                "multiplier": 3.0,
                "alternate_scroll": false,
                "scrollback_history": 50000
            },
            "bell": { "enabled": false },
            "colors": {
                "foreground": "#cccccc",
                "min_contrast": 3.0,
                "ansi": ["#000000", "#cc0000"]
            }
        }"##;
        let cfg: TerminalConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.font.family.as_deref(), Some("JetBrains Mono"));
        assert_eq!(cfg.font.size, Some(15.0));
        assert_eq!(cfg.font.weight, "bold");
        assert_eq!(cfg.cursor.shape, "bar");
        assert_eq!(cfg.cursor.color.as_deref(), Some("#ff0000"));
        assert!(!cfg.cursor.blink);
        assert_eq!(cfg.layout.line_height, 1.5);
        assert_eq!(cfg.layout.cell_width, Some(8.0));
        assert_eq!(cfg.layout.padding.top, 4.0);
        assert_eq!(cfg.scroll.multiplier, 3.0);
        assert!(!cfg.scroll.alternate_scroll);
        assert_eq!(cfg.scroll.scrollback_history, 50000);
        assert!(!cfg.bell.enabled);
        assert_eq!(cfg.colors.foreground.as_deref(), Some("#cccccc"));
        assert_eq!(cfg.colors.min_contrast, 3.0);
        assert_eq!(cfg.colors.ansi.len(), 2);
    }

    #[test]
    fn partial_config_uses_defaults_for_missing() {
        let json = r#"{ "font": { "size": 18.0 } }"#;
        let cfg: TerminalConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.font.size, Some(18.0));
        // Missing fields use defaults.
        assert_eq!(cfg.font.family.as_deref(), Some("Lilex"));
        assert_eq!(cfg.font.weight, "normal");
        assert_eq!(cfg.cursor.shape, "block");
        assert_eq!(cfg.layout.line_height, 1.2);
        assert_eq!(cfg.colors.foreground.as_deref(), Some("#efefef"));
    }

    #[test]
    fn tab_title_defaults_to_default() {
        // An empty/missing layout group → TabTitleMode::Default ("default").
        let cfg: TerminalConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(cfg.layout.tab_title, super::layout::TabTitleMode::Default);
    }

    #[test]
    fn tab_title_osc_parses_and_round_trips() {
        let cfg: TerminalConfig =
            serde_json::from_str(r#"{ "layout": { "tab_title": "osc" } }"#).unwrap();
        assert_eq!(cfg.layout.tab_title, super::layout::TabTitleMode::Osc);
        // Round-trip back to JSON and parse again.
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains("\"tab_title\":\"osc\""), "got: {json}");
        let again: TerminalConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(again.layout.tab_title, super::layout::TabTitleMode::Osc);
    }
    #[test]
    fn explicit_path_persistence_is_isolated_and_quarantines_corruption() {
        let directory = std::env::temp_dir().join(format!(
            "oneterm-terminal-config-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("terminal.json");
        let missing = TerminalConfig::load_from(&path);
        assert_eq!(missing.font.family, FontConfig::default().family);
        assert!(path.exists());
        let config = TerminalConfig::default();
        config.save_to(&path).unwrap();
        assert_eq!(
            TerminalConfig::load_from(&path).font.family,
            config.font.family
        );
        std::fs::write(&path, b"{not-json").unwrap();
        let loaded = TerminalConfig::load_from(&path);
        assert_eq!(loaded.font.family, FontConfig::default().family);
        assert!(!path.exists());
        assert!(std::fs::read_dir(&directory).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".invalid-")
        }));
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn legacy_fixture_migrates_and_current_save_is_idempotent() {
        let legacy = include_str!("../../tests/fixtures/persistence/terminal-v0.json");
        let config = TerminalConfig::parse_document(legacy).unwrap();
        assert_eq!(config.font.family.as_deref(), Some("Legacy Mono"));
        let directory = std::env::temp_dir().join(format!(
            "oneterm-terminal-schema-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        let path = directory.join("terminal.json");
        config.save_to(&path).unwrap();
        let value: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["schema_version"], CURRENT_SCHEMA_VERSION);
        let restored = TerminalConfig::load_from(&path);
        assert_eq!(restored.font.family, config.font.family);
        let _ = std::fs::remove_dir_all(directory);
    }
}
