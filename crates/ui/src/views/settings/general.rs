//! "General" settings page — UI font size + configurable key bindings.
//!
//! The UI font size drives the gpui-component [`Theme::font_size`] (the same
//! field the View ▸ Font Size menu writes); it is persisted by the `Theme`
//! observer in `theme::init`. The key bindings section is the editable group from
//! [`super::key_bindings`] (press-to-rebind + reset to default).

use gpui::{App, px};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Theme,
    setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage},
};

/// Build the "General" settings page.
pub(crate) fn page() -> SettingPage {
    SettingPage::new("General")
        .icon(Icon::new(IconName::Settings2))
        .group(interface_group())
        .group(super::key_bindings::key_bindings_group())
}

/// "Interface" group — the UI (non-terminal) font size.
fn interface_group() -> SettingGroup {
    SettingGroup::new()
        .title("Interface")
        .description("Adjust the font size of the OneTerm UI (menus, panels, dialogs).")
        .item(
            SettingItem::new(
                "UI Font Size",
                SettingField::number_input(
                    NumberFieldOptions {
                        min: 8.0,
                        max: 32.0,
                        ..Default::default()
                    },
                    |cx: &App| cx.theme().font_size.as_f32() as f64,
                    |val: f64, cx: &mut App| {
                        Theme::global_mut(cx).font_size = px(val as f32);
                        cx.refresh_windows();
                    },
                )
                .default_value(16.0),
            )
            .description("Font size (px) used for the application interface."),
        )
}
