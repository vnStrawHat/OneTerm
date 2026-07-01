//! Cursor blink / visibility logic.

use crate::state::{TerminalBlink, TerminalSettings};

impl super::LocalTerminalView {
    /// Decide whether to draw the cursor (blink logic).
    /// - Not focused → always draw.
    /// - Focused + blink off → always draw.
    /// - Focused + blink on → draw when `cursor_blink_visible`.
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
