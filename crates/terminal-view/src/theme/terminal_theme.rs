//! The [`TerminalTheme`] value, the [`build_terminal_theme`] builder that maps
//! the gpui-component active `Theme` into a prebuilt palette + resolved colors,
//! and the two override passes applied on top of it each frame
//! ([`apply_color_overrides`] from settings, [`apply_dynamic_colors`] from OSC).

use gpui::Hsla;
use gpui_component::Theme;

use oneterm_highlight::ClassStyles;
use oneterm_settings::ColorOverrides;
use oneterm_terminal::{DynamicColors, TerminalPalette};

use super::contrast::ensure_minimum_contrast;
use super::palette::{self, ColorTable, hsla_from_rgb, rgb_from_rgba};
use crate::highlight::{load_default_styles, to_gpui_hsla};
use crate::render::frame::Color;

/// WCAG AA threshold used when settings do not choose one (deviation 6).
pub(crate) const DEFAULT_MIN_CONTRAST: f32 = 4.5;

/// How far the prompt-line band moves from the terminal background toward the
/// side the terminal foreground is on (`US-0134`).
///
/// A band defined as "the background, slightly toward the text" is correct on a
/// light theme and on a dark one by construction: it can never be a dark stripe
/// under dark text, because it is always on the background's side of the pair.
/// Only the lightness moves, so the band never introduces a hue of its own.
///
/// The **direction** comes from the foreground and the **distance** is this
/// fixed fraction of the room left in that direction, rather than a fraction of
/// the gap between the two. A theme whose foreground and background are close in
/// lightness would otherwise get a band nobody can see, and the direction is the
/// one bit of the foreground that is robust.
const PROMPT_BAND_MIX: f32 = 0.11;

/// Terminal theme with a prebuilt palette + bg/fg (Hsla) + contrast threshold.
#[derive(Clone)]
pub(crate) struct TerminalTheme {
    pub palette: TerminalPalette,
    /// Default background (theme background) — for painting the element background.
    pub bg: Hsla,
    /// Default FG (theme foreground).
    pub fg: Hsla,
    /// Selection background color (highlights the selected text).
    pub selection: Hsla,
    /// Minimum contrast threshold (WCAG, default 4.5 ≈ AA).
    pub min_contrast: f32,
    /// Gutter text color (timestamp + line number). Default = dim fg (50% lightness).
    pub gutter_fg: Hsla,
    /// Gutter background color. Default = same as terminal background.
    pub gutter_bg: Hsla,
    /// Clock text color [HH:MM:SS]. Default = gutter_fg.
    pub clock_fg: Hsla,
    /// Line number text color. Default = gutter_fg.
    pub line_number_fg: Hsla,
    /// Search match highlight (non-active). Semi-transparent accent.
    pub search_match: Hsla,
    /// Active search match highlight (the current next/prev target).
    pub search_active: Hsla,
    /// Pre-resolved semantic class styles (Layer 2 — see `highlight`).
    /// Populated from the default semantic asset (parsed once per process);
    /// themes without a `terminal.semantic` block → all `None` → Layer 2 is a
    /// no-op.
    pub class_styles: &'static ClassStyles,
    /// `Color -> Hsla` lookup for the current palette; rebuilt by every builder
    /// and override pass below so it never lags the palette.
    pub colors: ColorTable,
}

impl TerminalTheme {
    /// Resolve a cell color through the palette in O(1).
    #[inline]
    pub(crate) fn color(&self, c: Color) -> Hsla {
        self.colors.color(c)
    }

    /// Push `fg` away from `bg` until the theme's minimum contrast is met.
    pub(crate) fn ensure_contrast(&self, fg: Hsla, bg: Hsla) -> Hsla {
        ensure_minimum_contrast(fg, bg, self.min_contrast)
    }

    /// The prompt-line background band (§8 item 6): this theme's terminal
    /// background nudged toward its foreground.
    ///
    /// Resolved per theme rather than taken from one fixed value in
    /// `assets/highlight/default.json`, which is what made it a dark band under
    /// dark text on a light theme. The asset (and any future per-theme
    /// `terminal.semantic.promptLineBg`) stays an **explicit override**: a theme
    /// that names a colour gets exactly that colour, and a theme that says
    /// nothing still gets something sane, which is §7's rule for the whole
    /// block.
    ///
    /// `reverse_video` is `DECSCNM`: the two defaults trade places, so the band
    /// is derived from the pair the screen is actually drawn with.
    pub(crate) fn prompt_line_bg(&self, reverse_video: bool) -> Hsla {
        if let Some(override_color) = self.class_styles.prompt_line_bg {
            return to_gpui_hsla(override_color);
        }
        let (bg, fg) = if reverse_video {
            (self.fg, self.bg)
        } else {
            (self.bg, self.fg)
        };
        // Which side of the background the text sits on; the band moves that way.
        let toward = if fg.l >= bg.l { 1.0 } else { 0.0 };
        Hsla {
            l: (bg.l + (toward - bg.l) * PROMPT_BAND_MIX).clamp(0.0, 1.0),
            a: 1.0,
            ..bg
        }
    }
}

