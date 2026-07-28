//! "About" settings page — version, description, and links.
//!
//! The version string comes from the `ONETERM_VERSION` compile-time env (the
//! same value shown by the OneTerm ▸ About dialog).

use gpui::{
    App, AppContext as _, Context, Element, IntoElement, ParentElement as _, Render, Styled,
    Window, prelude::FluentBuilder,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::DialogFooter,
    h_flex,
    label::Label,
    setting::{SettingField, SettingGroup, SettingItem, SettingPage},
    v_flex,
};

use super::{items_with_separators, updates};

struct AboutUpdateControls;

impl AboutUpdateControls {
    fn new(cx: &mut Context<Self>) -> Self {
        cx.observe(&updates::UpdateUiState::global(cx), |_, _, cx| cx.notify())
            .detach();
        Self
    }
}

impl Render for AboutUpdateControls {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = updates::UpdateUiState::global(cx).read(cx).clone();
        let status = state.status_text();

        v_flex()
            .gap_3()
            .w_full()
            .child(
                v_flex()
                    .gap_1()
                    .child(Label::new("Update Status").text_sm())
                    .child(
                        Label::new(status)
                            .text_sm()
                            .text_color(cx.theme().muted_foreground),
                    ),
            )
            .when(state.shows_install_button(), |this| {
                this.child(
                    h_flex().gap_2().child(
                        Button::new("about-install-update")
                            .primary()
                            .label(state.install_button_label())
                            .disabled(!state.can_install_update())
                            .on_click(|_, window, cx| {
                                updates::download_and_install_update(window, cx)
                            }),
                    ),
                )
            })
            .into_any_element()
    }
}

/// Open the About dialog from the application menu.
pub(crate) fn open_about_dialog(window: &mut Window, cx: &mut App) {
    let update_controls = cx.new(|cx| AboutUpdateControls::new(cx));
    window.open_alert_dialog(cx, move |alert, _, _| {
        alert
            .title("About OneTerm")
            .description(format!(
                "OneTerm v{}\n\nA terminal application for local and SSH sessions.\nBuilt with GPUI + alacritty_terminal.",
                env!("ONETERM_VERSION")
            ))
            .child(update_controls.clone())
            .footer(
                DialogFooter::new().gap_2().child(
                    Button::new("about-check-update")
                        .ghost()
                        .label("Check for Updates")
                        .on_click(|_, window, cx| updates::check_now(window, cx)),
                ).child(
                    Button::new("about-close")
                        .label("Close")
                        .on_click(|_, window, cx| window.close_dialog(cx)),
                ),
            )
    });
}

/// Build the "About" settings page.
pub(crate) fn page(cx: &gpui::App) -> SettingPage {
    SettingPage::new("About")
        .icon(Icon::new(IconName::Info))
        .resettable(false)
        .group(about_group())
        .group(links_group())
        .group(updates::network_group(cx))
        .group(updates::group(cx))
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
    SettingGroup::new()
        .title("Links")
        .items(items_with_separators(vec![
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
                                    cx.open_url("https://github.com/vnStrawHat/OneTerm");
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
