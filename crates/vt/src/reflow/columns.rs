//! The column reflow: rejoin wrapped rows into logical lines, redistribute them
//! at the new width, and carry every tracked anchor with the character it sat
//! on.
//!
//! Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/reflow-and-resize.md`
//! section "`reflow_columns` — the iterator".
//!
//! Derived from the algorithm of `avt`'s `Reflow` iterator (Apache-2.0,
//! <https://github.com/asciinema/avt>) as described in that design and in
//! `research/prior-art.md` § 9.3: group rows into logical lines on the wrap
//! flag, trim, redistribute. No `avt` source is copied — the storage, the cell
//! representation and the anchor mechanism are OneTerm's — but the approach is
//! theirs and the attribution is deliberate.

use std::collections::VecDeque;

use crate::cell::{Cell, CellContent, CellWidth};
use crate::grid::{Anchors, Pos, Row, RowFlags, RowId, Screen, SeqNo};

/// One position the reflow was asked to carry, and where it landed.
///
/// `dest` is a **produced-row index**, not a `RowId`: rows produced before the
/// oldest survivor are trimmed away (trap 32), and the index is what says which
/// side of that line a point fell on.
struct Point {
    key: Pos,
    offset: usize,
    dest: Option<(usize, u16)>,
}

/// Where the rows a logical line produced go.
trait RowSink {
    fn push(&mut self, cells: Vec<Cell>, wrapped: bool, hints: RowFlags) -> usize;
    /// A row with nothing in it: the ring stores `None` and allocates no cells,
    /// which is where the engine's empty-scrollback memory figure comes from.
    fn push_blank(&mut self) -> usize;
    /// The index the next pushed row will get.
    fn next_index(&self) -> usize;
}

/// Collects the reflow's output, dropping the oldest rows once the screen's own
/// bound is reached (trap 32). Capping here rather than afterwards is also what
/// keeps the result inside the ring, whose length is a session constant (R-30).
struct RingSink {
    rows: VecDeque<Option<Row>>,
    cap: usize,
    produced: usize,
    dropped: usize,
    seq: SeqNo,
}

impl RowSink for RingSink {
    fn push(&mut self, cells: Vec<Cell>, wrapped: bool, hints: RowFlags) -> usize {
        let row = Row::from_cells(cells, wrapped, hints, self.seq);
        self.store(Some(row))
    }

    fn push_blank(&mut self) -> usize {
        self.store(None)
    }

    fn next_index(&self) -> usize {
        self.produced
    }
}

impl RingSink {
    fn store(&mut self, row: Option<Row>) -> usize {
        let index = self.produced;
        self.produced += 1;
        self.rows.push_back(row);
        while self.rows.len() > self.cap {
            self.rows.pop_front();
            self.dropped += 1;
        }
        index
    }
}

/// Counts the rows a reflow *would* produce, for `measure_rows`.
struct CountSink {
    produced: usize,
}

impl RowSink for CountSink {
    fn push(&mut self, _cells: Vec<Cell>, _wrapped: bool, _hints: RowFlags) -> usize {
        self.push_blank()
    }

    fn push_blank(&mut self) -> usize {
        let index = self.produced;
        self.produced += 1;
        index
    }

    fn next_index(&self) -> usize {
        self.produced
    }
}

/// What one reflow did, for the caller that has to restate the cursor and the
/// viewport before anything calls `Screen::sync_anchors`.
pub(crate) struct Reflowed {
    pub(crate) rows_trimmed: u32,
    /// The cursor's new position. `col == new_cols` means "past the last
    /// column", which is how the pending wrap is carried through (trap 31).
    pub(crate) cursor: Option<Pos>,
}

