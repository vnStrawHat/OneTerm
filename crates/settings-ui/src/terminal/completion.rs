//! Completion settings group (docs/auto-completion/06 §5).
//!
//! Binds the `completion` block of `TerminalSettings` to the Terminal settings
//! page: master toggle, Tab-accept, history/prefix/visible numbers, per-source
//! toggles, fuzzy, alt-screen gating, coreutils-on-Windows, and a force-family
//! dropdown. Clearing session history is exposed via the `ClearCompletionHistory`
//! action (bindable in the key-bindings UI) since the settings widget has no
//! button field.

use gpui::{App, SharedString};
use gpui_component::setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem};
use oneterm_settings::TerminalSettings;

use crate::items_with_separators;

use super::persist;

fn count_field(
    get: impl Fn(&App) -> usize + 'static,
    set: impl Fn(usize, &mut App) + 'static,
    max: f64,
) -> SettingField<f64> {
    SettingField::number_input(
        NumberFieldOptions {
            min: 0.0,
            max,
            step: 1.0,
        },
        move |cx: &App| get(cx) as f64,
        move |val: f64, cx: &mut App| set(val.max(0.0) as usize, cx),
    )
}

/// Build the "Completion" settings group.
pub(super) fn group() -> SettingGroup {
    SettingGroup::new()
        .title("Completion")
        .description("Command auto-completion overlay + in-session history.")
        .items(items_with_separators(vec![
            SettingItem::new(
                "Enable Auto-Completion",
                SettingField::switch(
                    |cx: &App| TerminalSettings::global(cx).read(cx).completion.enabled,
                    |val: bool, cx: &mut App| {
                        update(cx, move |c| c.enabled = val);
                    },
                ),
            )
            .description("Master switch for the completion overlay + history capture."),
            SettingItem::new(
                "Accept With Tab",
                SettingField::switch(
                    |cx: &App| TerminalSettings::global(cx).read(cx).completion.accept_tab,
                    |val: bool, cx: &mut App| update(cx, move |c| c.accept_tab = val),
                ),
            )
            .description("When off, Tab is forwarded to the shell."),
            SettingItem::new(
                "Max Command History",
                count_field(
                    |cx| TerminalSettings::global(cx).read(cx).completion.max_history,
                    |v, cx| update(cx, move |c| c.max_history = v),
                    100_000.0,
                ),
            )
            .description("Per-family in-session history capacity (0 disables history)."),
            SettingItem::new(
                "Min Characters Before Suggesting",
                count_field(
                    |cx| {
                        TerminalSettings::global(cx)
                            .read(cx)
                            .completion
                            .min_prefix_len
                    },
                    |v, cx| update(cx, move |c| c.min_prefix_len = v),
                    16.0,
                ),
            )
            .description("Command suggestions appear after this many typed characters."),
            SettingItem::new(
                "Visible Suggestions",
                count_field(
                    |cx| {
                        TerminalSettings::global(cx)
                            .read(cx)
                            .completion
                            .max_visible_items
                    },
                    |v, cx| update(cx, move |c| c.max_visible_items = v.max(1)),
                    50.0,
                ),
            )
            .description("Rows shown in the overlay before scrolling."),
            SettingItem::new(
                "Source: History",
                SettingField::switch(
                    |cx: &App| {
                        TerminalSettings::global(cx)
                            .read(cx)
                            .completion
                            .source_memory
                    },
                    |val: bool, cx: &mut App| update(cx, move |c| c.source_memory = val),
                ),
            )
            .description("Suggest commands you ran this session."),
            SettingItem::new(
                "Source: Manual",
                SettingField::switch(
                    |cx: &App| {
                        TerminalSettings::global(cx)
                            .read(cx)
                            .completion
                            .source_manual
                    },
                    |val: bool, cx: &mut App| update(cx, move |c| c.source_manual = val),
                ),
            )
            .description("Hand-authored bundled catalogs (git, cargo, …)."),
            SettingItem::new(
                "Source: External",
                SettingField::switch(
                    |cx: &App| {
                        TerminalSettings::global(cx)
                            .read(cx)
                            .completion
                            .source_external
                    },
                    |val: bool, cx: &mut App| update(cx, move |c| c.source_external = val),
                ),
            )
            .description("Generated catalogs (Windows commands, coreutils)."),
            SettingItem::new(
                "Fuzzy Matching",
                SettingField::switch(
                    |cx: &App| TerminalSettings::global(cx).read(cx).completion.fuzzy,
                    |val: bool, cx: &mut App| update(cx, move |c| c.fuzzy = val),
                ),
            )
            .description("Secondary subsequence matching."),
            SettingItem::new(
                "Disable Inside Full-Screen Apps",
                SettingField::switch(
                    |cx: &App| {
                        TerminalSettings::global(cx)
                            .read(cx)
                            .completion
                            .disable_in_alt_screen
                    },
                    |val: bool, cx: &mut App| update(cx, move |c| c.disable_in_alt_screen = val),
                ),
            )
            .description("Suppress suggestions in vim/less/htop (alternate screen)."),
            SettingItem::new(
                "Allow Coreutils On Windows",
                SettingField::switch(
                    |cx: &App| {
                        TerminalSettings::global(cx)
                            .read(cx)
                            .completion
                            .windows_allow_coreutils
                    },
                    |val: bool, cx: &mut App| update(cx, move |c| c.windows_allow_coreutils = val),
                ),
            )
            .description(
                "Also suggest coreutils/linux commands in cmd/PowerShell (Git-Bash users).",
            ),
            SettingItem::new(
                "Force Shell Family",
                SettingField::dropdown(
                    vec![
                        ("".into(), "Auto-detect".into()),
                        ("cmd".into(), "cmd".into()),
                        ("powershell".into(), "PowerShell".into()),
                        ("unix".into(), "Unix".into()),
                    ],
                    |cx: &App| -> SharedString {
                        TerminalSettings::global(cx)
                            .read(cx)
                            .completion
                            .force_family
                            .clone()
                            .unwrap_or_default()
                            .into()
                    },
                    |val: SharedString, cx: &mut App| {
                        let value = val.to_string();
                        update(cx, move |c| {
                            c.force_family = if value.is_empty() {
                                None
                            } else {
                                Some(value.clone())
                            };
                        });
                    },
                ),
            )
            .description("Override the detected shell family for suggestions."),
        ]))
}

/// Update the live completion settings + persist.
fn update(cx: &mut App, f: impl FnOnce(&mut oneterm_settings::CompletionSettings)) {
    TerminalSettings::global(cx).update(cx, |s, cx| {
        f(&mut s.completion);
        cx.notify();
    });
    persist(cx);
}
