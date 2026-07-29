//! Shell settings group.

use gpui::{App, SharedString};
use gpui_component::setting::{SettingField, SettingGroup, SettingItem};
use oneterm_core::config::ShellKind;
use oneterm_settings::TerminalSettings;

use crate::items_with_separators;

use super::persist;

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

/// Build the "Shell" settings group.
pub(super) fn group() -> SettingGroup {
    let options: Vec<(SharedString, SharedString)> = SHELL_KINDS
        .iter()
        .map(|(_, label)| (SharedString::from(*label), SharedString::from(*label)))
        .collect();

    SettingGroup::new()
        .title("Shell")
        .description("Shell for new local terminals.")
        .items(items_with_separators(vec![
            SettingItem::new(
                "Shell",
                SettingField::dropdown(
                    options,
                    |cx: &App| {
                        let kind = TerminalSettings::global(cx).read(cx).shell.kind;
                        SHELL_KINDS
                            .iter()
                            .find(|(k, _)| *k == kind)
                            .map(|(_, label)| SharedString::from(*label))
                            .unwrap_or_else(|| "Custom".into())
                    },
                    |val: SharedString, cx: &mut App| {
                        let kind = SHELL_KINDS
                            .iter()
                            .find(|(_, label)| *label == val.as_ref())
                            .map(|(k, _)| *k)
                            .unwrap_or(ShellKind::Custom);
                        TerminalSettings::global(cx).update(cx, |s, cx| {
                            s.set_kind(kind);
                            cx.notify();
                        });
                        persist(cx);
                    },
                ),
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
                        TerminalSettings::global(cx).update(cx, |s, cx| {
                            s.set_program(val.to_string());
                            cx.notify();
                        });
                        persist(cx);
                    },
                ),
            )
            .description("Custom shell path."),
        ]))
}
