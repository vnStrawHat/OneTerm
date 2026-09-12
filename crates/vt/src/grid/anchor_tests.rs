//! The `anchor::tests::` suite the design's Verification section names.
//!
//! Everything here drives the anchors through a real screen, because the
//! property under test is "every row-moving primitive updates the list", not
//! "the list can be mutated".

use super::*;
use crate::grid::{Pos, Screen, ScreenKind, ScrollRegion, Size};

struct Fixture {
    screen: Screen,
    anchors: Anchors,
}

fn fixture(rows: u16, cols: u16) -> Fixture {
    let mut anchors = Anchors::new();
    let screen = Screen::new(ScreenKind::Primary, Size { rows, cols }, 100, &mut anchors);
    Fixture { screen, anchors }
}

impl Fixture {
    /// A consumer's anchor on one screen row.
    fn mark(&mut self, index: u16, tag: u32) -> AnchorId {
        let pos = Pos {
            row: self.screen.row_of_index(index),
            col: 0,
        };
        self.anchors.register(AnchorKind::Mark(tag), pos)
    }

    fn index_of(&self, id: AnchorId) -> Option<u16> {
        self.anchors
            .get(id)
            .and_then(|pos| self.screen.index_of(pos.row))
    }
}

#[test]
fn region_scroll_moves_anchors_with_their_content() {
    let mut f = fixture(6, 10);
    let inside = f.mark(3, 1);
    let below = f.mark(5, 2);

    f.screen
        .scroll_up(ScrollRegion { top: 1, bottom: 5 }, 2, &mut f.anchors);

    assert_eq!(
        f.index_of(inside),
        Some(1),
        "an anchor follows its content up the region"
    );
    assert_eq!(
        f.index_of(below),
        Some(5),
        "an anchor outside the region does not move"
    );
    f.screen.assert_integrity();
}

#[test]
fn anchor_in_a_blanked_region_dies() {
    let mut f = fixture(6, 10);
    let doomed = f.mark(1, 1);
    let survivor = f.mark(4, 2);

    f.screen
        .scroll_up(ScrollRegion { top: 1, bottom: 5 }, 2, &mut f.anchors);

    assert_eq!(
        f.anchors.get(doomed),
        None,
        "the top rows of the region were destroyed"
    );
    assert!(f.anchors.get(survivor).is_some());

    // `SD` blanks the other end of the region.
    let bottom = f.mark(4, 3);
    f.screen
        .scroll_down(ScrollRegion { top: 1, bottom: 5 }, 1, &mut f.anchors);
    assert_eq!(f.anchors.get(bottom), None);
    f.screen.assert_integrity();
}

#[test]
fn trim_kills_anchors_below_oldest() {
    let mut f = fixture(4, 10);
    let mark = f.mark(0, 1);
    let pinned = f.anchors.get(mark).expect("just registered");

    // Scroll far enough that the anchored row leaves the scrollback entirely.
    for _ in 0..200 {
        f.screen.scroll_up(ScrollRegion::full(4), 1, &mut f.anchors);
    }

    assert!(pinned.row < f.screen.oldest(), "the row really was trimmed");
    assert_eq!(f.anchors.get(mark), None);
    f.screen.assert_integrity();
}

#[test]
fn saved_cursor_survives_a_region_scroll() {
    let mut f = fixture(6, 10);
    f.screen.goto(3, 4);
    f.screen.save_cursor();

    f.screen
        .scroll_up(ScrollRegion { top: 1, bottom: 5 }, 1, &mut f.anchors);
    f.screen.goto(0, 0);
    f.screen.restore_cursor();

    assert_eq!(
        (f.screen.cursor_row_index(), f.screen.cursor().pos.col),
        (3, 4),
        "DECRC lands where the reference lands: the same place on the screen"
    );
    assert_eq!(
        f.anchors.get(f.screen.saved_cursor_anchor()),
        Some(f.screen.saved_cursor().pos),
        "and the anchor entry agrees with the cached field"
    );
    f.screen.assert_integrity();
}

#[test]
fn rows_scrolled_event_matches_the_anchor_shift() {
    let mut f = fixture(6, 10);
    let mark = f.mark(4, 1);
    let before = f.anchors.get(mark).expect("just registered").row;

    let report = f
        .screen
        .scroll_up(ScrollRegion { top: 1, bottom: 5 }, 2, &mut f.anchors);

    let after = f.anchors.get(mark).expect("still live").row;
    assert_eq!(report.scrolled.delta, -2);
    assert_eq!(
        after,
        before - 2,
        "the reported delta is the shift the anchors took"
    );
    assert_eq!(report.scrolled.top, f.screen.row_of_index(1));
    assert_eq!(report.scrolled.bottom, f.screen.row_of_index(4));
    assert_eq!(
        report.history_rows, 0,
        "a region above row 0 fills no history"
    );

    let report = f.screen.scroll_up(ScrollRegion::full(6), 3, &mut f.anchors);
    assert_eq!(report.history_rows, 3);
    assert_eq!(report.scrolled.delta, -3);
    f.screen.assert_integrity();
}

#[test]
fn remap_is_the_reflow_entry_point() {
    // Reflow (`US-0077`) moves every anchor through this one call, so the
    // mechanism is the same one the scroll primitives use.
    let mut f = fixture(4, 10);
    let moved = f.mark(1, 1);
    let dropped = f.mark(2, 2);
    let target = f.screen.row_of_index(3);

    f.anchors.remap(|pos| {
        if pos.col == 0 && pos.row == f.screen.row_of_index(2) {
            None
        } else {
            Some(Pos {
                row: target,
                col: pos.col,
            })
        }
    });

    assert_eq!(f.anchors.get(moved).map(|pos| pos.row), Some(target));
    assert_eq!(
        f.anchors.get(dropped),
        None,
        "an anchor whose content did not survive dies"
    );
}
