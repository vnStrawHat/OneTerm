//! Scrollbar state + the auto-hiding scrollbar overlay for [`TerminalView`].
//!
//! [`ScrollbarState`] caches the scrollback geometry each frame
//! (total/viewport/display-offset/line-height), tracks thumb drags and the
//! auto-hide timer, and queues a `pending_offset` that the view applies on the
//! next `render()` (calling `session.scroll(delta)`). The overlay itself has
//! no listeners: the wrapper div's mouse handlers hit-test the strip through
//! [`ScrollbarState::hit_test`] and feed drags back in.

use std::time::Instant;

use gpui::{
    Bounds, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Point, Styled as _,
    Window, div, px,
};

use super::TerminalView;

/// Seconds the scrollbar stays fully visible after the last scroll.
const VISIBLE_SECS: f32 = 2.0;
/// Seconds after which the scrollbar is fully hidden (fade between the two).
const HIDDEN_SECS: f32 = 3.0;
/// Minimum thumb height in pixels.
const MIN_THUMB_PX: f32 = 24.0;
/// Width of the strip that catches presses (the thumb is narrower).
const TRACK_WIDTH_PX: f32 = 12.0;

/// Cached scrollback geometry — updated each frame from `TerminalInfo` + metrics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ScrollGeometry {
    pub(super) total_lines: usize,
    pub(super) viewport_lines: usize,
    pub(super) display_offset: usize,
    pub(super) line_height: f32,
}

impl Default for ScrollGeometry {
    fn default() -> Self {
        Self {
            total_lines: 24,
            viewport_lines: 24,
            display_offset: 0,
            line_height: 16.0,
        }
    }
}

impl ScrollGeometry {
    /// Largest valid `display_offset` (0 when everything fits).
    fn max_offset(&self) -> usize {
        self.total_lines.saturating_sub(self.viewport_lines)
    }

    /// Track height in pixels (the viewport).
    fn track_height(&self) -> f32 {
        self.viewport_lines as f32 * self.line_height
    }

    /// Whether there is anything to scroll (and a usable line height).
    fn is_scrollable(&self) -> bool {
        self.total_lines > self.viewport_lines && self.line_height > 0.0
    }
}

/// Thumb placement for the overlay.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ThumbGeometry {
    top: f32,
    height: f32,
}

/// Compute the thumb rectangle for `geo`; `None` when nothing scrolls.
fn thumb_geometry(geo: &ScrollGeometry) -> Option<ThumbGeometry> {
    if !geo.is_scrollable() {
        return None;
    }
    let track_height = geo.track_height();
    let thumb_ratio = geo.viewport_lines as f32 / geo.total_lines as f32;
    let height = (thumb_ratio * track_height).max(MIN_THUMB_PX);
    let max_offset = geo.max_offset();
    let scroll_fraction = if max_offset > 0 {
        geo.display_offset as f32 / max_offset as f32
    } else {
        0.0
    };
    // `display_offset` grows upward (0 = bottom), the thumb grows downward.
    let top = (1.0 - scroll_fraction) * (track_height - height);
    Some(ThumbGeometry { top, height })
}

/// Map a y position on the track (pixels from the top of the terminal bounds)
/// to a `display_offset`. `None` when the geometry has no usable line height.
fn track_y_to_offset(geo: &ScrollGeometry, track_y: f32) -> Option<usize> {
    if geo.line_height <= 0.0 {
        return None;
    }
    let frac = 1.0 - (track_y / geo.track_height()).clamp(0.0, 1.0);
    Some((frac * geo.max_offset() as f32).round() as usize)
}

/// Opacity of the auto-hiding scrollbar `elapsed` seconds after the last
/// scroll: fully visible for [`VISIBLE_SECS`], then a quartic fade to zero at
/// [`HIDDEN_SECS`].
fn fade_opacity(elapsed: f32) -> f32 {
    if elapsed < VISIBLE_SECS {
        1.0
    } else if elapsed < HIDDEN_SECS {
        1.0 - (elapsed - VISIBLE_SECS).powi(4)
    } else {
        0.0
    }
}

