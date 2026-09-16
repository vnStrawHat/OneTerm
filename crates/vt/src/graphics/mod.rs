//! Images: decoding them, anchoring them to cells, and telling the embedder
//! when one dies.
//!
//! Design:
//! <https://github.com/vnStrawHat/OneTerm/blob/main/docs/spec-intakes/IN-0029-vt-engine/low-level-design/graphics.md>
//!
//! Three rules carry the whole module:
//!
//! * **A cell stores *which* image, never *where inside it*.** The
//!   covered cells share **one** interned [`Extras`](crate::intern::Extras)
//!   entry; the painter derives its offset from the [`Placement`], because a
//!   4096x4096 Sixel covers about 84 000 cells — more than the whole `u16`
//!   extras id space — and a per-cell offset would make every one of them a
//!   distinct interned value.
//! * **A placement is a tracked anchor.** It therefore moves with its content
//!   through `IL`, `DL`, `SU`, `SD`, a region scroll and reflow, by the one
//!   mechanism that also moves the cursor and the selection.
//! * **Liveness is derived from rows, not from a counter.** A counter
//!   decremented by the cell writer misses `Row::reset`, scroll blanking,
//!   `clear_viewport`, the alternate-screen wipe and reflow — that is, `CSI 2 J`,
//!   `clear` and every TUI repaint, which is the common case and precisely the
//!   capability being added.
//!
//! Nothing here assumes an image is Sixel: [`GraphicData`] is format-agnostic
//! RGBA and [`sixel::SixelParser`] is one producer of it.

mod placement;
mod sixel;

use std::sync::Arc;

use crate::grid::AnchorId;
use crate::intern::GraphicId;

pub(crate) use placement::{assert_integrity, drain_released, place, sweep};
pub(crate) use sixel::SixelParser;

/// The **fallback** cell Sixel pixels are divided by (VT240 / VT340: 10 x 20),
/// and the value Windows conhost uses, so the rows an image consumes agree with
/// a ConPTY host.
///
/// It is the fallback and not the rule: an image's footprint in cells is
/// `ceil(pixels / cell)` against the size the embedder set with
/// [`Terminal::set_cell_pixels`](crate::Terminal::set_cell_pixels), which is
/// also what `CSI 14 t` reports, so a program that sizes an image from that
/// reply covers the cells it meant. **An embedder that never calls
/// `set_cell_pixels` gets VT340 sizing**, unchanged from before `BUG-0062`.
///
/// The renderer draws the image at its own pixel size, clipped to the
/// footprint. It does not rescale to this cell, and there is no
/// `set_cell_size`.
pub const VIRTUAL_CELL: (u16, u16) = (10, 20);

/// Largest width or height an image may have. Pixels beyond it are dropped.
pub(crate) const MAX_DIMENSION: u32 = 4096;

/// The pixel budget one image may occupy while it is being decoded, which both
/// axes being clamped to [`MAX_DIMENSION`] enforces by construction.
pub(crate) const MAX_PIXEL_BYTES: usize = (MAX_DIMENSION as usize) * (MAX_DIMENSION as usize) * 4;

/// Live placements one terminal may hold.
///
/// A hostile stream can emit an image per row of a million-row scrollback, and
/// the release sweep is linear in live placements, so the count is bounded the
/// way every other table in the engine is: past the bound the **oldest**
/// placement is released, which frees the view's texture rather than leaking it.
pub(crate) const MAX_PLACEMENTS: usize = 256;

/// Decoded image pixels: RGBA8, row-major, stride `width * 4`, **straight
/// (non-premultiplied) alpha**.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GraphicData {
    /// The engine's handle for this image. A cell that shows the image stores
    /// this id, and [`VtEvent::GraphicReleased`](crate::VtEvent::GraphicReleased)
    /// reports it when the last such cell is gone.
    pub id: GraphicId,
    /// Width in pixels, at most 4096.
    pub width: u32,
    /// Height in pixels, at most 4096.
    pub height: u32,
    /// `width * height * 4` bytes of RGBA8.
    pub rgba: Vec<u8>,
}

/// One image placed on the grid.
///
/// `anchor` is an [`AnchorKind::Graphic`](crate::grid::AnchorKind) entry naming
/// the **top-left** cell, so the placement follows its content; `cols` and
/// `rows` are its extent in cells, already clipped on the right.
///
/// [`SnapshotPlacement`](crate::SnapshotPlacement) is the same image seen
/// through a snapshot, positioned by resolved row and column instead.
///
/// You read these and never build one: no API takes a `Placement`, and the
/// `#[non_exhaustive]` mark with no `Default` says so in the type system.
// Returned, never built outside the crate, and `pixel_size` is an early shape.
#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Placement {
    /// The interned image this places.
    pub id: GraphicId,
    /// The anchor naming the top-left cell the image starts at.
    pub anchor: AnchorId,
    /// The image's width in cells, already clipped on the right.
    pub cols: u16,
    /// The image's height in cells.
    pub rows: u16,
    /// The source image's size in pixels, as decoded.
    pub pixel_size: (u32, u32),
}

/// Per-terminal graphics state: the id counter, the placements, the images
/// waiting to be taken, and the in-flight decoder.
#[derive(Debug)]
pub(crate) struct GraphicsState {
    next_id: u64,
    /// Images decoded since the embedder last took them. `Terminal::take_graphics`
    /// is the **only** drain.
    pub(crate) pending: Vec<Arc<GraphicData>>,
    pub(crate) placements: Vec<Placement>,
    /// Ids released since the last batch handed them over, so a release caused
    /// by a `resize` — which has no event batch — still reaches the embedder.
    pub(crate) released: Vec<GraphicId>,
    /// The `DCS q` sequence currently being received, if any.
    pub(crate) parser: Option<SixelParser>,
}

impl Default for GraphicsState {
    fn default() -> GraphicsState {
        GraphicsState {
            // Ids start at 1 and are never reset, `RIS` included: a stale id in
            // the view's texture store must never collide with a new image.
            next_id: 1,
            pending: Vec::new(),
            placements: Vec::new(),
            released: Vec::new(),
            parser: None,
        }
    }
}

impl GraphicsState {
    fn next_id(&mut self) -> GraphicId {
        let id = GraphicId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    /// `RIS`: pending images and the in-flight decoder go, the id counter and
    /// the placements stay. The reset blanks every row, so the sweep releases
    /// the placements on its own — which is what makes the release event fire
    /// exactly once rather than once here and once there.
    pub(crate) fn reset(&mut self) {
        self.pending.clear();
        self.parser = None;
    }

    /// The placement covering `id`, for the painter's offset arithmetic.
    pub(crate) fn placement(&self, id: GraphicId) -> Option<&Placement> {
        self.placements.iter().find(|entry| entry.id == id)
    }
}

impl std::fmt::Debug for SixelParser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SixelParser")
    }
}

#[cfg(test)]
#[path = "graphics_tests.rs"]
mod tests;
