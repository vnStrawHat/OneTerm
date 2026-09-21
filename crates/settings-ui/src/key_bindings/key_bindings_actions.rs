//! The curated registry of rebindable actions.
//!
//! Each [`BindableAction`] entry maps an action id to its label, settings group,
//! built-in default keystroke, optional gpui key context, and a constructor that
//! builds a [`KeyBinding`] from a keystroke string. The array order is the
//! display order in the Key Bindings settings page.
//!
//! A default never takes a bare `Ctrl` plus a letter, digit or Space that
//! carries a terminal control character: in a terminal that keystroke belongs to
//! the foreground program. Every row here has `context: None`, so the rule
//! reaches every group and not only "App Menu". Four defaults are accepted
//! exceptions — `ctrl-w` (Close Panel), `ctrl-t` (New Terminal Tab), `ctrl-f`
//! (Find) and `ctrl-,` (Open Settings) — and
//! `the_only_bare_ctrl_defaults_are_the_ones_dec_0018_accepted` asserts that set
//! exactly. A fifth needs
//! `docs/decisions/DEC-0018-app-shortcuts-leave-single-ctrl-keys-to-the-terminal.md`
//! amended, not a comment here.

use gpui::{Action, KeyBinding, Keystroke};

use oneterm_actions::{About, AddPanel, NewSession, OpenSettings, Quit, ToggleGutter};
use oneterm_core::InputChannel;

// ── Bindable action registry ─────────────────────────────────────────

/// One rebindable action.
///
/// `make` builds a `KeyBinding` for the given keystroke string (returning `None`
/// if the keystroke is empty or fails to parse, since `KeyBinding::new` panics on
/// parse errors). `context` is the optional gpui key context (e.g. `"Input"`)
/// the binding should be scoped to; `None` means a global binding. `name_fn`
/// returns the action's registered name — used by [`super::apply_key_bindings`] to
/// filter stale defaults out of the gpui-component snapshot.
pub(super) struct BindableAction {
    pub id: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    pub default: Option<&'static str>,
    /// Optional gpui key context (e.g. `Some("Input")` for text-field actions).
    pub context: Option<&'static str>,
    /// Build a `KeyBinding` from a keystroke string + the action's context.
    pub make: fn(&str, Option<&str>) -> Option<KeyBinding>,
    /// Return the action's registered name (e.g. `"input::Copy"`).
    pub name_fn: fn() -> &'static str,
}

/// Build a `KeyBinding` for `action` at `ks` with optional `context`, validating
/// first (empty → unbound, unparseable → ignored) so `KeyBinding::new` never
/// panics.
fn make_binding<A: Action>(ks: &str, action: A, context: Option<&str>) -> Option<KeyBinding> {
    if ks.is_empty() || Keystroke::parse(ks).is_err() {
        return None;
    }
    Some(KeyBinding::new(ks, action, context))
}