/// Scrollbar state owned by the view: cached geometry, a pending offset from
/// a thumb drag / track click, the drag anchor, and the auto-hide timer.
#[derive(Debug, Default)]
pub(super) struct ScrollbarState {
    geometry: ScrollGeometry,
    /// `display_offset` requested by the user via the scrollbar — applied by
    /// the view on the next render.
    pending_offset: Option<usize>,
    /// `Some(track_y)` while the thumb is being dragged.
    drag_start: Option<f32>,
    /// Last scroll time — the scrollbar auto-hides a few seconds later.
    last_scroll: Option<Instant>,
}

impl ScrollbarState {
    /// Refresh the cached geometry — called in `render()`.
    pub(super) fn update(&mut self, geometry: ScrollGeometry) {
        self.geometry = geometry;
    }

    /// Take the pending offset (if any) — consumed by the view.
    pub(super) fn take_pending_offset(&mut self) -> Option<usize> {
        self.pending_offset.take()
    }

    /// Record a scroll so the scrollbar shows (and restarts its fade).
    pub(super) fn mark_scrolled(&mut self) {
        self.last_scroll = Some(Instant::now());
    }

    fn is_dragging(&self) -> bool {
        self.drag_start.is_some()
    }

    /// Stop a thumb drag. Returns `true` when a drag was in progress.
    pub(super) fn end_drag(&mut self) -> bool {
        self.drag_start.take().is_some()
    }

    /// Jump to the offset under `track_y` and start dragging from there
    /// (track click / drag move). Returns `false` when the geometry is unusable.
    pub(super) fn drag_to(&mut self, track_y: f32) -> bool {
        let Some(offset) = track_y_to_offset(&self.geometry, track_y) else {
            return false;
        };
        self.geometry.display_offset = offset;
        self.pending_offset = Some(offset);
        self.drag_start = Some(track_y);
        self.mark_scrolled();
        true
    }

    /// Current opacity (`None` = hidden): fully opaque while dragging,
    /// otherwise the auto-hide fade since the last scroll.
    fn opacity(&self, now: Instant) -> Option<f32> {
        if self.is_dragging() {
            return Some(1.0);
        }
        let elapsed = now.duration_since(self.last_scroll?).as_secs_f32();
        (elapsed < HIDDEN_SECS).then(|| fade_opacity(elapsed))
    }

    /// Whether `position` is on the visible scrollbar strip of an element
    /// with `bounds` — the press then drags the thumb instead of selecting.
    pub(super) fn hit_test(&self, position: Point<Pixels>, bounds: Bounds<Pixels>) -> bool {
        if thumb_geometry(&self.geometry).is_none() || self.opacity(Instant::now()).is_none() {
            return false;
        }
        let strip = Bounds::from_corners(
            gpui::point(bounds.right() - px(TRACK_WIDTH_PX), bounds.top()),
            bounds.bottom_right(),
        );
        strip.contains(&position)
    }
}

