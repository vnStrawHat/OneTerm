//! Sixel through a real [`Terminal`]: decoder, placement, cursor, liveness, the
//! release signal and the painter's offset.
//!
//! The first block reproduces the eleven expectations the adapter's own Sixel
//! tests pinned against the engine being replaced — same bytes, same expected
//! cells and cursor — under the names
//! `graphics.md` § Verification gives them. The second block is what this
//! engine adds: one extras entry per image, placements that move with their
//! content, and a release event.

use std::sync::Arc;
use std::time::Instant;

use super::*;
use crate::cell::Cell;
use crate::event::{EventBatch, VtEvent};
use crate::grid::{RowId, Size};
use crate::intern::Extras;
use crate::reflow::ResizePolicy;
use crate::render::{RenderState, RenderUpdate};
use crate::terminal::{Config, Terminal};

// ── Harness ─────────────────────────────────────────────────────────────────

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const CLEAR: [u8; 4] = [0, 0, 0, 0];

/// A `DCS q` sequence around `body`.
fn sixel(body: &str) -> Vec<u8> {
    format!("\x1bPq{body}\x1b\\").into_bytes()
}

struct Session {
    term: Terminal,
    batch: EventBatch,
    now: Instant,
}

impl Session {
    fn new(cols: u16, rows: u16) -> Session {
        Session::with_scrollback(cols, rows, 10_000)
    }

    fn with_scrollback(cols: u16, rows: u16, scrollback_limit: u32) -> Session {
        Session {
            term: Terminal::new(
                Size { rows, cols },
                Config {
                    scrollback_limit,
                    ..Config::default()
                },
            ),
            batch: EventBatch::new(),
            now: Instant::now(),
        }
    }

    fn feed(&mut self, bytes: &[u8]) {
        self.term.feed(bytes, &mut self.batch, self.now);
    }

    /// Deliver whatever a `resize` queued, without changing the grid.
    fn settle(&mut self) {
        self.feed(b"");
    }

    fn one_image(&mut self) -> Arc<GraphicData> {
        let mut images = self.term.take_graphics();
        assert_eq!(images.len(), 1, "exactly one image expected");
        images.pop().expect("one image")
    }

    fn row_id(&self, index: u16) -> RowId {
        self.term.screen().row_of_index(index)
    }

    fn cell(&self, row: RowId, col: u16) -> Cell {
        self.term.grid().screen_of(row).row(row).cell(col)
    }

    /// The image a cell belongs to, which is all a cell ever stores (R-21).
    fn graphic(&self, index: u16, col: u16) -> Option<GraphicId> {
        self.graphic_at(self.row_id(index), col)
    }

    fn graphic_at(&self, row: RowId, col: u16) -> Option<GraphicId> {
        self.term
            .interner()
            .resolve_extras(self.cell(row, col).extras_id())
            .graphic
    }

    /// The cell's `(col, row)` offset inside its image, derived from the
    /// placement the way the painter derives it.
    fn offset(&self, index: u16, col: u16) -> Option<(GraphicId, u16, u16)> {
        let row = self.row_id(index);
        let id = self.graphic_at(row, col)?;
        let placement = self
            .term
            .placements()
            .iter()
            .find(|placement| placement.id == id)?;
        let top = self.term.grid().anchors().get(placement.anchor)?;
        Some((
            id,
            col - top.col,
            u16::try_from(row.distance(top.row)).expect("inside the placement"),
        ))
    }

    fn cursor(&self) -> (u16, u16) {
        (
            self.term.screen().cursor_row_index(),
            self.term.screen().cursor().pos.col,
        )
    }

    fn released(&self) -> Vec<GraphicId> {
        self.batch
            .iter()
            .filter_map(|event| match event {
                VtEvent::GraphicReleased(id) => Some(*id),
                _ => None,
            })
            .collect()
    }

    fn replies(&self) -> String {
        let mut out = String::new();
        for event in self.batch.iter() {
            if let VtEvent::Reply(span) = event {
                out.push_str(&String::from_utf8_lossy(self.batch.bytes(*span)));
            }
        }
        out
    }

