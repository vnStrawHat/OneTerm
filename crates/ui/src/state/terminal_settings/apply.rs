//! Apply `TerminalConfig` → `TerminalSettings`.

use crate::state::terminal_config::{
    BellConfig, ColorsConfig, CursorConfig, FontConfig, LayoutConfig, ScrollConfig, SecurityConfig,
    TerminalConfig,
};

use super::{
    ColorOverrides, TerminalBlink, TerminalCursorShape, TerminalPadding, TerminalSettings,
    parse_hex_color, parse_weight,
};

impl TerminalSettings {
    /// Apply config from `terminal.json` into the settings.
    pub(crate) fn apply_config(&mut self, cfg: &TerminalConfig) {
        let font: &FontConfig = &cfg.font;
        if font.family.is_some() {
            self.font_family = font.family.as_ref().map(|s| s.clone().into());
        }
        if font.size.is_some() {
            self.font_size = font.size;
            self.base_font_size = font.size;
        }
        self.font_weight = parse_weight(&font.weight);
        self.font_features = font.features.iter().map(|s| s.clone().into()).collect();

        let cursor: &CursorConfig = &cfg.cursor;
        self.cursor_shape = TerminalCursorShape::from_str(&cursor.shape);
        self.cursor_blink = if cursor.blink {
            TerminalBlink::On
        } else {
            TerminalBlink::Off
        };
        self.cursor_color = cursor.color.as_deref().and_then(parse_hex_color);

        let layout: &LayoutConfig = &cfg.layout;
        self.line_height_factor = layout.line_height;
        self.cell_width = layout.cell_width;
        self.padding = TerminalPadding {
            top: layout.padding.top,
            right: layout.padding.right,
            bottom: layout.padding.bottom,
            left: layout.padding.left,
        };
        self.show_gutter = layout.show_gutter;
        self.semantic_highlighting = layout.semantic_highlighting;
        self.auto_hide_right_dock_on_local = layout.auto_hide_right_dock_on_local;
        self.tab_title_mode = layout.tab_title;

        self.shell = cfg.shell.clone();

        let scroll: &ScrollConfig = &cfg.scroll;
        self.scroll_multiplier = scroll.multiplier;
        self.alternate_scroll = scroll.alternate_scroll;
        self.scrollback_history = scroll.scrollback_history;

        let bell: &BellConfig = &cfg.bell;
        self.bell_enabled = bell.enabled;

        let security: &SecurityConfig = &cfg.security;
        self.allow_clipboard_read = security.allow_clipboard_read;

        let colors: &ColorsConfig = &cfg.colors;
        self.color_overrides = ColorOverrides {
            foreground: colors.foreground.as_deref().and_then(parse_hex_color),
            background: colors.background.as_deref().and_then(parse_hex_color),
            cursor: colors.cursor.as_deref().and_then(parse_hex_color),
            selection: colors.selection.as_deref().and_then(parse_hex_color),
            gutter_fg: colors.gutter_fg.as_deref().and_then(parse_hex_color),
            gutter_bg: colors.gutter_bg.as_deref().and_then(parse_hex_color),
            clock_fg: colors.clock_fg.as_deref().and_then(parse_hex_color),
            line_number_fg: colors.line_number_fg.as_deref().and_then(parse_hex_color),
            min_contrast: colors.min_contrast,
            ansi: colors
                .ansi
                .iter()
                .filter_map(|s| parse_hex_color(s))
                .collect(),
        };
    }
}