impl TerminalView {
    /// The scrollbar overlay on the right edge; `None` when nothing scrolls or
    /// the bar has faded out. Painted only — the wrapper's listeners drive it.
    pub(super) fn render_scrollbar(&self, window: &Window) -> Option<impl IntoElement> {
        let thumb = thumb_geometry(&self.scrollbar.geometry)?;
        let opacity = self.scrollbar.opacity(Instant::now())?;
        // Keep repainting while the fade runs, so the bar disappears on time
        // even when nothing else (blink, output) requests a frame.
        if !self.scrollbar.is_dragging() {
            window.request_animation_frame();
        }
        let thumb_bg = gpui::hsla(0.0, 0.0, 0.5, opacity * 0.8);
        Some(
            div()
                .id("terminal-scrollbar")
                .absolute()
                .top_0()
                .right_0()
                .bottom_0()
                .w(px(TRACK_WIDTH_PX))
                .child(
                    div()
                        .id("scrollbar-thumb")
                        .absolute()
                        .top(px(thumb.top))
                        .right(px(2.0))
                        .w(px(8.0))
                        .h(px(thumb.height))
                        .rounded_sm()
                        .bg(thumb_bg),
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    use gpui::{Bounds, point, px, size};

    use super::{ScrollGeometry, ScrollbarState, fade_opacity, thumb_geometry, track_y_to_offset};

    fn geo(total: usize, viewport: usize, offset: usize) -> ScrollGeometry {
        ScrollGeometry {
            total_lines: total,
            viewport_lines: viewport,
            display_offset: offset,
            line_height: 10.0,
        }
    }

    #[test]
    fn no_thumb_when_everything_fits() {
        assert_eq!(thumb_geometry(&geo(24, 24, 0)), None);
        assert_eq!(thumb_geometry(&geo(10, 24, 0)), None);
        let mut g = geo(100, 24, 0);
        g.line_height = 0.0;
        assert_eq!(thumb_geometry(&g), None);
    }

    #[test]
    fn thumb_sits_at_the_bottom_when_not_scrolled() {
        // 100 lines, 20 visible, 10 px lines: track 200 px, thumb 40 px.
        let t = thumb_geometry(&geo(100, 20, 0)).expect("scrollable");
        assert_eq!(t.height, 40.0);
        assert_eq!(t.top, 160.0);
    }

    #[test]
    fn thumb_reaches_the_top_at_max_offset() {
        let t = thumb_geometry(&geo(100, 20, 80)).expect("scrollable");
        assert_eq!(t.top, 0.0);
    }

    #[test]
    fn thumb_never_shrinks_below_the_minimum() {
        let t = thumb_geometry(&geo(100_000, 20, 0)).expect("scrollable");
        assert_eq!(t.height, 24.0);
    }

    #[test]
    fn track_position_maps_to_offset_and_clamps() {
        let g = geo(100, 20, 0);
        // Top of the track → oldest content (max offset).
        assert_eq!(track_y_to_offset(&g, 0.0), Some(80));
        // Bottom of the track → live view.
        assert_eq!(track_y_to_offset(&g, 200.0), Some(0));
        // Middle → half way.
        assert_eq!(track_y_to_offset(&g, 100.0), Some(40));
        // Outside the track clamps.
        assert_eq!(track_y_to_offset(&g, -50.0), Some(80));
        assert_eq!(track_y_to_offset(&g, 999.0), Some(0));
    }

    #[test]
    fn track_position_needs_a_line_height() {
        let mut g = geo(100, 20, 0);
        g.line_height = 0.0;
        assert_eq!(track_y_to_offset(&g, 10.0), None);
    }

    #[test]
    fn fade_is_opaque_then_fades_to_zero() {
        assert_eq!(fade_opacity(0.0), 1.0);
        assert_eq!(fade_opacity(1.99), 1.0);
        let mid = fade_opacity(2.5);
        assert!(mid > 0.0 && mid < 1.0);
        assert_eq!(fade_opacity(3.0), 0.0);
    }

    #[test]
    fn drag_queues_a_pending_offset_and_tracks_the_drag() {
        let mut s = ScrollbarState::default();
        s.update(geo(100, 20, 0));
        assert!(!s.is_dragging());
        assert!(s.drag_to(0.0));
        assert!(s.is_dragging());
        assert_eq!(s.geometry.display_offset, 80);
        assert_eq!(s.take_pending_offset(), Some(80));
        assert_eq!(s.take_pending_offset(), None);
        assert!(s.end_drag());
        assert!(!s.end_drag());
    }

    #[test]
    fn hit_test_needs_a_shown_bar_and_the_right_strip() {
        let bounds = Bounds::new(point(px(100.0), px(50.0)), size(px(400.0), px(200.0)));
        let on_strip = point(px(495.0), px(100.0));
        let mut s = ScrollbarState::default();
        s.update(geo(100, 20, 0));
        // Never scrolled: the bar is hidden, the press belongs to the grid.
        assert!(!s.hit_test(on_strip, bounds));
        s.mark_scrolled();
        assert!(s.hit_test(on_strip, bounds));
        assert!(!s.hit_test(point(px(300.0), px(100.0)), bounds), "grid");
        assert!(!s.hit_test(point(px(495.0), px(10.0)), bounds), "above");
        // Nothing to scroll: no thumb, no strip.
        s.update(geo(20, 20, 0));
        assert!(!s.hit_test(on_strip, bounds));
    }
}
