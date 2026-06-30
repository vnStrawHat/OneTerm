//! `TerminalSettings` — config shell toàn cục + rendering options.
//!
//! Config được load từ `terminal.json` (xem `terminal_config.rs`) tại init.
//! `TerminalSettingsPanel` cập nhật shell kind → notify.
//! Group A: cursor shape, cursor blink, font features, bell.
//!
//! Module gốc `terminal_settings.rs` đã được tách thành `terminal_settings/`.

use gpui::{App, AppContext, Entity, FontWeight, Global, Hsla, SharedString};
use oneterm_core::LocalShellConfig;

use crate::state::terminal_config::TerminalConfig;

pub(crate) mod apply;
pub(crate) mod color;
pub(crate) mod font;
pub(crate) mod mutators;

pub use color::parse_hex_color;
pub use font::{default_terminal_font_fallbacks, parse_weight};

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
    /// Có thể thay đổi runtime qua zoom shortcuts (Ctrl +/−/0).
    pub font_size: Option<f32>,
    /// Font size gốc từ config (terminal.json) — dùng cho Ctrl+0 reset zoom.
    /// Không bị zoom thay đổi.
    pub base_font_size: Option<f32>,
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
    /// Bật/tắt gutter (timestamp + line number bên trái terminal).
    pub show_gutter: bool,

    // ── Scroll ──
    /// Scroll multiplier cho mouse wheel.
    pub scroll_multiplier: f32,
    /// Alternate scroll mode (alt-screen → arrow keys).
    pub alternate_scroll: bool,
    /// Số dòng scrollback history tối đa (default 10000).
    /// Tổng dòng trong gutter = scrollback_history + viewport lines.
    pub scrollback_history: usize,

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
            base_font_size: None,
            font_weight: FontWeight::default(),
            font_features: Vec::new(),
            cursor_shape: TerminalCursorShape::Block,
            cursor_blink: TerminalBlink::On,
            cursor_color: None,
            line_height_factor: 1.2,
            cell_width: None,
            padding: TerminalPadding::default(),
            show_gutter: true,
            scroll_multiplier: 1.0,
            alternate_scroll: true,
            scrollback_history: 10_000,
            bell_enabled: true,
            color_overrides: ColorOverrides::default(),
        }
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
}
