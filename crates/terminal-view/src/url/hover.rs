//! Hover state for URL highlighting + Ctrl+click.

use gpui::{Pixels, Point};

use super::DetectedUrl;

/// The URL under the mouse (if any), whether Ctrl is held, and the last mouse
/// position (to re-detect when Ctrl is pressed/released without a move).
#[derive(Debug, Default)]
pub(crate) struct UrlHover {
    /// URL currently hovered — for highlight + click to open.
    hovered: Option<DetectedUrl>,
    /// Ctrl currently held — tracked to toggle the cursor style.
    ctrl_held: bool,
    /// Last mouse position (window coordinates).
    last_mouse_pos: Option<Point<Pixels>>,
    /// The grid cell `(row, col)` the last detection ran for. Sub-cell mouse
    /// moves re-use that result instead of re-querying the grid (PERF-15).
    last_cell: Option<(i32, i32)>,
}

impl UrlHover {
    /// Whether a URL is currently hovered.
    pub(crate) fn is_hovering(&self) -> bool {
        self.hovered.is_some()
    }

    /// The last known mouse position.
    pub(crate) fn last_mouse_pos(&self) -> Option<Point<Pixels>> {
        self.last_mouse_pos
    }

    /// Whether a fresh detection is needed for the pointer now being over
    /// `cell` with `ctrl`: only when the cell or the Ctrl state changed since
    /// the last detection. Records the position either way.
    pub(crate) fn needs_detection(
        &mut self,
        position: Point<Pixels>,
        cell: (i32, i32),
        ctrl: bool,
    ) -> bool {
        self.last_mouse_pos = Some(position);
        self.last_cell != Some(cell) || self.ctrl_held != ctrl
    }

    /// Record the mouse position and the detected URL + Ctrl state. Returns
    /// `true` when the visible state (URL span or Ctrl) changed.
    pub(crate) fn set(
        &mut self,
        position: Point<Pixels>,
        cell: (i32, i32),
        url: Option<DetectedUrl>,
        ctrl: bool,
    ) -> bool {
        self.last_mouse_pos = Some(position);
        self.last_cell = Some(cell);
        let changed = self.ctrl_held != ctrl
            || self.hovered.as_ref().map(url_identity) != url.as_ref().map(url_identity);
        if changed {
            self.ctrl_held = ctrl;
            self.hovered = url;
        }
        changed
    }

    /// The mouse left the grid: remember the position and clear the hover.
    /// Returns `true` when there was a hover/Ctrl state to clear.
    pub(crate) fn leave(&mut self, position: Point<Pixels>) -> bool {
        self.last_mouse_pos = Some(position);
        self.last_cell = None;
        let changed = self.hovered.is_some() || self.ctrl_held;
        self.hovered = None;
        self.ctrl_held = false;
        changed
    }
}

fn url_identity(u: &DetectedUrl) -> (&String, usize, usize, usize) {
    (&u.url, u.row, u.start_col, u.end_col)
}

#[cfg(test)]
mod tests {
    use gpui::{point, px};

    use super::UrlHover;
    use crate::url::DetectedUrl;

    fn url(row: usize) -> DetectedUrl {
        DetectedUrl {
            url: "https://example.com".to_string(),
            display_text: None,
            row,
            start_col: 0,
            end_col: 19,
        }
    }

    #[test]
    fn set_reports_changes_only() {
        let mut h = UrlHover::default();
        let p = point(px(1.0), px(2.0));
        assert!(!h.set(p, (0, 0), None, false));
        assert!(h.set(p, (0, 0), Some(url(0)), false));
        assert!(h.is_hovering());
        // Same URL, same Ctrl → unchanged.
        assert!(!h.set(p, (0, 0), Some(url(0)), false));
        // Ctrl toggles → changed.
        assert!(h.set(p, (0, 0), Some(url(0)), true));
        // Different row → changed.
        assert!(h.set(p, (1, 0), Some(url(1)), true));
        assert_eq!(h.last_mouse_pos(), Some(p));
    }

    #[test]
    fn leave_clears_and_reports() {
        let mut h = UrlHover::default();
        let p = point(px(1.0), px(2.0));
        assert!(!h.leave(p));
        h.set(p, (0, 0), Some(url(0)), true);
        assert!(h.leave(p));
        assert!(!h.is_hovering());
        assert!(!h.leave(p));
    }

    /// PERF-15: moving within the same cell (same Ctrl) does not re-query.
    #[test]
    fn detection_is_skipped_within_the_same_cell() {
        let mut h = UrlHover::default();
        let p = point(px(1.0), px(2.0));
        assert!(h.needs_detection(p, (3, 4), false));
        h.set(p, (3, 4), None, false);
        assert!(!h.needs_detection(point(px(1.5), px(2.5)), (3, 4), false));
        assert!(h.needs_detection(p, (3, 5), false), "new cell");
        assert!(h.needs_detection(p, (3, 4), true), "Ctrl changed");
        // Leaving the grid forgets the cell so re-entering detects again.
        h.leave(p);
        assert!(h.needs_detection(p, (3, 4), false));
    }
}
