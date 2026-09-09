//! "About" settings page — version, description, and links.
//!
//! The version string is the workspace `CARGO_PKG_VERSION` (the same value
//! shown by the OneTerm ▸ About dialog).

use gpui::{
    AnyElement, App, AppContext as _, Context, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Role, StatefulInteractiveElement as _, Styled, Window, div,
    prelude::FluentBuilder, px,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::DialogFooter,
    h_flex,
    label::Label,
    setting::{RenderOptions, SettingField, SettingGroup, SettingItem, SettingPage},
    v_flex,
};
use oneterm_theme::icon::AppIcon;

use super::updates;

const REPOSITORY_LINK_ROLE: Role = Role::Link;

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
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("about-install-update")
                                .primary()
                                .label(state.install_button_label())
                                .disabled(!state.can_install_update())
                                .on_click(|_, window, cx| {
                                    updates::download_and_install_update(window, cx)
                                }),
                        )
                        .when(state.can_skip_update(), |this| {
                            this.child(
                                Button::new("about-skip-update")
                                    .ghost()
                                    .label("Skip This Version")
                                    .on_click(|_, _, cx| updates::skip_offered_version(cx)),
                            )
                        }),
                )
            })
            .into_any_element()
    }
}

/// Open the About dialog from the application menu.
pub(crate) fn open_about_dialog(window: &mut Window, cx: &mut App) {
    let update_controls = cx.new(|cx| AboutUpdateControls::new(cx));
    window.open_alert_dialog(cx, move |alert, _, cx| {
        alert
            .title("About OneTerm")
            .width(px(520.))
            .child(
                v_flex()
                    .gap_5()
                    .w_full()
                    .child(app_identity(cx))
                    .child(links_section(cx))
                    .child(update_controls.clone()),
            )
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AboutGroup {
    Links,
    Network,
    Updates,
    Identity,
}

// GPUI Kit 0.6 numbers sidebar entries after filtering out untitled groups,
// while scroll targets index every group. Every group here carries a title so
// the identity block can lead the page without desynchronising the sidebar.
const ABOUT_GROUP_ORDER: [AboutGroup; 4] = [
    AboutGroup::Identity,
    AboutGroup::Links,
    AboutGroup::Network,
    AboutGroup::Updates,
];

/// Build the "About" settings page.
pub(crate) fn page(cx: &gpui::App) -> SettingPage {
    ABOUT_GROUP_ORDER.into_iter().fold(
        SettingPage::new("About")
            .icon(Icon::new(IconName::Info))
            .resettable(true),
        |page, group| {
            page.group(match group {
                AboutGroup::Identity => about_group(),
                AboutGroup::Links => links_group(),
                AboutGroup::Network => updates::network_group(cx),
                AboutGroup::Updates => updates::group(cx),
            })
        },
    )
}

/// The "Application" group — app name, version, and a short description.
/// Titled so the sidebar lists it and its scroll target stays index-aligned.
fn about_group() -> SettingGroup {
    SettingGroup::new()
        .title("Application")
        .item(SettingItem::render(|_options, _, cx| app_identity(cx)))
}

fn app_identity(cx: &App) -> AnyElement {
    v_flex()
        .gap_3()
        .w_full()
        .items_center()
        .justify_center()
        .child(
            Icon::new(AppIcon::Terminal)
                .with_size(px(96.))
                .text_color(oneterm_theme::brand_accent()),
        )
        .child(Label::new("OneTerm").text_xl())
        .child(
            Label::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                .text_sm()
                .text_color(cx.theme().muted_foreground),
        )
        .child(
            Label::new("A Terminal application for SSH / SFTP / Local Shell")
                .text_sm()
                .text_color(cx.theme().muted_foreground),
        )
        .into_any_element()
}

/// The "Links" group — GitHub repository.
fn links_group() -> SettingGroup {
    SettingGroup::new().title("Links").item(SettingItem::new(
        "GitHub Repository",
        SettingField::element(
            |_options: &RenderOptions, _window: &mut Window, cx: &mut App| {
                repository_link("settings-open-repo", cx)
            },
        ),
    ))
}

fn links_section(cx: &App) -> AnyElement {
    v_flex()
        .gap_2()
        .w_full()
        .child(
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .gap_3()
                .child(Label::new("GitHub Repository").text_sm())
                .child(repository_link("about-open-repo", cx)),
        )
        .into_any_element()
}

/// GitHub page of the repository this build was configured for (the same
/// `owner/repo` the updater queries), so a fork build links to itself.
fn github_repository_url() -> String {
    format!("https://github.com/{}", oneterm_update::UPDATE_REPOSITORY)
}

fn repository_link(id: &'static str, cx: &App) -> AnyElement {
    div()
        .id(id)
        .role(REPOSITORY_LINK_ROLE)
        .py_0p5()
        .text_sm()
        .text_color(cx.theme().link)
        .text_decoration_1()
        .cursor_pointer()
        .child(github_repository_url())
        .on_click(|_, _, cx| {
            cx.open_url(&github_repository_url());
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_link_exposes_link_accessibility_role() {
        assert!(matches!(REPOSITORY_LINK_ROLE, Role::Link));
    }

    #[test]
    fn identity_group_leads_the_about_page() {
        assert_eq!(ABOUT_GROUP_ORDER.first(), Some(&AboutGroup::Identity));
    }
}
