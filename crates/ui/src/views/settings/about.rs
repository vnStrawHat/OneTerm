//! "About" settings page — version, description, and links.
//!
//! The version string comes from the `ONETERM_VERSION` compile-time env (the
//! same value shown by the OneTerm ▸ About dialog).

use gpui::{Element, IntoElement, ParentElement as _, Styled};
use gpui_component::{
    ActiveTheme as _, Icon, IconName,
    button::Button,
    h_flex,
    label::Label,
    setting::{SettingField, SettingGroup, SettingItem, SettingPage},
    v_flex,
};

use super::items_with_separators;

/// Build the "About" settings page.
pub(crate) fn page() -> SettingPage {
    SettingPage::new("About")
        .icon(Icon::new(IconName::Info))
        .resettable(false)
        .group(about_group())
        .group(links_group())
}

/// The "About" group — app name, version, and a short description.
fn about_group() -> SettingGroup {
    SettingGroup::new().item(SettingItem::render(|_options, _, cx| {
        v_flex()
            .gap_3()
            .w_full()
            .items_center()
            .justify_center()
            .child(Icon::new(IconName::SquareTerminal).size_12())
            .child(Label::new("OneTerm").text_xl())
            .child(
                Label::new(format!("Version {}", env!("ONETERM_VERSION")))
                    .text_sm()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(
                Label::new(
                    "A terminal application for local and SSH sessions. \
                     Built with GPUI + alacritty_terminal.",
                )
                .text_sm()
                .text_color(cx.theme().muted_foreground),
            )
            .into_any()
    }))
}

/// The "Links" group — GitHub repository and documentation.
fn links_group() -> SettingGroup {
    SettingGroup::new().title("Links").items(items_with_separators(vec![
        SettingItem::new(
            "GitHub Repository",
            SettingField::render(|_options, _window, _cx| {
                h_flex()
                    .w_full()
                    .justify_between()
                    .child("Source code and releases.")
                    .child(
                        Button::new("open-repo")
                            .outline()
                            .label("Repository...")
                            .on_click(|_, _, cx| {
                                cx.open_url("https://github.com/longbridge/gpui-component");
                            }),
                    )
                    .into_any_element()
            }),
        )
        .description("Open the GitHub repository in your default browser."),
        SettingItem::new(
            "Built With",
            SettingField::render(|_options, _window, _cx| {
                h_flex()
                    .w_full()
                    .justify_between()
                    .child("GPUI + alacritty_terminal + gpui-component.")
                    .child(
                        Button::new("open-gpui")
                            .outline()
                            .label("gpui-component...")
                            .on_click(|_, _, cx| {
                                cx.open_url("https://github.com/longbridge/gpui-component");
                            }),
                    )
                    .into_any_element()
            }),
        )
        .description("The GUI framework and component library powering OneTerm."),
    ]))
}