/// Reflow every live row of `screen` to `new_cols`, moving every tracked anchor
/// with its character.
pub(crate) fn reflow_columns(
    screen: &mut Screen,
    new_cols: u16,
    anchors: &mut Anchors,
) -> Reflowed {
    let old_cols = screen.cols();
    let cursor = *screen.cursor();
    let lane = screen.row_range();
    let cursor_key = Pos {
        row: cursor.pos.row,
        col: if cursor.pending_wrap {
            old_cols
        } else {
            cursor.pos.col
        },
    };

    let mut points = collect_points(anchors, &lane, cursor_key);
    let mut sink = RingSink {
        rows: VecDeque::new(),
        cap: screen.scrollback_limit() as usize + screen.rows() as usize,
        produced: 0,
        dropped: 0,
        seq: screen.seq(),
    };

    let mut line = Line::default();
    let mut next_point = 0;
    let (oldest, newest) = (screen.oldest(), screen.newest());
    let mut id = oldest;
    loop {
        let row = screen.take_row(id);
        let flags = row
            .as_ref()
            .map_or(RowFlags::empty(), |row| row.header().flags);
        line.push_row(id, row.as_ref().map(Row::cells), flags);
        let last = id == newest;
        if last || !flags.contains(RowFlags::WRAPPED) {
            let points = line.claim_points(&mut points, &mut next_point);
            line.finish(new_cols, cursor_key, points, &mut sink);
        }
        if last {
            break;
        }
        id = id + 1;
    }

    // The screen is always at least as tall as its viewport.
    while sink.rows.len() < screen.rows() as usize {
        sink.push_blank();
    }

    let (dropped, base) = (sink.dropped, screen.newest() + 1);
    screen.install_rows(sink.rows.into(), new_cols);

    let resolve = |dest: Option<(usize, u16)>| -> Option<Pos> {
        let (index, col) = dest?;
        // Its logical line was trimmed out of history.
        let index = index.checked_sub(dropped)?;
        Some(Pos {
            row: base + index as u64,
            col,
        })
    };
    let table: Vec<(Pos, Option<Pos>)> = points
        .iter()
        .map(|point| (point.key, resolve(point.dest)))
        .collect();
    let last_col = new_cols - 1;
    anchors.remap(|pos| {
        // Lane check rather than a screen discriminant on `AnchorKind`: only the
        // primary screen reflows, and the two screens draw from disjoint runs of
        // the id space, so the row already says which screen an entry is on.
        if !lane.contains(&pos.row) {
            return Some(pos);
        }
        let mapped = table
            .iter()
            .find(|(key, _)| *key == pos)
            .and_then(|(_, mapped)| *mapped)?;
        Some(Pos {
            row: mapped.row,
            col: mapped.col.min(last_col),
        })
    });

    Reflowed {
        rows_trimmed: dropped as u32,
        cursor: table
            .iter()
            .find(|(key, _)| *key == cursor_key)
            .and_then(|(_, mapped)| *mapped),
    }
}

/// The viewport row conhost puts the cursor on after re-wrapping the viewport at
/// `new_cols`.
///
/// The rows from the top of the screen down to the cursor row go through the
/// same iterator, so joins, splits and wide spacers are decided by the code that
/// reflows the real screen. History above the screen never enters the count, and
/// the top row starts a logical line whether or not the row above it wrapped —
/// which is the measured conhost rule (BUG-0051 § Measurements (b)).
pub(crate) fn measure_rows(screen: &Screen, new_cols: u16) -> u16 {
    let cursor = screen.cursor();
    let old_cols = screen.cols();
    let cursor_key = Pos {
        row: cursor.pos.row,
        col: if cursor.pending_wrap {
            old_cols
        } else {
            cursor.pos.col
        },
    };
    let mut points = vec![Point {
        key: cursor_key,
        offset: 0,
        dest: None,
    }];
    let mut sink = CountSink { produced: 0 };

    let mut line = Line::default();
    let mut next_point = 0;
    let top = screen.screen_top();
    let last_row = cursor.pos.row.max(top).min(screen.newest());
    let mut id = top;
    loop {
        let row = screen.row(id);
        let flags = row.flags();
        let cells = if row.is_allocated() {
            Some(row.cells())
        } else {
            None
        };
        line.push_row(id, cells, flags);
        let last = id == last_row;
        if last || !flags.contains(RowFlags::WRAPPED) {
            let points = line.claim_points(&mut points, &mut next_point);
            line.finish(new_cols, cursor_key, points, &mut sink);
        }
        if last {
            break;
        }
        id = id + 1;
    }
    points[0]
        .dest
        .map_or(0, |(index, _)| index.min(u16::MAX as usize) as u16)
}

