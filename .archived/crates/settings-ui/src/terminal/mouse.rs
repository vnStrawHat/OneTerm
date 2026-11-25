//! Mouse settings group.

use gpui::App;
use gpui_component::setting::{SettingField, SettingGroup, SettingItem};
use oneterm_settings::TerminalSettings;

use super::set;

/// Build the "Mouse" settings group.
pub(super) fn group() -> SettingGroup {
    SettingGroup::new()
        .title("Mouse")
        .item(
            SettingItem::new(
                "Right-Click Context Menu",
                SettingField::switch(
                    |cx: &App| TerminalSettings::global(cx).read(cx).show_context_menu,
                    |val: bool, cx: &mut App| {
                        set(cx, |s| s.show_context_menu = val);
                    },
                )
                .default_value(true),
            )
            .description("Show OneTerm right-click menu."),
        )
        .item(
            SettingItem::new(
                "Copy on Select",
                SettingField::switch(
                    |cx: &App| TerminalSettings::global(cx).read(cx).copy_on_select,
                    |val: bool, cx: &mut App| {
                        set(cx, |s| s.copy_on_select = val);
                    },
                )
                .default_value(true),
            )
            .description("Copy the selection to the clipboard when the mouse button is released."),
        )
        .item(
            SettingItem::new(
                "Middle-Click Paste",
                SettingField::switch(
                    |cx: &App| TerminalSettings::global(cx).read(cx).middle_click_paste,
                    |val: bool, cx: &mut App| {
                        set(cx, |s| s.middle_click_paste = val);
                    },
                )
                .default_value(true),
            )
            .description(
                "Paste the clipboard on a middle click. A program in mouse mode gets the click instead unless Shift is held.",
            ),
        )
}
