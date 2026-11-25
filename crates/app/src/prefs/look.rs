//! The settings of how terminals look (specs/oneterm, "Settings"): signals of the app's `Settings`
//! under `terminal.*`, and the view's `TermLook` they make.

use quadra::{Color, Settings, Signal};
use quadra_term::{TermColors, TermLook};

/// The weights of a face, from thin to black; the first is the usual one.
pub const WEIGHTS: [&str; 9] =
    ["Normal", "Thin", "Extra light", "Light", "Medium", "Semibold", "Bold", "Extra bold", "Black"];
const WEIGHT_OF: [u16; 9] = [400, 100, 200, 300, 500, 600, 700, 800, 900];
/// The colours a terminal may have over the theme's: their keys and names.
pub const COLORS: [(&str, &str); 8] = [
    ("terminal.colors.foreground", "Foreground"),
    ("terminal.colors.background", "Background"),
    ("terminal.colors.cursor", "Cursor"),
    ("terminal.colors.selection", "Selection"),
    ("terminal.colors.gutter_fg", "Gutter text"),
    ("terminal.colors.gutter_bg", "Gutter background"),
    ("terminal.colors.clock_fg", "Gutter clock"),
    ("terminal.colors.line_number_fg", "Gutter line number"),
];
/// What a colour of [`COLORS`] is at first: empty, the theme's, but the gutter's line number.
pub fn color_default(key: &str) -> &'static str {
    if key == "terminal.colors.line_number_fg" { "#2b7f99" } else { "" }
}

/// The sixteen ANSI colours: their keys and names, the normal eight then the bright.
pub const ANSI: [(&str, &str); 16] = [
    ("terminal.colors.ansi.black", "Black"),
    ("terminal.colors.ansi.red", "Red"),
    ("terminal.colors.ansi.green", "Green"),
    ("terminal.colors.ansi.yellow", "Yellow"),
    ("terminal.colors.ansi.blue", "Blue"),
    ("terminal.colors.ansi.magenta", "Magenta"),
    ("terminal.colors.ansi.cyan", "Cyan"),
    ("terminal.colors.ansi.white", "White"),
    ("terminal.colors.ansi.bright_black", "Bright black"),
    ("terminal.colors.ansi.bright_red", "Bright red"),
    ("terminal.colors.ansi.bright_green", "Bright green"),
    ("terminal.colors.ansi.bright_yellow", "Bright yellow"),
    ("terminal.colors.ansi.bright_blue", "Bright blue"),
    ("terminal.colors.ansi.bright_magenta", "Bright magenta"),
    ("terminal.colors.ansi.bright_cyan", "Bright cyan"),
    ("terminal.colors.ansi.bright_white", "Bright white"),
];
/// The room around the grid: each side's key, name and default.
pub const PADDING: [(&str, &str, f64); 4] = [
    ("terminal.padding.top", "Padding top", 5.0),
    ("terminal.padding.right", "Padding right", 0.0),
    ("terminal.padding.bottom", "Padding bottom", 0.0),
    ("terminal.padding.left", "Padding left", 5.0),
];
/// The fallback families at first: the symbols of patched fonts, which prompts draw with.
pub const FALLBACKS: &str = "Symbols Nerd Font Mono, Symbols Nerd Font";

/// The settings, as signals: writing one saves it.
#[derive(Clone, Copy)]
pub struct LookPrefs {
    pub font_size: Signal<f64>,
    /// A family of the system; empty, the kit's mono face.
    pub font_family: Signal<String>,
    /// Families separated by commas, tried for what the face lacks.
    pub fallbacks: Signal<String>,
    pub line_height: Signal<f64>,
    /// The terminal's weight, an index of `WEIGHTS`.
    pub weight: Signal<usize>,
    /// A cell's width in pixels; 0, the advance of `0`.
    pub cell_width: Signal<f64>,
    /// Top, right, bottom, left.
    pub padding: [Signal<f64>; 4],
    /// Colours over the theme's, as `#rrggbb`; empty, the theme's.
    pub colors: [Signal<String>; 8],
    pub ansi: [Signal<String>; 16],
    /// A WCAG ratio; 1 is off.
    pub min_contrast: Signal<f64>,
    /// The gutter: when each row was written and its line number.
    pub show_gutter: Signal<bool>,
    /// A word's cells shaped together, so the font's ligatures show.
    pub ligatures: Signal<bool>,
    /// Glyphs smoothed by pixel, not by ClearType's subpixels: no coloured fringes.
    pub grayscale: Signal<bool>,
    /// Text in the default colour coloured by what it means.
    pub semantic_highlighting: Signal<bool>,
}

impl LookPrefs {
    /// The settings of `s`.
    pub fn of(s: &Settings) -> LookPrefs {
        LookPrefs {
            font_size: s.value("terminal.font_size", 15.0),
            font_family: s.value("terminal.font_family", String::new()),
            fallbacks: s.value("terminal.font_fallbacks", FALLBACKS.to_string()),
            line_height: s.value("terminal.line_height", 1.2),
            weight: s.choice("terminal.font_weight", &WEIGHTS),
            cell_width: s.value("terminal.cell_width", 0.0),
            padding: PADDING.map(|(key, _, default)| s.value(key, default)),
            colors: COLORS.map(|(key, _)| s.value(key, color_default(key).to_string())),
            ansi: ANSI.map(|(key, _)| s.value(key, String::new())),
            min_contrast: s.value("terminal.colors.min_contrast", 4.5),
            show_gutter: s.value("terminal.show_gutter", false),
            ligatures: s.value("terminal.ligatures", true),
            grayscale: s.value("terminal.grayscale", true),
            semantic_highlighting: s.value("terminal.semantic_highlighting", true),
        }
    }

    /// How a terminal looks, read where it is called.
    pub fn look(&self) -> TermLook {
        let hex = |it: &Signal<String>| it.with(|words| Color::parse_hex(words.trim()));
        let [foreground, background, cursor, selection, gutter_fg, gutter_bg, clock_fg, line_number_fg] =
            self.colors.each_ref().map(hex);
        let [top, right, bottom, left] = self.padding.map(|it| it.get().clamp(0.0, 64.0) as f32);
        let cell = self.cell_width.get();
        let fallbacks = self.fallbacks.with(|words| {
            words.split(',').map(str::trim).filter(|it| !it.is_empty()).map(String::from).collect()
        });
        TermLook {
            family: self.font_family.get(),
            fallbacks,
            size: self.font_size.get().clamp(6.0, 48.0) as f32,
            line_height: self.line_height.get().clamp(1.0, 2.5) as f32,
            weight: WEIGHT_OF[self.weight.get().min(8)],
            cell_width: (cell > 0.0).then(|| cell.clamp(1.0, 48.0) as f32),
            padding: quadra::Edges { top, right, bottom, left },
            min_contrast: self.min_contrast.get().clamp(1.0, 21.0) as f32,
            gutter: self.show_gutter.get(),
            ligatures: self.ligatures.get(),
            grayscale: self.grayscale.get(),
            highlighting: self.semantic_highlighting.get(),
            colors: TermColors {
                foreground,
                background,
                cursor,
                selection,
                ansi: self.ansi.each_ref().map(hex),
                gutter_fg,
                gutter_bg,
                clock_fg,
                line_number_fg,
                // The classes take the colour theme's ANSI colours.
                highlights: Default::default(),
            },
        }
    }
}
