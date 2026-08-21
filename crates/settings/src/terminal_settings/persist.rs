//! Reverse mapping `TerminalSettings` → `TerminalConfig` + persistence.
//!
//! `from_config` (in `apply.rs`) goes config → settings at load time. This
//! module provides the inverse — `to_config` — so the live settings can be
//! written back to `terminal.json` when the user changes them in the Settings
//! UI. `save` is a convenience that builds the config and delegates to
//! `TerminalConfig::save`.
//!
//! Only the fields that have a 1:1 representation in both structs are mapped.
//! Color overrides (stored as `Hsla` in settings) are serialized back to
//! `"#RRGGBB"` strings via [`hsla_to_hex`](super::hsla_to_hex).

use gpui::{App, FontWeight, Hsla};
use oneterm_core::AppError;

use crate::terminal_config::{
    BellConfig, ColorsConfig, CursorConfig, FontConfig, LayoutConfig, MouseConfig, PaddingConfig,
    ScrollConfig, SecurityConfig, TerminalConfig,
};

use super::{TerminalBlink, TerminalCursorShape, TerminalSettings, hsla_to_hex};

// Roundtrip tests live in a sibling `persist_tests.rs` (same convention as
// `terminal_config/document_tests.rs`).
#[cfg(test)]
#[path = "persist_tests.rs"]
mod persist_tests;

/// Map a [`gpui::FontWeight`] back to its config string (the inverse of
/// [`parse_weight`](super::font::parse_weight)).
fn weight_to_string(w: FontWeight) -> String {
    match w {
        FontWeight::THIN => "thin",
        FontWeight::EXTRA_LIGHT => "extra_light",
        FontWeight::LIGHT => "light",
        FontWeight::NORMAL => "normal",
        FontWeight::MEDIUM => "medium",
        FontWeight::SEMIBOLD => "semibold",
        FontWeight::BOLD => "bold",
        FontWeight::EXTRA_BOLD => "extra_bold",
        FontWeight::BLACK => "black",
        _ => "normal",
    }
    .into()
}

/// Map a [`TerminalCursorShape`] back to its config string.
fn shape_to_string(s: TerminalCursorShape) -> &'static str {
    match s {
        TerminalCursorShape::Block => "block",
        TerminalCursorShape::Bar => "bar",
        TerminalCursorShape::Underline => "underline",
    }
}

/// Serialize an optional `Hsla` override into a `"#RRGGBB"` string (or `None`).
fn color_to_hex(c: Option<Hsla>) -> Option<String> {
    c.map(hsla_to_hex)
}

impl TerminalSettings {
    /// Build a [`TerminalConfig`] snapshot from the live settings.
    ///
    /// This is the inverse of [`TerminalSettings::from_config`]. The result
    /// can be passed to [`TerminalConfig::save`] to persist the settings.
    pub fn to_config(&self) -> TerminalConfig {
        let co = &self.color_overrides;

        TerminalConfig {
            font: FontConfig {
                family: self.font_family.as_ref().map(|s| s.to_string()),
                // Persist the configured size, not the live zoom-modified
                // `font_size`; otherwise a zoomed session becomes the new base
                // on the next launch (CORR-12).
                size: self.base_font_size,
                weight: weight_to_string(self.font_weight),
                features: self.font_features.iter().map(|s| s.to_string()).collect(),
            },
            cursor: CursorConfig {
                shape: shape_to_string(self.cursor_shape).to_string(),
                color: color_to_hex(self.cursor_color),
                blink: matches!(self.cursor_blink, TerminalBlink::On),
            },
            layout: LayoutConfig {
                line_height: self.line_height_factor,
                cell_width: self.cell_width,
                padding: PaddingConfig {
                    top: self.padding.top,
                    right: self.padding.right,
                    bottom: self.padding.bottom,
                    left: self.padding.left,
                },
                show_gutter: self.show_gutter,
                semantic_highlighting: self.semantic_highlighting,
                tab_title: self.tab_title_mode,
            },
            shell: self.shell.clone(),
            scroll: ScrollConfig {
                multiplier: self.scroll_multiplier,
                alternate_scroll: self.alternate_scroll,
                scrollback_history: self.scrollback_history,
            },
            mouse: MouseConfig {
                show_context_menu: self.show_context_menu,
                copy_on_select: self.copy_on_select,
            },
            bell: BellConfig {
                enabled: self.bell_enabled,
            },
            security: SecurityConfig {
                allow_clipboard_read: self.allow_clipboard_read,
            },
            completion: self.completion.clone(),
            logging: self.logging.clone(),
            sftp: self.sftp.clone(),
            ssh: {
                let keepalive = self.ssh.keepalive();
                crate::terminal_config::SshSettingsConfig {
                    keepalive_enabled: keepalive.enabled(),
                    keepalive_interval_secs: keepalive.interval_secs(),
                    keepalive_max: keepalive.max(),
                }
            },
            colors: ColorsConfig {
                foreground: color_to_hex(co.foreground),
                background: color_to_hex(co.background),
                cursor: color_to_hex(co.cursor),
                selection: color_to_hex(co.selection),
                gutter_fg: color_to_hex(co.gutter_fg),
                gutter_bg: color_to_hex(co.gutter_bg),
                clock_fg: color_to_hex(co.clock_fg),
                line_number_fg: color_to_hex(co.line_number_fg),
                min_contrast: co.min_contrast,
                // A slot without an override stays an empty string so the
                // positions after it survive the round trip.
                ansi: co
                    .ansi
                    .iter()
                    .map(|c| c.map(hsla_to_hex).unwrap_or_default())
                    .collect(),
            },
        }
    }

    /// Persist the live settings to `terminal.json`.
    ///
    /// Builds a [`TerminalConfig`] snapshot (via [`Self::to_config`]) and writes
    /// it. Callers should first update the in-memory settings and `cx.notify()`,
    /// then call this — the config file is the source of truth across restarts.
    /// Refused with [`AppError::ConfigLoad`] while [`Self::persist_blocked`] is
    /// set: the file on disk could not be read and may still be the user's.
    pub fn save(&self) -> Result<(), AppError> {
        if self.persist_blocked {
            return Err(Self::persist_blocked_error());
        }
        self.to_config().save()?;
        Ok(())
    }

    fn persist_blocked_error() -> AppError {
        AppError::config_load(
            "terminal.json",
            "the file could not be read at startup; refusing to overwrite it",
        )
    }

    /// Schedule persistence of the current global settings off the UI thread.
    pub fn persist_global(cx: &App) {
        let settings = Self::global(cx).read(cx);
        if settings.persist_blocked {
            log::warn!("{}", Self::persist_blocked_error());
            return;
        }
        let config = settings.to_config();
        cx.background_executor()
            .spawn(async move {
                if let Err(error) = config.save() {
                    log::warn!("Failed to save terminal.json: {error}");
                }
            })
            .detach();
    }
}
