//! The tracked-anchor list: one mechanism, shared by every row-moving primitive
//! and by reflow.
//!
//! Design:
//! <https://github.com/vnStrawHat/OneTerm/blob/main/docs/spec-intakes/IN-0029-vt-engine/low-level-design/grid-and-scrollback.md>,
//! section "Tracked anchors", which is the canonical definition of
//! [`AnchorKind`].
//!
//! A [`RowId`] names a position, so a consumer that stored one has no way to
//! know its content moved. Anchoring is therefore an engine service: a consumer
//! registers an anchor and reads it back. The list is small and bounded — one
//! saved cursor, two selection ends, one per live graphics placement, one per
//! visible mark — so a linear scan per scroll is cheaper than any index.

use std::ops::Range;

use crate::grid::{Pos, RowId};
use crate::intern::GraphicId;

/// Handle to one entry. Slots are recycled, so a released id may name a later
/// anchor; a consumer that released an id must not keep using it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct AnchorId(u32);

/// Everything the engine tracks across row motion.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AnchorKind {
    /// The active screen's cursor.
    Cursor,
    /// `DECSC`.
    SavedCursor,
    /// The first visible row, so reflow can restate the viewport.
    ViewportTop,
    /// Where an active selection began.
    SelectionStart,
    /// Where an active selection currently ends.
    SelectionEnd,
    /// One per placement.
    Graphic(GraphicId),
    /// `OSC 133` prompt marks.
    Mark(u32),
}

impl AnchorKind {
    /// Whether the screen owns this entry as the cached mirror of a field.
    ///
    /// The cursor and the viewport top are ordinary fields on the screen, because
    /// the print path writes the cursor on every glyph and must not pay a lookup.
    /// The primitives below move those fields by their own rules — the cursor
    /// keeps its position on the *screen* across a scroll, where a content anchor
    /// follows its *content* — and then write the result back here, so the entry
    /// stays the authority for reflow and for anything reading the list.
    pub(crate) fn is_screen_owned(self) -> bool {
        matches!(
            self,
            AnchorKind::Cursor | AnchorKind::SavedCursor | AnchorKind::ViewportTop
        )
    }
}

/// One tracked position. A dead anchor keeps its slot until it is released, so
/// the consumer can observe that its mark disappeared rather than silently
/// pointing at unrelated content.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Anchor {
    /// What this entry tracks.
    pub kind: AnchorKind,
    /// Where that content is now. Meaningless once `alive` is false.
    pub pos: Pos,
    /// False once the content was blanked, trimmed or reflowed away.
    pub alive: bool,
}

/// The engine's anchor list.
#[derive(Debug, Default)]
pub struct Anchors {
    entries: Vec<Anchor>,
    free: Vec<AnchorId>,
}

impl Anchors {
    /// An empty list.
    pub fn new() -> Anchors {
        Anchors::default()
    }

    pub(crate) fn register(&mut self, kind: AnchorKind, pos: Pos) -> AnchorId {
        let anchor = Anchor {
            kind,
            pos,
            alive: true,
        };
        if let Some(id) = self.free.pop() {
            self.entries[id.0 as usize] = anchor;
            return id;
        }
        self.entries.push(anchor);
        AnchorId((self.entries.len() - 1) as u32)
    }

    /// `None` once the anchor's content has been blanked, trimmed or reflowed
    /// away.
    pub fn get(&self, id: AnchorId) -> Option<Pos> {
        self.entries
            .get(id.0 as usize)
            .filter(|anchor| anchor.alive)
            .map(|anchor| anchor.pos)
    }

    /// Move an anchor, reviving it: the caller has just told us where its
    /// content is.
    pub(crate) fn set(&mut self, id: AnchorId, pos: Pos) {
        if let Some(anchor) = self.entries.get_mut(id.0 as usize) {
            anchor.pos = pos;
            anchor.alive = true;
        }
    }

    /// What an anchor tracks, or `None` if the id was never issued.
    pub fn kind(&self, id: AnchorId) -> Option<AnchorKind> {
        self.entries.get(id.0 as usize).map(|anchor| anchor.kind)
    }

    pub(crate) fn release(&mut self, id: AnchorId) {
        if let Some(anchor) = self.entries.get_mut(id.0 as usize) {
            anchor.alive = false;
            if !self.free.contains(&id) {
                self.free.push(id);
            }
        }
    }

