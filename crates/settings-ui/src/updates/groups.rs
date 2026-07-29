//! Settings groups for update preferences and status.

use gpui::{App, IntoElement, ParentElement as _, Styled, Window};
use gpui_component::{
    ActiveTheme as _,
    label::Label,
    setting::{RenderOptions, SettingField, SettingGroup, SettingItem},
    v_flex,
};
use oneterm_update::UpdateConfig;

use crate::items_with_separators;

use super::{
    config::{set_auto_check, set_proxy_url, set_verify_certificates},
    state::UpdateUiState,
};

/// Build the About-page update group.
pub(crate) fn group(cx: &App) -> SettingGroup {
    let state = UpdateUiState::global(cx).read(cx).clone();
    let config = UpdateUiState::config(cx).read(cx).clone();
    SettingGroup::new()
        .title("Updates")
        .items(items_with_separators(vec![
            auto_check_item(config),
            status_item(state),
        ]))
}

/// Build the About-page update network group.
pub(crate) fn network_group(cx: &App) -> SettingGroup {
    let config = UpdateUiState::config(cx).read(cx).clone();
    SettingGroup::new()
        .title("Network")
        .items(items_with_separators(vec![
            proxy_item(config.clone()),
            certificate_item(config),
        ]))
}

fn auto_check_item(config: UpdateConfig) -> SettingItem {
    SettingItem::new(
        "Automatic Checks",
        SettingField::switch(
            move |_cx| config.auto_check,
            |checked, cx| {
                set_auto_check(cx, checked);
            },
        ),
    )
    .description("Automatic update checks.")
}

fn proxy_item(config: UpdateConfig) -> SettingItem {
    SettingItem::new(
        "Proxy URL",
        SettingField::input(
            move |_cx| config.proxy_url.clone().unwrap_or_default().into(),
            |value, cx| {
                set_proxy_url(
                    cx,
                    if value.is_empty() {
                        None
                    } else {
                        Some(value.to_string())
                    },
                );
            },
        ),
    )
    .description("Blank uses system proxy.")
}

fn certificate_item(config: UpdateConfig) -> SettingItem {
    SettingItem::new(
        "Verify Certificates",
        SettingField::switch(
            move |_cx| config.verify_certificates,
            |checked, cx| {
                set_verify_certificates(cx, checked);
            },
        ),
    )
    .description("Verify TLS certificates.")
}

fn status_item(state: UpdateUiState) -> SettingItem {
    SettingItem::new(
        "Update Status",
        SettingField::element(
            move |_options: &RenderOptions, _window: &mut Window, cx: &mut App| {
                v_flex()
                    .w_full()
                    .gap_1()
                    .child(Label::new(state.status_text()).text_color(cx.theme().muted_foreground))
                    .into_any_element()
            },
        ),
    )
    .description("GitHub Releases status.")
}
