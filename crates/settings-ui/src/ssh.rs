//! SSH settings page — connection liveness for newly opened SSH sessions plus
//! the SFTP browser's editor workflow (IN-0021 merged the former "SFTP" page
//! into this one).
//!
//! Reads/writes the `ssh` and `sftp` groups of [`TerminalSettings`] and persists
//! to `terminal.json` through the shared [`crate::terminal::set`] helper.

use gpui::{App, SharedString};
use gpui_component::{
    Icon, IconName,
    setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage},
};
use oneterm_core::{
    MAX_SSH_KEEPALIVE_INTERVAL_SECS, MAX_SSH_KEEPALIVE_MAX, MIN_SSH_KEEPALIVE_INTERVAL_SECS,
    MIN_SSH_KEEPALIVE_MAX,
};
use oneterm_settings::{EditorMode, TerminalSettings};

use crate::terminal::set;

const MODE_OS_DEFAULT: &str = "os_default";
const MODE_CUSTOM: &str = "custom";

const BYTES_PER_MB: f64 = 1024.0 * 1024.0;

/// Build the "SSH" settings page.
pub(crate) fn page(cx: &App) -> SettingPage {
    SettingPage::new("SSH")
        .resettable(true)
        .icon(Icon::new(IconName::Network))
        .groups(
            groups(cx)
                .into_iter()
                .map(|(title, group)| group.title(title)),
        )
}

/// The page's groups with their titles, in render order.
///
/// gpui-component keeps `SettingGroup::title` private, so the titles live here
/// (instead of inside each builder) to give the order a test surface.
fn groups(cx: &App) -> Vec<(&'static str, SettingGroup)> {
    vec![
        ("Connection", connection_group()),
        ("SFTP Editor", sftp_editor_group(cx)),
        ("SFTP Edit Limit", sftp_edit_limit_group()),
    ]
}

/// "Connection" group — transport keepalive policy.
fn connection_group() -> SettingGroup {
    SettingGroup::new()
        .description("Keepalive settings applied to newly opened SSH sessions.")
        .items(vec![
            SettingItem::new(
                "Enable Keepalive",
                SettingField::switch(
                    |cx: &App| TerminalSettings::global(cx).read(cx).ssh.keepalive_enabled,
                    |value, cx| set(cx, move |settings| settings.ssh.keepalive_enabled = value),
                )
                .default_value(oneterm_settings::SshSettingsConfig::default().keepalive_enabled),
            )
            .description("Detect peers or network paths that stop responding."),
            SettingItem::new(
                "Keepalive Interval (seconds)",
                SettingField::number_input(
                    NumberFieldOptions {
                        min: MIN_SSH_KEEPALIVE_INTERVAL_SECS as f64,
                        max: MAX_SSH_KEEPALIVE_INTERVAL_SECS as f64,
                        step: 1.0,
                    },
                    |cx: &App| {
                        TerminalSettings::global(cx)
                            .read(cx)
                            .ssh
                            .keepalive_interval_secs as f64
                    },
                    |value, cx| {
                        set(cx, move |settings| {
                            settings.ssh.keepalive_interval_secs = (value as u64).clamp(
                                MIN_SSH_KEEPALIVE_INTERVAL_SECS,
                                MAX_SSH_KEEPALIVE_INTERVAL_SECS,
                            );
                        });
                    },
                )
                .default_value(
                    oneterm_settings::SshSettingsConfig::default().keepalive_interval_secs as f64,
                ),
            )
            .description("Seconds between keepalive requests."),
            SettingItem::new(
                "Keepalive Max",
                SettingField::number_input(
                    NumberFieldOptions {
                        min: MIN_SSH_KEEPALIVE_MAX as f64,
                        max: MAX_SSH_KEEPALIVE_MAX as f64,
                        step: 1.0,
                    },
                    |cx: &App| TerminalSettings::global(cx).read(cx).ssh.keepalive_max as f64,
                    |value, cx| {
                        set(cx, move |settings| {
                            settings.ssh.keepalive_max = (value as usize)
                                .clamp(MIN_SSH_KEEPALIVE_MAX, MAX_SSH_KEEPALIVE_MAX);
                        });
                    },
                )
                .default_value(oneterm_settings::SshSettingsConfig::default().keepalive_max as f64),
            )
            .description("Unanswered requests tolerated before the connection is closed."),
        ])
}

