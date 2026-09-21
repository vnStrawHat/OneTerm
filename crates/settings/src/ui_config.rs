//! Persistent UI-level settings — UI font size, theme name, and key bindings.
//!
//! Stored in `ui_config.json` (dev: `target/ui_config.json`, release:
//! `~/.OneTerm/ui_config.json`), mirroring the `terminal.json` pattern. Loaded at startup
//! (`UiConfig::init`) and applied in `theme::init` (theme name + font size) and
//! `OneTermWorkspace::bind_keys` (key bindings). Changes are persisted back by:
//!
//! - the `Theme` global observer installed by [`UiConfig::observe_theme`]
//!   (font size + theme name — fires on any `Theme::global_mut` mutation, e.g.
//!   the View ▸ Font Size menu, the Appearance page, or the theme menus), and
//! - the key-binding rebind UI (writes the `key_bindings` map + saves).
//!
//! The global `Entity<UiConfig>` is the single source of truth for the file's
//! content: the observer updates `theme_name`/`ui_font_size` on it before
//! saving, and the rebind UI updates `key_bindings` on it before saving.

use std::collections::HashMap;
use std::path::Path;

use gpui::{App, AppContext, Entity, Global};
use gpui_component::{ActiveTheme as _, Theme};
use oneterm_core::{
    AppError, RightDockMode, atomic_write, config_dir, parse_versioned_document, quarantine_file,
    set_schema_version,
};
use serde::{Deserialize, Serialize};

// File path is resolved at runtime via config_dir().join("ui_config.json") —
// debug → target/, release → ~/.OneTerm/ (see oneterm_core::config_dir).

// ── Config struct ────────────────────────────────────────────────────

/// Persisted UI settings. All fields optional — `None` means "use the built-in
/// default" (so a partial/old config file still works).
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
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
    /// agent-status event within `max(this, 3 × heartbeat_interval)` (while its
    /// terminal process is alive) is marked "stale" (see
    /// `docs/agent-panel-display.md` §9, `docs/osc-agent-status.md` §5.3).
    /// `None` = the built-in default ([`UiConfig::DEFAULT_AGENT_STALE_THRESHOLD_MS`], 5 min).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub agent_stale_threshold_ms: Option<u64>,

    /// `ui_config.json` existed but could not be read at startup (e.g.
    /// permission denied), so this is the built-in default and must not be
    /// written back over a possibly valid file (CORR-61). Never persisted.
    #[serde(skip)]
    pub persist_blocked: bool,
}

const CURRENT_SCHEMA_VERSION: u32 = 1;
const DOCUMENT_NAME: &str = "ui_config.json";

impl UiConfig {
    fn parse_document(raw: &str) -> Result<Self, AppError> {
        parse_versioned_document(raw, CURRENT_SCHEMA_VERSION, DOCUMENT_NAME)
    }
    /// Built-in default for [`UiConfig::agent_stale_threshold_ms`] — 5 minutes.
    pub const DEFAULT_AGENT_STALE_THRESHOLD_MS: u64 = 300_000;

    /// The effective agent-panel staleness threshold in ms (config value or the
    /// built-in default). A value of `0` disables staleness marking.
    pub fn agent_stale_threshold_ms(&self) -> u64 {
        self.agent_stale_threshold_ms
            .unwrap_or(Self::DEFAULT_AGENT_STALE_THRESHOLD_MS)
    }

