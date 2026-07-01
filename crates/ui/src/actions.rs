//! UI-level actions for OneTerm.

use gpui::{SharedString, actions};
use gpui_component::{ThemeMode, dock::DockPlacement};
use serde::Deserialize;

/// Add a new panel to the dock at the given placement.
#[derive(Clone, PartialEq, Eq, Deserialize, gpui::Action)]
#[action(namespace = oneterm, no_json)]
pub struct AddPanel(pub DockPlacement);

actions!(
    oneterm,
    [
        /// Quit the application.
        Quit,
        /// Open the About dialog.
        About,
        /// Toggle the dock toggle button.
        ToggleDockToggleButton,
        /// Toggle the gutter (timestamp + line number) in the terminal.
        ToggleGutter,
        /// Add a new SessionPanel to the right dock.
        AddSession,
        /// Add a new SftpPanel to the right dock.
        AddSftpBrowser,
        /// Open the dialog to create a new SSH session (saved to `ssh_session.json`).
        NewSession,
    ]
);

/// Action to select the UI font size (used by `FontSizeSelector`).
#[derive(Clone, PartialEq, Eq, Deserialize, gpui::Action)]
#[action(namespace = oneterm, no_json)]
pub struct SelectFont(pub usize);

/// Change the UI language.
#[derive(Clone, PartialEq, Eq, Deserialize, gpui::Action)]
#[action(namespace = oneterm, no_json)]
pub struct SelectLocale(pub SharedString);

/// Switch theme (by name registered in `ThemeRegistry`).
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = oneterm, no_json)]
pub struct SwitchTheme(pub SharedString);

/// Switch theme mode (Light/Dark).
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = oneterm, no_json)]
pub struct SwitchThemeMode(pub ThemeMode);
