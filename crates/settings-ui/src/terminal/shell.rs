//! Shell settings group.

use gpui::{App, SharedString};
use gpui_component::setting::{SettingField, SettingGroup, SettingItem};
use oneterm_core::config::ShellKind;
use oneterm_settings::TerminalSettings;

use super::set;

/// Shell presets shown in the dropdown (label is used as both key and value).
const SHELL_KINDS: &[(ShellKind, &str)] = &[
    (ShellKind::Cmd, "cmd.exe (Windows)"),
    (ShellKind::PowerShell, "Windows PowerShell 5.x"),
    (ShellKind::Pwsh, "PowerShell 7+ (pwsh)"),
    (ShellKind::Bash, "Bash"),
    (ShellKind::Zsh, "Zsh"),
    (ShellKind::Sh, "Sh"),
    (ShellKind::Custom, "Custom"),
];

fn shell_label(kind: ShellKind) -> SharedString {
    SHELL_KINDS
        .iter()
        .find(|(candidate, _)| *candidate == kind)
        .map(|(_, label)| SharedString::from(*label))
        .unwrap_or_else(|| "Custom".into())
}

/// Build the "Shell" settings group.
pub(super) fn group() -> SettingGroup {
    let options: Vec<(SharedString, SharedString)> = SHELL_KINDS
        .iter()
        .map(|(_, label)| (SharedString::from(*label), SharedString::from(*label)))
        .collect();

    SettingGroup::new()
        .title("Shell")
        .description("Shell for new local terminals.")
        .items(vec![
            SettingItem::new(
                "Shell",
                SettingField::dropdown(
                    options,
                    |cx: &App| shell_label(TerminalSettings::global(cx).read(cx).shell.kind),
                    |val: SharedString, cx: &mut App| {
                        let kind = SHELL_KINDS
                            .iter()
                            .find(|(_, label)| *label == val.as_ref())
                            .map(|(k, _)| *k)
                            .unwrap_or(ShellKind::Custom);
                        set(cx, |s| s.set_kind(kind));
                    },
                )
                .default_value(shell_label(
                    oneterm_core::config::LocalShellConfig::default().kind,
                )),
            )
            .description("Choose shell kind."),
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
            .description("Custom shell path."),
        ])
}
