//! Mutator/setter helpers cho `TerminalSettings`.

use gpui::SharedString;

use oneterm_core::config::ShellKind;

use super::{TerminalBlink, TerminalCursorShape, TerminalSettings};

/// Bước zoom (px) mỗi lần nhấn Ctrl++/Ctrl+−.
const ZOOM_STEP: f32 = 1.0;
/// Font size tối đa/tối thiểu (px).
const ZOOM_MIN: f32 = 6.0;
const ZOOM_MAX: f32 = 100.0;

impl TerminalSettings {
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

    /// Zoom in — tăng font_size lên `ZOOM_STEP` px (tối đa `ZOOM_MAX`).
    /// Nếu `font_size` đang `None` (dùng theme default), khởi tạo từ `theme_default`.
    pub fn zoom_in(&mut self, theme_default: f32) {
        let current = self.font_size.unwrap_or(theme_default);
        self.font_size = Some((current + ZOOM_STEP).min(ZOOM_MAX));
    }

    /// Zoom out — giảm font_size xuống `ZOOM_STEP` px (tối thiểu `ZOOM_MIN`).
    /// Nếu `font_size` đang `None` (dùng theme default), khởi tạo từ `theme_default`.
    pub fn zoom_out(&mut self, theme_default: f32) {
        let current = self.font_size.unwrap_or(theme_default);
        self.font_size = Some((current - ZOOM_STEP).max(ZOOM_MIN));
    }

    /// Reset zoom — đặt `font_size` về `base_font_size` (giá trị gốc từ config).
    pub fn reset_zoom(&mut self) {
        self.font_size = self.base_font_size;
    }
}
