//! Apply color overrides from config → `TerminalTheme`.

use super::super::theme::{TerminalTheme, hsla_from_vte, vte_from_rgba};
use super::LocalTerminalView;
use oneterm_settings::ColorOverrides;
use oneterm_terminal::DynamicColors;

/// Apply dynamic OSC-set colors (OSC 10/11/12 + OSC 4 palette) on top of
/// `theme`, so a program changing fg/bg/cursor/palette at runtime takes visual
/// effect. OSC-set colors win over the theme + static config overrides (they are
/// explicit runtime requests). OSC 104 clears an override back to `None`, which
/// makes resolution fall back to the theme automatically.
pub(crate) fn apply_dynamic_colors(mut theme: TerminalTheme, dc: &DynamicColors) -> TerminalTheme {
    if let Some(fg) = dc.foreground {
        theme.palette.foreground = fg;
        theme.fg = hsla_from_vte(fg);
    }
    if let Some(bg) = dc.background {
        theme.palette.background = bg;
        theme.bg = hsla_from_vte(bg);
        theme.gutter_bg = theme.bg;
    }
    if let Some(cursor) = dc.cursor {
        theme.palette.cursor = cursor;
    }
    // OSC 4 palette overrides (indices 0-255) — resolution consults these first.
    theme.palette.indexed = dc.indexed;
    theme
}

impl LocalTerminalView {
    /// Apply color overrides from config → theme.
    pub(crate) fn apply_color_overrides(
        &self,
        theme: TerminalTheme,
        co: &ColorOverrides,
    ) -> TerminalTheme {
        let _ = self;
        let mut t = theme;
        if let Some(fg) = co.foreground {
            t.fg = fg;
            t.palette.foreground = vte_from_rgba(fg.to_rgb());
        }
        if let Some(bg) = co.background {
            t.bg = bg;
            t.palette.background = vte_from_rgba(bg.to_rgb());
        }
        if let Some(c) = co.cursor {
            t.palette.cursor = vte_from_rgba(c.to_rgb());
        }
        if let Some(sel) = co.selection {
            t.selection = sel;
        }
        if let Some(gf) = co.gutter_fg {
            t.gutter_fg = gf;
            t.clock_fg = gf;
            t.line_number_fg = gf;
        }
        if let Some(gb) = co.gutter_bg {
            t.gutter_bg = gb;
        }
        if let Some(cf) = co.clock_fg {
            t.clock_fg = cf;
        }
        if let Some(lnf) = co.line_number_fg {
            t.line_number_fg = lnf;
        }
        t.min_contrast = co.min_contrast;
        for (i, &color) in co.ansi.iter().enumerate() {
            if i < 16 {
                t.palette.ansi[i] = vte_from_rgba(color.to_rgb());
            }
        }
        t
    }
}