/// One logical line under construction: the cells of every row that wrapped into
/// it, and where each of those rows starts inside them.
#[derive(Default)]
struct Line {
    cells: Vec<Cell>,
    starts: Vec<(RowId, usize)>,
    hints: RowFlags,
}

impl Line {
    fn push_row(&mut self, id: RowId, cells: Option<&[Cell]>, flags: RowFlags) {
        self.starts.push((id, self.cells.len()));
        if let Some(cells) = cells {
            // A trailing `LeadingWideSpacer` holds the place of a glyph that
            // wrapped to the next row: it is not content and the redistribution
            // puts a fresh one back wherever the new width needs one (trap 33).
            let end = match cells.last() {
                Some(cell) if cell.width() == CellWidth::LeadingWideSpacer => cells.len() - 1,
                _ => cells.len(),
            };
            self.cells.extend_from_slice(&cells[..end]);
        }
        // `HAS_GRAPHIC` cannot be re-derived without the interner, so it is
        // carried over. Over-approximating is what a content hint promises.
        if flags.contains(RowFlags::HAS_GRAPHIC) {
            self.hints.insert(RowFlags::HAS_GRAPHIC);
        }
    }

    /// The points that sit on this line, as a slice of the sorted list.
    fn claim_points<'a>(&self, points: &'a mut [Point], next: &mut usize) -> &'a mut [Point] {
        let start = *next;
        let last_row = self.starts.last().map_or(RowId::default(), |(id, _)| *id);
        while *next < points.len() && points[*next].key.row <= last_row {
            *next += 1;
        }
        &mut points[start..*next]
    }

    fn offset_of(&self, pos: Pos) -> Option<usize> {
        self.starts
            .iter()
            .find(|(id, _)| *id == pos.row)
            .map(|(_, start)| start + pos.col as usize)
    }

    /// Trim, apply the cursor's whitespace assumption, lay the line out, and
    /// start over.
    fn finish(
        &mut self,
        new_cols: u16,
        cursor_key: Pos,
        points: &mut [Point],
        sink: &mut dyn RowSink,
    ) {
        let cursor_offset = self.offset_of(cursor_key);
        let mut length = self.cells.len();
        while length > 0 && is_trimmable(self.cells[length - 1]) {
            length -= 1;
        }
        // Windows Terminal's `REFLOW_JANK_CURSOR_WRAP` assumption: on the
        // cursor's logical line the trailing blanks up to the cursor are treated
        // as content, which is what keeps the cursor's distance from the text
        // before it.
        let length = length.max(cursor_offset.unwrap_or(0));
        self.cells.resize(length, Cell::EMPTY);

        for point in points.iter_mut() {
            // An anchor on a cell the trim removed lands at the end of its
            // logical line rather than vanishing.
            point.offset = self.offset_of(point.key).unwrap_or(0).min(length);
        }
        emit_line(&self.cells, new_cols, self.hints, points, sink);

        self.cells.clear();
        self.starts.clear();
        self.hints = RowFlags::empty();
    }
}

