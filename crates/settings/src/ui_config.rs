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
    AppError, atomic_write, config_dir, parse_versioned_document, quarantine_file,
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

    /// Whether the right dock is open, remembered per kind of active tab
    /// (`IN-0054`): it follows the tab, and a dock button records the user's
    /// choice for the current kind. Omitted while it is the default. The
    /// former `right_dock_mode` field is ignored on read and dropped on write.
    #[serde(default, skip_serializing_if = "RightDockPrefs::is_default")]
    pub right_dock: RightDockPrefs,

    /// Show the FPS HUD (`US-0151`). Off by default; omitted from the file while off.
    /// Written only through [`UiConfig::set_show_fps`].
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub show_fps: bool,

    /// Minutes without keyboard or mouse input before every window shows the lock
    /// screen (`US-0159`; only once a master password exists). `None` = the default,
    /// [`Self::DEFAULT_LOCK_AFTER_IDLE_MINUTES`]. Read through
    /// [`Self::lock_timeout_minutes`], which clamps it to [`Self::LOCK_AFTER_IDLE_RANGE`];
    /// a value that is not a non-negative integer loads as `None` instead of
    /// quarantining the whole file. Written only through
    /// [`Self::set_lock_after_idle_minutes`].
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient_minutes"
    )]
    pub lock_after_idle_minutes: Option<u64>,

    /// The vertical tab rail (`IN-0055`): `{ "enabled", "width", "collapsed" }`. Omitted while
    /// every field is its default; a value of the wrong shape loads as the default instead of
    /// quarantining the whole file. Written only through
    /// [`UiConfig::set_vertical_tabs_enabled`] (`width` and `collapsed` get their writers
    /// with the resize and collapse packets).
    #[serde(
        default,
        skip_serializing_if = "VerticalTabs::is_default",
        deserialize_with = "lenient_vertical_tabs"
    )]
    pub vertical_tabs: VerticalTabs,

    /// `ui_config.json` existed but could not be read at startup (e.g.
    /// permission denied), so this is the built-in default and must not be
    /// written back over a possibly valid file (CORR-61). Never persisted.
    #[serde(skip)]
    pub persist_blocked: bool,
}

const CURRENT_SCHEMA_VERSION: u32 = 1;

/// `lock_after_idle_minutes` as written by hand: a non-negative integer, or absent. Anything
/// else (a string, a fraction, a negative number) reads as absent, so one bad value costs the
/// default timeout rather than the whole file's quarantine.
fn lenient_minutes<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u64>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    let minutes = value.as_u64();
    if minutes.is_none() && !value.is_null() {
        log::warn!(
            "ui_config.json: lock_after_idle_minutes {value} is not a whole number of minutes; using the default"
        );
    }
    Ok(minutes)
}

/// `vertical_tabs` as written by hand. Anything that is not an object reads as the default
/// (with a warning), and so does each field of the wrong type, so one bad value costs that
/// setting rather than the document. `width` is clamped on read and never rewritten by it.
fn lenient_vertical_tabs<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<VerticalTabs, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(VerticalTabs::from_value(&value))
}
const DOCUMENT_NAME: &str = "ui_config.json";

/// The vertical tab rail's persisted state (`IN-0055`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct VerticalTabs {
    /// Draw Terminal Tabs as a rail on the left instead of a strip above the terminal.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub enabled: bool,
    /// The rail's width in px, within [`Self::WIDTH_RANGE`]: the user's preference, applied
    /// clamped to the tab group's own width.
    #[serde(skip_serializing_if = "VerticalTabs::is_default_width")]
    pub width: u32,
    /// The rail is shown as a narrow strip. Remembered while the rail is off.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub collapsed: bool,
}

impl Default for VerticalTabs {
    fn default() -> Self {
        Self {
            enabled: false,
            width: Self::DEFAULT_WIDTH,
            collapsed: false,
        }
    }
}

impl VerticalTabs {
    /// The rail's width until the user sets one.
    pub const DEFAULT_WIDTH: u32 = 220;
    /// The widths a stored preference may take.
    pub const WIDTH_RANGE: std::ops::RangeInclusive<u32> = 140..=400;

    fn is_default(&self) -> bool {
        *self == Self::default()
    }

    fn is_default_width(width: &u32) -> bool {
        *width == Self::DEFAULT_WIDTH
    }

