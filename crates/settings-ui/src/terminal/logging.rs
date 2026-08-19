//! Terminal printable-output logging settings group.

use gpui::{
    App, AppContext as _, Entity, InteractiveElement as _, IntoElement as _, MouseButton,
    ParentElement as _, PathPromptOptions, SharedString, Styled as _, Window,
    prelude::FluentBuilder as _,
};
use gpui_component::{
    AxisExt as _, Sizable as _,
    input::{Input, InputState},
    setting::{RenderOptions, SettingField, SettingGroup, SettingItem},
};
use oneterm_core::{LOG_CONTENT_FORMAT, LOG_FILE_NAME_FORMAT, LogWriteMode};
use oneterm_settings::TerminalSettings;

use crate::items_with_separators;

use super::set;

const WRITE_MODES: &[(&str, LogWriteMode)] = &[
    ("Append", LogWriteMode::Append),
    ("Overwrite", LogWriteMode::Overwrite),
];

/// Build the terminal output logging settings group.
pub(super) fn group() -> SettingGroup {
    SettingGroup::new()
        .title("Logging")
        .description("Write printable terminal output to timestamped log files.")
        .items(items_with_separators(vec![
            SettingItem::new(
                "Automatic: Local Shell",
                SettingField::switch(
                    |cx: &App| TerminalSettings::global(cx).read(cx).logging.local,
                    |value, cx| set(cx, move |settings| settings.logging.local = value),
                ),
            )
            .description("Start logging every new local shell."),
            SettingItem::new(
                "Automatic: SSH",
                SettingField::switch(
                    |cx: &App| TerminalSettings::global(cx).read(cx).logging.ssh,
                    |value, cx| set(cx, move |settings| settings.logging.ssh = value),
                ),
            )
            .description("Start logging new SSH terminals unless a saved session overrides it."),
            SettingItem::new("Log Folder", log_folder_field())
                .description("Default: <user_home>/.OneTerm/logs."),
            SettingItem::new(
                "Existing File",
                SettingField::dropdown(
                    WRITE_MODES
                        .iter()
                        .map(|(label, _)| ((*label).into(), (*label).into()))
                        .collect(),
                    |cx: &App| {
                        let mode = TerminalSettings::global(cx).read(cx).logging.write_mode;
                        WRITE_MODES
                            .iter()
                            .find(|(_, candidate)| *candidate == mode)
                            .map(|(label, _)| SharedString::from(*label))
                            .unwrap_or_else(|| "Append".into())
                    },
                    |value, cx| {
                        let mode = WRITE_MODES
                            .iter()
                            .find(|(label, _)| *label == value.as_ref())
                            .map(|(_, mode)| *mode)
                            .unwrap_or_default();
                        set(cx, move |settings| settings.logging.write_mode = mode);
                    },
                ),
            )
            .description("Overwrite truncates once when logging starts; Append preserves content."),
            SettingItem::new(
                "File Name Format",
                SettingField::input(|_| LOG_FILE_NAME_FORMAT.into(), |_, _| {}),
            )
            .description("Fixed for this release. %n is the local process or SSH endpoint.")
            .disabled(true),
            SettingItem::new(
                "Content Format",
                SettingField::input(|_| LOG_CONTENT_FORMAT.into(), |_, _| {}),
            )
            .description("Fixed for this release. %msg is one printable output line.")
            .disabled(true),
        ]))
}

struct LogFolderInputState {
    input: Entity<InputState>,
}

fn log_folder_field() -> SettingField<SharedString> {
    SettingField::element(
        move |options: &RenderOptions, window: &mut Window, cx: &mut App| {
            let value = TerminalSettings::global(cx)
                .read(cx)
                .logging
                .directory
                .to_string_lossy()
                .to_string();
            let key = SharedString::from(format!(
                "log-folder-input-{}-{}-{}",
                options.page_ix, options.group_ix, options.item_ix
            ));
            let state_entity = window.use_keyed_state(key, cx, {
                let value = value.clone();
                |window, cx| LogFolderInputState {
                    input: cx.new(|cx| InputState::new(window, cx).default_value(value)),
                }
            });

            state_entity.update(cx, |state, cx| {
                if state.input.read(cx).value().as_ref() != value {
                    state.input.update(cx, |input, cx| {
                        input.set_value(value.clone(), window, cx);
                    });
                }
            });

            let input = state_entity.read(cx).input.clone();
            gpui::div()
                .relative()
                .map(|this| {
                    if options.layout.is_horizontal() {
                        this.w_64()
                    } else {
                        this.w_full()
                    }
                })
                .child(
                    Input::new(&input)
                        .with_size(options.size)
                        .tab_index(-1)
                        .w_full(),
                )
                .child(
                    gpui::div()
                        .absolute()
                        .top_0()
                        .right_0()
                        .bottom_0()
                        .left_0()
                        .when(!options.disabled, |this| {
                            this.cursor_pointer().on_mouse_down(
                                MouseButton::Left,
                                move |_, window, cx| {
                                    open_log_folder_picker(input.clone(), window, cx);
                                },
                            )
                        }),
                )
                .into_any_element()
        },
    )
}

fn open_log_folder_picker(input: Entity<InputState>, window: &mut Window, cx: &mut App) {
    let receiver = cx.prompt_for_paths(PathPromptOptions {
        files: false,
        directories: true,
        multiple: false,
        prompt: Some("Select terminal log folder".into()),
    });
    window
        .spawn(cx, async move |cx| {
            let selected = match receiver.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) => None,
                Ok(Err(error)) => {
                    log::warn!("failed to open native folder picker: {error}");
                    return;
                }
                Err(error) => {
                    log::warn!("native folder picker response was dropped: {error}");
                    return;
                }
            };
            if let Some(path) = selected {
                _ = cx.update(|window, cx| {
                    input.update(cx, |input, cx| {
                        input.set_value(path.to_string_lossy().to_string(), window, cx);
                    });
                    set(cx, move |settings| settings.logging.directory = path);
                });
            }
        })
        .detach();
}
