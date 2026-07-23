//! Persistent UI-level settings — UI font size, theme name, and key bindings.
//!
//! Stored in `ui_config.json` (dev: `target/ui_config.json`, release:
//! `~/.OneTerm/ui_config.json`), mirroring the `terminal.json` pattern. Loaded at startup
//! (`UiConfig::init`) and applied in `theme::init` (theme name + font size) and
//! `OneTermWorkspace::bind_keys` (key bindings). Changes are persisted back by:
//!
//! - a `Theme` global observer (font size + theme name — fires on any
//!   `Theme::global_mut` mutation, e.g. the View ▸ Font Size menu, the
//!   Appearance page, or the theme menus), and
//! - the key-binding rebind UI (writes the `key_bindings` map + saves).
//!
//! The global `Entity<UiConfig>` is the single source of truth for the file's
//! content: the observer updates `theme_name`/`ui_font_size` on it before
//! saving, and the rebind UI updates `key_bindings` on it before saving.

use std::collections::HashMap;
use std::path::Path;

use gpui::{App, AppContext, Entity, Global};
use oneterm_core::{
    RightDockMode, atomic_write, config_dir, migrate_json_value, quarantine_file,
    set_schema_version,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// File path is resolved at runtime via config_dir().join("ui_config.json") —
// debug → target/, release → ~/.OneTerm/ (see oneterm_core::config_dir).

// ── Config struct ────────────────────────────────────────────────────

/// Persisted UI settings. All fields optional — `None` means "use the built-in
/// default" (so a partial/old config file still works).
#[derive(Serialize, Deserialize, Default, Clone)]
pub struct UiConfig {
    /// UI (non-terminal) font size in px. `None` = default 16.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ui_font_size: Option<f32>,

    /// The active theme's name (e.g. "Zed One Dark"). `None` = default theme.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub theme_name: Option<String>,

    /// Per-action key-binding overrides: action id → keystroke string
    /// (e.g. `"open_settings" → "ctrl-,"`). An empty value means "unbind".
    /// Missing entries fall back to the built-in default.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub key_bindings: HashMap<String, String>,

    /// Which set of panels the right dock displays (SSH Client vs Agent vs None).
    /// Defaults to [`RightDockMode::SshClient`] when absent in an old config file.
    /// Written by the title bar mode toggle group, read at startup by
    /// `OneTermWorkspace::new`.
    #[serde(default)]
    pub right_dock_mode: RightDockMode,

    /// Agent Panel staleness threshold in milliseconds. An agent card with no
    /// OSC 9;7 event within `max(this, 3 × heartbeat_interval)` (while its
    /// terminal process is alive) is marked "stale" (see
    /// `docs/agent-panel-display.md` §9, `docs/osc-agent-status.md` §5.3).
    /// `None` = the built-in default ([`UiConfig::DEFAULT_AGENT_STALE_THRESHOLD_MS`], 5 min).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub agent_stale_threshold_ms: Option<u64>,
}

const CURRENT_SCHEMA_VERSION: u32 = 1;

impl UiConfig {
    fn parse_document(raw: &str) -> std::io::Result<Self> {
        let value: Value = serde_json::from_str(raw).map_err(std::io::Error::other)?;
        let value = migrate_json_value(
            value,
            CURRENT_SCHEMA_VERSION,
            "ui_config.json",
            |_, value| {
                if !value.is_object() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "ui_config.json schema must be an object",
                    ));
                }
                Ok(value)
            },
        )?;
        serde_json::from_value(value).map_err(std::io::Error::other)
    }
    /// Built-in default for [`UiConfig::agent_stale_threshold_ms`] — 5 minutes.
    pub const DEFAULT_AGENT_STALE_THRESHOLD_MS: u64 = 300_000;

    /// The effective agent-panel staleness threshold in ms (config value or the
    /// built-in default). A value of `0` disables staleness marking.
    pub fn agent_stale_threshold_ms(&self) -> u64 {
        self.agent_stale_threshold_ms
            .unwrap_or(Self::DEFAULT_AGENT_STALE_THRESHOLD_MS)
    }

    /// Load the config from file. Missing or invalid input selects defaults;
    /// invalid JSON is quarantined and only a missing file is initialized.
    pub fn load() -> Self {
        Self::load_from(&config_dir().join("ui_config.json"))
    }

    /// Load the config from an explicit path for deterministic callers and tests.
    pub fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(&path) {
            Ok(raw) => Self::parse_document(&raw).unwrap_or_else(|e| {
                log::error!("ui_config.json parse or migration error: {e} — using defaults");
                if let Err(quarantine_error) = quarantine_file(&path) {
                    log::warn!("failed to quarantine ui_config.json: {quarantine_error}");
                }
                Self::default()
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let cfg = Self::default();
                match serde_json::to_string_pretty(&cfg) {
                    Ok(json) => match atomic_write(&path, json.as_bytes()) {
                        Ok(()) => log::info!("Created default ui_config.json at {path:?}"),
                        Err(write_error) => log::warn!(
                            "failed to create default ui_config.json at {path:?}: {write_error}"
                        ),
                    },
                    Err(serialize_error) => {
                        log::warn!("failed to serialize default ui_config.json: {serialize_error}")
                    }
                }
                cfg
            }
            Err(error) => {
                log::error!("failed to read ui_config.json: {error}; using defaults");
                Self::default()
            }
        }
    }

    /// Save the config to `ui_config.json` (pretty-printed).
    pub fn save(&self) -> std::io::Result<()> {
        self.save_to(&config_dir().join("ui_config.json"))
    }

    /// Save the config to an explicit path for deterministic callers and tests.
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        let mut value = serde_json::to_value(self).map_err(std::io::Error::other)?;
        set_schema_version(&mut value, CURRENT_SCHEMA_VERSION)?;
        let json = serde_json::to_string_pretty(&value).map_err(std::io::Error::other)?;
        atomic_write(&path, json.as_bytes())?;
        log::debug!("Saved ui_config.json to {path:?}");
        Ok(())
    }
}