    /// Read a `vertical_tabs` JSON value leniently: a field that is missing or of the wrong
    /// type is that field's default.
    fn from_value(value: &serde_json::Value) -> Self {
        let defaults = Self::default();
        let object = match value {
            serde_json::Value::Object(object) => object,
            serde_json::Value::Null => return defaults,
            other => {
                log::warn!(
                    "ui_config.json: vertical_tabs {other} is not an object; using the default"
                );
                return defaults;
            }
        };
        let flag = |key: &str| match object.get(key) {
            None | Some(serde_json::Value::Null) => false,
            Some(serde_json::Value::Bool(flag)) => *flag,
            Some(other) => {
                log::warn!(
                    "ui_config.json: vertical_tabs.{key} {other} is not true or false; using the default"
                );
                false
            }
        };
        let width = match object.get("width") {
            None | Some(serde_json::Value::Null) => defaults.width,
            Some(other) => match other.as_f64().filter(|width| width.is_finite()) {
                Some(width) => (width.round().max(0.) as u32)
                    .clamp(*Self::WIDTH_RANGE.start(), *Self::WIDTH_RANGE.end()),
                None => {
                    log::warn!(
                        "ui_config.json: vertical_tabs.width {other} is not a number; using the default"
                    );
                    defaults.width
                }
            },
        };
        Self {
            enabled: flag("enabled"),
            width,
            collapsed: flag("collapsed"),
        }
    }
}

/// The right dock's open state per kind of active Terminal Tab (`IN-0054`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RightDockPrefs {
    /// Open while a local-shell tab is active. Default: closed.
    pub local: bool,
    /// Open while an SSH tab is active. Default: open.
    pub ssh: bool,
}

impl Default for RightDockPrefs {
    fn default() -> Self {
        Self {
            local: false,
            ssh: true,
        }
    }
}

impl RightDockPrefs {
    /// The remembered open state for a tab of this kind.
    pub fn open_for(self, is_local: bool) -> bool {
        if is_local { self.local } else { self.ssh }
    }

