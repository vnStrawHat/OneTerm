//! Pointer state machine: hit-test, selection, URL ctrl-click, scrollbar drag,
//! wheel.
//!
//! Every method reads the grid through the [`GridGeometry`] the element wrote
//! in its last prepaint, forwards to the session itself, and returns a
//! [`MouseOutcome`] for the two things that need the view: opening a URL (which
//! needs a dialog) and copy-on-select (which needs the clipboard and a window).
//! The view marks the scrollbar scrolled and notifies for anything but
//! [`MouseOutcome::Ignored`].

use gpui::{
    App, Entity, Modifiers, ModifiersChangedEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, ScrollDelta, ScrollWheelEvent,
};

use alacritty_terminal::selection::SelectionType;
use oneterm_terminal::url_policy::{TargetDecision, validate_target_with_display};
use oneterm_terminal::{LineRangeCells, MouseModifiers, TerminalMouseButton, TerminalSession};

use crate::render::metrics::GridGeometry;
use crate::url::{DetectedUrl, URL_WINDOW, UrlHover, detect_url_at};

/// A wheel event below this many lines is dropped: trackpads emit a stream of
/// sub-pixel deltas that would otherwise churn the scrollback by zero.
const MIN_WHEEL_LINES: f32 = 0.001;

/// What the pointer is currently doing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Drag {
    #[default]
    None,
    Selecting,
    Scrollbar,
}

/// The pointer state that survives between events.
///
/// URL hover state is *not* duplicated here: [`UrlHover`] already owns the last
/// cell and Ctrl state and is the thing the render inputs read.
#[derive(Debug, Default)]
pub(crate) struct MouseState {
    drag: Drag,
}

/// What the view knows and the mouse machine cannot read for itself.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MouseInputs {
    /// The hit-test contract from the last paint; `None` before the first one.
    pub geometry: Option<GridGeometry>,
    /// The pointer is over the scrollbar track or thumb.
    pub over_scrollbar: bool,
    /// `terminal.show_context_menu`: the right button opens the menu instead
    /// of reaching the program.
    pub show_context_menu: bool,
    /// `terminal.copy_on_select`.
    pub copy_on_select: bool,
    /// `terminal.scroll_multiplier`.
    pub scroll_multiplier: f32,
}

/// A Ctrl/Cmd-clicked URL and what the target policy decided about it. The
/// view opens it, shows the confirmation dialog, or logs the denial.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct UrlOpen {
    pub url: DetectedUrl,
    pub decision: TargetDecision,
}

/// What the view still has to do after a mouse event.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MouseOutcome {
    /// The event did not reach the grid — no repaint needed.
    Ignored,
    /// Handled; mark the scrollbar scrolled and notify.
    Handled,
    /// Map `track_y` (relative to the element's top edge) to a display offset.
    ScrollbarDrag {
        track_y: f32,
    },
    OpenUrl(UrlOpen),
    /// copy-on-select fired: run [`crate::input::copy_selection`].
    CopySelection,
}

impl MouseState {
    pub(crate) fn down(
        &mut self,
        event: &MouseDownEvent,
        session: &Entity<Box<dyn TerminalSession>>,
        inputs: &MouseInputs,
        cx: &mut App,
    ) -> MouseOutcome {
        let Some(geometry) = inputs.geometry else {
            return MouseOutcome::Ignored;
        };
        // The scrollbar sits on top of the grid, so its hit test wins.
        if event.button == MouseButton::Left && inputs.over_scrollbar {
            self.drag = Drag::Scrollbar;
            return MouseOutcome::ScrollbarDrag {
                track_y: track_y(&geometry, event.position),
            };
        }
        // The context menu is registered on the wrapper; forwarding the press
        // as well would start a selection behind the open menu.
        if event.button == MouseButton::Right && inputs.show_context_menu {
            return MouseOutcome::Ignored;
        }
        let Some((row, col)) = geometry.pixel_to_grid(event.position) else {
            return MouseOutcome::Ignored;
        };
        if event.button == MouseButton::Left
            && (event.modifiers.control || event.modifiers.platform)
            && let Some(url) = detect_url_at_cell(session, row as usize, col as usize, cx)
        {
            let decision = validate_target_with_display(&url.url, url.display_text.as_deref());
            return MouseOutcome::OpenUrl(UrlOpen { url, decision });
        }
        let Some(button) = to_button(event.button) else {
            return MouseOutcome::Ignored;
        };
        let selection = selection_type(event.click_count, event.modifiers.alt);
        let mods = to_mods(&event.modifiers);
        session.update(cx, |s, _| {
            s.mouse_down(row, col, button, selection, mods);
        });
        if button == TerminalMouseButton::Left {
            self.drag = Drag::Selecting;
        }
        MouseOutcome::Handled
    }

