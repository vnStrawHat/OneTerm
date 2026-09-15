//! One copied row: cells, run-length styles and the row's own cluster arena.
//!
//! Everything here is a **value**. An interned id indexes tables the render
//! thread cannot read once the lock is released, and the grapheme arena may
//! renumber on a later batch, so a copied row resolves the style, the cluster
//! and the hyperlink while the lock is still held. The cost is about 20 bytes
//! per style run instead of two per cell — for **changed rows only**, and a
//! uniformly styled 200-column row is one run.

// Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/damage-and-render-state.md`
// section "Resolved values, never ids" (R-14).

use crate::cell::{CellContent, CellWidth, Semantic, Style};
use crate::grid::{RowId, RowRef, SeqNo};
use crate::intern::{ExtrasId, GraphicId, Hyperlink, HyperlinkId, Interner};
use crate::snapshot::palette::Palette;

/// A run of columns sharing one style, carrying the **resolved value**.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StyleRun {
    /// The half-open column range this run covers.
    pub cols: std::ops::Range<u16>,
    /// The style every column in the run paints with, already resolved.
    pub style: Style,
}

/// What a copied cell holds instead of a `char`: a scalar, or a span into the
/// row's own cluster arena.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SnapshotContent {
    /// A single Unicode scalar value, the common case.
    Scalar(char),
    /// A grapheme cluster, as a span of [`SnapshotRow::clusters`].
    Cluster {
        /// Index of the cluster's first `char` in [`SnapshotRow::clusters`].
        start: u32,
        /// The cluster's length in `char`s.
        len: u32,
    },
}

/// One copied cell. `run` indexes the row's [`StyleRun`] list rather than
/// repeating the style per cell.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SnapshotCell {
    /// The text this cell paints.
    pub content: SnapshotContent,
    /// Whether the cell is narrow, the left half of a wide glyph, or that
    /// glyph's spacer.
    pub width: CellWidth,
    /// Shell-prompt marking from `OSC 133`, if the program sent any.
    pub semantic: Semantic,
    /// Index into [`SnapshotRow::runs`] of the style this cell paints with; see
    /// [`SnapshotRow::style_of`].
    pub run: u16,
    /// The `OSC 8` hyperlink this cell belongs to; resolve it with
    /// [`crate::SnapshotState::hyperlink`].
    pub hyperlink: Option<HyperlinkId>,
    /// The image covering this cell, if any; resolve it with
    /// [`crate::SnapshotState::placement`].
    pub graphic: Option<GraphicId>,
}

/// One viewport row as the renderer sees it.
///
/// Allocations are reused across frames: a row that is copied again clears its
/// vectors and refills them, so the steady state allocates nothing.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct SnapshotRow {
    /// The row's absolute id. It names the same content for as long as that
    /// content is live, so a consumer's per-row cache can be keyed by it.
    pub id: RowId,
    /// The sequence number this copy was taken at.
    pub seq: SeqNo,
    /// Whether the line continues on the next row rather than ending here.
    pub wrapped: bool,
    /// One entry per column, left to right.
    pub cells: Vec<SnapshotCell>,
    /// The style runs [`SnapshotCell::run`] indexes.
    pub runs: Vec<StyleRun>,
    /// Backs every [`SnapshotContent::Cluster`] in this row.
    pub clusters: Vec<char>,
}

impl SnapshotRow {
    /// The style of one cell, without a table lookup outside this row.
    pub fn style_of(&self, cell: &SnapshotCell) -> &Style {
        // A cell always names a run this row owns; the fallback keeps a
        // corrupted index from panicking the render thread.
        self.runs
            .get(cell.run as usize)
            .map_or(&Style::DEFAULT, |run| &run.style)
    }

    /// The codepoints of a [`SnapshotContent::Cluster`] span.
    pub fn cluster(&self, start: u32, len: u32) -> &[char] {
        let start = start as usize;
        self.clusters
            .get(start..start + len as usize)
            .unwrap_or(&[])
    }

    /// Phase 1: copy one grid row, resolving everything the view would
    /// otherwise need the engine for.
    pub(crate) fn copy_from(
        &mut self,
        row: RowRef<'_>,
        interner: &Interner,
        links: &mut Vec<(HyperlinkId, Hyperlink)>,
    ) {
        self.id = row.id();
        self.seq = row.seq();
        self.wrapped = row.wrapped();
        self.cells.clear();
        self.runs.clear();
        self.clusters.clear();

        let mut open_run: Option<crate::intern::StyleId> = None;
        for (col, cell) in row.cells().iter().enumerate() {
            let col = col as u16;
            let style_id = cell.style_id();
            if open_run != Some(style_id) {
                if let Some(run) = self.runs.last_mut() {
                    run.cols.end = col;
                }
                self.runs.push(StyleRun {
                    cols: col..col + 1,
                    style: *interner.resolve_style(style_id),
                });
                open_run = Some(style_id);
            }

            let content = match cell.content() {
                CellContent::Scalar(scalar) => SnapshotContent::Scalar(scalar),
                CellContent::Grapheme(id) => {
                    let cluster = interner.resolve_grapheme(id);
                    let start = self.clusters.len() as u32;
                    self.clusters.extend_from_slice(cluster);
                    SnapshotContent::Cluster {
                        start,
                        len: cluster.len() as u32,
                    }
                }
            };

            let (hyperlink, graphic) = if cell.extras_id() == ExtrasId::NONE {
                (None, None)
            } else {
                let extras = interner.resolve_extras(cell.extras_id());
                (
                    extras.hyperlink.map(|id| resolve_link(id, interner, links)),
                    extras.graphic,
                )
            };

            self.cells.push(SnapshotCell {
                content,
                width: cell.width(),
                semantic: cell.semantic(),
                run: (self.runs.len() - 1) as u16,
                hyperlink,
                graphic,
            });
        }
        if let Some(run) = self.runs.last_mut() {
            run.cols.end = row.cells().len() as u16;
        }
    }

    /// Phase 2, outside the lock: named and indexed colours become pixels.
    /// Idempotent, because a resolved `Rgb` maps to itself.
    pub fn map_colors(&mut self, palette: &Palette) {
        for run in &mut self.runs {
            run.style.fg = crate::cell::Color::Rgb(palette.resolve(run.style.fg));
            run.style.bg = crate::cell::Color::Rgb(palette.resolve(run.style.bg));
            if let Some(underline) = run.style.underline_color {
                run.style.underline_color =
                    Some(crate::cell::Color::Rgb(palette.resolve(underline)));
            }
        }
    }
}

/// Copy a hyperlink's strings into the snapshot state's own table once per
/// distinct id, so a link whose interned entry is later replaced cannot change
/// the painted URL mid-frame.
fn resolve_link(
    id: HyperlinkId,
    interner: &Interner,
    links: &mut Vec<(HyperlinkId, Hyperlink)>,
) -> HyperlinkId {
    if links.iter().any(|(known, _)| *known == id) {
        return id;
    }
    if let Some(link) = interner.hyperlinks.resolve(id) {
        links.push((id, link.clone()));
    }
    id
}
