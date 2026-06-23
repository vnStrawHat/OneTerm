//! Terminal panel — mỗi tab là 1 Terminal.

pub mod terminal_element;
pub mod terminal_element_box;
pub mod terminal_element_cell;
pub mod terminal_element_layout;
pub mod terminal_handlers;
pub mod terminal_ime;
pub mod terminal_panel;
pub mod terminal_render;
pub mod terminal_scrollbar;
pub mod terminal_settings_panel;
pub mod terminal_view;
pub mod theme;
pub mod url;

pub use terminal_panel::TerminalPanel;
pub use terminal_settings_panel::TerminalSettingsPanel;
pub use terminal_view::LocalTerminalView;
pub use theme::{TerminalTheme, build_terminal_theme, ensure_minimum_contrast, resolve_cell_color};