    pub(crate) fn moved(
        &mut self,
        event: &MouseMoveEvent,
        session: &Entity<Box<dyn TerminalSession>>,
        inputs: &MouseInputs,
        hover: &mut UrlHover,
        cx: &mut App,
    ) -> MouseOutcome {
        let Some(geometry) = inputs.geometry else {
            return MouseOutcome::Ignored;
        };
        if self.drag == Drag::Scrollbar {
            // A release outside the window never reaches us as an up event.
            if event.pressed_button != Some(MouseButton::Left) {
                self.drag = Drag::None;
                return MouseOutcome::Handled;
            }
            return MouseOutcome::ScrollbarDrag {
                track_y: track_y(&geometry, event.position),
            };
        }

        let mods = to_mods(&event.modifiers);
        // A selection that runs past the edge keeps extending: the backend
        // clamps the coordinates to the grid.
        if self.drag == Drag::Selecting && event.pressed_button == Some(MouseButton::Left) {
            let (row, col) = clamped_grid(&geometry, event.position);
            session.update(cx, |s, _| s.mouse_drag(row, col, mods));
            return MouseOutcome::Handled;
        }

        let Some((row, col)) = geometry.pixel_to_grid(event.position) else {
            // Outside the grid the hover must go, or the highlight sticks.
            return if hover.leave(event.position) {
                MouseOutcome::Handled
            } else {
                MouseOutcome::Ignored
            };
        };
        session.update(cx, |s, _| s.mouse_move(row, col, mods));
        update_hover(
            session,
            hover,
            event.position,
            (row, col),
            event.modifiers.control,
            cx,
        );
        MouseOutcome::Handled
    }

    pub(crate) fn up(
        &mut self,
        event: &MouseUpEvent,
        session: &Entity<Box<dyn TerminalSession>>,
        inputs: &MouseInputs,
        cx: &mut App,
    ) -> MouseOutcome {
        // Any button release ends a scrollbar drag, including one that started
        // with a different button or ended outside the window.
        if self.drag == Drag::Scrollbar {
            self.drag = Drag::None;
            return MouseOutcome::Handled;
        }
        self.drag = Drag::None;
        let Some(geometry) = inputs.geometry else {
            return MouseOutcome::Ignored;
        };
        if event.button == MouseButton::Right && inputs.show_context_menu {
            return MouseOutcome::Ignored;
        }
        let Some((row, col)) = geometry.pixel_to_grid(event.position) else {
            return MouseOutcome::Ignored;
        };
        let Some(button) = to_button(event.button) else {
            return MouseOutcome::Ignored;
        };
        let mods = to_mods(&event.modifiers);
        session.update(cx, |s, _| s.mouse_up(row, col, button, mods));
        // Copy-on-select is opt-in (SEC-10): a plain drag must not overwrite
        // the clipboard.
        if button == TerminalMouseButton::Left
            && inputs.copy_on_select
            && session.read(cx).has_selection()
        {
            return MouseOutcome::CopySelection;
        }
        MouseOutcome::Handled
    }

    pub(crate) fn wheel(
        &self,
        event: &ScrollWheelEvent,
        session: &Entity<Box<dyn TerminalSession>>,
        inputs: &MouseInputs,
        cx: &mut App,
    ) -> MouseOutcome {
        let Some(geometry) = inputs.geometry else {
            return MouseOutcome::Ignored;
        };
        let Some((row, col)) = geometry.pixel_to_grid(event.position) else {
            return MouseOutcome::Ignored;
        };
        let lines = wheel_lines(
            event.delta,
            geometry.metrics.line_height,
            inputs.scroll_multiplier,
        );
        if lines.abs() < MIN_WHEEL_LINES {
            return MouseOutcome::Ignored;
        }
        let mods = to_mods(&event.modifiers);
        // The backend picks mouse-mode bytes, alt-screen arrows, or scrollback.
        session.update(cx, |s, _| s.wheel(f64::from(lines), row, col, mods));
        MouseOutcome::Handled
    }

    /// Ctrl pressed or released without moving: the hover highlight and the
    /// pointer cursor depend on it, so re-detect at the last known cell.
    pub(crate) fn modifiers_changed(
        &self,
        event: &ModifiersChangedEvent,
        session: &Entity<Box<dyn TerminalSession>>,
        inputs: &MouseInputs,
        hover: &mut UrlHover,
        cx: &mut App,
    ) -> MouseOutcome {
        let (Some(geometry), Some(position)) = (inputs.geometry, hover.last_mouse_pos()) else {
            return MouseOutcome::Ignored;
        };
        let Some(cell) = geometry.pixel_to_grid(position) else {
            return MouseOutcome::Ignored;
        };
        update_hover(session, hover, position, cell, event.modifiers.control, cx)
            .then_some(MouseOutcome::Handled)
            .unwrap_or(MouseOutcome::Ignored)
    }

