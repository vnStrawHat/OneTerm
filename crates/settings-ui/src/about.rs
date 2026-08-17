//! "About" settings page — version, description, and links.
//!
//! The version string is the workspace `CARGO_PKG_VERSION` (the same value
//! shown by the OneTerm ▸ About dialog).

use std::sync::atomic::{AtomicU8, Ordering};

use gpui::{
    AnyElement, App, AppContext as _, Context, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, StatefulInteractiveElement as _, Styled, Window, div,
    prelude::FluentBuilder, px, rgb,
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

const TEST_CRASH_CLICK_COUNT: u8 = 10;

static ABOUT_ICON_CLICKS: AtomicU8 = AtomicU8::new(0);

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

/// Build the "About" settings page.
pub(crate) fn page(cx: &gpui::App) -> SettingPage {
    SettingPage::new("About")
        .icon(Icon::new(IconName::Info))
        .resettable(true)
        .group(about_group())
        .group(links_group())
        .group(updates::network_group(cx))
        .group(updates::group(cx))
}

/// The "About" group — app name, version, and a short description.
fn about_group() -> SettingGroup {
    SettingGroup::new().item(SettingItem::render(|_options, _, cx| app_identity(cx)))
}

fn app_identity(cx: &App) -> AnyElement {
    v_flex()
        .gap_3()
        .w_full()
        .items_center()
        .justify_center()
        .child(
            div()
                .id("about-app-icon")
                .cursor_pointer()
                .child(
                    Icon::new(AppIcon::Terminal)
                        .with_size(px(96.))
                        .text_color(rgb(0x58c4dc)),
                )
                .on_click(|_, _, _| {
                    if register_about_icon_click() {
                        // This diagnostic-only panic verifies the crash recovery flow end to end.
                        panic!("Intentional crash triggered by ten clicks on the About icon");
                    }
                }),
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

fn register_about_icon_click() -> bool {
    ABOUT_ICON_CLICKS
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
            Some(if count + 1 == TEST_CRASH_CLICK_COUNT {
                0
            } else {
                count + 1
            })
        })
        .is_ok_and(|previous| previous + 1 == TEST_CRASH_CLICK_COUNT)
}

/// GitHub page of the repository this build was configured for (the same
/// `owner/repo` the updater queries), so a fork build links to itself.
fn github_repository_url() -> String {
    format!("https://github.com/{}", oneterm_update::UPDATE_REPOSITORY)
}

fn repository_link(id: &'static str, cx: &App) -> AnyElement {
    div()
        .id(id)
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
    fn crash_trigger_fires_on_every_tenth_click() {
        ABOUT_ICON_CLICKS.store(0, Ordering::Relaxed);

        for _ in 0..TEST_CRASH_CLICK_COUNT - 1 {
            assert!(!register_about_icon_click());
        }
        assert!(register_about_icon_click());
        assert!(!register_about_icon_click());
    }
}
