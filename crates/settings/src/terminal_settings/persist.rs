//! Reverse mapping `TerminalSettings` → `TerminalConfig` + persistence.
//!
//! `apply_config` (in `apply.rs`) goes config → settings at load time. This
//! module provides the inverse — `to_config` — so the live settings can be
//! written back to `terminal.json` when the user changes them in the Settings
//! UI. `save` is a convenience that builds the config and delegates to
//! `TerminalConfig::save`.
//!
//! Only the fields that have a 1:1 representation in both structs are mapped.
//! Color overrides (stored as `Hsla` in settings) are serialized back to
//! `"#RRGGBB"` strings via [`hsla_to_hex`](super::hsla_to_hex).

use gpui::{App, FontWeight, Hsla};

use crate::terminal_config::{
    BellConfig, ColorsConfig, CursorConfig, FontConfig, LayoutConfig, MouseConfig, PaddingConfig,
    ScrollConfig, SecurityConfig, TerminalConfig,
};

use super::{TerminalBlink, TerminalCursorShape, TerminalSettings, hsla_to_hex};

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
    /// This is the inverse of [`TerminalSettings::apply_config`]. The result
    /// can be passed to [`TerminalConfig::save`] to persist the settings.
    pub fn to_config(&self) -> TerminalConfig {
        let co = &self.color_overrides;

        TerminalConfig {
            font: FontConfig {
                family: self.font_family.as_ref().map(|s| s.to_string()),
                size: self.font_size,
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
            },
            bell: BellConfig {
                enabled: self.bell_enabled,
            },
            security: SecurityConfig {
                allow_clipboard_read: self.allow_clipboard_read,
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
                ansi: co.ansi.iter().map(|c| hsla_to_hex(*c)).collect(),
            },
        }
    }

    /// Persist the live settings to `terminal.json`.
    ///
    /// Builds a [`TerminalConfig`] snapshot (via [`Self::to_config`]) and writes
    /// it. Callers should first update the in-memory settings and `cx.notify()`,
    /// then call this — the config file is the source of truth across restarts.
    pub fn save(&self) -> std::io::Result<()> {
        self.to_config().save()
    }

    /// Schedule persistence of the current global settings off the UI thread.
    pub fn persist_global(cx: &App) {
        let config = Self::global(cx).read(cx).to_config();
        cx.background_executor()
            .spawn(async move {
                if let Err(error) = config.save() {
                    log::warn!("Failed to save terminal.json: {error}");
                }
            })
            .detach();
    }
}
