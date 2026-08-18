//! Settings groups for update preferences and status.

use gpui::{App, IntoElement, ParentElement as _, SharedString, Styled, Window};
use gpui_component::{
    ActiveTheme as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    label::Label,
    setting::{NumberFieldOptions, RenderOptions, SettingField, SettingGroup, SettingItem},
    v_flex,
};
use oneterm_update::{MAX_CHECK_INTERVAL_HOURS, UpdateChannel, UpdateConfig};

use crate::items_with_separators;

use super::{config::update_preference, state::UpdateUiState};

const CHANNEL_STABLE: &str = "stable";
const CHANNEL_PREVIEW: &str = "preview";

/// Build the About-page update group.
pub(crate) fn group(cx: &App) -> SettingGroup {
    let state = UpdateUiState::global(cx).read(cx).clone();
    let config = UpdateUiState::config(cx).read(cx).clone();
    let mut items = vec![
        auto_check_item(config.clone()),
        interval_item(config.clone()),
        channel_item(config.clone()),
    ];
    if let Some(skipped) = config.skipped_version.clone() {
        items.push(skipped_version_item(skipped));
    }
    items.push(status_item(state));
    SettingGroup::new()
        .title("Updates")
        .items(items_with_separators(items))
}

/// Build the About-page update network group.
pub(crate) fn network_group(cx: &App) -> SettingGroup {
    let config = UpdateUiState::config(cx).read(cx).clone();
    let mut items = vec![proxy_item(config.clone()), certificate_item(config.clone())];
    if !config.verify_certificates {
        items.push(insecure_certificates_warning());
    }
    SettingGroup::new()
        .title("Network")
        .items(items_with_separators(items))
}

fn auto_check_item(config: UpdateConfig) -> SettingItem {
    SettingItem::new(
        "Automatic Checks",
        SettingField::switch(
            move |_cx| config.auto_check,
            |checked, cx| {
                update_preference(cx, |c| c.auto_check = checked);
            },
        ),
    )
    .description("Check GitHub Releases at startup once the interval has elapsed.")
}

fn interval_item(config: UpdateConfig) -> SettingItem {
    SettingItem::new(
        "Check Interval (hours)",
        SettingField::number_input(
            NumberFieldOptions {
                min: 1.0,
                max: MAX_CHECK_INTERVAL_HOURS as f64,
                step: 1.0,
            },
            move |_cx| config.effective_check_interval_hours() as f64,
            |hours, cx| {
                update_preference(cx, |c| c.check_interval_hours = hours_from_field(hours));
            },
        ),
    )
    .description("Minimum time between automatic checks.")
}

/// Round and clamp a number-field value into the accepted interval range.
pub(super) fn hours_from_field(hours: f64) -> u64 {
    if !hours.is_finite() {
        return 1;
    }
    (hours.round().max(1.0) as u64).min(MAX_CHECK_INTERVAL_HOURS)
}

fn channel_item(config: UpdateConfig) -> SettingItem {
    let options = vec![
        (
            SharedString::from(CHANNEL_STABLE),
            SharedString::from("Stable"),
        ),
        (
            SharedString::from(CHANNEL_PREVIEW),
            SharedString::from("Preview (includes prereleases)"),
        ),
    ];
    SettingItem::new(
        "Channel",
        SettingField::dropdown(
            options,
            move |_cx| SharedString::from(channel_key(config.channel)),
            |value, cx| {
                update_preference(cx, |c| c.channel = channel_from_key(value.as_ref()));
            },
        ),
    )
    .description("Preview also offers GitHub prereleases; drafts are never offered.")
}

pub(super) fn channel_key(channel: UpdateChannel) -> &'static str {
    match channel {
        UpdateChannel::Stable => CHANNEL_STABLE,
        UpdateChannel::Preview => CHANNEL_PREVIEW,
    }
}

pub(super) fn channel_from_key(key: &str) -> UpdateChannel {
    if key == CHANNEL_PREVIEW {
        UpdateChannel::Preview
    } else {
        UpdateChannel::Stable
    }
}

fn skipped_version_item(skipped: String) -> SettingItem {
    let label = format!("OneTerm {skipped} is skipped and will not be offered again.");
    SettingItem::new(
        "Skipped Version",
        SettingField::element(
            move |_options: &RenderOptions, _window: &mut Window, cx: &mut App| {
                h_flex()
                    .w_full()
                    .gap_2()
                    .items_center()
                    .justify_end()
                    .child(Label::new(label.clone()).text_color(cx.theme().muted_foreground))
                    .child(
                        Button::new("update-clear-skipped-version")
                            .ghost()
                            .small()
                            .label("Clear")
                            .on_click(|_, _, cx| {
                                update_preference(cx, |c| c.skipped_version = None)
                            }),
                    )
                    .into_any_element()
            },
        ),
    )
    .description("Clear to be offered this version again.")
}

fn proxy_item(config: UpdateConfig) -> SettingItem {
    SettingItem::new(
        "Proxy URL",
        SettingField::input(
            move |_cx| config.proxy_url.clone().unwrap_or_default().into(),
            |value, cx| {
                update_preference(cx, |c| {
                    c.proxy_url = (!value.is_empty()).then(|| value.to_string());
                });
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
                update_preference(cx, |c| c.verify_certificates = checked);
            },
        ),
    )
    .description(
        "Verify GitHub TLS certificates. Turn off only on a trusted network that intercepts TLS.",
    )
}

/// Red banner shown while certificate verification is off (SEC-18): the
/// SHA-256 digest travels over the same connection, so it no longer proves
/// anything about the archive.
fn insecure_certificates_warning() -> SettingItem {
    SettingItem::render(
        |_options: &RenderOptions, _window: &mut Window, cx: &mut App| {
            v_flex()
                .w_full()
                .gap_1()
                .child(
                    Label::new("Insecure: certificate verification is disabled")
                        .text_sm()
                        .text_color(cx.theme().danger),
                )
                .child(
                    Label::new(
                        "Anyone on the network path can serve a forged release list, archive, \
                         and matching checksum. Downloaded updates are not authenticated \
                         until this is re-enabled.",
                    )
                    .text_sm()
                    .text_color(cx.theme().danger),
                )
                .into_any_element()
        },
    )
    .keywords(["insecure", "certificate", "tls"])
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_keys_round_trip() {
        assert_eq!(
            channel_from_key(channel_key(UpdateChannel::Preview)),
            UpdateChannel::Preview
        );
        assert_eq!(
            channel_from_key(channel_key(UpdateChannel::Stable)),
            UpdateChannel::Stable
        );
        assert_eq!(channel_from_key("garbage"), UpdateChannel::Stable);
    }

    #[test]
    fn interval_field_values_are_rounded_and_clamped() {
        assert_eq!(hours_from_field(0.0), 1);
        assert_eq!(hours_from_field(-5.0), 1);
        assert_eq!(hours_from_field(23.6), 24);
        assert_eq!(hours_from_field(f64::NAN), 1);
        assert_eq!(hours_from_field(1e12), MAX_CHECK_INTERVAL_HOURS);
    }
}
