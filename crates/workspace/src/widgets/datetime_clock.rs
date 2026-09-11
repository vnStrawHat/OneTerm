//! Datetime clock shown on the left side of the status bar, refreshed every 1s.

use std::time::Duration;

use chrono::Local;
use gpui::{App, Entity, Window};
use gpui_component::Icon;
use oneterm_theme::AppIcon;

use super::status_text::{Label, StatusText};

/// Clock that shows the local time, refreshed every 1 second.
pub fn datetime_clock(window: &mut Window, cx: &mut App) -> Entity<StatusText> {
    StatusText::new_entity(
        "datetime-clock",
        Duration::from_secs(1),
        false,
        Some(Icon::new(AppIcon::Clock)),
        Box::new(|_| {
            Some(Label::from(
                Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            ))
        }),
        window,
        cx,
    )
}
