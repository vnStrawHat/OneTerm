//! Scroll settings group.

use gpui::App;
use gpui_component::setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem};
use oneterm_settings::TerminalSettings;

use crate::items_with_separators;

use super::set;

/// Build the "Scroll" settings group.
pub(super) fn group() -> SettingGroup {
    SettingGroup::new()
        .title("Scroll")
        .description("Wheel speed and alt-screen mode.")
        .items(items_with_separators(vec![
            SettingItem::new(
                "Scroll Multiplier",
                SettingField::number_input(
                    NumberFieldOptions {
                        min: 0.0,
                        max: 20.0,
                        step: 0.5,
                    },
                    |cx: &App| TerminalSettings::global(cx).read(cx).scroll_multiplier as f64,
                    |val: f64, cx: &mut App| {
                        set(cx, |s| s.scroll_multiplier = val as f32);
                    },
                ),
            )
            .description("Wheel speed."),
            SettingItem::new(
                "Alternate Scroll",
                SettingField::switch(
                    |cx: &App| TerminalSettings::global(cx).read(cx).alternate_scroll,
                    |val: bool, cx: &mut App| {
                        set(cx, |s| s.alternate_scroll = val);
                    },
                ),
            )
            .description("Send arrows in alt-screen."),
        ]))
}
