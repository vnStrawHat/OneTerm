//! SSH settings page — connection liveness for newly opened SSH sessions.
//!
//! Reads/writes the `ssh` group of [`TerminalSettings`] and persists to
//! `terminal.json` through the shared [`crate::terminal::set`] helper.

use gpui::App;
use gpui_component::{
    Icon, IconName,
    setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage},
};
use oneterm_core::{
    MAX_SSH_KEEPALIVE_INTERVAL_SECS, MAX_SSH_KEEPALIVE_MAX, MIN_SSH_KEEPALIVE_INTERVAL_SECS,
    MIN_SSH_KEEPALIVE_MAX,
};
use oneterm_settings::TerminalSettings;

use crate::{items_with_separators, terminal::set};

/// Build the "SSH" settings page.
pub(crate) fn page() -> SettingPage {
    SettingPage::new("SSH")
        .resettable(true)
        .icon(Icon::new(IconName::Network))
        .group(group())
}

/// "Connection" group — transport keepalive policy.
fn group() -> SettingGroup {
    SettingGroup::new()
        .title("Connection")
        .description("Keepalive settings applied to newly opened SSH sessions.")
        .items(items_with_separators(vec![
            SettingItem::new(
                "Enable Keepalive",
                SettingField::switch(
                    |cx: &App| TerminalSettings::global(cx).read(cx).ssh.keepalive_enabled,
                    |value, cx| set(cx, move |settings| settings.ssh.keepalive_enabled = value),
                ),
            )
            .description("Detect peers or network paths that stop responding."),
            SettingItem::new(
                "Keepalive Interval (seconds)",
                SettingField::number_input(
                    NumberFieldOptions {
                        min: MIN_SSH_KEEPALIVE_INTERVAL_SECS as f64,
                        max: MAX_SSH_KEEPALIVE_INTERVAL_SECS as f64,
                        step: 1.0,
                    },
                    |cx: &App| {
                        TerminalSettings::global(cx)
                            .read(cx)
                            .ssh
                            .keepalive_interval_secs as f64
                    },
                    |value, cx| {
                        set(cx, move |settings| {
                            settings.ssh.keepalive_interval_secs = (value as u64).clamp(
                                MIN_SSH_KEEPALIVE_INTERVAL_SECS,
                                MAX_SSH_KEEPALIVE_INTERVAL_SECS,
                            );
                        });
                    },
                ),
            )
            .description("Seconds between keepalive requests."),
            SettingItem::new(
                "Keepalive Max",
                SettingField::number_input(
                    NumberFieldOptions {
                        min: MIN_SSH_KEEPALIVE_MAX as f64,
                        max: MAX_SSH_KEEPALIVE_MAX as f64,
                        step: 1.0,
                    },
                    |cx: &App| TerminalSettings::global(cx).read(cx).ssh.keepalive_max as f64,
                    |value, cx| {
                        set(cx, move |settings| {
                            settings.ssh.keepalive_max = (value as usize)
                                .clamp(MIN_SSH_KEEPALIVE_MAX, MAX_SSH_KEEPALIVE_MAX);
                        });
                    },
                ),
            )
            .description("Unanswered requests tolerated before the connection is closed."),
        ]))
}
