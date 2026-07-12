//! The curated registry of rebindable actions.
//!
//! Each [`BindableAction`] entry maps an action id to its label, settings group,
//! built-in default keystroke, optional gpui key context, and a constructor that
//! builds a [`KeyBinding`] from a keystroke string. The array order is the
//! display order in the Key Bindings settings page.

use gpui::{Action, KeyBinding, Keystroke};

use crate::actions::{
    About, AddPanel, NewSession, OpenSettings, Quit, ToggleAutoHideRightDock, ToggleGutter,
};

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
        make: |ks, ctx| {
            make_binding(
                ks,
                AddPanel(gpui_component::dock::DockPlacement::Center),
                ctx,
            )
        },
        name_fn: <AddPanel as Action>::name_for_type,
    },
    BindableAction {
        id: "new_ssh_session",
        label: "New SSH Session",
        group: "App Menu",
        default: Some("ctrl-s"),
        context: None,
        make: |ks, ctx| make_binding(ks, NewSession, ctx),
        name_fn: <NewSession as Action>::name_for_type,
    },
    BindableAction {
        id: "toggle_gutter",
        label: "Toggle Gutter",
        group: "App Menu",
        default: Some("ctrl-g"),
        context: None,
        make: |ks, ctx| make_binding(ks, ToggleGutter, ctx),
        name_fn: <ToggleGutter as Action>::name_for_type,
    },
    BindableAction {
        id: "auto_hide_right_dock",
        label: "Auto-hide Right Dock",
        group: "App Menu",
        default: Some("ctrl-h"),
        context: None,
        make: |ks, ctx| make_binding(ks, ToggleAutoHideRightDock, ctx),
        name_fn: <ToggleAutoHideRightDock as Action>::name_for_type,
    },
    BindableAction {
        id: "about",
        label: "About OneTerm",
        group: "App Menu",
        default: Some("ctrl-space"),
        context: None,
        make: |ks, ctx| make_binding(ks, About, ctx),
        name_fn: <About as Action>::name_for_type,
    },
    BindableAction {
        id: "quit",
        label: "Quit",
        group: "App Menu",
        default: Some("ctrl-q"),
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
        make: |ks, ctx| make_binding(ks, crate::actions::TerminalCopy, ctx),
        name_fn: <crate::actions::TerminalCopy as Action>::name_for_type,
    },
    BindableAction {
        id: "terminal_paste",
        label: "Paste",
        group: "Edit Menu",
        default: Some("ctrl-shift-v"),
        context: None,
        make: |ks, ctx| make_binding(ks, crate::actions::TerminalPaste, ctx),
        name_fn: <crate::actions::TerminalPaste as Action>::name_for_type,
    },
    BindableAction {
        id: "find",
        label: "Find",
        group: "Edit Menu",
        default: Some("ctrl-f"),
        context: None,
        make: |ks, ctx| make_binding(ks, crate::actions::Find, ctx),
        name_fn: <crate::actions::Find as Action>::name_for_type,
    },
    BindableAction {
        id: "terminal_select_all",
        label: "Select All",
        group: "Edit Menu",
        default: Some("ctrl-shift-a"),
        context: None,
        make: |ks, ctx| make_binding(ks, crate::actions::TerminalSelectAll, ctx),
        name_fn: <crate::actions::TerminalSelectAll as Action>::name_for_type,
    },
    BindableAction {
        id: "terminal_clear",
        label: "Clear",
        group: "Edit Menu",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, crate::actions::TerminalClear, ctx),
        name_fn: <crate::actions::TerminalClear as Action>::name_for_type,
    },
    // ── Terminal context-menu actions ────────────────────────────
    BindableAction {
        id: "split_right",
        label: "Split Right",
        group: "Terminal Context Menu",
        default: Some("ctrl-shift-right"),
        context: None,
        make: |ks, ctx| make_binding(ks, crate::actions::SplitRight, ctx),
        name_fn: <crate::actions::SplitRight as Action>::name_for_type,
    },
    BindableAction {
        id: "split_left",
        label: "Split Left",
        group: "Terminal Context Menu",
        default: Some("ctrl-shift-left"),
        context: None,
        make: |ks, ctx| make_binding(ks, crate::actions::SplitLeft, ctx),
        name_fn: <crate::actions::SplitLeft as Action>::name_for_type,
    },
    BindableAction {
        id: "split_up",
        label: "Split Up",
        group: "Terminal Context Menu",
        default: Some("ctrl-shift-up"),
        context: None,
        make: |ks, ctx| make_binding(ks, crate::actions::SplitUp, ctx),
        name_fn: <crate::actions::SplitUp as Action>::name_for_type,
    },
    BindableAction {
        id: "split_down",
        label: "Split Down",
        group: "Terminal Context Menu",
        default: Some("ctrl-shift-down"),
        context: None,
        make: |ks, ctx| make_binding(ks, crate::actions::SplitDown, ctx),
        name_fn: <crate::actions::SplitDown as Action>::name_for_type,
    },
    BindableAction {
        id: "close_space",
        label: "Close Space",
        group: "Terminal Context Menu",
        default: Some("ctrl-shift-x"),
        context: None,
        make: |ks, ctx| make_binding(ks, crate::actions::CloseSpace, ctx),
        name_fn: <crate::actions::CloseSpace as Action>::name_for_type,
    },
    // ── Session tabs context-menu actions ────────────────────────
    BindableAction {
        id: "open_session",
        label: "Open Session",
        group: "Session Tabs Context Menu",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, crate::actions::OpenSession, ctx),
        name_fn: <crate::actions::OpenSession as Action>::name_for_type,
    },
    BindableAction {
        id: "delete_session",
        label: "Delete Session",
        group: "Session Tabs Context Menu",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, crate::actions::DeleteSession, ctx),
        name_fn: <crate::actions::DeleteSession as Action>::name_for_type,
    },
    BindableAction {
        id: "session_property",
        label: "Session Property",
        group: "Session Tabs Context Menu",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, crate::actions::SessionProperty, ctx),
        name_fn: <crate::actions::SessionProperty as Action>::name_for_type,
    },
    // ── SFTP context-menu actions ────────────────────────────────
    BindableAction {
        id: "sftp_open",
        label: "SFTP Open",
        group: "SFTP Context Menu",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, crate::actions::SftpOpen, ctx),
        name_fn: <crate::actions::SftpOpen as Action>::name_for_type,
    },
    BindableAction {
        id: "sftp_download",
        label: "SFTP Download",
        group: "SFTP Context Menu",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, crate::actions::SftpDownload, ctx),
        name_fn: <crate::actions::SftpDownload as Action>::name_for_type,
    },
    BindableAction {
        id: "sftp_rename",
        label: "SFTP Rename",
        group: "SFTP Context Menu",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, crate::actions::SftpRename, ctx),
        name_fn: <crate::actions::SftpRename as Action>::name_for_type,
    },
    BindableAction {
        id: "sftp_delete",
        label: "SFTP Delete",
        group: "SFTP Context Menu",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, crate::actions::SftpDelete, ctx),
        name_fn: <crate::actions::SftpDelete as Action>::name_for_type,
    },
    BindableAction {
        id: "sftp_properties",
        label: "SFTP Properties",
        group: "SFTP Context Menu",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, crate::actions::SftpProperties, ctx),
        name_fn: <crate::actions::SftpProperties as Action>::name_for_type,
    },
    BindableAction {
        id: "sftp_upload_files",
        label: "SFTP Upload Files",
        group: "SFTP Context Menu",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, crate::actions::SftpUploadFiles, ctx),
        name_fn: <crate::actions::SftpUploadFiles as Action>::name_for_type,
    },
    BindableAction {
        id: "sftp_upload_folder",
        label: "SFTP Upload Folder",
        group: "SFTP Context Menu",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, crate::actions::SftpUploadFolder, ctx),
        name_fn: <crate::actions::SftpUploadFolder as Action>::name_for_type,
    },
    BindableAction {
        id: "sftp_new_folder",
        label: "SFTP New Folder",
        group: "SFTP Context Menu",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, crate::actions::SftpNewFolder, ctx),
        name_fn: <crate::actions::SftpNewFolder as Action>::name_for_type,
    },
    BindableAction {
        id: "sftp_refresh",
        label: "SFTP Refresh",
        group: "SFTP Context Menu",
        default: None,
        context: None,
        make: |ks, ctx| make_binding(ks, crate::actions::SftpRefresh, ctx),
        name_fn: <crate::actions::SftpRefresh as Action>::name_for_type,
    },
];
