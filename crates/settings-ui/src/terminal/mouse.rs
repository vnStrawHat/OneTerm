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
                ),
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
                ),
            )
            .description("Copy the selection to the clipboard when the mouse button is released."),
        )
}