    fn render(&mut self) -> (RenderState, RenderUpdate) {
        let mut state = RenderState::new();
        let update = self.term.render_update(&mut state, self.now);
        (state, update)
    }
}

fn pixel(image: &GraphicData, x: u32, y: u32) -> [u8; 4] {
    let index = ((y * image.width + x) * 4) as usize;
    image.rgba[index..index + 4].try_into().expect("four bytes")
}

// ── The IN-0028 expectations, at engine level ───────────────────────────────

#[test]
fn decodes_a_minimal_sixel() {
    let mut session = Session::new(20, 5);
    session.feed(&sixel("#0;2;100;0;0#0~"));
    let image = session.one_image();
    assert_eq!((image.width, image.height), (1, 6));
    for y in 0..6 {
        assert_eq!(pixel(&image, 0, y), RED, "row {y}");
    }
    assert!(
        session.term.take_graphics().is_empty(),
        "an image is handed out exactly once"
    );
}

#[test]
fn rgb_and_hls_colour_registers() {
    // Register 0 red, register 1 blue; column 0 red, `$` back to x = 0, skip one
    // column with `?`, column 1 blue.
    let mut session = Session::new(20, 5);
    session.feed(&sixel("#0;2;100;0;0#1;2;0;0;100#0~$#1?~"));
    let image = session.one_image();
    assert_eq!((image.width, image.height), (2, 6));
    assert_eq!(pixel(&image, 0, 0), RED);
    assert_eq!(pixel(&image, 0, 5), RED);
    assert_eq!(pixel(&image, 1, 0), BLUE);

    // DEC HLS hue 120 is red (hue 0 is blue).
    let mut session = Session::new(20, 5);
    session.feed(&sixel("#2;1;120;50;100#2~"));
    let image = session.one_image();
    assert_eq!(pixel(&image, 0, 0), RED, "DEC HLS hue 120 is red");
}

#[test]
fn repeat_and_band_control_characters() {
    // `!3` repeats the column; `A` (0x41 - 0x3F = 2) sets only the second pixel.
    let mut session = Session::new(20, 5);
    session.feed(&sixel("#0;2;100;0;0#0!3A"));
    let image = session.one_image();
    assert_eq!((image.width, image.height), (3, 6));
    assert_eq!(pixel(&image, 2, 1), RED);

    // `-` starts a new band six pixels down.
    let mut session = Session::new(20, 5);
    session.feed(&sixel("#0;2;100;0;0#0~-~"));
    let image = session.one_image();
    assert_eq!((image.width, image.height), (1, 12));
    assert_eq!(pixel(&image, 0, 11), RED);
}

#[test]
fn untouched_pixels_stay_transparent() {
    // Parity: `P2` is ignored and nothing paints a background.
    let mut session = Session::new(20, 5);
    session.feed(b"\x1bP0;1;0q#0;2;100;0;0#0!3A\x1b\\");
    let image = session.one_image();
    assert_eq!(pixel(&image, 2, 1), RED);
    assert_eq!(pixel(&image, 2, 0), CLEAR, "unset rows stay transparent");
    assert_eq!(pixel(&image, 0, 0), CLEAR);
}

#[test]
fn raster_attributes_declare_the_size() {
    let mut session = Session::new(20, 5);
    session.feed(&sixel("\"1;1;1;3#0;2;100;0;0#0~-~"));
    let image = session.one_image();
    assert_eq!(
        (image.width, image.height),
        (1, 3),
        "the raster size wins over the data"
    );
    assert_eq!(pixel(&image, 0, 2), RED);

    let mut session = Session::new(20, 5);
    session.feed(&sixel("#0;2;100;0;0#0~-~"));
    let image = session.one_image();
    assert_eq!((image.width, image.height), (1, 12), "measured when absent");
}

#[test]
fn dimensions_clamp_at_4096() {
    let mut session = Session::new(20, 5);
    session.feed(&sixel("#0!99999~"));
    let image = session.one_image();
    assert_eq!((image.width, image.height), (MAX_DIMENSION, 6));
    assert_eq!(image.rgba.len(), (MAX_DIMENSION * 6 * 4) as usize);
}

