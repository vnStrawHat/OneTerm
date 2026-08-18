//! Security settings group.

use gpui::App;
use gpui_component::setting::{SettingField, SettingGroup, SettingItem};
use oneterm_settings::TerminalSettings;

use super::set;

/// Build the "Security" settings group.
pub(super) fn group() -> SettingGroup {
    SettingGroup::new().title("Security").item(
        SettingItem::new(
            "Allow Clipboard Read (OSC 52)",
            SettingField::switch(
                |cx: &App| TerminalSettings::global(cx).read(cx).allow_clipboard_read,
                |val: bool, cx: &mut App| {
                    set(cx, |s| s.allow_clipboard_read = val);
                },
            ),
        )
        .description("Allow OSC 52 clipboard reads."),
    )
}
