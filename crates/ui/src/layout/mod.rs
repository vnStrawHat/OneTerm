//! Layout chính của OneTerm.

pub mod app_menus;
pub mod statusbar;
pub mod title_bar;
pub mod workspace;

pub use workspace::{OneTermWorkspace, save_dock_state_on_close};