/// Pixels are virtual 10 x 20 cells (VT340 / conhost). A 25 x 45 image with five
/// bands covers 3 x 3 cells, and the image row below the cursor is still placed.
#[test]
fn placed_at_the_cursor_column_and_clipped_right() {
    let mut session = Session::new(10, 6);
    session.feed(b"\x1b[3;4H"); // line 2, column 3
    session.feed(&sixel("\"1;1;25;45#0~-~-~-~-~"));
    let image = session.one_image();
    for (row, index) in [(0u16, 2u16), (1, 3), (2, 4)] {
        for (col, column) in [(0u16, 3u16), (1, 4), (2, 5)] {
            assert_eq!(
                session.offset(index, column),
                Some((image.id, col, row)),
                "row {index} column {column}"
            );
        }
        assert_eq!(session.graphic(index, 6), None, "nothing past the image");
        assert_eq!(session.graphic(index, 2), None, "nothing before it");
    }
    assert_eq!(
        session
            .cell(session.row_id(2), 3)
            .text_char(&session.term.interner().graphemes),
        ' ',
        "the cell's text is untouched"
    );

    // Wider than the grid: clipped on the right, never wrapped.
    let mut session = Session::new(4, 5);
    session.feed(b"\x1b[1;3H"); // column 2 of 4
    session.feed(&sixel("\"1;1;80;8#0~"));
    let image = session.one_image();
    assert_eq!(session.offset(0, 3), Some((image.id, 1, 0)));
    assert_eq!(session.cursor(), (0, 2), "one band: the cursor row is kept");
    let placement = session.term.placements()[0];
    assert_eq!(placement.cols, 2, "eight cells clipped to the two that fit");
}

/// The band count, not the raster height, decides how far the cursor moves:
/// two bands (top of the last at 6 px) stay on the row, four bands (18 px) too,
/// five bands (24 px) move one row, as `bands * 6 / 20` on a VT340.
#[test]
fn cursor_descends_bands_times_six_over_twenty() {
    for (body, rows_down) in [
        ("#0~-~", 0u16),
        ("#0~-~-~-~", 0),
        ("#0~-~-~-~-~", 1),
        ("#0~-~-~-~-~-~-~-~", 2),
    ] {
        let mut session = Session::new(10, 8);
        session.feed(&sixel(&format!("\"1;1;8;60{body}")));
        let image = session.one_image();
        assert_eq!(session.cursor(), (rows_down, 0), "{body}");
        // Every one of the three cell rows the 60 px image covers is referenced.
        for row in 0..3u16 {
            assert_eq!(
                session.offset(row, 0),
                Some((image.id, 0, row)),
                "{body} row {row}"
            );
        }
    }
}

#[test]
fn scrolls_into_history_with_its_cells() {
    let mut session = Session::new(10, 3);
    session.feed(b"\x1b[3;1H"); // the last line
    session.feed(&sixel("\"1;1;8;32#0~-~-~-~-~")); // 1 col x 2 rows, cursor 1 row down
    let image = session.one_image();
    // The line feed at the bottom pushes the top line into history: the image
    // rows end on lines 1 and 2 and the cursor stays on the last line.
    assert_eq!(session.term.screen().history_len(), 1, "one line scrolled");
    assert_eq!(session.offset(1, 0), Some((image.id, 0, 0)));
    assert_eq!(session.offset(2, 0), Some((image.id, 0, 1)));
    assert_eq!(session.cursor(), (2, 0));

    // And the whole image survives scrolling off the screen entirely.
    session.feed(b"\r\n\r\n\r\n\r\n");
    let placement = session.term.placements()[0];
    let top = session
        .term
        .grid()
        .anchors()
        .get(placement.anchor)
        .expect("the anchor followed its content");
    assert!(
        session.term.screen().index_of(top.row).is_none(),
        "the image is wholly in history"
    );
    assert_eq!(session.graphic_at(top.row, 0), Some(image.id));
    assert_eq!(session.graphic_at(top.row + 1, 0), Some(image.id));
    assert!(
        session.released().is_empty(),
        "an image in scrollback is still alive"
    );
}

