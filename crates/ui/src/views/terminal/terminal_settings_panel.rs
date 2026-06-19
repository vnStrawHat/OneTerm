//! `TerminalSettingsPanel` — dock panel chọn shell kind cho terminal.
//!
//! #20: shell picker (6 preset). Cập nhật `TerminalSettings` global →
//! `TerminalPanel` đọc khi spawn terminal mới. Custom program + font/scrollback
//! mở rộng sau.

use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled, Window, div,
};
use gpui_component::{
    ActiveTheme,
    dock::{Panel, PanelControl, PanelEvent},
};
use myterm2_core::LocalShellConfig;
use myterm2_core::config::ShellKind;

use crate::state::TerminalSettings;

/// Panel cài đặt terminal (chọn shell).
pub struct TerminalSettingsPanel {
    focus_handle: FocusHandle,
    settings: Entity<TerminalSettings>,
}

impl TerminalSettingsPanel {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            settings: TerminalSettings::global(cx),
        }
    }

    pub fn new_entity(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    fn kind_label(k: &ShellKind) -> &'static str {
        match k {
            ShellKind::Cmd => "cmd.exe (Windows)",
            ShellKind::PowerShell => "Windows PowerShell 5.x",
            ShellKind::Pwsh => "PowerShell 7+ (pwsh)",
            ShellKind::Bash => "Bash",
            ShellKind::Zsh => "Zsh",
            ShellKind::Sh => "Sh",
            ShellKind::Custom => "Custom",
        }
    }

    fn all_kinds() -> [ShellKind; 7] {
        [
            ShellKind::Cmd,
            ShellKind::PowerShell,
            ShellKind::Pwsh,
            ShellKind::Bash,
            ShellKind::Zsh,
            ShellKind::Sh,
            ShellKind::Custom,
        ]
    }
}

impl EventEmitter<PanelEvent> for TerminalSettingsPanel {}

impl Focusable for TerminalSettingsPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for TerminalSettingsPanel {
    fn panel_name(&self) -> &'static str {
        "terminal-settings"
    }

    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        "Terminal Settings"
    }

    fn closable(&self, _: &App) -> bool {
        true
    }

    fn zoomable(&self, _: &App) -> Option<PanelControl> {
        None
    }
}

impl Render for TerminalSettingsPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let current_kind = self.settings.read(cx).shell.kind;
        let settings = self.settings.clone();

        let mut list = div().flex().flex_col().gap_2().w_full();

        for kind in Self::all_kinds() {
            let label = Self::kind_label(&kind);
            let active = current_kind == kind;
            let settings = settings.clone();
            let bg = if active { theme.primary } else { theme.muted };
            let fg = if active {
                theme.primary_foreground
            } else {
                theme.foreground
            };
            list = list.child(
                div()
                    .id(label)
                    .px_3()
                    .py_2()
                    .rounded_lg()
                    .bg(bg)
                    .text_color(fg)
                    .text_sm()
                    .cursor_pointer()
                    .child(label)
                    .on_click(move |_e, _w, cx: &mut App| {
                        settings.update(cx, |s, cx| {
                            s.set_kind(kind);
                            cx.notify();
                        });
                    }),
            );
        }

        // Hint: program path chỉ áp dụng khi Custom (chỉnh qua file config sau).
        let hint = div().text_color(theme.muted_foreground).text_xs().child(
            "Chọn shell mặc định cho terminal mới. \
             Custom: đặt `program` qua config file.",
        );

        div()
            .id("terminal-settings-panel")
            .size_full()
            .track_focus(&self.focus_handle)
            .p_4()
            .flex()
            .flex_col()
            .gap_4()
            .child(div().text_color(theme.foreground).text_sm().child("Shell"))
            .child(list)
            .child(hint)
    }
}

/// Trả `LocalShellConfig` hiện tại từ settings global (dùng khi spawn).
pub fn current_shell_config(cx: &App) -> LocalShellConfig {
    TerminalSettings::global(cx).read(cx).shell.clone()
}

/// (dự phòng) đọc program path tuỳ chỉnh.
#[allow(dead_code)]
fn _custom_program(cfg: &LocalShellConfig) -> Option<String> {
    cfg.program
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
}
