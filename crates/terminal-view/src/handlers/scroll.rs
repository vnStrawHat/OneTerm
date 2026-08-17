//! Scroll wheel handler for `LocalTerminalView`.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{App, Entity, InteractiveElement as _, ScrollDelta, ScrollWheelEvent};

use oneterm_terminal::TerminalSession;

use super::super::element::RenderCache;
use super::super::view::LocalTerminalView;
use super::super::view::grid::pixel_to_grid;
use oneterm_settings::TerminalSettings;

/// Attach the scroll wheel handler.
pub(crate) fn attach_scroll(
    div: gpui::Stateful<gpui::Div>,
    session: Entity<Box<dyn TerminalSession>>,
    render_cache: Rc<RefCell<RenderCache>>,
    view: Entity<LocalTerminalView>,
) -> gpui::Stateful<gpui::Div> {
    div.on_scroll_wheel({
        let s = session.clone();
        let cache = render_cache.clone();
        let view = view.clone();
        move |e: &ScrollWheelEvent, _w, cx: &mut App| {
            let metrics = cache.borrow().metrics;
            let (row, col) = match pixel_to_grid(&metrics, e.position) {
                Some(rc) => rc,
                None => return,
            };
            let line_h = f32::from(metrics.line_height);
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
                let mods = oneterm_terminal::mouse_encode::MouseModifiers {
                    shift: e.modifiers.shift,
                    alt: e.modifiers.alt,
                    ctrl: e.modifiers.control,
                };
                s.update(cx, |s, _| s.wheel(delta_y as f64, row, col, mods));
                // Re-render + update scrollbar visibility.
                let _ = view.update(cx, |v, cx| {
                    v.scrollbar.mark_scrolled();
                    cx.notify();
                });
            }
        }
    })
}
