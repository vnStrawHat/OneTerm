//! The dock's left edge (specs/oneterm, "M9 in detail"): a 6 px strip over the 1 px line, dragged
//! to make the dock 220 to 800 px wide; the width is a setting.

use std::cell::Cell;
use std::rc::Rc;

use quadra::{Cursor, Length, PointerKind, Signal, div};

pub(super) const LEAST: f64 = 220.0;
pub(super) const MOST: f64 = 800.0;

/// The strip, laid over the dock's left edge: dragging it left widens the dock.
pub(super) fn grip(width: Signal<f64>) -> quadra::Div {
    let held: Rc<Cell<Option<(f32, f64)>>> = Rc::default();
    let (press, drag) = (held.clone(), held.clone());
    div()
        .absolute()
        .left(-3.0)
        .top(0.0)
        .width(6.0)
        .height(Length::Pct(100.0))
        .cursor(Cursor::ResizeHorizontal)
        .test_id("sftp-grip")
        .on_pointer(PointerKind::Down, move |e| {
            press.set(Some((e.at.x, width.get_untracked())));
            e.capture_pointer();
            e.stop();
        })
        .on_pointer(PointerKind::Move, move |e| {
            if let Some((from, was)) = drag.get() {
                let to = (was - f64::from(e.at.x - from)).clamp(LEAST, MOST).round();
                if to != width.get_untracked() {
                    width.set(to);
                }
            }
        })
        .on_pointer(PointerKind::Up, move |_| held.set(None))
}
