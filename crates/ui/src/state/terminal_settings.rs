//! `TerminalSettings` — config shell toàn cục + rendering options.
//!
//! Config được load từ `terminal.json` (xem `terminal_config.rs`) tại init.
//! `TerminalSettingsPanel` cập nhật shell kind → notify.
//! Group A: cursor shape, cursor blink, font features, bell.

use gpui::{App, AppContext, Entity, Global, Hsla, FontWeight, SharedString};
use myterm2_core::LocalShellConfig;
use myterm2_core::config::ShellKind;

use crate::state::terminal_config::{
    BellConfig, ColorsConfig, CursorConfig, FontConfig, LayoutConfig,
    ScrollConfig, TerminalConfig,
};

/// Platform-specific default font fallback stack for the terminal. These
/// families are tried (in order) when the primary mono font is missing a
/// glyph. They are all monospace fonts with good Unicode block/shade/box
/// coverage, so ASCII art / TUI glyphs render consistently.
fn default_terminal_font_fallbacks() -> Vec<SharedString> {
    #[cfg(target_os = "windows")]
    {
        vec![
            "Cascadia Mono".into(),
            "Cascadia Code".into(),
            "DejaVu Sans Mono".into(),
            "Lucida Console".into(),
            "Courier New".into(),
            "MS Gothic".into(),
            "NSimSun".into(),
        ]
    }
    #[cfg(target_os = "macos")]
    {
        vec![
            "Menlo".into(),
            "Monaco".into(),
            "Courier New".into(),
            "Apple Symbols".into(),
        ]
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        vec![
            "DejaVu Sans Mono".into(),
            "Noto Sans Mono".into(),
            "Ubuntu Mono".into(),
            "Liberation Mono".into(),
            "Courier New".into(),
        ]
    }
}

/// Hình dáng con trỏ terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TerminalCursorShape {
    /// Block đầy — mặc định.
    #[default]
    Block,
    /// Thanh dọc hẹp `│`.
    Bar,
    /// Gạch dưới `_`.
    Underline,
}

impl TerminalCursorShape {
    /// Parse từ string ("block" | "bar" | "underline").
    fn from_str(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "bar" => Self::Bar,
            "underline" => Self::Underline,
            _ => Self::Block,
        }
    }
}

/// Chế độ nhấp nháy con trỏ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TerminalBlink {
    /// Nhấp nháy khi focus (mặc định).
    #[default]
    On,
    /// Không nhấp nháy.
    Off,
}

/// Padding 4 phía cho terminal content (px).
#[derive(Debug, Clone, Copy, Default)]
pub struct TerminalPadding {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

/// Color overrides — nếu `Some` thì ghi đè theme, `None` = dùng theme.
#[derive(Debug, Clone, Default)]
pub struct ColorOverrides {
    pub foreground: Option<Hsla>,
    pub background: Option<Hsla>,
    pub cursor: Option<Hsla>,
    pub selection: Option<Hsla>,
    pub gutter_fg: Option<Hsla>,
    pub gutter_bg: Option<Hsla>,
    pub clock_fg: Option<Hsla>,
    pub line_number_fg: Option<Hsla>,
    pub min_contrast: f32,
    pub ansi: Vec<Hsla>,
}

/// Config terminal toàn cục (shell + rendering options).
/// Load từ `terminal.json` lúc init.
pub struct TerminalSettings {
    // ── Shell ──
    pub shell: LocalShellConfig,

    // ── Font ──
    /// Font family chính (None = dùng theme mono font).
    pub font_family: Option<SharedString>,
    /// Font fallback stack.
    pub font_fallbacks: Vec<SharedString>,
    /// Font size in px (None = dùng theme mono font size).
    pub font_size: Option<f32>,
    /// Font weight.
    pub font_weight: FontWeight,
    /// Font features (OpenType): vd ["calt", "liga"].
    pub font_features: Vec<SharedString>,

    // ── Cursor ──
    /// Hình dáng con trỏ (Block/Bar/Underline).
    pub cursor_shape: TerminalCursorShape,
    /// Bật/tắt nhấp nháy con trỏ.
    pub cursor_blink: TerminalBlink,
    /// Màu con trỏ (None = theme caret).
    pub cursor_color: Option<Hsla>,

    // ── Layout ──
    /// Line height multiplier (1.2 = 120% font size).
    pub line_height_factor: f32,
    /// Cell width override in px (None = auto từ font advance).
    pub cell_width: Option<f32>,
    /// Padding quanh terminal content.
    pub padding: TerminalPadding,

    // ── Scroll ──
    /// Scroll multiplier cho mouse wheel.
    pub scroll_multiplier: f32,
    /// Alternate scroll mode (alt-screen → arrow keys).
    pub alternate_scroll: bool,

    // ── Bell ──
    /// Bật/tắt bell indicator.
    pub bell_enabled: bool,

