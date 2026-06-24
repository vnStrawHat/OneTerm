//! Custom scrollbar overlay cho `LocalTerminalView`.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    App, Context, InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent,
    ParentElement as _, Styled as _, div, px,
};

use super::LocalTerminalView;
use crate::views::terminal::element::GridMetrics;
use crate::views::terminal::theme::TerminalTheme;

impl LocalTerminalView {
    /// Render custom scrollbar — div overlay ở cạnh phải.
    pub(crate) fn render_scrollbar(
        &mut self,
        _theme: &TerminalTheme,
        metrics: &Rc<RefCell<GridMetrics>>,
        cx: &mut Context<LocalTerminalView>,
    ) -> Option<impl IntoElement> {
        let (total, viewport, display_offset, line_h) = self.scroll_handle.state_info();

        if total <= viewport || line_h <= 0.0 {
            return None;
        }

        let max_offset = total.saturating_sub(viewport);
        let thumb_ratio = viewport as f32 / total as f32;
        let track_height = viewport as f32 * line_h;
        let thumb_height = (thumb_ratio * track_height).max(24.0);
        let scroll_fraction = if max_offset > 0 {
            display_offset as f32 / max_offset as f32
        } else {
            0.0
        };
        let thumb_top = (1.0 - scroll_fraction) * (track_height - thumb_height);

        let now = std::time::Instant::now();
        let is_dragging = self.scrollbar_drag_start.is_some();
        let is_visible = is_dragging
            || self
                .last_scroll_time
                .map(|t| now.duration_since(t).as_secs_f32() < 3.0)
                .unwrap_or(false);

        if !is_visible {
            return None;
        }

        let opacity = if is_dragging {
            1.0
        } else {
            self.last_scroll_time
                .map(|t| {
                    let elapsed = now.duration_since(t).as_secs_f32();
                    if elapsed < 2.0 {
                        1.0
                    } else if elapsed < 3.0 {
                        1.0 - (elapsed - 2.0).powi(4)
                    } else {
                        0.0
                    }
                })
                .unwrap_or(0.0)
        };

        let thumb_bg = gpui::hsla(0.0, 0.0, 0.5, opacity * 0.8);
        let view = cx.entity();
        let m_down = metrics.clone();

        Some(
            div()
                .id("terminal-scrollbar")
                .absolute()
                .top_0()
                .right_0()
                .bottom_0()
                .w(px(12.0))
                .on_mouse_down(
                    MouseButton::Left,
                    move |e: &MouseDownEvent, _w, cx: &mut App| {
                        let track_y = {
                            let gm = m_down.borrow();
                            match gm.bounds {
                                Some(b) => f32::from(e.position.y - b.origin.y),
                                None => return,
                            }
                        };
                        let _ = view.update(cx, |v, cx| {
                            let (total, vp, _, lh) = v.scroll_handle.state_info();
                            if lh <= 0.0 {
                                return;
                            }
                            let track_h = vp as f32 * lh;
                            let max_off = total.saturating_sub(vp);
                            let frac = 1.0 - ((track_y / track_h).clamp(0.0, 1.0));
                            let new_offset = (frac * max_off as f32).round() as usize;
                            v.scroll_handle.update(total, vp, new_offset, lh);
                            v.scroll_handle.future_display_offset.set(Some(new_offset));
                            v.scrollbar_drag_start = Some(track_y);
                            v.last_scroll_time = Some(std::time::Instant::now());
                            cx.notify();
                        });
                        cx.stop_propagation();
                    },
                )
                .child(
                    div()
                        .id("scrollbar-thumb")
                        .absolute()
                        .top(px(thumb_top))
                        .right(px(2.0))
                        .w(px(8.0))
                        .h(px(thumb_height))
                        .rounded_sm()
                        .bg(thumb_bg),
                ),
        )
    }
}