// ── Global entity ────────────────────────────────────────────────────

/// Global wrapper for `Entity<UiConfig>`.
pub struct UiConfigGlobal(pub Entity<UiConfig>);

impl Global for UiConfigGlobal {}

impl UiConfig {
    /// The global `Entity<UiConfig>` (panics if not initialized).
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<UiConfigGlobal>().0.clone()
    }

    /// Initialize the global — load `ui_config.json` (called from `ui::init`,
    /// before `theme::init` so the saved theme/font can be applied).
    pub fn init(cx: &mut App) {
        let cfg = Self::load();
        let entity = cx.new(|_| cfg);
        cx.set_global(UiConfigGlobal(entity));
    }

    /// Schedule persistence of a snapshot of the global config off the UI thread.
    pub fn persist(cx: &App) {
        let snapshot = Self::global(cx).read(cx).clone();
        cx.background_executor()
            .spawn(async move {
                if let Err(e) = snapshot.save() {
                    log::warn!("Failed to save ui_config.json: {e}");
                }
            })
            .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_path_roundtrip_and_corruption_quarantine_are_isolated() {
        let directory = std::env::temp_dir().join(format!(
            "oneterm-ui-config-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("ui_config.json");
        let missing = UiConfig::load_from(&path);
        assert_eq!(missing.right_dock_mode, RightDockMode::SshClient);
        assert!(path.exists());
        let config = UiConfig {
            ui_font_size: Some(18.0),
            theme_name: Some("Test Theme".into()),
            key_bindings: [("open_settings".into(), "ctrl-,".into())]
                .into_iter()
                .collect(),
            right_dock_mode: RightDockMode::Agent,
            agent_stale_threshold_ms: Some(42),
        };
        config.save_to(&path).unwrap();
        let restored = UiConfig::load_from(&path);
        assert_eq!(restored.ui_font_size, Some(18.0));
        assert_eq!(restored.theme_name.as_deref(), Some("Test Theme"));
        assert_eq!(
            restored
                .key_bindings
                .get("open_settings")
                .map(String::as_str),
            Some("ctrl-,")
        );
        assert_eq!(restored.agent_stale_threshold_ms, Some(42));
        std::fs::write(&path, b"not-json").unwrap();
        let fallback = UiConfig::load_from(&path);
        assert!(fallback.theme_name.is_none());
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
    fn legacy_partial_schema_uses_current_defaults() {
        let config: UiConfig = serde_json::from_str(
            r#"{"ui_font_size":14.0,"key_bindings":{"open_settings":"ctrl-,"}}"#,
        )
        .unwrap();
        assert_eq!(config.ui_font_size, Some(14.0));
        assert_eq!(config.right_dock_mode, RightDockMode::SshClient);
        assert_eq!(
            config.agent_stale_threshold_ms(),
            UiConfig::DEFAULT_AGENT_STALE_THRESHOLD_MS
        );
    }

    #[test]
    fn legacy_fixture_migrates_and_current_save_is_idempotent() {
        let legacy = include_str!("../tests/fixtures/persistence/ui-config-v0.json");
        let config = UiConfig::parse_document(legacy).unwrap();
        assert_eq!(config.theme_name.as_deref(), Some("Legacy Theme"));
        let directory = std::env::temp_dir().join(format!(
            "oneterm-ui-schema-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        let path = directory.join("ui_config.json");
        config.save_to(&path).unwrap();
        let value: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["schema_version"], CURRENT_SCHEMA_VERSION);
        let restored = UiConfig::load_from(&path);
        assert_eq!(restored.theme_name, config.theme_name);
        let _ = std::fs::remove_dir_all(directory);
    }
}