#[test]
fn overwriting_or_erasing_a_cell_drops_the_reference() {
    let mut session = Session::new(10, 5);
    session.feed(&sixel("\"1;1;16;16#0~"));
    let image = session.one_image();
    assert_eq!(session.graphic(0, 0), Some(image.id));
    assert_eq!(session.graphic(0, 1), Some(image.id));

    session.feed(b"\x1b[1;2Hx"); // overwrite column 1
    assert_eq!(session.graphic(0, 1), None, "text drops the reference");
    assert_eq!(session.graphic(0, 0), Some(image.id), "and only that cell");

    // R-13: `CSI 2 J` on the primary screen scrolls a graphic-bearing row into
    // scrollback rather than deciding it was blank, so the image survives.
    session.feed(b"\x1b[2J");
    assert_eq!(session.graphic(0, 0), None, "gone from the viewport");
    let top = session.term.screen().screen_top();
    assert_eq!(session.graphic_at(top - 1, 0), Some(image.id), "in history");
}

#[test]
fn ris_drops_pending_images_but_not_the_id_counter() {
    let mut session = Session::new(10, 5);
    session.feed(&sixel("\"1;1;16;16#0~"));
    let first = session.one_image().id;

    session.feed(&sixel("#0~"));
    session.feed(b"\x1bc"); // RIS
    assert!(
        session.term.take_graphics().is_empty(),
        "RIS drops pending images"
    );

    session.feed(&sixel("#0~"));
    let third = session.one_image().id;
    assert!(
        third.0 > first.0 + 1,
        "the id counter survives RIS: {third:?} after {first:?}"
    );
}

#[test]
fn clear_screen_does_not_drop_pending_images() {
    let mut session = Session::new(10, 5);
    session.feed(&sixel("\"1;1;16;16#0~"));
    session.feed(b"\x1b[2J");
    let images = session.term.take_graphics();
    assert_eq!(images.len(), 1, "the pixels are still owed to the embedder");
}

#[test]
fn da1_advertises_sixel() {
    let mut session = Session::new(10, 3);
    session.feed(b"\x1b[c");
    let reply = session.replies();
    assert_eq!(reply, "\x1b[?62;4;22c");
    assert!(
        reply
            .split(';')
            .any(|part| part.trim_end_matches('c') == "4"),
        "extension 4 is Sixel, which is what tmux, lsix, chafa and timg look for"
    );
}

#[test]
fn empty_and_non_sixel_dcs_place_nothing() {
    let mut session = Session::new(20, 5);
    session.feed(b"\x1b[2;2H");
    session.feed(&sixel(""));
    session.feed(b"\x1bP1$r0m\x1b\\"); // DECRSPS, not Sixel
    assert!(session.term.take_graphics().is_empty());
    assert_eq!(session.cursor(), (1, 1));
    assert_eq!(session.graphic(1, 1), None);
    assert!(session.term.placements().is_empty());
}

// ── What this engine adds ───────────────────────────────────────────────────

/// R-21: a 400 x 200-cell image grows the extras table by exactly one, because
/// the cell stores *which* image and never *where inside it*.
#[test]
fn one_extras_entry_per_image() {
    let mut session = Session::with_scrollback(400, 210, 16);
    let before = session.term.interner().extras.entries();
    session.feed(&sixel("\"1;1;4000;4000#0;2;100;0;0#0~"));
    let image = session.one_image();
    let after = session.term.interner().extras.entries();
    assert_eq!(
        after - before,
        1,
        "one interned extras entry for 80 000 cells"
    );

    let placement = session.term.placements()[0];
    assert_eq!((placement.cols, placement.rows), (400, 200));
    assert_eq!(session.offset(199, 399), Some((image.id, 399, 199)));
}

/// R-02: an in-region scroll moves content between row ids, so the placement
/// follows its **anchor** — not a `RowId` it once recorded.
#[test]
fn placement_moves_with_an_in_region_scroll() {
    let mut session = Session::new(10, 10);
    session.feed(b"\x1b[3;8r"); // DECSTBM rows 3..8, i.e. indices 2..8
    session.feed(b"\x1b[4;1H"); // index 3
    session.feed(&sixel("\"1;1;8;16#0~"));
    let image = session.one_image();
    assert_eq!(session.offset(3, 0), Some((image.id, 0, 0)));

    session.feed(b"\x1b[S"); // SU 1 inside the region
    assert_eq!(
        session.offset(2, 0),
        Some((image.id, 0, 0)),
        "the image moved up with its content"
    );
    assert!(session.released().is_empty());

    session.feed(b"\x1b[3;1H\x1b[L"); // IL 1 at the region top pushes it back down
    assert_eq!(session.offset(3, 0), Some((image.id, 0, 0)));
}

