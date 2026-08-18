//! Breadcrumb (cwd path) of the active terminal session, shown in the StatusBar.
//!
//! Refreshes every 500ms — the cwd (OSC 7) updates asynchronously from the PTY
//! listener. Hidden when no active terminal has a breadcrumb (e.g. no cwd yet).

use std::time::Duration;

use gpui::{App, Entity, WeakEntity, Window};
use gpui_component::dock::DockArea;

use super::status_text::StatusText;

/// Indicator showing the breadcrumb (cwd path) of the active terminal session.
pub fn breadcrumb(
    dock_area: WeakEntity<DockArea>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<StatusText> {
    StatusText::new_entity(
        "breadcrumb-indicator",
        Duration::from_millis(500),
        true,
        Box::new(move |cx| {
            let dock_area = dock_area.upgrade()?;
            oneterm_state::active_terminal::breadcrumb(&dock_area, cx)
        }),
        window,
        cx,
    )
}
