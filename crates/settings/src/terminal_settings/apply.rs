//! Build `TerminalSettings` from `TerminalConfig` (config → live).
//!
//! Every live field is derived from the config, so `TerminalSettings::default()`
//! is `from_config(&TerminalConfig::default())` and the defaults live in one
//! place. The inverse mapping lives in [`super::persist`].

use gpui::Hsla;

use crate::terminal_config::{
    BellConfig, ColorsConfig, CursorConfig, FontConfig, LayoutConfig, MouseConfig, ScrollConfig,
    SecurityConfig, TerminalConfig,
};

use super::{
    ColorOverrides, TerminalBlink, TerminalCursorShape, TerminalPadding, TerminalSettings,
    parse_hex_color, parse_weight,
};

/// Parse the `colors.ansi` list slot by slot. An entry that is not a valid hex
/// colour keeps its position as `None` (theme colour) and is logged, so the
/// entries after it are not shifted down (CORR-60).
fn parse_ansi_overrides(ansi: &[String]) -> Vec<Option<Hsla>> {
    ansi.iter()
        .enumerate()
        .map(|(slot, text)| {
            let color = parse_hex_color(text);
            if color.is_none() && !text.trim().is_empty() {
                log::warn!(
                    "terminal.json colors.ansi[{slot}] = {text:?} is not a hex colour; keeping the theme colour"
                );
            }
            color
        })
        .collect()
}

impl TerminalSettings {
    /// Build the live settings from a `terminal.json` config.
    pub fn from_config(cfg: &TerminalConfig) -> Self {
        let font: &FontConfig = &cfg.font;
        let cursor: &CursorConfig = &cfg.cursor;
        let layout: &LayoutConfig = &cfg.layout;
        let scroll: &ScrollConfig = &cfg.scroll;
        let mouse: &MouseConfig = &cfg.mouse;
        let bell: &BellConfig = &cfg.bell;
        let security: &SecurityConfig = &cfg.security;
        let colors: &ColorsConfig = &cfg.colors;

        Self {
            shell: cfg.shell.clone(),

            font_family: font.family.as_ref().map(|s| s.clone().into()),
            font_size: font.size,
            base_font_size: font.size,
            font_weight: parse_weight(&font.weight),
            font_features: font.features.iter().map(|s| s.clone().into()).collect(),

            cursor_shape: TerminalCursorShape::from_str(&cursor.shape),
            cursor_blink: if cursor.blink {
                TerminalBlink::On
            } else {
                TerminalBlink::Off
            },
            cursor_color: cursor.color.as_deref().and_then(parse_hex_color),

            line_height_factor: layout.line_height,
            cell_width: layout.cell_width,
            padding: TerminalPadding {
                top: layout.padding.top,
                right: layout.padding.right,
                bottom: layout.padding.bottom,
                left: layout.padding.left,
            },
            show_gutter: layout.show_gutter,
            semantic_highlighting: layout.semantic_highlighting,
            tab_title_mode: layout.tab_title,

            show_context_menu: mouse.show_context_menu,
            copy_on_select: mouse.copy_on_select,

            scroll_multiplier: scroll.multiplier,
            alternate_scroll: scroll.alternate_scroll,
            scrollback_history: scroll.scrollback_history,

            bell_enabled: bell.enabled,

            allow_clipboard_read: security.allow_clipboard_read,

            color_overrides: ColorOverrides {
                foreground: colors.foreground.as_deref().and_then(parse_hex_color),
                background: colors.background.as_deref().and_then(parse_hex_color),
                cursor: colors.cursor.as_deref().and_then(parse_hex_color),
                selection: colors.selection.as_deref().and_then(parse_hex_color),
                gutter_fg: colors.gutter_fg.as_deref().and_then(parse_hex_color),
                gutter_bg: colors.gutter_bg.as_deref().and_then(parse_hex_color),
                clock_fg: colors.clock_fg.as_deref().and_then(parse_hex_color),
                line_number_fg: colors.line_number_fg.as_deref().and_then(parse_hex_color),
                min_contrast: colors.min_contrast,
                ansi: parse_ansi_overrides(&colors.ansi),
            },

            completion: cfg.completion.clone(),
            logging: cfg.logging.clone(),
            sftp: cfg.sftp.clone(),

            persist_blocked: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_ansi_entry_keeps_its_slot_instead_of_shifting_the_palette() {
        let ansi: Vec<String> = ["#000000", "not-a-colour", "#00FF00", ""]
            .into_iter()
            .map(str::to_string)
            .collect();
        let parsed = parse_ansi_overrides(&ansi);
        assert_eq!(parsed.len(), 4);
        assert!(parsed[0].is_some());
        assert!(parsed[1].is_none(), "invalid entry keeps its position");
        assert_eq!(parsed[2], parse_hex_color("#00FF00"));
        assert!(parsed[3].is_none(), "blank entry means no override");
    }
}