/// R-22, the case a per-cell counter missed: on the alternate screen `CSI 2 J`
/// resets every row, which is what every TUI repaint does.
#[test]
fn release_event_fires_on_clear_screen() {
    let mut session = Session::new(10, 5);
    session.feed(b"\x1b[?1049h");
    session.feed(&sixel("\"1;1;16;16#0~"));
    let image = session.one_image();
    assert_eq!(session.graphic(0, 0), Some(image.id));

    session.feed(b"\x1b[2J");
    assert_eq!(session.released(), vec![image.id]);
    assert!(session.term.placements().is_empty());

    session.feed(b"x");
    assert!(session.released().is_empty(), "released exactly once");
}

#[test]
fn release_event_fires_on_row_reset_and_scroll_blank() {
    // A row reset: `RIS` blanks every row and drops the history with it.
    let mut session = Session::new(10, 5);
    session.feed(&sixel("\"1;1;16;16#0~"));
    let image = session.one_image();
    session.feed(b"\x1bc");
    assert_eq!(session.released(), vec![image.id]);

    // A scroll blank: the region rotates by its own height and is then blank.
    let mut session = Session::new(10, 4);
    session.feed(b"\x1b[2;4r\x1b[2;1H");
    session.feed(&sixel("\"1;1;16;16#0~"));
    let image = session.one_image();
    assert_eq!(session.graphic(1, 0), Some(image.id));
    session.feed(b"\x1b[3S");
    assert_eq!(session.released(), vec![image.id]);
    assert_eq!(session.graphic(1, 0), None);
}

#[test]
fn release_event_fires_when_the_anchor_row_is_trimmed() {
    let mut session = Session::with_scrollback(10, 3, 0);
    session.feed(b"\x1b[1;1H");
    session.feed(&sixel("\"1;1;16;16#0~"));
    let image = session.one_image();
    assert!(session.released().is_empty());

    // With no scrollback the first row leaving the screen is discarded.
    session.feed(b"\x1b[3;1H\r\n");
    assert_eq!(session.released(), vec![image.id]);
}

#[test]
fn release_event_fires_when_reflow_drops_the_anchor() {
    let mut session = Session::with_scrollback(10, 4, 0);
    session.feed(&sixel("\"1;1;16;16#0~")); // the image is the oldest row
    let image = session.one_image();
    // Three rows of one wrapped logical line below it: halving the width doubles
    // them, and with no scrollback the overflow — the image's row — is dropped.
    session.feed(b"\x1b[2;1H");
    session.feed(&b"0123456789".repeat(3));

    let outcome = session
        .term
        .resize(Size { rows: 4, cols: 5 }, ResizePolicy::default());
    assert!(outcome.reflowed && outcome.rows_trimmed > 0);
    assert!(
        session.term.placements().is_empty(),
        "the anchored row did not survive the reflow"
    );
    // `resize` has no batch of its own; the next feed delivers what it queued.
    session.settle();
    assert_eq!(session.released(), vec![image.id]);
}

/// The arithmetic that replaces the per-cell offset the old engine stored.
#[test]
fn painter_offset_is_derived_from_the_placement() {
    let mut session = Session::new(10, 6);
    session.feed(b"\x1b[3;4H");
    session.feed(&sixel("\"1;1;25;45#0~-~-~-~-~"));
    let image = session.one_image();
    let (state, _) = session.render();

    let placement = *state.placement(image.id).expect("copied into the state");
    assert_eq!((placement.cols, placement.rows), (3, 3));
    assert_eq!(placement.pixel_size, (25, 45));
    assert_eq!(placement.col, 3);

    for row in 0..3u16 {
        for col in 0..3u16 {
            let id = session.row_id(2 + row);
            assert_eq!(
                state.graphic_offset(image.id, id, 3 + col),
                Some((col, row)),
                "row {row} column {col}"
            );
            let cell = &state.rows()[(2 + row) as usize].cells[(3 + col) as usize];
            assert_eq!(cell.graphic, Some(image.id), "the id reaches the painter");
        }
    }
    assert_eq!(
        state.graphic_offset(image.id, session.row_id(2), 6),
        None,
        "a cell outside the placement has no offset"
    );
    assert_eq!(
        state.graphic_offset(GraphicId(9_999), session.row_id(2), 3),
        None
    );
}

