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
        // The outcome would otherwise only be visible in the About status text
        // (CORR-69): tell the user where the package is.
        UpdateUiStatus::ManualInstall(package_dir) => window.push_notification(
            notify_with_title(
                NotificationType::Warning,
                format!(
                    "The install location is not writable. Install the update manually from {}.",
                    package_dir.display()
                ),
                "Manual install required",
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
