//! Scroll wheel handler for `LocalTerminalView`.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{App, Entity, InteractiveElement as _, ScrollDelta, ScrollWheelEvent};

use oneterm_core::TerminalSession;

use super::super::element::GridMetrics;
use super::super::view::LocalTerminalView;
use crate::state::TerminalSettings;

/// Attach the scroll wheel handler.
pub(crate) fn attach_scroll(
    div: gpui::Stateful<gpui::Div>,
    session: Entity<Box<dyn TerminalSession>>,
    metrics: Rc<RefCell<GridMetrics>>,
    view: Entity<LocalTerminalView>,
) -> gpui::Stateful<gpui::Div> {
    div.on_scroll_wheel({
        let s = session.clone();
        let m = metrics.clone();
        let view = view.clone();
        move |e: &ScrollWheelEvent, _w, cx: &mut App| {
            let (row, col) = match LocalTerminalView::pixel_to_grid(&m.borrow(), e.position) {
                Some(rc) => rc,
                None => return,
            };
            let line_h = f32::from(m.borrow().line_height);
            let delta_y = match e.delta {
                ScrollDelta::Pixels(p) => {
                    if line_h > 0.0 {
                        f32::from(p.y) / line_h
                    } else {
                        0.0
                    }
                }
                ScrollDelta::Lines(l) => l.y,
            };
            // Apply scroll_multiplier setting.
            let multiplier = TerminalSettings::global(cx).read(cx).scroll_multiplier;
            let delta_y = delta_y * multiplier;
            if delta_y.abs() >= 0.001 {
                s.update(cx, |s, _| s.wheel(delta_y as f64, row, col));
                // Re-render + update scrollbar visibility.
                let _ = view.update(cx, |v, cx| {
                    v.last_scroll_time = Some(std::time::Instant::now());
                    cx.notify();
                });
            }
        }
    })
}
