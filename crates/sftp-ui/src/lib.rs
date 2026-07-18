//! SFTP browser — panel for browsing remote files.
//!
//! The original `file_browser.rs` module was split into several submodules
//! to comply with the ~400 lines/file rule (see `docs/agents/structure.md` §2).

mod actions;
mod browser_state;
mod panel;
mod panel_actions;
mod panel_ops;
mod persistence;
mod render;
mod render_transfer;
mod table_delegate;
mod table_delegate_menu;
mod transfer;
mod types;

pub use panel::SftpPanel;

use gpui::App;
use oneterm_ui::dock::register_panel;

/// Initialize the SFTP feature: register the "sftp" dock panel so saved layouts
/// deserialize. Called by the app aggregator.
pub fn init(cx: &mut App) {
    register_panel(cx, "sftp", |_, _, _, window, cx| {
        Box::new(panel::SftpPanel::new_entity(window, cx))
    });
}