    /// Load the config from `ui_config.json`. See [`Self::load_from`] for the
    /// outcome contract.
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
            Ok(raw) => Ok(Self::parse_document(&raw).unwrap_or_else(|e| {
                log::error!("{e} — using defaults");
                if let Err(quarantine_error) = quarantine_file(path) {
                    log::warn!("failed to quarantine ui_config.json: {quarantine_error}");
                }
                Self::default()
            })),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // M4 (`DEC-0019`): an elevated window writes no configuration —
                // not even the first-run default. Under over-the-shoulder
                // elevation this directory belongs to another account and must
                // be left without a trace; the flag makes the window say once
                // that it is running on the defaults.
                if oneterm_core::elevation::is_elevated() {
                    log::info!(
                        "elevated window: {DOCUMENT_NAME} is absent; using the defaults and writing nothing"
                    );
                    return Ok(Self::defaults_with_persist_blocked());
                }
                let cfg = Self::default();
                match serde_json::to_string_pretty(&cfg) {
                    Ok(json) => match atomic_write(path, json.as_bytes()) {
                        Ok(()) => log::info!("Created default ui_config.json at {path:?}"),
                        Err(write_error) => log::warn!(
                            "failed to create default ui_config.json at {path:?}: {write_error}"
                        ),
                    },
                    Err(serialize_error) => {
                        log::warn!("failed to serialize default ui_config.json: {serialize_error}")
                    }
                }
                Ok(cfg)
            }
            Err(error) => Err(AppError::config_load(DOCUMENT_NAME, error)),
        }
    }

    /// The defaults, flagged so they are never written over an unreadable file.
    fn defaults_with_persist_blocked() -> Self {
        Self {
            persist_blocked: true,
            ..Self::default()
        }
    }

    fn persist_blocked_error() -> AppError {
        AppError::config_load(
            DOCUMENT_NAME,
            "the file could not be read at startup; refusing to overwrite it",
        )
    }

    /// Why this write is refused, or `None` when it may go ahead.
    ///
    /// Two reasons, and they are not the same one: the file could not be read
    /// and may still be the user's (CORR-61), or this is an elevated window,
    /// which reads configuration and writes none of it (`DEC-0019` M4). Both
    /// shared write entry points ask here, so neither reason can be honoured in
    /// one and missed in the other.
    ///
    /// `elevated` is a parameter rather than a read of the process global so the
    /// rule can be tested without flipping process state under the other tests.
    fn write_refusal(&self, elevated: bool) -> Option<AppError> {
        if elevated {
            return Some(AppError::config_load(
                DOCUMENT_NAME,
                "an elevated OneTerm window writes no configuration",
            ));
        }
        self.persist_blocked.then(Self::persist_blocked_error)
    }

    /// Save the config to `ui_config.json` (pretty-printed). Refused with
    /// [`AppError::ConfigLoad`] while [`Self::persist_blocked`] is set.
    pub fn save(&self) -> Result<(), AppError> {
        self.save_to(&config_dir().join(DOCUMENT_NAME))
    }

    /// Save the config to an explicit path for deterministic callers and tests.
    pub fn save_to(&self, path: &Path) -> Result<(), AppError> {
        if let Some(refusal) = self.write_refusal(oneterm_core::elevation::is_elevated()) {
            return Err(refusal);
        }
        let mut value = serde_json::to_value(self)?;
        set_schema_version(&mut value, CURRENT_SCHEMA_VERSION)?;
        let json = serde_json::to_string_pretty(&value)?;
        atomic_write(path, json.as_bytes())?;
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

    /// Initialize the global — load `ui_config.json` once (called from the
    /// composition root before `theme::init` so the saved theme/font can be
    /// applied). Idempotent: later calls keep the loaded entity.
    pub fn init(cx: &mut App) {
        if cx.has_global::<UiConfigGlobal>() {
            return;
        }
        let cfg = Self::load().unwrap_or_else(|error| {
            log::error!("{error}; using defaults and refusing to overwrite the file");
            Self::defaults_with_persist_blocked()
        });
        let entity = cx.new(|_| cfg);
        cx.set_global(UiConfigGlobal(entity));
    }

    /// Persist the theme name + UI font size whenever the global `Theme` changes
    /// (View ▸ Font Size menu, Appearance page, theme menus, …).
    ///
    /// Register this after the startup theme has been applied so init
    /// mutations do not trigger a write. Notifications that leave the persisted
    /// pair unchanged (e.g. the list-style override applied right after a theme
    /// switch) are coalesced into no write at all.
    pub fn observe_theme(cx: &mut App) {
        cx.observe_global::<Theme>(|cx| {
            let (name, size) = {
                let theme = cx.theme();
                (theme.theme_name().to_string(), theme.font_size.as_f32())
            };
            let changed = Self::global(cx).update(cx, |cfg, _cx| {
                let changed = cfg.theme_name.as_deref() != Some(name.as_str())
                    || cfg.ui_font_size != Some(size);
                cfg.theme_name = Some(name);
                cfg.ui_font_size = Some(size);
                changed
            });
            if changed {
                Self::persist(cx);
            }
        })
        .detach();
    }

    /// Schedule persistence of a snapshot of the global config off the UI thread.
    /// Does nothing (with a warning) while [`Self::persist_blocked`] is set.
    pub fn persist(cx: &App) {
        let snapshot = Self::global(cx).read(cx).clone();
        if let Some(refusal) = snapshot.write_refusal(oneterm_core::elevation::is_elevated()) {
            log::warn!("{refusal}");
            return;
        }
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
        let missing = UiConfig::load_from(&path).unwrap();
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
            persist_blocked: false,
        };
        config.save_to(&path).unwrap();
        let restored = UiConfig::load_from(&path).unwrap();
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
        let fallback = UiConfig::load_from(&path).unwrap();
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
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["schema_version"], CURRENT_SCHEMA_VERSION);
        let restored = UiConfig::load_from(&path).unwrap();
        assert_eq!(restored.theme_name, config.theme_name);
        let _ = std::fs::remove_dir_all(directory);
    }

    /// M4 (`DEC-0019`): an elevated window reads `ui_config.json` and writes
    /// none of it back, whatever the file's own state was.
    #[test]
    fn an_elevated_window_refuses_to_write_a_perfectly_readable_config() {
        let readable = UiConfig::default();
        assert!(readable.write_refusal(false).is_none());
        let refusal = readable
            .write_refusal(true)
            .expect("an elevated window must not write ui_config.json");
        assert!(refusal.to_string().contains("elevated"));
        // ...and the unreadable-file refusal still stands on its own reason.
        let blocked = UiConfig::defaults_with_persist_blocked();
        assert!(
            blocked
                .write_refusal(false)
                .is_some_and(|error| error.to_string().contains("could not be read"))
        );
    }

    #[test]
    fn unreadable_document_is_typed_and_blocked_defaults_refuse_to_save() {
        let directory = std::env::temp_dir().join(format!(
            "oneterm-ui-unreadable-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        // A directory in place of the file fails to read with something other
        // than NotFound on every platform, standing in for a permission failure.
        let path = directory.join("ui_config.json");
        std::fs::create_dir_all(&path).unwrap();
        let error = UiConfig::load_from(&path).unwrap_err();
        assert!(
            matches!(&error, AppError::ConfigLoad { document, .. } if document == "ui_config.json"),
            "expected ConfigLoad, got {error}"
        );
        assert!(path.is_dir(), "an unreadable document must not be replaced");

        let blocked = UiConfig::defaults_with_persist_blocked();
        let target = directory.join("other.json");
        assert!(matches!(
            blocked.save_to(&target),
            Err(AppError::ConfigLoad { .. })
        ));
        assert!(!target.exists());
        let _ = std::fs::remove_dir_all(directory);
    }
}