/// Build a `TerminalTheme` from the gpui-component active `Theme`.
pub(crate) fn build_terminal_theme(theme: &Theme) -> TerminalTheme {
    let c = &theme.colors;
    let fg = c.foreground;
    let bg = c.background;
    // cursor = caret color (fallback foreground).
    let cursor_rgba = if c.caret.a > 0.0 {
        c.caret.to_rgb()
    } else {
        c.foreground.to_rgb()
    };
    let palette = TerminalPalette {
        foreground: palette::rgb_from_rgba(c.foreground.to_rgb()),
        background: palette::rgb_from_rgba(c.background.to_rgb()),
        cursor: palette::rgb_from_rgba(cursor_rgba),
        ansi: palette::ANSI_16,
        indexed: [None; 256],
    };
    TerminalTheme {
        colors: ColorTable::from_palette(&palette),
        palette,
        bg,
        fg,
        selection: if bg.l < 0.5 {
            gpui::hsla(0.589, 0.69, 0.165, 1.0) // #0d2847 dark blue
        } else {
            gpui::hsla(0.569, 0.92, 0.949, 1.0) // #e6f4fe light blue
        },
        min_contrast: DEFAULT_MIN_CONTRAST,
        gutter_fg: gpui::hsla(fg.h, fg.s, fg.l * 0.5, fg.a),
        gutter_bg: bg,
        clock_fg: gpui::hsla(fg.h, fg.s, fg.l * 0.5, fg.a),
        line_number_fg: gpui::hsla(fg.h, fg.s, fg.l * 0.5, fg.a),
        // Search highlights: yellow-ish, semi-transparent for non-active,
        // more opaque for the active match. Tuned for both light/dark backgrounds.
        search_match: gpui::hsla(0.13, 0.85, 0.5, 0.35),
        search_active: gpui::hsla(0.13, 0.9, 0.55, 0.7),
        class_styles: load_default_styles(),
    }
}

/// Apply the static colour overrides from settings on top of the theme.
pub(crate) fn apply_color_overrides(theme: TerminalTheme, co: &ColorOverrides) -> TerminalTheme {
    let mut t = theme;
    if let Some(fg) = co.foreground {
        t.fg = fg;
        t.palette.foreground = rgb_from_rgba(fg.to_rgb());
    }
    if let Some(bg) = co.background {
        t.bg = bg;
        t.palette.background = rgb_from_rgba(bg.to_rgb());
    }
    if let Some(c) = co.cursor {
        t.palette.cursor = rgb_from_rgba(c.to_rgb());
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
    // Deviation 6: an unset / non-positive setting keeps the theme default
    // instead of silently disabling enforcement; `0 < v <= 1` is the explicit
    // "off" (a ratio of 1 is always met); anything above is the threshold.
    if co.min_contrast > 0.0 {
        t.min_contrast = co.min_contrast;
    }
    for (i, color) in co.ansi.iter().enumerate().take(16) {
        if let Some(color) = color {
            t.palette.ansi[i] = rgb_from_rgba(color.to_rgb());
        }
    }
    t.colors = ColorTable::from_palette(&t.palette);
    t
}

/// Apply dynamic OSC-set colors (OSC 10/11/12 + OSC 4 palette) on top of
/// `theme`, so a program changing fg/bg/cursor/palette at runtime takes visual
/// effect. OSC-set colors win over the theme + static config overrides (they are
/// explicit runtime requests). OSC 104 clears an override back to `None`, which
/// makes resolution fall back to the theme automatically.
pub(crate) fn apply_dynamic_colors(mut theme: TerminalTheme, dc: &DynamicColors) -> TerminalTheme {
    if let Some(fg) = dc.foreground {
        theme.palette.foreground = fg;
        theme.fg = hsla_from_rgb(fg);
    }
    if let Some(bg) = dc.background {
        theme.palette.background = bg;
        theme.bg = hsla_from_rgb(bg);
        theme.gutter_bg = theme.bg;
    }
    if let Some(cursor) = dc.cursor {
        theme.palette.cursor = cursor;
    }
    // OSC 4 palette overrides (indices 0-255) — resolution consults these first.
    theme.palette.indexed = dc.indexed;
    theme.colors = ColorTable::from_palette(&theme.palette);
    theme
}
