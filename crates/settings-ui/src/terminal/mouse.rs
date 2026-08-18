//! Mouse settings group.

use gpui::App;
use gpui_component::setting::{SettingField, SettingGroup, SettingItem};
use oneterm_settings::TerminalSettings;

use super::persist;

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
                        TerminalSettings::global(cx).update(cx, |s, cx| {
                            s.show_context_menu = val;
                            cx.notify();
                        });
                        persist(cx);
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
                        TerminalSettings::global(cx).update(cx, |s, cx| {
                            s.copy_on_select = val;
                            cx.notify();
                        });
                        persist(cx);
                    },
                ),
            )
            .description("Copy the selection to the clipboard when the mouse button is released."),
        )
}
