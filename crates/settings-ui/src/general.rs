//! "General" settings page — the settings a new user looks for first.
//!
//! The Settings window opens on this page, so it carries the three things
//! somebody reaches Settings for before anything else: the theme, the UI font
//! size, and the shell new local terminals start. The theme groups are built by
//! [`super::appearance`] and the shell group by [`super::terminal::shell`];
//! this module only composes them, so no setting changed owner when the
//! separate Appearance page was folded away (`US-0122`).
//!
//! The UI font size drives the gpui-component [`Theme::font_size`] (the same
//! field the View ▸ Font Size menu writes); it is persisted by the `Theme`
//! observer in `theme::init`.

use gpui::{App, px};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Theme,
    setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage},
};

/// Build the "General" settings page.
pub(crate) fn page(cx: &App) -> SettingPage {
    SettingPage::new("General")
        .resettable(true)
        .icon(Icon::new(IconName::Settings2))
        .group(super::appearance::theme_group(cx))
        .group(interface_group())
        .group(super::terminal::shell_group())
}

/// "Interface" group — the UI (non-terminal) font size.
fn interface_group() -> SettingGroup {
    SettingGroup::new().title("Interface").item(
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
        .description("Size of all non-terminal text, in px. Terminal text has its own size."),
    )
}
