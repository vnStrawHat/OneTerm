//! Apply color overrides từ config → `TerminalTheme`.

use super::super::theme::{TerminalTheme, vte_from_rgba};
use super::LocalTerminalView;
use crate::state::ColorOverrides;

impl LocalTerminalView {
    /// Apply color overrides từ config → theme.
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
