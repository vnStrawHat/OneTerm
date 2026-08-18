//! Update status notifications.

use gpui::{App, Window};
use gpui_component::{WindowExt as _, notification::NotificationType};
use oneterm_theme::notif_ext::{notify, notify_with_title};

use super::state::{UpdateUiState, UpdateUiStatus};

pub(super) fn notify_status(state: &UpdateUiState, window: &mut Window, cx: &mut App) {
    match &state.status {
        UpdateUiStatus::Available(version) => window.push_notification(
            notify_with_title(
                NotificationType::Info,
                format!("OneTerm {version} is available."),
                "Update available",
                cx,
            ),
            cx,
        ),

        UpdateUiStatus::Failed(error) => window.push_notification(
            notify(
                NotificationType::Error,
                format!("Update failed: {error}"),
                cx,
            ),
            cx,
        ),
        UpdateUiStatus::RestartScheduled => window.push_notification(
            notify(
                NotificationType::Info,
                "Quit OneTerm to let the update helper complete installation.",
                cx,
            ),
            cx,
        ),
        UpdateUiStatus::Restarted => window.push_notification(
            notify(
                NotificationType::Success,
                "Update installed. Restarting OneTerm.",
                cx,
            ),
            cx,
        ),
        _ => {}
    }
}
