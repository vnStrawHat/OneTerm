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

#[cfg(test)]
mod tests;

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
