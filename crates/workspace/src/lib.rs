//! OneTerm application shell.
//!
//! Feature-agnostic app frame: the `DockArea` workspace, title bar, app menus,
//! status bar, status-bar widgets, dock-state persistence, and zoom. It depends
//! only on the low crates (`oneterm-state` / `-settings` / `-actions` / `-theme`)
//! and `gpui` / `gpui-component`; it never depends on a feature panel crate.
//! Feature panels register themselves into the dock at init and the shell builds
//! them by name via the gpui-component `PanelRegistry`.

pub mod layout;
pub mod widgets;

pub use layout::OneTermWorkspace;
