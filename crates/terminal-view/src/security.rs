//! The user's [`TerminalSecurityPolicy`], derived from [`TerminalSettings`].
//!
//! One derivation feeds both sinks of terminal-controlled data (SEC-08): the
//! backend's `OscRouter` (through the session factory, at spawn/connect time)
//! and the view's own notification queue.

use gpui::App;

use oneterm_settings::TerminalSettings;
use oneterm_terminal::TerminalSecurityPolicy;

/// Build the security policy for a session from the live settings.
///
/// Only the user-facing knobs are mapped; the size caps and the notification
/// rate keep the policy defaults.
pub(crate) fn security_policy_from_settings(settings: &TerminalSettings) -> TerminalSecurityPolicy {
    TerminalSecurityPolicy {
        allow_remote_clipboard_read: settings.allow_clipboard_read,
        ..TerminalSecurityPolicy::default()
    }
}

/// The security policy for a new session, read from the global settings.
/// For callers that create sessions outside the terminal panel (SSH connect
/// dialogs) and therefore have only an `App`.
pub fn terminal_security_policy(cx: &App) -> TerminalSecurityPolicy {
    security_policy_from_settings(TerminalSettings::global(cx).read(cx))
}

#[cfg(test)]
mod tests {
    use oneterm_settings::TerminalSettings;

    use super::security_policy_from_settings;

    #[test]
    fn clipboard_read_setting_drives_the_remote_read_flag() {
        let mut settings = TerminalSettings::default();
        assert!(!security_policy_from_settings(&settings).allow_remote_clipboard_read);
        settings.allow_clipboard_read = true;
        let policy = security_policy_from_settings(&settings);
        assert!(policy.allow_remote_clipboard_read);
        // Writes stay default-off: no setting exposes them.
        assert!(!policy.allow_remote_clipboard_write);
    }
}
