//! Bell settings group.

use gpui::App;
use gpui_component::setting::{SettingField, SettingGroup, SettingItem};
use oneterm_settings::TerminalSettings;

use super::persist;

/// Build the "Bell" settings group.
pub(super) fn group() -> SettingGroup {
    SettingGroup::new().title("Bell").item(
        SettingItem::new(
            "Bell Enabled",
            SettingField::switch(
                |cx: &App| TerminalSettings::global(cx).read(cx).bell_enabled,
                |val: bool, cx: &mut App| {
                    TerminalSettings::global(cx).update(cx, |s, cx| {
                        s.bell_enabled = val;
                        cx.notify();
                    });
                    persist(cx);
                },
            ),
        )
        .description("Show terminal bell indicator."),
    )
}
