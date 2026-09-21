//! Which actions an elevated OneTerm window may run (`DEC-0019` M1, `IN-0043`).
//!
//! M1 removes SSH, SFTP, saved sessions and the Agent panel from the elevated
//! instance. Removing the *rows* that reach them is not the same as removing the
//! *actions*: a key binding dispatches straight to the handler, and
//! `new_ssh_session` ships bound to `ctrl-shift-n` as a global binding, so the
//! Quick Connect dialog opened in an elevated window with no row involved.
//!
//! So the rule lives here, once, as data — not as a guard repeated in a dozen
//! handlers where the thirteenth is the one that gets forgotten. The table is
//! **exhaustive**: every id in `BINDABLE_ACTIONS` must appear, and
//! `settings-ui`'s `every_bindable_action_is_classified_for_an_elevated_window`
//! fails when one does not. A new action cannot slip through silently; somebody
//! has to decide which side of M1 it is on.
//!
//! This crate is the home because it owns the action vocabulary and sits below
//! every crate that dispatches one — `workspace`, `settings-ui`, `session-ui`
//! and `sftp-ui` are all on layer 3 and may not name each other (R2/R5).

/// Action ids an elevated window must not run: everything that reaches SSH,
/// SFTP, the saved-session store or the Agent panel.
///
/// Keep sorted, and keep the reason obvious from the id.
const DENIED_WHEN_ELEVATED: &[&str] = &[
    // The saved-session surfaces. `new_ssh_session` is the one that shipped
    // reachable: `ctrl-shift-n`, global context, straight into Quick Connect.
    "delete_session",
    "new_ssh_session",
    "open_session",
    "session_property",
    // The SFTP browser's context menu. Under M1 there is no SFTP panel in an
    // elevated window, so these are unreachable by construction today; they are
    // listed anyway, because "unreachable" is a property of the current layout
    // and this is a property of the action.
    "sftp_delete",
    "sftp_download",
    "sftp_new_folder",
    "sftp_open",
    "sftp_properties",
    "sftp_refresh",
    "sftp_rename",
    "sftp_upload_files",
    "sftp_upload_folder",
];

/// Action ids an elevated window may run: the terminal, the window chrome and
/// the local-shell surfaces M1 deliberately keeps.
///
/// Listed rather than defaulted to "allowed", so the exhaustiveness test has
/// something to check against and a new action fails until it is classified.
const ALLOWED_WHEN_ELEVATED: &[&str] = &[
    "about",
    "close_input_channel",
    "close_panel",
    "close_space",
    // Reopens the *active tab's* live session. In an elevated window every tab
    // is a local shell — there is no route to an SSH one — so this duplicates a
    // local shell, which is the thing an elevated window is for.
    "duplicate_session",
    "find",
    "join_input_channel_a",
    "join_input_channel_b",
    "join_input_channel_c",
    "join_input_channel_d",
    "join_input_channel_e",
    "leave_input_channel",
    "new_terminal_tab",
    "open_settings",
    "quit",
    "split_down",
    "split_left",
    "split_right",
    "split_up",
    "terminal_clear",
    "terminal_copy",
    "terminal_paste",
    "terminal_select_all",
    "toggle_gutter",
    "toggle_zoom",
];

/// Whether `action_id` may run in an elevated window.
///
/// `None` means the id is not in the table at all — a programming error the
/// exhaustiveness test catches. Callers treat it as **denied**: an action nobody
/// classified is not one to run under an administrator token.
pub fn action_classified_for_elevated_window(action_id: &str) -> Option<bool> {
    if DENIED_WHEN_ELEVATED.contains(&action_id) {
        return Some(false);
    }
    if ALLOWED_WHEN_ELEVATED.contains(&action_id) {
        return Some(true);
    }
    None
}

/// Whether `action_id` may run given this process's elevation.
///
/// Unelevated: everything, unchanged. Elevated: the table above, with an
/// unclassified id denied.
pub fn action_allowed_when_elevated(action_id: &str) -> bool {
    if !oneterm_core::elevation::is_restricted() {
        return true;
    }
    match action_classified_for_elevated_window(action_id) {
        Some(allowed) => allowed,
        None => {
            log::warn!(
                "action {action_id:?} is not classified for an elevated window; refusing it"
            );
            false
        }
    }
}

/// The ids denied in an elevated window, for the exhaustiveness test.
pub fn denied_when_elevated() -> &'static [&'static str] {
    DENIED_WHEN_ELEVATED
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_id_is_classified_exactly_once() {
        for id in DENIED_WHEN_ELEVATED {
            assert!(
                !ALLOWED_WHEN_ELEVATED.contains(id),
                "{id} is in both tables"
            );
        }
        for table in [DENIED_WHEN_ELEVATED, ALLOWED_WHEN_ELEVATED] {
            let mut sorted = table.to_vec();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), table.len(), "duplicate id in {table:?}");
        }
    }

    #[test]
    fn an_unclassified_action_is_not_allowed_in_an_elevated_window() {
        assert_eq!(
            action_classified_for_elevated_window("brand_new_action"),
            None
        );
        // ...and every SSH/SFTP surface is classified denied.
        for id in DENIED_WHEN_ELEVATED {
            assert_eq!(action_classified_for_elevated_window(id), Some(false));
        }
    }

    /// The **other** direction, and the one the packet's own risk 4 names: "a
    /// guard whose condition is wrong in the other direction".
    ///
    /// A typo in the denied table would silently unbind `Ctrl+T`, copy, paste or
    /// the splits in an elevated window — `apply_key_bindings` re-adds only the
    /// allowed ids, so a misclassified one ends with **no binding at all**. An
    /// elevated window that cannot open a tab is a broken window, and nothing
    /// else here would notice.
    #[test]
    fn the_local_terminal_stays_usable_in_an_elevated_window() {
        for id in [
            // What the window is for.
            "new_terminal_tab",
            "split_left",
            "split_right",
            "split_up",
            "split_down",
            "close_space",
            "close_panel",
            // ...and the things a terminal is useless without.
            "find",
            "terminal_copy",
            "terminal_paste",
            "terminal_select_all",
            "terminal_clear",
            // ...and the window chrome, which M1 never touched.
            "open_settings",
            "about",
            "quit",
            "toggle_zoom",
        ] {
            assert_eq!(
                action_classified_for_elevated_window(id),
                Some(true),
                "{id} must keep working in an elevated window"
            );
        }
    }

    #[test]
    fn an_unelevated_process_may_run_everything() {
        // The process global is `NotElevated` here, so the table is not consulted.
        assert!(action_allowed_when_elevated("new_ssh_session"));
        assert!(action_allowed_when_elevated("brand_new_action"));
    }
}