/// Lay one logical line out at `new_cols`, recording where each point landed.
fn emit_line(
    cells: &[Cell],
    new_cols: u16,
    hints: RowFlags,
    points: &mut [Point],
    sink: &mut dyn RowSink,
) {
    let cols = new_cols as usize;
    let len = cells.len();
    if len == 0 {
        let index = sink.push_blank();
        for point in points.iter_mut() {
            point.dest = Some((index, 0));
        }
        return;
    }

    let mut row: Vec<Cell> = Vec::with_capacity(cols);
    let mut i = 0;
    loop {
        // Trap 33: a wide glyph that would straddle the new last column is
        // replaced there by a `LeadingWideSpacer` and moves to the next row.
        if i < len && cols >= 2 && row.len() + 2 > cols && is_wide_pair(cells, i) {
            row.push(Cell::EMPTY.with_width(CellWidth::LeadingWideSpacer));
            row.resize(cols, Cell::EMPTY);
            sink.push(row, true, hints);
            row = Vec::with_capacity(cols);
        }
        let here = (sink.next_index(), row.len() as u16);
        for point in points.iter_mut().filter(|point| point.offset == i) {
            point.dest = Some(here);
        }
        if i == len {
            row.resize(cols, Cell::EMPTY);
            sink.push(row, false, hints);
            return;
        }

        if cols >= 2 && is_wide_pair(cells, i) {
            // The spacer half moves with its glyph, so an anchor on it has to be
            // recorded here: the walk steps over both cells at once.
            let spacer = (here.0, here.1 + 1);
            for point in points.iter_mut().filter(|point| point.offset == i + 1) {
                point.dest = Some(spacer);
            }
            row.push(cells[i]);
            row.push(cells[i + 1]);
            i += 2;
        } else {
            row.push(narrowed(cells[i]));
            i += 1;
        }

        if row.len() >= cols {
            let wrapped = i < len;
            let index = sink.push(row, wrapped, hints);
            row = Vec::with_capacity(cols);
            if !wrapped {
                // The line ended exactly at the row boundary, so a point at its
                // end sits past the last column — the pending wrap (trap 31).
                for point in points.iter_mut().filter(|point| point.offset == len) {
                    point.dest = Some((index, new_cols));
                }
                return;
            }
        }
    }
}

fn is_wide_pair(cells: &[Cell], i: usize) -> bool {
    cells[i].width() == CellWidth::Wide
        && cells
            .get(i + 1)
            .is_some_and(|next| next.width() == CellWidth::WideSpacer)
}

/// A half-pair that cannot be kept — an orphan, or a wide glyph in a
/// single-column terminal — degrades to a blank that keeps its style, exactly as
/// `repair_wide_pairs` does elsewhere.
fn narrowed(cell: Cell) -> Cell {
    match cell.width() {
        CellWidth::Narrow => cell,
        _ => cell
            .with_width(CellWidth::Narrow)
            .with_content(CellContent::Scalar(' ')),
    }
}

/// What the trailing-blank trim removes: a plain blank cell.
///
/// A `WideSpacer` also reads as a space, so the width test is what stops the
/// trim from orphaning the wide glyph in the column before it. A styled blank
/// stays: its background is content.
fn is_trimmable(cell: Cell) -> bool {
    cell.width() == CellWidth::Narrow && cell.is_blank()
}

/// Every live anchor in this screen's lane, plus the cursor's pending-wrap
/// position, sorted so each logical line can claim its own in one pass.
fn collect_points(anchors: &Anchors, lane: &std::ops::Range<RowId>, cursor_key: Pos) -> Vec<Point> {
    let mut points: Vec<Point> = anchors
        .iter()
        .filter(|(_, anchor)| anchor.alive && lane.contains(&anchor.pos.row))
        .map(|(_, anchor)| Point {
            key: anchor.pos,
            offset: 0,
            dest: None,
        })
        .chain(std::iter::once(Point {
            key: cursor_key,
            offset: 0,
            dest: None,
        }))
        .collect();
    points.sort_unstable_by_key(|point| (point.key.row, point.key.col));
    points.dedup_by_key(|point| point.key);
    points
}
