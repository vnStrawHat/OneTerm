//! "About" settings page — version, description, and links.
//!
//! The version string comes from the `ONETERM_VERSION` compile-time env (the
//! same value shown by the OneTerm ▸ About dialog).

use gpui::{
    App, AppContext as _, Context, Element, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, StatefulInteractiveElement as _, Styled, Window, div,
    prelude::FluentBuilder, px,
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
use oneterm_theme::icon::AppIcon;

use super::updates;

const GITHUB_REPOSITORY_URL: &str = "https://github.com/vnStrawHat/OneTerm";

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
                "OneTerm v{}\n\nA terminal application for local and SSH sessions.",
                env!("ONETERM_VERSION")
            ))
            .child(update_controls.clone())
            .footer(
                DialogFooter::new()
                    .gap_2()
                    .child(
                        Button::new("about-check-update")
                            .ghost()
                            .label("Check for Updates")
                            .on_click(|_, window, cx| updates::check_now(window, cx)),
                    )
                    .child(
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
            .child(AppIcon::TerminalLogo.colored().size(px(96.)))
            .child(Label::new("OneTerm").text_xl())
            .child(
                Label::new(format!("Version {}", env!("ONETERM_VERSION")))
                    .text_sm()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(
                Label::new("A terminal application for local and SSH sessions.")
                    .text_sm()
                    .text_color(cx.theme().muted_foreground),
            )
            .into_any()
    }))
}

/// The "Links" group — GitHub repository.
fn links_group() -> SettingGroup {
    SettingGroup::new().title("Links").item(SettingItem::new(
        "GitHub Repository",
        SettingField::render(|_options, _window, cx| {
            div()
                .id("open-repo")
                .py_0p5()
                .text_sm()
                .text_color(cx.theme().link)
                .text_decoration_1()
                .cursor_pointer()
                .child(GITHUB_REPOSITORY_URL)
                .on_click(|_, _, cx| {
                    cx.open_url(GITHUB_REPOSITORY_URL);
                })
                .into_any_element()
        }),
    ))
}