#[test]
fn payload_byte_cap_aborts_an_endless_sixel() {
    let mut session = Session::new(10, 5);
    let mut stream = b"\x1bPq#0;2;100;0;0".to_vec();
    // Thirty-two mebibytes with no `ST` in sight.
    stream.resize(32 * 1024 * 1024, b'?');
    let stats = session.term.feed(&stream, &mut session.batch, session.now);

    assert_eq!(stats.aborted_dcs, 1, "aborted once, not once per byte");
    assert!(session.term.take_graphics().is_empty());
    assert!(session.term.placements().is_empty());
    assert_eq!(session.graphic(0, 0), None);
}

#[test]
fn aborted_dcs_stamps_no_cells() {
    let mut session = Session::new(10, 5);
    session.feed(b"\x1bPq\"1;1;16;16#0~\x18"); // CAN inside the payload
    assert!(session.term.take_graphics().is_empty());
    assert!(session.term.placements().is_empty());
    assert_eq!(session.graphic(0, 0), None);
    assert_eq!(session.cursor(), (0, 0));
}

/// R-16: the pixels wait in the engine until `take_graphics`, which is the only
/// drain — so a frame the renderer skips cannot lose an image.
#[test]
fn images_survive_a_frame_skipped_by_mode_2026() {
    let mut session = Session::new(10, 5);
    session.feed(b"hello");
    let mut state = RenderState::new();
    session.term.render_update(&mut state, session.now);

    session.feed(b"\x1b[?2026h");
    session.feed(&sixel("\"1;1;16;16#0~"));
    assert_eq!(
        session.term.render_update(&mut state, session.now),
        RenderUpdate::Unchanged,
        "the frame is suppressed"
    );
    // And no number of render states drains it.
    let mut second = RenderState::new();
    session.term.render_update(&mut second, session.now);
    assert_eq!(session.term.take_graphics().len(), 1);
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "no live placement")]
fn integrity_rejects_a_dangling_graphic_ref() {
    let mut session = Session::new(10, 5);
    session.feed(&sixel("\"1;1;16;16#0~"));
    let row = session.row_id(0);

    let state = session.term.state_for_tests();
    let bogus = state.interner.extras(&Extras {
        hyperlink: None,
        graphic: Some(GraphicId(9_999)),
    });
    let cell = state.grid.screen().row(row).cell(0).with_extras(bogus);
    state.grid.screen_mut().row_mut(row).repair(0, cell);
    super::assert_integrity(state);
}

/// The ConPTY byte-loss shape: a corrupt band must produce wrong pixels, never
/// a panic and never an unbounded allocation.
#[test]
fn corrupt_band_does_not_panic() {
    let mut session = Session::new(20, 6);
    let mut stream = b"\x1bPq".to_vec();
    // A deterministic pseudo-random payload over the printable range, which is
    // what a dropped byte inside a real DCS leaves behind.
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    for _ in 0..20_000 {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        stream.push(0x20 + (seed >> 33) as u8 % 0x5F);
    }
    stream.extend_from_slice(b"\x1b\\");
    session.feed(&stream);

    // Whatever came out, the engine is still consistent and still usable.
    for image in session.term.take_graphics() {
        assert!(image.width <= MAX_DIMENSION && image.height <= MAX_DIMENSION);
        assert_eq!(
            image.rgba.len(),
            (image.width as usize) * (image.height as usize) * 4
        );
    }
    session.feed(b"\x1bcok");
    assert_eq!(session.cursor(), (0, 2));
}
