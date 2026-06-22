//! `TerminalSettings` — config shell toàn cục (chọn qua settings panel).
//!
//! #20: shell picker. Entity dùng chung, `TerminalPanel` đọc khi spawn.
//! `TerminalSettingsPanel` cập nhật kind → notify.
//! Group A: cursor shape, cursor blink, font features, bell.

use gpui::{App, AppContext, Entity, Global, SharedString};
use myterm2_core::LocalShellConfig;
use myterm2_core::config::ShellKind;

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

/// Chế độ nhấp nháy con trỏ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TerminalBlink {
    /// Nhấp nháy khi focus (mặc định).
    #[default]
    On,
    /// Không nhấp nháy.
    Off,
}

/// Config terminal toàn cục (shell + rendering options).
pub struct TerminalSettings {
    pub shell: LocalShellConfig,
    /// Hình dáng con trỏ (Block/Bar/Underline).
    pub cursor_shape: TerminalCursorShape,
    /// Bật/tắt nhấp nháy con trỏ.
    pub cursor_blink: TerminalBlink,
    /// Font features cho terminal (ligatures, stylistic sets…).
    /// Vd: `["calt", "liga"]` → bật ligatures; `[]` → tắt.
    pub font_features: Vec<SharedString>,
    /// Font fallback stack — dùng khi primary mono font thiếu glyph
    /// (Unicode block/shade/box, ASCII art/TUI). Defaults tốt theo platform.
    pub font_fallbacks: Vec<SharedString>,
    /// Bật/tắt bell indicator (🔔 trong tab khi nhận `\x07`).
    pub bell_enabled: bool,
    /// Scroll multiplier cho mouse wheel (1.0 = default, 3.0 = nhanh 3x).
    pub scroll_multiplier: f32,
    /// Alternate scroll mode: trong alt-screen (vim/less/htop), mouse wheel
    /// gửi arrow keys thay vì scroll scrollback.
    pub alternate_scroll: bool,
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            shell: LocalShellConfig::default(),
            cursor_shape: TerminalCursorShape::Block,
            cursor_blink: TerminalBlink::On,
            font_features: Vec::new(),
            font_fallbacks: default_terminal_font_fallbacks(),
            bell_enabled: true,
            scroll_multiplier: 1.0,
            alternate_scroll: true,
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

    /// Khởi tạo global (gọi ở `ui::init`).
    pub fn init(cx: &mut App) {
        let entity = cx.new(|_| Self::default());
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
