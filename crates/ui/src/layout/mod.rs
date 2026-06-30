//! Layout chính của myTerm2.

pub mod app_menus;
pub mod statusbar;
pub mod title_bar;
pub mod workspace;

pub use workspace::{MyTermWorkspace, save_dock_state_on_close};