/// The curated set of rebindable actions (order = display order).
pub(super) const BINDABLE_ACTIONS: &[BindableAction] = &[
    // ── App / workspace ───────────────────────────────────────────
    BindableAction {
        id: "toggle_zoom",
        label: "Zoom Active Panel",
        group: "App Menu",
        default: Some("shift-escape"),
        context: None,
        make: |ks, ctx| make_binding(ks, gpui_component::dock::ToggleZoom, ctx),
        name_fn: <gpui_component::dock::ToggleZoom as Action>::name_for_type,
    },
    BindableAction {
        id: "close_panel",
        label: "Close Panel",
        group: "App Menu",
        default: Some("ctrl-w"),
        context: None,
        make: |ks, ctx| make_binding(ks, gpui_component::dock::ClosePanel, ctx),
        name_fn: <gpui_component::dock::ClosePanel as Action>::name_for_type,
    },
    BindableAction {
        id: "new_terminal_tab",
        label: "New Terminal Tab",
        group: "App Menu",
        default: Some("ctrl-t"),
        context: None,
        make: |ks, ctx| make_binding(ks, AddPanel, ctx),
        name_fn: <AddPanel as Action>::name_for_type,
    },
    BindableAction {
        id: "new_ssh_session",
        label: "New SSH Session",
        group: "App Menu",
        // `ctrl-s` until `DEC-0018`: `^S` is XOFF and belongs to the terminal.
        default: Some("ctrl-shift-n"),
        context: None,
        make: |ks, ctx| make_binding(ks, NewSession, ctx),
        name_fn: <NewSession as Action>::name_for_type,
    },
    BindableAction {
        id: "toggle_gutter",
        label: "Toggle Gutter",
        group: "App Menu",
        // `ctrl-g` until `DEC-0018`: `^G` is BEL and the readline/Emacs abort.
        // Ships unbound rather than moved — a view toggle with no other entry
        // point buys nothing with a default keystroke.
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, ToggleGutter, ctx),
        name_fn: <ToggleGutter as Action>::name_for_type,
    },
    BindableAction {
        id: "about",
        label: "About OneTerm",
        group: "App Menu",
        // `ctrl-space` until `DEC-0018`: `^@` is set-mark, and the IME toggle
        // on several input methods. Then `f1` until the owner's amendment of
        // 2026-09-17: a bound `F1` never reaches the foreground program, so it
        // was taken from the help key of `mc`, `nano`, `htop`, `vim` and `less`.
        // Ships unbound rather than moved a second time, for Toggle Gutter's
        // reason — About is the first item of the application menu, and a
        // default keystroke buys a twice-in-a-lifetime dialog nothing.
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, About, ctx),
        name_fn: <About as Action>::name_for_type,
    },
    BindableAction {
        id: "quit",
        label: "Quit",
        group: "App Menu",
        // `ctrl-q` until `DEC-0018`: `^Q` is XON and belongs to the terminal.
        default: Some("ctrl-shift-q"),
        context: None,
        make: |ks, ctx| make_binding(ks, Quit, ctx),
        name_fn: <Quit as Action>::name_for_type,
    },
    BindableAction {
        id: "open_settings",
        label: "Open Settings",
        group: "App Menu",
        default: Some("ctrl-,"),
        context: None,
        make: |ks, ctx| make_binding(ks, OpenSettings, ctx),
        name_fn: <OpenSettings as Action>::name_for_type,
    },
    // ── Edit menu — terminal-scoped actions (shared with context menu) ──
    // These dispatch the same Terminal* actions as the right-click context
    // menu, so key bindings apply uniformly regardless of entry point.
    BindableAction {
        id: "terminal_copy",
        label: "Copy",
        group: "Edit Menu",
        default: Some("ctrl-shift-c"),
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::TerminalCopy, ctx),
        name_fn: <oneterm_actions::TerminalCopy as Action>::name_for_type,
    },
    BindableAction {
        id: "terminal_paste",
        label: "Paste",
        group: "Edit Menu",
        default: Some("ctrl-shift-v"),
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::TerminalPaste, ctx),
        name_fn: <oneterm_actions::TerminalPaste as Action>::name_for_type,
    },
    BindableAction {
        id: "find",
        label: "Find",
        group: "Edit Menu",
        default: Some("ctrl-f"),
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::Find, ctx),
        name_fn: <oneterm_actions::Find as Action>::name_for_type,
    },
    BindableAction {
        id: "terminal_select_all",
        label: "Select All",
        group: "Edit Menu",
        default: Some("ctrl-shift-a"),
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::TerminalSelectAll, ctx),
        name_fn: <oneterm_actions::TerminalSelectAll as Action>::name_for_type,
    },
    BindableAction {
        id: "terminal_clear",
        label: "Clear",
        group: "Edit Menu",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::TerminalClear, ctx),
        name_fn: <oneterm_actions::TerminalClear as Action>::name_for_type,
    },
    // ── Terminal context-menu actions ────────────────────────────
    BindableAction {
        id: "duplicate_session",
        label: "Duplicate Session",
        group: "Terminal Context Menu",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::DuplicateSession, ctx),
        name_fn: <oneterm_actions::DuplicateSession as Action>::name_for_type,
    },
    BindableAction {
        id: "split_right",
        label: "Split Right",
        group: "Terminal Context Menu",
        default: Some("ctrl-shift-right"),
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::SplitRight, ctx),
        name_fn: <oneterm_actions::SplitRight as Action>::name_for_type,
    },
    BindableAction {
        id: "split_left",
        label: "Split Left",
        group: "Terminal Context Menu",
        default: Some("ctrl-shift-left"),
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::SplitLeft, ctx),
        name_fn: <oneterm_actions::SplitLeft as Action>::name_for_type,
    },
    BindableAction {
        id: "split_up",
        label: "Split Up",
        group: "Terminal Context Menu",
        default: Some("ctrl-shift-up"),
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::SplitUp, ctx),
        name_fn: <oneterm_actions::SplitUp as Action>::name_for_type,
    },
    BindableAction {
        id: "split_down",
        label: "Split Down",
        group: "Terminal Context Menu",
        default: Some("ctrl-shift-down"),
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::SplitDown, ctx),
        name_fn: <oneterm_actions::SplitDown as Action>::name_for_type,
    },
    BindableAction {
        id: "close_space",
        label: "Close Space",
        group: "Terminal Context Menu",
        default: Some("ctrl-shift-x"),
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::CloseSpace, ctx),
        name_fn: <oneterm_actions::CloseSpace as Action>::name_for_type,
    },
    // ── Broadcast input channel actions ──────────────────────────
    BindableAction {
        id: "join_input_channel_a",
        label: "Join Input Channel A",
        group: "Input Channel",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::JoinInputChannel(InputChannel::A), ctx),
        name_fn: <oneterm_actions::JoinInputChannel as Action>::name_for_type,
    },
    BindableAction {
        id: "join_input_channel_b",
        label: "Join Input Channel B",
        group: "Input Channel",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::JoinInputChannel(InputChannel::B), ctx),
        name_fn: <oneterm_actions::JoinInputChannel as Action>::name_for_type,
    },
    BindableAction {
        id: "join_input_channel_c",
        label: "Join Input Channel C",
        group: "Input Channel",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::JoinInputChannel(InputChannel::C), ctx),
        name_fn: <oneterm_actions::JoinInputChannel as Action>::name_for_type,
    },
    BindableAction {
        id: "join_input_channel_d",
        label: "Join Input Channel D",
        group: "Input Channel",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::JoinInputChannel(InputChannel::D), ctx),
        name_fn: <oneterm_actions::JoinInputChannel as Action>::name_for_type,
    },
    BindableAction {
        id: "join_input_channel_e",
        label: "Join Input Channel E",
        group: "Input Channel",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::JoinInputChannel(InputChannel::E), ctx),
        name_fn: <oneterm_actions::JoinInputChannel as Action>::name_for_type,
    },
    BindableAction {
        id: "leave_input_channel",
        label: "Leave Input Channel",
        group: "Input Channel",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::LeaveInputChannel, ctx),
        name_fn: <oneterm_actions::LeaveInputChannel as Action>::name_for_type,
    },
    BindableAction {
        id: "close_input_channel",
        label: "Close Input Channel",
        group: "Input Channel",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::CloseInputChannel, ctx),
        name_fn: <oneterm_actions::CloseInputChannel as Action>::name_for_type,
    },
    // ── Session tabs context-menu actions ────────────────────────
    BindableAction {
        id: "open_session",
        label: "Open Session",
        group: "Session Tabs Context Menu",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::OpenSession, ctx),
        name_fn: <oneterm_actions::OpenSession as Action>::name_for_type,
    },
    BindableAction {
        id: "delete_session",
        label: "Delete Session",
        group: "Session Tabs Context Menu",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::DeleteSession, ctx),
        name_fn: <oneterm_actions::DeleteSession as Action>::name_for_type,
    },
    BindableAction {
        id: "session_property",
        label: "Session Properties",
        group: "Session Tabs Context Menu",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::SessionProperty, ctx),
        name_fn: <oneterm_actions::SessionProperty as Action>::name_for_type,
    },
    // ── SFTP context-menu actions ────────────────────────────────
    BindableAction {
        id: "sftp_open",
        label: "SFTP Open",
        group: "SFTP Context Menu",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::SftpOpen, ctx),
        name_fn: <oneterm_actions::SftpOpen as Action>::name_for_type,
    },
    BindableAction {
        id: "sftp_download",
        label: "SFTP Download",
        group: "SFTP Context Menu",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::SftpDownload, ctx),
        name_fn: <oneterm_actions::SftpDownload as Action>::name_for_type,
    },
    BindableAction {
        id: "sftp_rename",
        label: "SFTP Rename",
        group: "SFTP Context Menu",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::SftpRename, ctx),
        name_fn: <oneterm_actions::SftpRename as Action>::name_for_type,
    },
    BindableAction {
        id: "sftp_delete",
        label: "SFTP Delete",
        group: "SFTP Context Menu",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::SftpDelete, ctx),
        name_fn: <oneterm_actions::SftpDelete as Action>::name_for_type,
    },
    BindableAction {
        id: "sftp_properties",
        label: "SFTP Properties",
        group: "SFTP Context Menu",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::SftpProperties, ctx),
        name_fn: <oneterm_actions::SftpProperties as Action>::name_for_type,
    },
    BindableAction {
        id: "sftp_upload_files",
        label: "SFTP Upload Files",
        group: "SFTP Context Menu",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::SftpUploadFiles, ctx),
        name_fn: <oneterm_actions::SftpUploadFiles as Action>::name_for_type,
    },
    BindableAction {
        id: "sftp_upload_folder",
        label: "SFTP Upload Folder",
        group: "SFTP Context Menu",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::SftpUploadFolder, ctx),
        name_fn: <oneterm_actions::SftpUploadFolder as Action>::name_for_type,
    },
    BindableAction {
        id: "sftp_new_folder",
        label: "SFTP New Folder",
        group: "SFTP Context Menu",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::SftpNewFolder, ctx),
        name_fn: <oneterm_actions::SftpNewFolder as Action>::name_for_type,
    },
    BindableAction {
        id: "sftp_refresh",
        label: "SFTP Refresh",
        group: "SFTP Context Menu",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, oneterm_actions::SftpRefresh, ctx),
        name_fn: <oneterm_actions::SftpRefresh as Action>::name_for_type,
    },
];

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::BINDABLE_ACTIONS;

    /// `MAJ-3`: every rebindable action must be classified for an elevated
    /// window, and every SSH / SFTP / session one must be denied.
    ///
    /// The point of the test is the **first** assertion: a new action added to
    /// `BINDABLE_ACTIONS` and forgotten in `oneterm_actions::elevated_policy`
    /// fails here, so it cannot reach an administrator token by nobody having
    /// thought about it. `new_ssh_session` — bound to `ctrl-shift-n`, global
    /// context — is the one that did.
    #[test]
    fn every_bindable_action_is_classified_for_an_elevated_window() {
        let unclassified: Vec<&str> = BINDABLE_ACTIONS
            .iter()
            .filter(|action| {
                oneterm_actions::action_classified_for_elevated_window(action.id).is_none()
            })
            .map(|action| action.id)
            .collect();
        assert!(
            unclassified.is_empty(),
            "these actions are not classified for an elevated window (add them to \
             oneterm_actions::elevated_policy, denied unless they are local-terminal only): \
             {unclassified:?}"
        );

        // Every id the policy denies must actually exist here, so the table
        // cannot rot into a list of names nothing dispatches.
        for denied in oneterm_actions::denied_when_elevated() {
            assert!(
                BINDABLE_ACTIONS.iter().any(|action| action.id == *denied),
                "{denied} is denied but is not a bindable action any more"
            );
        }

        // And the surfaces M1 names by hand are on the denied side.
        for id in [
            "new_ssh_session",
            "open_session",
            "delete_session",
            "session_property",
            "sftp_open",
            "sftp_upload_files",
            "sftp_delete",
        ] {
            assert_eq!(
                oneterm_actions::action_classified_for_elevated_window(id),
                Some(false),
                "{id} reaches SSH or SFTP and must not run in an elevated window"
            );
        }

        // ...while the terminal keeps working, which is what the window is for.
        for id in ["new_terminal_tab", "terminal_copy", "split_right", "find"] {
            assert_eq!(
                oneterm_actions::action_classified_for_elevated_window(id),
                Some(true),
                "{id} is a local-terminal action and must keep working"
            );
        }
    }

    #[test]
    fn the_app_defaults_are_the_ones_dec_0018_records() {
        let default_for = |id: &str| {
            BINDABLE_ACTIONS
                .iter()
                .find(|action| action.id == id)
                .unwrap_or_else(|| panic!("{id} must be registered"))
                .default
        };
        assert_eq!(default_for("new_ssh_session"), Some("ctrl-shift-n"));
        assert_eq!(default_for("quit"), Some("ctrl-shift-q"));
        // Both ship unbound: Toggle Gutter by `DEC-0018` as accepted, About by
        // its amendment of 2026-09-17.
        assert_eq!(default_for("about"), None);
        assert_eq!(default_for("toggle_gutter"), None);
        // Kept by DEC-0018's explicit exception, not by oversight.
        assert_eq!(default_for("close_panel"), Some("ctrl-w"));
        assert_eq!(default_for("new_terminal_tab"), Some("ctrl-t"));
        // Not in DEC-0018's scope, and no fifth default moved.
        assert_eq!(default_for("toggle_zoom"), Some("shift-escape"));
        assert_eq!(default_for("open_settings"), Some("ctrl-,"));
    }

    /// The bare-`Ctrl` defaults `DEC-0018` accepted as exceptions, with the
    /// keystroke each one keeps.
    ///
    /// Every `BINDABLE_ACTIONS` row has `context: None`, so "App Menu" is a
    /// display heading and not a scope: every default in the registry is global
    /// and every one of them is in the rule's reach. The list is therefore the
    /// whole exception set, not the App Menu's share of it — `find` is an Edit
    /// Menu row and is in it.
    const ACCEPTED_SINGLE_CTRL_DEFAULTS: [(&str, &str); 4] = [
        ("close_panel", "ctrl-w"),
        ("new_terminal_tab", "ctrl-t"),
        ("find", "ctrl-f"),
        ("open_settings", "ctrl-,"),
    ];

    #[test]
    fn the_only_bare_ctrl_defaults_are_the_ones_dec_0018_accepted() {
        let mut found: Vec<(&str, &str)> = BINDABLE_ACTIONS
            .iter()
            .filter_map(|action| Some((action.id, action.default?)))
            .filter(|(_, default)| default.strip_prefix("ctrl-").is_some_and(is_one_key))
            .collect();
        found.sort_unstable();

        let mut accepted = ACCEPTED_SINGLE_CTRL_DEFAULTS;
        accepted.sort_unstable();

        // Not "no row breaks the rule" but "these rows and no others": a fifth
        // bare-Ctrl default added later fails here, and so does removing one of
        // the four without amending `DEC-0018`.
        assert_eq!(
            found.as_slice(),
            accepted.as_slice(),
            "the set of bare-Ctrl defaults changed; amend DEC-0018 rather than this test"
        );
    }

    /// Whether what follows `ctrl-` is one key the terminal reads as a control
    /// character: a letter, a digit, Space, or a punctuation key that produces
    /// one (`ctrl-,` does not, but it is in the accepted list anyway, so the
    /// predicate stays simple and the list does the deciding).
    fn is_one_key(rest: &str) -> bool {
        rest == "space" || rest.chars().count() == 1
    }

    #[test]
    fn no_two_shipped_defaults_share_a_keystroke_in_one_context() {
        let mut seen: HashMap<(Option<&str>, gpui::Keystroke), &str> = HashMap::new();
        for action in BINDABLE_ACTIONS {
            let Some(default) = action.default else {
                continue;
            };
            let stroke = gpui::Keystroke::parse(default)
                .unwrap_or_else(|_| panic!("{} defaults to unparseable {default}", action.id));
            if let Some(other) = seen.insert((action.context, stroke), action.id) {
                panic!("{} and {} both default to {default}", other, action.id);
            }
        }
    }

    #[test]
    fn duplicate_session_is_bindable_without_a_default_keystroke() {
        let action = BINDABLE_ACTIONS
            .iter()
            .find(|action| action.id == "duplicate_session")
            .expect("Duplicate Session action must be registered");

        assert_eq!(action.label, "Duplicate Session");
        assert_eq!(action.group, "Terminal Context Menu");
        assert_eq!(action.default, None);
        assert_eq!((action.name_fn)(), "oneterm::DuplicateSession");
    }

    #[test]
    fn the_seven_input_channel_actions_ship_unbound() {
        let ids: Vec<_> = BINDABLE_ACTIONS
            .iter()
            .filter(|action| action.group == "Input Channel")
            .map(|action| {
                assert_eq!(action.default, None, "{} must ship unbound", action.id);
                action.id
            })
            .collect();

        assert_eq!(
            ids,
            vec![
                "join_input_channel_a",
                "join_input_channel_b",
                "join_input_channel_c",
                "join_input_channel_d",
                "join_input_channel_e",
                "leave_input_channel",
                "close_input_channel",
            ]
        );
    }
}
