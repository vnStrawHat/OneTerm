//! Shell settings group.

use gpui::{App, SharedString};
use gpui_component::setting::{SettingField, SettingGroup, SettingItem};
use oneterm_core::config::ShellKind;
use oneterm_settings::TerminalSettings;

use super::set;

/// Shell presets shown in the dropdown, in the order it offers them.
///
/// The labels come from [`ShellKind::display_name`] — the one list the tab bar's
/// "+" menu rows and the tab labels also read — so the dropdown a user picks
/// their default shell from can no longer name it one thing while the tab it
/// produces names it another (`US-0114`, `F-114.1`). Until this list was folded
/// in, picking "cmd.exe (Windows)" here opened a tab reading "Command Prompt".
///
/// The label is the dropdown's key as well as its value, but it is **not**
/// persisted: the setter below maps the label back to a `ShellKind` and stores
/// the enum, so the wording is the widget's alone.
const SHELL_KINDS: &[ShellKind] = &[
    ShellKind::Cmd,
    ShellKind::PowerShell,
    ShellKind::Pwsh,
    ShellKind::Bash,
    ShellKind::Zsh,
    ShellKind::Sh,
    ShellKind::Custom,
];

fn shell_label(kind: ShellKind) -> SharedString {
    SharedString::from(kind.display_name())
}

/// Build the "Shell" settings group.
pub(super) fn group() -> SettingGroup {
    let options: Vec<(SharedString, SharedString)> = SHELL_KINDS
        .iter()
        .map(|kind| (shell_label(*kind), shell_label(*kind)))
        .collect();

    SettingGroup::new()
        .title("Shell")
        .description("Used by every new local terminal.")
        .items(vec![
            SettingItem::new(
                "Shell",
                SettingField::dropdown(
                    options,
                    |cx: &App| shell_label(TerminalSettings::global(cx).read(cx).shell.kind),
                    |val: SharedString, cx: &mut App| {
                        let kind = SHELL_KINDS
                            .iter()
                            .copied()
                            .find(|kind| kind.display_name() == val.as_ref())
                            .unwrap_or(ShellKind::Custom);
                        set(cx, |s| s.set_kind(kind));
                    },
                )
                .default_value(shell_label(
                    oneterm_core::config::LocalShellConfig::default().kind,
                )),
            ),
            SettingItem::new(
                "Custom Program",
                SettingField::input(
                    |cx: &App| {
                        TerminalSettings::global(cx)
                            .read(cx)
                            .shell
                            .program
                            .as_ref()
                            .map(|s| SharedString::from(s.to_string_lossy().to_string()))
                            .unwrap_or_default()
                    },
                    |val: SharedString, cx: &mut App| {
                        set(cx, |s| s.set_program(val.to_string()));
                    },
                )
                .default_value(SharedString::default()),
            )
            .description("Path to the shell executable. Custom only."),
        ])
}
