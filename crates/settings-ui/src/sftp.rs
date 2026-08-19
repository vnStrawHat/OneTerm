//! SFTP settings page — the editor used by the SFTP browser's "Edit" action
//! and the maximum file size opened without a confirmation prompt.
//!
//! Reads/writes the `sftp` group of [`TerminalSettings`] and persists to
//! `terminal.json` through the shared [`super::terminal::set`] helper.

use gpui::{App, SharedString};
use gpui_component::{
    Icon, IconName,
    setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage},
};
use oneterm_settings::{EditorMode, TerminalSettings};

use crate::items_with_separators;

const MODE_OS_DEFAULT: &str = "os_default";
const MODE_CUSTOM: &str = "custom";

const BYTES_PER_MB: f64 = 1024.0 * 1024.0;

/// Build the "SFTP" settings page.
pub(crate) fn page(cx: &App) -> SettingPage {
    SettingPage::new("SFTP")
        .resettable(true)
        .icon(Icon::new(IconName::Folder))
        .group(editor_group(cx))
        .group(edit_group())
}

/// "Editor" group — how the "Edit" action opens a remote file locally.
fn editor_group(cx: &App) -> SettingGroup {
    let is_custom = TerminalSettings::global(cx).read(cx).sftp.editor.mode == EditorMode::Custom;

    SettingGroup::new()
        .title("Editor")
        .description("Which editor the SFTP browser's Edit action opens a remote file with.")
        .items(items_with_separators(vec![
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
                        update(cx, move |e| e.mode = mode);
                    },
                ),
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
                        update(cx, move |e| e.program = program);
                    },
                ),
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
                        update(cx, move |e| e.args = args.clone());
                    },
                ),
            )
            .disabled(!is_custom)
            .description(
                "Arguments passed before the file path (space-separated). Custom mode only.",
            ),
        ]))
}

/// "Edit" group — the size gate for the Edit action.
fn edit_group() -> SettingGroup {
    SettingGroup::new()
        .title("Edit")
        .description("Limits for opening remote files for editing.")
        .items(items_with_separators(vec![
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
                ),
            )
            .description("Files larger than this prompt before opening. 0 = no limit."),
        ]))
}

/// Update the live editor config + persist.
fn update(cx: &mut App, f: impl FnOnce(&mut oneterm_settings::EditorConfig)) {
    set_sftp(cx, |s| f(&mut s.editor));
}

/// Apply `f` to the live `sftp` group, notify, and persist to `terminal.json`.
fn set_sftp(cx: &mut App, f: impl FnOnce(&mut oneterm_settings::SftpConfig)) {
    super::terminal::set(cx, |s| f(&mut s.sftp));
}
