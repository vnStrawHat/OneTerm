//! Cursor blink / visibility logic.

use crate::state::{TerminalBlink, TerminalSettings};

impl super::LocalTerminalView {
    /// Quyết định có vẽ cursor không (blink logic).
    /// - Không focus → luôn vẽ.
    /// - Focus + blink off → luôn vẽ.
    /// - Focus + blink on → vẽ khi `cursor_blink_visible`.
    pub(crate) fn should_show_cursor(&self, focused: bool, settings: &TerminalSettings) -> bool {
        if !focused {
            return true;
        }
        match settings.cursor_blink {
            TerminalBlink::Off => true,
            TerminalBlink::On => self.cursor_blink_visible,
        }
    }
}