    /// Remember `open` for a tab of this kind.
    pub fn set(&mut self, is_local: bool, open: bool) {
        if is_local {
            self.local = open;
        } else {
            self.ssh = open;
        }
    }

    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl UiConfig {
    fn parse_document(raw: &str) -> Result<Self, AppError> {
        parse_versioned_document(raw, CURRENT_SCHEMA_VERSION, DOCUMENT_NAME)
    }
    /// The idle lock's default timeout (intake `IN-0050` Q2).
    pub const DEFAULT_LOCK_AFTER_IDLE_MINUTES: u64 = 15;
    /// The idle lock's timeout range: no "Never" (intake Q2), at most a working day.
    pub const LOCK_AFTER_IDLE_RANGE: std::ops::RangeInclusive<u64> = 1..=480;

    /// The idle lock's timeout in minutes, clamped to [`Self::LOCK_AFTER_IDLE_RANGE`].
    pub fn lock_timeout_minutes(&self) -> u64 {
        self.lock_after_idle_minutes
            .unwrap_or(Self::DEFAULT_LOCK_AFTER_IDLE_MINUTES)
            .clamp(
                *Self::LOCK_AFTER_IDLE_RANGE.start(),
                *Self::LOCK_AFTER_IDLE_RANGE.end(),
            )
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
                if oneterm_core::elevation::is_restricted() {
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
        if let Some(refusal) = self.write_refusal(oneterm_core::elevation::is_restricted()) {
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

    /// Switch the FPS HUD on or off: the one writer of [`Self::show_fps`], shared by the
    /// toggle action and the Settings switch. Persists and redraws every window.
    pub fn set_show_fps(show: bool, cx: &mut App) {
        Self::global(cx).update(cx, |cfg, cx| {
            cfg.show_fps = show;
            cx.notify();
        });
        Self::persist(cx);
        cx.refresh_windows();
    }

    /// The vertical tab rail's settings, or the defaults before [`Self::init`] has run (the
    /// shell's tests draw tab groups without a config).
    pub fn read_vertical_tabs(cx: &App) -> VerticalTabs {
        cx.try_global::<UiConfigGlobal>()
            .map(|global| global.0.read(cx).vertical_tabs)
            .unwrap_or_default()
    }

    /// Switch the vertical tab rail on or off: the one writer of `vertical_tabs.enabled`,
    /// used by the Settings switch. The entity changes first, so the layout flips on the next
    /// frame even where the write is refused (an elevated window). Persists and redraws every
    /// window.
    pub fn set_vertical_tabs_enabled(enabled: bool, cx: &mut App) {
        Self::global(cx).update(cx, |cfg, cx| {
            cfg.vertical_tabs.enabled = enabled;
            cx.notify();
        });
        Self::persist(cx);
        cx.refresh_windows();
    }

    /// Set the idle lock's timeout (clamped to [`Self::LOCK_AFTER_IDLE_RANGE`]): the one
    /// writer of `lock_after_idle_minutes`, used by Settings > Security. Persists; the
    /// lock timer observes the config and restarts with the new value.
    pub fn set_lock_after_idle_minutes(minutes: u64, cx: &mut App) {
        Self::global(cx).update(cx, |cfg, cx| {
            cfg.lock_after_idle_minutes = Some(minutes);
            cfg.lock_after_idle_minutes = Some(cfg.lock_timeout_minutes());
            cx.notify();
        });
        Self::persist(cx);
    }

    /// Schedule persistence of a snapshot of the global config off the UI thread.
    /// Does nothing (with a warning) while [`Self::persist_blocked`] is set.
    pub fn persist(cx: &App) {
        let snapshot = Self::global(cx).read(cx).clone();
        if let Some(refusal) = snapshot.write_refusal(oneterm_core::elevation::is_restricted()) {
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

    /// Removes the fixture directory on drop, including when an assertion
    /// panics (`Drop` runs during unwind): a manual `std::fs::remove_dir_all`
    /// as a test's last statement never runs if an earlier assertion in the
    /// same test panics (`BUG-0083`, same latent-leak shape
    /// `crates/app/src/crash_report.rs`'s `TemporaryDirectory` fixed for
    /// `BUG-0082`).
    struct TemporaryDirectory(std::path::PathBuf);

    impl Drop for TemporaryDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    impl std::ops::Deref for TemporaryDirectory {
        type Target = std::path::Path;

        fn deref(&self) -> &std::path::Path {
            &self.0
        }
    }

    #[test]
    fn explicit_path_roundtrip_and_corruption_quarantine_are_isolated() {
        let directory = TemporaryDirectory(std::env::temp_dir().join(format!(
            "oneterm-ui-config-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        )));
        std::fs::create_dir_all(&*directory).unwrap();
        let path = directory.join("ui_config.json");
        let missing = UiConfig::load_from(&path).unwrap();
        assert_eq!(missing.right_dock, RightDockPrefs::default());
        assert!(path.exists());
        let config = UiConfig {
            ui_font_size: Some(18.0),
            theme_name: Some("Test Theme".into()),
            key_bindings: [("open_settings".into(), "ctrl-,".into())]
                .into_iter()
                .collect(),
            right_dock: RightDockPrefs {
                local: true,
                ssh: false,
            },
            show_fps: true,
            lock_after_idle_minutes: Some(30),
            vertical_tabs: VerticalTabs {
                enabled: true,
                width: 260,
                collapsed: true,
            },
            persist_blocked: false,
        };
        config.save_to(&path).unwrap();
        let restored = UiConfig::load_from(&path).unwrap();
        assert!(restored.show_fps);
        assert_eq!(restored.lock_timeout_minutes(), 30);
        assert_eq!(restored.vertical_tabs, config.vertical_tabs);
        assert_eq!(restored.ui_font_size, Some(18.0));
        assert_eq!(restored.theme_name.as_deref(), Some("Test Theme"));
        assert_eq!(
            restored
                .key_bindings
                .get("open_settings")
                .map(String::as_str),
            Some("ctrl-,")
        );
        assert_eq!(
            restored.right_dock,
            RightDockPrefs {
                local: true,
                ssh: false
            }
        );
        std::fs::write(&path, b"not-json").unwrap();
        let fallback = UiConfig::load_from(&path).unwrap();
        assert!(fallback.theme_name.is_none());
        assert!(!path.exists());
        assert!(std::fs::read_dir(&*directory).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".invalid-")
        }));
    }
    #[test]
    fn legacy_partial_schema_uses_current_defaults() {
        let config: UiConfig = serde_json::from_str(
            r#"{"ui_font_size":14.0,"key_bindings":{"open_settings":"ctrl-,"}}"#,
        )
        .unwrap();
        assert_eq!(config.ui_font_size, Some(14.0));
        assert_eq!(config.right_dock, RightDockPrefs::default());
        // The default is not written back, so an untouched file keeps its shape.
        assert!(
            !serde_json::to_string(&config)
                .unwrap()
                .contains("right_dock")
        );
        // The FPS HUD is off unless the file says otherwise (`US-0151`), and "off" is not
        // written back, so an untouched file keeps its shape.
        assert!(!config.show_fps);
        assert!(!serde_json::to_string(&config).unwrap().contains("show_fps"));
    }

    /// `DEC-0022` / `IN-0054`: a file written before the right dock followed the
    /// tab kind still loads. `right_dock_mode` (any value, including the
    /// withdrawn `"agent"`) and `agent_stale_threshold_ms` are ignored on read
    /// and dropped on write; the new defaults apply (owner ruling) — no
    /// quarantine, no lost settings.
    #[test]
    fn a_config_from_the_agent_panel_era_loads_and_heals() {
        let directory = TemporaryDirectory(std::env::temp_dir().join(format!(
            "oneterm-ui-config-agent-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        )));
        std::fs::create_dir_all(&*directory).unwrap();
        let path = directory.join("ui_config.json");
        UiConfig {
            theme_name: Some("Kept".into()),
            ..UiConfig::default()
        }
        .save_to(&path)
        .unwrap();
        let mut document: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        document["right_dock_mode"] = "agent".into();
        document["agent_stale_threshold_ms"] = 42.into();
        std::fs::write(&path, document.to_string()).unwrap();

        let loaded = UiConfig::load_from(&path).unwrap();
        assert_eq!(loaded.right_dock, RightDockPrefs::default());
        assert_eq!(loaded.theme_name.as_deref(), Some("Kept"));
        assert!(path.exists(), "the document was not quarantined");

        loaded.save_to(&path).unwrap();
        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(written.get("right_dock_mode").is_none());
        assert!(written.get("agent_stale_threshold_ms").is_none());
    }

    /// `US-0159`: 15 minutes unless the file says otherwise, clamped to 1..=480 (no
    /// "Never", intake Q2), and a hand-edited value of the wrong type loads as the
    /// default instead of quarantining the whole document.
    #[test]
    fn the_idle_lock_timeout_defaults_clamps_and_parses_leniently() {
        let parse = |raw: &str| UiConfig::parse_document(raw).unwrap();
        let absent = parse(r#"{"schema_version":1}"#);
        assert_eq!(absent.lock_timeout_minutes(), 15);
        assert!(
            !serde_json::to_string(&absent)
                .unwrap()
                .contains("lock_after_idle")
        );
        let minutes = |value: &str| {
            parse(&format!(
                r#"{{"theme_name":"T","lock_after_idle_minutes":{value}}}"#
            ))
        };
        assert_eq!(minutes("60").lock_timeout_minutes(), 60);
        assert_eq!(minutes("0").lock_timeout_minutes(), 1);
        assert_eq!(minutes("100000").lock_timeout_minutes(), 480);
        for wrong in [r#""ten""#, "-5", "2.5", "true", "null"] {
            let config = minutes(wrong);
            assert_eq!(config.lock_timeout_minutes(), 15, "{wrong}");
            assert_eq!(config.theme_name.as_deref(), Some("T"), "{wrong}");
        }
    }

    #[test]
    fn the_vertical_tab_rail_is_off_by_default_and_the_default_is_not_written() {
        let parse = |raw: &str| UiConfig::parse_document(raw).unwrap();
        let absent = parse(r#"{"schema_version":1}"#);
        assert_eq!(absent.vertical_tabs, VerticalTabs::default());
        assert!(!absent.vertical_tabs.enabled);
        assert!(
            !serde_json::to_string(&absent)
                .unwrap()
                .contains("vertical_tabs")
        );
        // An object that spells out the defaults is still "default" and is not written back.
        let spelled = parse(r#"{"vertical_tabs":{"enabled":false,"width":220,"collapsed":false}}"#);
        assert_eq!(spelled.vertical_tabs, VerticalTabs::default());
        assert!(
            !serde_json::to_string(&spelled)
                .unwrap()
                .contains("vertical_tabs")
        );

        // Only the fields that differ are written; the shape round-trips.
        let on = parse(r#"{"vertical_tabs":{"enabled":true}}"#);
        assert!(on.vertical_tabs.enabled);
        assert_eq!(on.vertical_tabs.width, VerticalTabs::DEFAULT_WIDTH);
        let value = serde_json::to_value(&on).unwrap();
        assert_eq!(value["vertical_tabs"], serde_json::json!({"enabled": true}));
        let full = parse(r#"{"vertical_tabs":{"enabled":true,"width":260,"collapsed":true}}"#);
        assert_eq!(
            full.vertical_tabs,
            VerticalTabs {
                enabled: true,
                width: 260,
                collapsed: true
            }
        );
        let again = parse(&serde_json::to_string(&full).unwrap());
        assert_eq!(again.vertical_tabs, full.vertical_tabs);
    }

    /// A hand-edited `vertical_tabs` of the wrong shape costs that setting, never the document:
    /// the rest of the file loads and nothing is quarantined (the parse is `parse_document`
    /// itself, which `load_from` quarantines on failure).
    #[test]
    fn a_garbage_vertical_tabs_value_loads_as_the_default() {
        let parse = |value: &str| {
            UiConfig::parse_document(&format!(
                r#"{{"theme_name":"Kept","vertical_tabs":{value}}}"#
            ))
            .expect("a bad vertical_tabs value must not fail the document")
        };
        for wrong in [
            r#""on""#,
            "5",
            "true",
            "[true]",
            "null",
            r#"{"enabled":"yes"}"#,
            r#"{"enabled":1}"#,
            r#"{"enabled":null}"#,
        ] {
            let config = parse(wrong);
            assert_eq!(config.vertical_tabs, VerticalTabs::default(), "{wrong}");
            assert_eq!(config.theme_name.as_deref(), Some("Kept"), "{wrong}");
        }
        // A bad field costs only that field.
        let partial = parse(r#"{"enabled":true,"width":"wide","collapsed":"no"}"#);
        assert_eq!(
            partial.vertical_tabs,
            VerticalTabs {
                enabled: true,
                ..VerticalTabs::default()
            }
        );
    }

    /// The stored width is a preference inside 140..=400: out-of-range numbers are clamped
    /// where they are read, and the read itself writes nothing.
    #[test]
    fn the_vertical_tab_width_is_clamped_on_read() {
        let width = |value: &str| {
            UiConfig::parse_document(&format!(r#"{{"vertical_tabs":{{"width":{value}}}}}"#))
                .unwrap()
                .vertical_tabs
                .width
        };
        assert_eq!(width("260"), 260);
        assert_eq!(width("260.4"), 260);
        assert_eq!(width("139"), 140);
        assert_eq!(width("0"), 140);
        assert_eq!(width("-30"), 140);
        assert_eq!(width("401"), 400);
        assert_eq!(width("100000"), 400);
    }

    /// The field is additive: a document with it survives a load and a save, and the rest of
    /// the document is untouched.
    #[test]
    fn vertical_tabs_survive_a_load_and_a_save() {
        let directory = TemporaryDirectory(std::env::temp_dir().join(format!(
            "oneterm-ui-vertical-tabs-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        )));
        std::fs::create_dir_all(&*directory).unwrap();
        let path = directory.join("ui_config.json");
        std::fs::write(
            &path,
            r#"{"schema_version":1,"theme_name":"Kept","vertical_tabs":{"enabled":true,"width":300}}"#,
        )
        .unwrap();
        let loaded = UiConfig::load_from(&path).unwrap();
        assert!(path.exists(), "the document was not quarantined");
        assert!(loaded.vertical_tabs.enabled);
        assert_eq!(loaded.vertical_tabs.width, 300);
        loaded.save_to(&path).unwrap();
        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            written["vertical_tabs"],
            serde_json::json!({"enabled": true, "width": 300})
        );
        assert_eq!(written["theme_name"], "Kept");
    }

    #[test]
    fn legacy_fixture_migrates_and_current_save_is_idempotent() {
        let legacy = include_str!("../tests/fixtures/persistence/ui-config-v0.json");
        let config = UiConfig::parse_document(legacy).unwrap();
        assert_eq!(config.theme_name.as_deref(), Some("Legacy Theme"));
        let directory = TemporaryDirectory(std::env::temp_dir().join(format!(
            "oneterm-ui-schema-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        )));
        let path = directory.join("ui_config.json");
        config.save_to(&path).unwrap();
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["schema_version"], CURRENT_SCHEMA_VERSION);
        let restored = UiConfig::load_from(&path).unwrap();
        assert_eq!(restored.theme_name, config.theme_name);
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
        let directory = TemporaryDirectory(std::env::temp_dir().join(format!(
            "oneterm-ui-unreadable-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        )));
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
    }
}