    /// Called by every row-moving primitive.
    ///
    /// An anchor whose row is inside `kill` loses its content and is marked
    /// dead; one inside `rows` but not `kill` moves with its content by `delta`.
    /// Both ranges are in [`RowId`] space: the caller converts its
    /// viewport-coordinate `ScrollRegion` with `Screen::rows_of` (N-13, one
    /// coordinate space inside the anchor list, converted at the boundary).
    ///
    /// The screen-owned entries are skipped: their fields move by a different
    /// rule and are written back through [`Anchors::set`] before the primitive
    /// returns.
    pub(crate) fn shift_region(&mut self, rows: Range<RowId>, delta: i32, kill: Range<RowId>) {
        for anchor in &mut self.entries {
            if !anchor.alive || anchor.kind.is_screen_owned() {
                continue;
            }
            if kill.contains(&anchor.pos.row) {
                anchor.alive = false;
                continue;
            }
            if rows.contains(&anchor.pos.row) {
                anchor.pos.row = shift(anchor.pos.row, delta);
            }
        }
    }

    /// Reflow: every live anchor is offered its old position and takes the new
    /// one, or dies when its content did not survive.
    pub(crate) fn remap(&mut self, f: impl Fn(Pos) -> Option<Pos>) {
        for anchor in &mut self.entries {
            if !anchor.alive {
                continue;
            }
            match f(anchor.pos) {
                Some(pos) => anchor.pos = pos,
                None => anchor.alive = false,
            }
        }
    }

    /// Trap 28: a resize that changed the column count invalidates the
    /// selection, and so does a `KeepViewportTop` correction that moved the
    /// screen. Killing rather than releasing keeps the consumer's handles valid,
    /// so it observes that its selection disappeared instead of reading an
    /// unrelated one.
    pub(crate) fn kill_selection(&mut self) {
        for anchor in &mut self.entries {
            if matches!(
                anchor.kind,
                AnchorKind::SelectionStart | AnchorKind::SelectionEnd
            ) {
                anchor.alive = false;
            }
        }
    }

    /// History was trimmed: anything in `origin..oldest` is gone.
    ///
    /// **Lane-scoped on purpose.** The two screens draw from disjoint runs of
    /// the id space, and the alternate screen has no scrollback at all, so it
    /// trims on every scroll with an `oldest` that is above *every* primary row
    /// id. A bound of "below `oldest`" alone would therefore kill every mark,
    /// selection end and graphics placement on the primary screen the first time
    /// a program inside the alternate screen printed a line.
    pub(crate) fn trim(&mut self, origin: RowId, oldest: RowId) {
        for anchor in &mut self.entries {
            if anchor.alive && anchor.pos.row >= origin && anchor.pos.row < oldest {
                anchor.alive = false;
            }
        }
    }

    /// Live entries inside one screen's run, for the integrity walk.
    pub fn live_in(&self, lane: Range<RowId>) -> usize {
        self.entries
            .iter()
            .filter(|anchor| anchor.alive && lane.contains(&anchor.pos.row))
            .count()
    }

    /// Live screen-owned entries inside one screen's run. Always exactly three —
    /// the cursor, the saved cursor and the viewport top — because a screen
    /// registers them once and never releases them.
    pub(crate) fn live_screen_owned_in(&self, lane: Range<RowId>) -> usize {
        self.entries
            .iter()
            .filter(|anchor| {
                anchor.alive && anchor.kind.is_screen_owned() && lane.contains(&anchor.pos.row)
            })
            .count()
    }

    /// Every entry with its id, dead ones included.
    pub fn iter(&self) -> impl Iterator<Item = (AnchorId, &Anchor)> {
        self.entries
            .iter()
            .enumerate()
            .map(|(index, anchor)| (AnchorId(index as u32), anchor))
    }

    /// Live entries, for the debug assertion that a selection never leaks more
    /// than two of them.
    pub fn live(&self) -> usize {
        self.entries.iter().filter(|anchor| anchor.alive).count()
    }
}

fn shift(row: RowId, delta: i32) -> RowId {
    if delta >= 0 {
        row + delta as u64
    } else {
        row - delta.unsigned_abs() as u64
    }
}

#[cfg(test)]
#[path = "anchor_tests.rs"]
mod tests;
