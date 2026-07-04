//! Persistent UI-level settings — UI font size, theme name, and key bindings.
//!
//! Stored in `ui_config.json` (dev: `target/ui_config.json`, release:
//! `ui_config.json`), mirroring the `terminal.json` pattern. Loaded at startup
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

use gpui::{App, AppContext, Entity, Global};
use serde::{Deserialize, Serialize};

// ── Config path ──────────────────────────────────────────────────────

#[cfg(debug_assertions)]
const CONFIG_FILE: &str = "target/ui_config.json";
#[cfg(not(debug_assertions))]
const CONFIG_FILE: &str = "ui_config.json";

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
}

impl UiConfig {
    /// Load the config from file. Missing/unparseable → default (and a default
    /// file is written if absent, like `TerminalConfig::load`).
    pub fn load() -> Self {
        let path = std::path::PathBuf::from(CONFIG_FILE);
        match std::fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str::<UiConfig>(&raw).unwrap_or_else(|e| {
                log::error!("ui_config.json parse error: {e} — using defaults");
                Self::default()
            }),
            Err(_) => {
                let cfg = Self::default();
                if let Ok(json) = serde_json::to_string_pretty(&cfg) {
                    if std::fs::write(&path, json).is_ok() {
                        log::info!("Created default ui_config.json at {path:?}");
                    }
                }
                cfg
            }
        }
    }

    /// Save the config to `ui_config.json` (pretty-printed).
    pub fn save(&self) -> std::io::Result<()> {
        let path = std::path::PathBuf::from(CONFIG_FILE);
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(&path, json)?;
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

    /// Persist the global config to disk (reads the global entity + saves).
    pub fn persist(cx: &App) {
        let entity = Self::global(cx);
        let res = entity.read(cx).save();
        if let Err(e) = res {
            log::warn!("Failed to save ui_config.json: {e}");
        }
    }
}
