//! Mutator/setter helpers cho `TerminalSettings`.

use gpui::SharedString;

use myterm2_core::config::ShellKind;

use super::{TerminalBlink, TerminalCursorShape, TerminalSettings};

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
}
