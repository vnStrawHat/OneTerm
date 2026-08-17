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

    /// Record the mouse position and the detected URL + Ctrl state. Returns
    /// `true` when the visible state (URL span or Ctrl) changed.
    pub(crate) fn set(
        &mut self,
        position: Point<Pixels>,
        url: Option<DetectedUrl>,
        ctrl: bool,
    ) -> bool {
        self.last_mouse_pos = Some(position);
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
            row,
            start_col: 0,
            end_col: 19,
        }
    }

    #[test]
    fn set_reports_changes_only() {
        let mut h = UrlHover::default();
        let p = point(px(1.0), px(2.0));
        assert!(!h.set(p, None, false));
        assert!(h.set(p, Some(url(0)), false));
        assert!(h.is_hovering());
        // Same URL, same Ctrl → unchanged.
        assert!(!h.set(p, Some(url(0)), false));
        // Ctrl toggles → changed.
        assert!(h.set(p, Some(url(0)), true));
        // Different row → changed.
        assert!(h.set(p, Some(url(1)), true));
        assert_eq!(h.last_mouse_pos(), Some(p));
    }

    #[test]
    fn leave_clears_and_reports() {
        let mut h = UrlHover::default();
        let p = point(px(1.0), px(2.0));
        assert!(!h.leave(p));
        h.set(p, Some(url(0)), true);
        assert!(h.leave(p));
        assert!(!h.is_hovering());
        assert!(!h.leave(p));
    }
}
