//! Terminal config JSON — load/save `terminal.json`.
//!
//! The config file is organized into logical groups: font, cursor, layout, shell,
//! scroll, bell, colors. If the file does not exist → create a default file so the
//! user can see the available options.
//!
//! Path: `target/terminal.json` (debug) / `~/.OneTerm/terminal.json` (release).

use serde::{Deserialize, Serialize};

use oneterm_core::{LocalShellConfig, config_dir};

pub mod bell;
pub mod colors;
pub mod cursor;
pub mod font;
pub mod layout;
pub mod scroll;
pub mod security;

pub use bell::BellConfig;
pub use colors::ColorsConfig;
pub use cursor::CursorConfig;
pub use font::FontConfig;
pub use layout::{LayoutConfig, PaddingConfig, SemanticHighlightingMode, TabTitleMode};
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
    pub bell: BellConfig,
    #[serde(default)]
    pub colors: ColorsConfig,
    #[serde(default)]
    pub security: SecurityConfig,
}

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
    /// Load the config from file. If the file does not exist → create a default + return default.
    /// Supports `//` and `/* */` comments in the JSON.
    pub fn load() -> Self {
        let path = config_dir().join("terminal.json");
        match std::fs::read_to_string(&path) {
            Ok(raw) => {
                let json = strip_json_comments(&raw);
                match serde_json::from_str::<TerminalConfig>(&json) {
                    Ok(cfg) => cfg,
                    Err(e) => {
                        log::error!("terminal.json parse error: {e} — using defaults");
                        Self::default()
                    }
                }
            }
            Err(_) => {
                // File does not exist → create a default file so the user can see the options.
                let cfg = Self::default();
                if let Ok(json) = serde_json::to_string_pretty(&cfg) {
                    if std::fs::write(&path, json).is_ok() {
                        log::info!("Created default terminal.json at {path:?}");
                    }
                }
                cfg
            }
        }
    }

    /// Save the config to `terminal.json` (pretty-printed, no comments).
    pub fn save(&self) -> std::io::Result<()> {
        let path = config_dir().join("terminal.json");
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(&path, json)?;
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
        assert_eq!(cfg.font.family.as_deref(), Some("Cascadia Mono"));
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
        assert_eq!(cfg.font.family.as_deref(), Some("Cascadia Mono"));
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
}