/// "SFTP Editor" group — how the SFTP browser's "Edit" action opens a remote
/// file locally.
fn sftp_editor_group(cx: &App) -> SettingGroup {
    let is_custom = TerminalSettings::global(cx).read(cx).sftp.editor.mode == EditorMode::Custom;

    SettingGroup::new()
        .description("Which editor the SFTP browser's Edit action opens a remote file with.")
        .items(vec![
            SettingItem::new(
                "Editor",
                SettingField::dropdown(
                    vec![
                        (MODE_OS_DEFAULT.into(), "OS default application".into()),
                        (MODE_CUSTOM.into(), "Custom command".into()),
                    ],
                    |cx: &App| -> SharedString {
                        match TerminalSettings::global(cx).read(cx).sftp.editor.mode {
                            EditorMode::OsDefault => MODE_OS_DEFAULT.into(),
                            EditorMode::Custom => MODE_CUSTOM.into(),
                        }
                    },
                    |val: SharedString, cx: &mut App| {
                        let mode = if val.as_ref() == MODE_CUSTOM {
                            EditorMode::Custom
                        } else {
                            EditorMode::OsDefault
                        };
                        update_editor(cx, move |e| e.mode = mode);
                    },
                )
                .default_value(MODE_OS_DEFAULT),
            )
            .description("OS default opens the associated application; Custom runs your command."),
            SettingItem::new(
                "Custom Program",
                SettingField::input(
                    |cx: &App| -> SharedString {
                        TerminalSettings::global(cx)
                            .read(cx)
                            .sftp
                            .editor
                            .program
                            .clone()
                            .into()
                    },
                    |val: SharedString, cx: &mut App| {
                        let program = val.to_string();
                        update_editor(cx, move |e| e.program = program);
                    },
                )
                .default_value(SharedString::default()),
            )
            .disabled(!is_custom)
            .description("Editor executable (e.g. code, notepad). Used only in Custom mode."),
            SettingItem::new(
                "Custom Arguments",
                SettingField::input(
                    |cx: &App| -> SharedString {
                        TerminalSettings::global(cx)
                            .read(cx)
                            .sftp
                            .editor
                            .args
                            .join(" ")
                            .into()
                    },
                    |val: SharedString, cx: &mut App| {
                        let args = val
                            .split_whitespace()
                            .map(str::to_string)
                            .collect::<Vec<_>>();
                        update_editor(cx, move |e| e.args = args.clone());
                    },
                )
                .default_value(SharedString::default()),
            )
            .disabled(!is_custom)
            .description(
                "Arguments passed before the file path (space-separated). Custom mode only.",
            ),
        ])
}

/// "SFTP Edit Limit" group — the size gate for the Edit action.
fn sftp_edit_limit_group() -> SettingGroup {
    SettingGroup::new()
        .description("Limits for opening remote files for editing.")
        .items(vec![
            SettingItem::new(
                "Max Edit File Size (MB)",
                SettingField::number_input(
                    NumberFieldOptions {
                        min: 0.0,
                        max: 4096.0,
                        step: 1.0,
                    },
                    |cx: &App| {
                        TerminalSettings::global(cx)
                            .read(cx)
                            .sftp
                            .edit_max_file_size as f64
                            / BYTES_PER_MB
                    },
                    |val: f64, cx: &mut App| {
                        let bytes = (val.max(0.0) * BYTES_PER_MB).round() as u64;
                        set_sftp(cx, move |s| s.edit_max_file_size = bytes);
                    },
                )
                .default_value(1.0),
            )
            .description("Files larger than this prompt before opening. 0 = no limit."),
        ])
}

/// Update the live editor config + persist.
fn update_editor(cx: &mut App, f: impl FnOnce(&mut oneterm_settings::EditorConfig)) {
    set_sftp(cx, |s| f(&mut s.editor));
}

/// Apply `f` to the live `sftp` group, notify, and persist to `terminal.json`.
fn set_sftp(cx: &mut App, f: impl FnOnce(&mut oneterm_settings::SftpConfig)) {
    set(cx, |s| f(&mut s.sftp));
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::AppContext as _;
    use oneterm_settings::terminal_settings::TerminalSettingsGlobal;

    /// IN-0021: the SSH page owns the SFTP groups; the order is part of the request.
    #[gpui::test]
    fn ssh_page_lists_connection_then_the_two_sftp_groups(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let settings = cx.new(|_| TerminalSettings::default());
            cx.set_global(TerminalSettingsGlobal(settings));

            let titles: Vec<&str> = groups(cx).into_iter().map(|(title, _)| title).collect();
            assert_eq!(titles, ["Connection", "SFTP Editor", "SFTP Edit Limit"]);
        });
    }
}