    // ── Colors ──
    /// Color overrides cho terminal theme.
    pub color_overrides: ColorOverrides,
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            shell: LocalShellConfig::default(),
            font_family: None,
            font_fallbacks: default_terminal_font_fallbacks(),
            font_size: None,
            font_weight: FontWeight::default(),
            font_features: Vec::new(),
            cursor_shape: TerminalCursorShape::Block,
            cursor_blink: TerminalBlink::On,
            cursor_color: None,
            line_height_factor: 1.2,
            cell_width: None,
            padding: TerminalPadding::default(),
            scroll_multiplier: 1.0,
            alternate_scroll: true,
            bell_enabled: true,
            color_overrides: ColorOverrides::default(),
        }
    }
}

/// Parse "#RRGGBB" → Hsla. Trả None nếu parse fail.
fn parse_hex_color(s: &str) -> Option<Hsla> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some(gpui::Rgba {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
    .into())
}

/// Parse font weight từ string → FontWeight.
fn parse_weight(s: &str) -> FontWeight {
    match s.to_ascii_lowercase().as_str() {
        "thin" => FontWeight::THIN,
        "extra_light" | "extralight" => FontWeight::EXTRA_LIGHT,
        "light" => FontWeight::LIGHT,
        "normal" | "regular" => FontWeight::NORMAL,
        "medium" => FontWeight::MEDIUM,
        "semibold" => FontWeight::SEMIBOLD,
        "bold" => FontWeight::BOLD,
        "extra_bold" | "extrabold" => FontWeight::EXTRA_BOLD,
        "black" => FontWeight::BLACK,
        _ => FontWeight::default(),
    }
}

impl TerminalSettings {
    /// Apply config từ `terminal.json` vào settings.
    fn apply_config(&mut self, cfg: &TerminalConfig) {
        // ── Font ──
        let font: &FontConfig = &cfg.font;
        if font.family.is_some() {
            self.font_family = font.family.as_ref().map(|s| s.clone().into());
        }
        if !font.fallback_fonts.is_empty() {
            self.font_fallbacks = font.fallback_fonts.iter().map(|s| s.clone().into()).collect();
        }
        if font.size.is_some() {
            self.font_size = font.size;
        }
        self.font_weight = parse_weight(&font.weight);
        self.font_features = font.features.iter().map(|s| s.clone().into()).collect();

        // ── Cursor ──
        let cursor: &CursorConfig = &cfg.cursor;
        self.cursor_shape = TerminalCursorShape::from_str(&cursor.shape);
        self.cursor_blink = if cursor.blink { TerminalBlink::On } else { TerminalBlink::Off };
        self.cursor_color = cursor.color.as_deref().and_then(parse_hex_color);

        // ── Layout ──
        let layout: &LayoutConfig = &cfg.layout;
        self.line_height_factor = layout.line_height;
        self.cell_width = layout.cell_width;
        self.padding = TerminalPadding {
            top: layout.padding.top,
            right: layout.padding.right,
            bottom: layout.padding.bottom,
            left: layout.padding.left,
        };

        // ── Shell ──
        self.shell = cfg.shell.clone();

        // ── Scroll ──
        let scroll: &ScrollConfig = &cfg.scroll;
        self.scroll_multiplier = scroll.multiplier;
        self.alternate_scroll = scroll.alternate_scroll;

        // ── Bell ──
        let bell: &BellConfig = &cfg.bell;
        self.bell_enabled = bell.enabled;

        // ── Colors ──
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
            ansi: colors.ansi.iter().filter_map(|s| parse_hex_color(s)).collect(),
        };
    }
}

/// Global wrapper (pattern như `AppStateGlobal`).
pub struct TerminalSettingsGlobal(pub Entity<TerminalSettings>);

impl Global for TerminalSettingsGlobal {}

impl TerminalSettings {
    /// `Entity<TerminalSettings>` toàn cục (panic nếu chưa init).
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<TerminalSettingsGlobal>().0.clone()
    }

    /// Khởi tạo global — load `terminal.json` và apply (gọi ở `ui::init`).
    pub fn init(cx: &mut App) {
        let cfg = TerminalConfig::load();
        let entity = cx.new(|_| {
            let mut settings = Self::default();
            settings.apply_config(&cfg);
            settings
        });
        cx.set_global(TerminalSettingsGlobal(entity));
    }

    /// Đặt shell kind (reset program tự detect).
    pub fn set_kind(&mut self, kind: ShellKind) {
        self.shell.kind = kind;
        self.shell.program = None;
    }

    /// Đặt đường dẫn program tùy chỉnh (Custom).
    pub fn set_program(&mut self, program: String) {
        self.shell.program = if program.trim().is_empty() {
            None
        } else {
            Some(program.into())
        };
    }

    /// Đặt hình dáng con trỏ.
    pub fn set_cursor_shape(&mut self, shape: TerminalCursorShape) {
        self.cursor_shape = shape;
    }

    /// Đặt chế độ nhấp nháy.
    pub fn set_cursor_blink(&mut self, blink: TerminalBlink) {
        self.cursor_blink = blink;
    }

    /// Đặt font features.
    pub fn set_font_features(&mut self, features: Vec<SharedString>) {
        self.font_features = features;
    }
}