//! Terminal panel — each tab is one Terminal.

pub mod box_drawing;
pub mod cell;
pub mod element;
pub mod handlers;
pub mod ime;
pub mod layout;
pub mod panel;
pub mod render;
pub mod scrollbar;
pub mod search;
pub mod settings_panel;
pub mod theme;
pub mod url;
pub mod view;

pub use panel::TerminalPanel;
pub use settings_panel::TerminalSettingsPanel;
pub use theme::{TerminalTheme, build_terminal_theme, ensure_minimum_contrast, resolve_cell_color};
pub use view::LocalTerminalView;
