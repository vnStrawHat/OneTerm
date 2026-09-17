//! Breadcrumb (cwd path) of the active terminal session, shown in the StatusBar.
//!
//! Refreshes every 500ms — the cwd (OSC 7) updates asynchronously from the PTY
//! listener. Hidden when no active terminal has a breadcrumb (e.g. no cwd yet).

use std::time::Duration;

use gpui::{App, Entity, WeakEntity, Window};
use gpui_component::{Icon, IconName, dock::DockArea};

use super::status_text::{Budget, Presentation, Shorten, StatusText};

/// Indicator showing the breadcrumb (cwd path) of the active terminal session.
///
/// `budget` is the width the status bar leaves for the path; the bar refreshes
/// it every frame.
pub fn breadcrumb(
    dock_area: WeakEntity<DockArea>,
    budget: Budget,
    window: &mut Window,
    cx: &mut App,
) -> Entity<StatusText> {
    StatusText::new_entity(
        "breadcrumb-indicator",
        Duration::from_millis(500),
        Presentation {
            icon: Some(Icon::new(IconName::FolderOpen)),
            copyable: true,
            // A deep cwd is unbounded, so it is the indicator that gives way
            // (`US-0112`).
            shorten: Shorten::PathTail(budget),
        },
        Box::new(move |cx| {
            let dock_area = dock_area.upgrade()?;
            oneterm_state::active_terminal::breadcrumb(&dock_area, cx).map(Into::into)
        }),
        window,
        cx,
    )
}
