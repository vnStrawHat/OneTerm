//! `TerminalSettings` — config shell toàn cục (chọn qua settings panel).
//!
//! #20: shell picker. Entity dùng chung, `TerminalPanel` đọc khi spawn.
//! `TerminalSettingsPanel` cập nhật kind → notify.

use gpui::{App, AppContext, Entity, Global};
use myterm2_core::LocalShellConfig;
use myterm2_core::config::ShellKind;

/// Config terminal toàn cục (shell + tùy chọn sau này: font, scrollback…).
pub struct TerminalSettings {
    pub shell: LocalShellConfig,
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            shell: LocalShellConfig::default(),
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
}
