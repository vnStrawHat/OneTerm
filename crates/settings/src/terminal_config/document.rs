//! The aggregate `terminal.json` document: schema, comment stripping, and
//! load/save/migrate logic.
//!
//! The config file is organized into logical groups: font, cursor, layout, shell,
//! scroll, mouse, bell, colors. If the file does not exist → create a default file so the
//! user can see the available options.
//!
//! Path: `target/terminal.json` (debug) / `~/.OneTerm/terminal.json` (release).

use std::path::Path;

use serde::{Deserialize, Serialize};

use oneterm_core::{
    AppError, LocalShellConfig, atomic_write, config_dir, parse_versioned_document,
    quarantine_file, set_schema_version,
};

use super::{
    BellConfig, ColorsConfig, CompletionConfig, CursorConfig, FontConfig, LayoutConfig,
    MouseConfig, ScrollConfig, SecurityConfig,
};

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

const DOCUMENT_NAME: &str = "terminal.json";

impl TerminalConfig {
    fn parse_document(raw: &str) -> Result<Self, AppError> {
        parse_versioned_document(raw, CURRENT_SCHEMA_VERSION, DOCUMENT_NAME)
    }

    fn serialize_document(&self) -> std::io::Result<String> {
        let mut value = serde_json::to_value(self).map_err(std::io::Error::other)?;
        set_schema_version(&mut value, CURRENT_SCHEMA_VERSION)?;
        serde_json::to_string_pretty(&value).map_err(std::io::Error::other)
    }

    /// Load the config from `terminal.json`. Supports `//` and `/* */` comments
    /// in the JSON. See [`Self::load_from`] for the outcome contract.
    pub fn load() -> Result<Self, AppError> {
        Self::load_from(&config_dir().join(DOCUMENT_NAME))
    }

    /// Load the config from an explicit path for deterministic callers and tests.
    ///
    /// - A missing file is created with the defaults and the defaults are returned.
    /// - A file that does not parse or migrate is quarantined (with a recovery
    ///   log) and the defaults are returned.
    /// - Any other read failure (permissions, I/O) is returned as
    ///   [`AppError::ConfigLoad`]: the file may still be valid, so the caller
    ///   must not overwrite it with defaults (CORR-61).
    pub fn load_from(path: &Path) -> Result<Self, AppError> {
        match std::fs::read_to_string(path) {
            Ok(raw) => {
                let json = strip_json_comments(&raw);
                match Self::parse_document(&json) {
                    Ok(cfg) => Ok(cfg),
                    Err(e) => {
                        log::error!("{e} — using defaults");
                        if let Err(quarantine_error) = quarantine_file(path) {
                            log::warn!("failed to quarantine terminal.json: {quarantine_error}");
                        }
                        Ok(Self::default())
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // A missing file is the only read failure that safely selects defaults.
                let cfg = Self::default();
                match cfg.serialize_document() {
                    Ok(json) => match atomic_write(path, json.as_bytes()) {
                        Ok(()) => log::info!("Created default terminal.json at {path:?}"),
                        Err(write_error) => log::warn!(
                            "failed to create default terminal.json at {path:?}: {write_error}"
                        ),
                    },
                    Err(serialize_error) => {
                        log::warn!("failed to serialize default terminal.json: {serialize_error}")
                    }
                }
                Ok(cfg)
            }
            Err(error) => Err(AppError::config_load(DOCUMENT_NAME, error)),
        }
    }

    /// Save the config to `terminal.json` (pretty-printed, no comments).
    pub fn save(&self) -> std::io::Result<()> {
        self.save_to(&config_dir().join(DOCUMENT_NAME))
    }

    /// Save the config to an explicit path for deterministic callers and tests.
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        let json = self.serialize_document()?;
        atomic_write(path, json.as_bytes())?;
        log::info!("Saved terminal.json to {path:?}");
        Ok(())
    }
}

// Substantial parsing/persistence tests live in a sibling `document_tests.rs`
// (see code-style.md).
#[cfg(test)]
#[path = "document_tests.rs"]
mod document_tests;
