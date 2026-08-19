//! UI-level actions for OneTerm.

use gpui::{SharedString, actions};
use gpui_component::ThemeMode;
use serde::Deserialize;

// Re-exported so call-sites can use `oneterm_actions::RightDockMode` without
// reaching into `oneterm_core` directly. Defined in `oneterm_core` (the lowest
// crate that needs it) so the settings crate can persist it without a
// same-layer dependency on `oneterm_actions`.
pub use oneterm_core::RightDockMode;

/// Add a new TerminalPanel to the center dock.
#[derive(Clone, PartialEq, Eq, Deserialize, gpui::Action)]
#[action(namespace = oneterm, no_json)]
pub struct AddPanel;

/// Add a new TerminalPanel to the center dock with a specific shell kind.
#[derive(Clone, PartialEq, Eq, Deserialize, gpui::Action)]
#[action(namespace = oneterm, no_json)]
pub struct AddPanelWithShell(pub oneterm_core::ShellKind);

/// Swap the right dock to show the panels for the given [`RightDockMode`].
///
/// Dispatched by the title bar mode toggle group; handled by the workspace,
/// which rebuilds the right dock `DockItem` and persists the choice.
#[derive(Clone, PartialEq, Eq, Deserialize, gpui::Action)]
#[action(namespace = oneterm, no_json)]
pub struct SetRightDockMode(pub RightDockMode);

actions!(
    oneterm,
    [
        /// Quit the application.
        Quit,
        /// Open the About dialog.
        About,
        /// Toggle the gutter (timestamp + line number) in the terminal.
        ToggleGutter,
        /// Open the dialog to create a new SSH session (saved to `ssh_session.json`).
        NewSession,
        /// Open the General Settings panel (font, theme, key bindings).
        OpenSettings,
        /// Activate the in-terminal search bar (Find).
        Find,
        // ── Terminal context-menu actions ───────────────────────────
        /// Duplicate the session in the active terminal Space.
        DuplicateSession,
        /// Split the active terminal Space to the right.
        SplitRight,
        /// Split the active terminal Space to the left.
        SplitLeft,
        /// Split the active terminal Space upward.
        SplitUp,
        /// Split the active terminal Space downward.
        SplitDown,
        /// Copy the terminal selection to the clipboard.
        TerminalCopy,
        /// Paste the clipboard contents into the active terminal.
        TerminalPaste,
        /// Select all text in the active terminal.
        TerminalSelectAll,
        /// Clear the active terminal screen.
        TerminalClear,
        /// Close the active terminal Space (not the whole tab).
        CloseSpace,
        // ── Session tabs context-menu actions ───────────────────────
        /// Open the connect dialog for the selected session.
        OpenSession,
        /// Delete the selected session from the store.
        DeleteSession,
        /// Open the property dialog for the selected session.
        SessionProperty,
        // ── SFTP context-menu actions ───────────────────────────────
        /// Open the selected file/folder in the SFTP browser.
        SftpOpen,
        /// Edit the selected remote file locally (download, open in editor).
        SftpEdit,
        /// Download the selected file/folder from the remote.
        SftpDownload,
        /// Rename the selected file/folder.
        SftpRename,
        /// Delete the selected file/folder.
        SftpDelete,
        /// Show properties of the selected file/folder.
        SftpProperties,
        /// Upload files to the current remote directory.
        SftpUploadFiles,
        /// Upload a folder to the current remote directory.
        SftpUploadFolder,
        /// Create a new folder in the current remote directory.
        SftpNewFolder,
        /// Refresh the SFTP file listing.
        SftpRefresh,
        // ── Completion actions ──────────────────────────────────────
        /// Force-open the completion overlay at the cursor (default Ctrl+Shift+Space).
        TriggerCompletion,
    ]
);

/// Switch theme (by name registered in `ThemeRegistry`).
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = oneterm, no_json)]
pub struct SwitchTheme(pub SharedString);

/// Switch theme mode (Light/Dark).
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = oneterm, no_json)]
pub struct SwitchThemeMode(pub ThemeMode);
