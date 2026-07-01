//! Mutator/setter helpers for `TerminalSettings`.

use gpui::SharedString;

use oneterm_core::config::ShellKind;

use super::{TerminalBlink, TerminalCursorShape, TerminalSettings};

/// Zoom step (px) per Ctrl++/Ctrl+− press.
const ZOOM_STEP: f32 = 1.0;
/// Max/min font size (px).
const ZOOM_MIN: f32 = 6.0;
const ZOOM_MAX: f32 = 100.0;

impl TerminalSettings {
    /// Set the shell kind (resets the auto-detected program).
    pub fn set_kind(&mut self, kind: ShellKind) {
        self.shell.kind = kind;
        self.shell.program = None;
    }

    /// Set a custom program path (Custom).
    pub fn set_program(&mut self, program: String) {
        self.shell.program = if program.trim().is_empty() {
            None
        } else {
            Some(program.into())
        };
    }

    /// Set the cursor shape.
    pub fn set_cursor_shape(&mut self, shape: TerminalCursorShape) {
        self.cursor_shape = shape;
    }

    /// Set the blink mode.
    pub fn set_cursor_blink(&mut self, blink: TerminalBlink) {
        self.cursor_blink = blink;
    }

    /// Set the font features.
    pub fn set_font_features(&mut self, features: Vec<SharedString>) {
        self.font_features = features;
    }

    /// Zoom in — increase font_size by `ZOOM_STEP` px (up to `ZOOM_MAX`).
    /// If `font_size` is currently `None` (using the theme default), initialize from `theme_default`.
    pub fn zoom_in(&mut self, theme_default: f32) {
        let current = self.font_size.unwrap_or(theme_default);
        self.font_size = Some((current + ZOOM_STEP).min(ZOOM_MAX));
    }

    /// Zoom out — decrease font_size by `ZOOM_STEP` px (down to `ZOOM_MIN`).
    /// If `font_size` is currently `None` (using the theme default), initialize from `theme_default`.
    pub fn zoom_out(&mut self, theme_default: f32) {
        let current = self.font_size.unwrap_or(theme_default);
        self.font_size = Some((current - ZOOM_STEP).max(ZOOM_MIN));
    }

    /// Reset zoom — set `font_size` back to `base_font_size` (the original config value).
    pub fn reset_zoom(&mut self) {
        self.font_size = self.base_font_size;
    }
}