    /// The pointer left the terminal: drop the hover highlight.
    pub(crate) fn exit(&self, position: Point<Pixels>, hover: &mut UrlHover) -> MouseOutcome {
        if hover.leave(position) {
            MouseOutcome::Handled
        } else {
            MouseOutcome::Ignored
        }
    }

    #[cfg(test)]
    pub(crate) fn drag(&self) -> Drag {
        self.drag
    }
}

/// Selection type for a click: Alt is always a block selection, otherwise the
/// click count picks character / word / line.
pub(crate) fn selection_type(click_count: usize, alt: bool) -> SelectionType {
    if alt {
        return SelectionType::Block;
    }
    match click_count {
        2 => SelectionType::Semantic,
        n if n >= 3 => SelectionType::Lines,
        _ => SelectionType::Simple,
    }
}

/// The modifier subset the mouse encoders report.
pub(crate) fn to_mods(m: &Modifiers) -> MouseModifiers {
    MouseModifiers {
        shift: m.shift,
        alt: m.alt,
        ctrl: m.control,
    }
}

/// `None` for the navigation buttons, which have no terminal encoding.
pub(crate) fn to_button(button: MouseButton) -> Option<TerminalMouseButton> {
    match button {
        MouseButton::Left => Some(TerminalMouseButton::Left),
        MouseButton::Right => Some(TerminalMouseButton::Right),
        MouseButton::Middle => Some(TerminalMouseButton::Middle),
        MouseButton::Navigate(_) => None,
    }
}

/// Lines of scrollback for one wheel event: line deltas are converted to
/// pixels by GPUI, then back with our own line height so a precise trackpad
/// and a notched wheel scale the same way.
pub(crate) fn wheel_lines(delta: ScrollDelta, line_height: Pixels, multiplier: f32) -> f32 {
    let height = f32::from(line_height);
    if height <= 0.0 {
        return 0.0;
    }
    f32::from(delta.pixel_delta(line_height).y) / height * multiplier
}

/// Y position inside the element, which is what the scrollbar's track maps.
fn track_y(geometry: &GridGeometry, position: Point<Pixels>) -> f32 {
    f32::from(position.y - geometry.bounds.origin.y)
}

/// Fractional grid coordinates clamped into the grid — used while a selection
/// drag runs past an edge.
fn clamped_grid(geometry: &GridGeometry, position: Point<Pixels>) -> (f32, f32) {
    let metrics = &geometry.metrics;
    let x = f32::from(position.x - geometry.origin.x) / f32::from(metrics.cell_width);
    let y = f32::from(position.y - geometry.origin.y) / f32::from(metrics.line_height);
    let last_row = (f32::from(geometry.size.rows) - 1.0).max(0.0);
    let last_col = (f32::from(geometry.size.cols) - 1.0).max(0.0);
    (y.clamp(0.0, last_row), x.clamp(0.0, last_col))
}

/// Re-detect the URL under `cell` when the cell or the Ctrl state changed.
/// Returns `true` when the visible hover state changed.
fn update_hover(
    session: &Entity<Box<dyn TerminalSession>>,
    hover: &mut UrlHover,
    position: Point<Pixels>,
    (row, col): (f32, f32),
    ctrl: bool,
    cx: &mut App,
) -> bool {
    let cell = (row as i32, col as i32);
    if !hover.needs_detection(position, cell, ctrl) {
        return false;
    }
    let url = detect_url_at_cell(session, row as usize, col as usize, cx);
    hover.set(position, cell, url, ctrl)
}

/// Detect the URL at display cell `(row, col)`.
///
/// Only a small window of lines around `row` is queried: enough for a wrapped
/// URL, and O(window × cols) instead of cloning the grid on every move.
pub(crate) fn detect_url_at_cell(
    session: &Entity<Box<dyn TerminalSession>>,
    row: usize,
    col: usize,
    cx: &App,
) -> Option<DetectedUrl> {
    let start = row.saturating_sub(URL_WINDOW);
    let LineRangeCells { cells, num_cols } = session
        .read(cx)
        .query_line_range_cells(start, URL_WINDOW * 2 + 1);
    detect_url_at(&cells, num_cols, row - start, col).map(|mut url| {
        url.row += start;
        url
    })
}
