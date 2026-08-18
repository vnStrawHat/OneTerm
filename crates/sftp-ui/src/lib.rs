//! SFTP browser — panel for browsing remote files.

mod actions;
mod browser_state;
mod browser_view;
mod panel;
mod panel_actions;
mod panel_ops;
mod persistence;
mod render;
mod render_transfer;
mod table_delegate;
mod table_delegate_menu;
#[cfg(test)]
mod test_backend;
mod transfer;
mod transfer_queue;
mod types;

pub use panel::SftpPanel;

use gpui::App;
use gpui_component::dock::register_panel;
use oneterm_state::panel_names;

/// Initialize the SFTP feature: install the per-backend browser state store and
/// register the "sftp" dock panel so saved layouts deserialize. Called by the
/// app aggregator.
pub fn init(cx: &mut App) {
    browser_state::SftpBrowserStore::init(cx);
    register_panel(cx, panel_names::SFTP, |dock_area, _, _, window, cx| {
        Box::new(panel::SftpPanel::new_entity_in_workspace(
            dock_area.entity_id(),
            window,
            cx,
        ))
    });
}
