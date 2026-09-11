//! Sixel through the vendored `Term`: decoder, cell placement, cursor, DA1
//! (IN-0028 / US-0066). The bytes go through `Processor::advance` exactly as the
//! pump feeds them.

use std::sync::{Arc, Mutex};

use alacritty_terminal::event::{Event, EventListener, VoidListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::graphics::{GraphicCell, GraphicData, MAX_DIMENSION};
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};

fn term(cols: usize, lines: usize) -> Term<VoidListener> {
    Term::new(Config::default(), &TermSize::new(cols, lines), VoidListener)
}

fn feed<L: EventListener>(term: &mut Term<L>, bytes: &[u8]) {
    Processor::<StdSyncHandler>::new().advance(term, bytes);
}

fn sixel(body: &str) -> Vec<u8> {
    format!("\x1bPq{body}\x1b\\").into_bytes()
}

fn one_image<L: EventListener>(term: &mut Term<L>) -> Arc<GraphicData> {
    let mut images = term.take_graphics();
    assert_eq!(images.len(), 1, "exactly one image expected");
    images.pop().unwrap()
}

fn pixel(image: &GraphicData, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * image.width + x) * 4) as usize;
    image.rgba[i..i + 4].try_into().unwrap()
}

fn graphic_at<L: EventListener>(term: &Term<L>, line: i32, col: usize) -> Option<GraphicCell> {
    term.grid()[Line(line)][Column(col)].graphic()
}

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const CLEAR: [u8; 4] = [0, 0, 0, 0];

#[test]
fn decodes_colors_columns_and_carriage_return() {
    let mut t = term(20, 5);
    // Register 0 red, register 1 blue; column 0 red, `$` back to x = 0, skip one
    // column with `?`, column 1 blue.
    feed(&mut t, &sixel("#0;2;100;0;0#1;2;0;0;100#0~$#1?~"));
    let image = one_image(&mut t);
    assert_eq!((image.width, image.height), (2, 6));
    assert_eq!(pixel(&image, 0, 0), RED);
    assert_eq!(pixel(&image, 0, 5), RED);
    assert_eq!(pixel(&image, 1, 0), BLUE);
    assert!(t.take_graphics().is_empty(), "an image is handed out once");
}

#[test]
fn repeat_hls_and_partial_columns() {
    let mut t = term(20, 5);
    // `!3` repeats the column; DEC HLS hue 120 is red; `A` (0x41 - 0x3F = 2) sets only
    // the second pixel of the column.
    feed(&mut t, &sixel("#2;1;120;50;100#2!3A"));
    let image = one_image(&mut t);
    assert_eq!((image.width, image.height), (3, 6));
    assert_eq!(pixel(&image, 2, 1), RED);
    assert_eq!(pixel(&image, 2, 0), CLEAR, "unset rows stay transparent");
}

#[test]
fn raster_attributes_fix_the_size_and_a_new_band_moves_down() {
    let mut t = term(20, 5);
    feed(&mut t, &sixel("\"1;1;1;3#0;2;100;0;0#0~-~"));
    let image = one_image(&mut t);
    assert_eq!(
        (image.width, image.height),
        (1, 3),
        "raster size wins over the data"
    );
    assert_eq!(pixel(&image, 0, 2), RED);
    let mut t = term(20, 5);
    feed(&mut t, &sixel("#0;2;100;0;0#0~-~"));
    let image = one_image(&mut t);
    assert_eq!((image.width, image.height), (1, 12));
    assert_eq!(pixel(&image, 0, 11), RED);
}

#[test]
fn width_is_clamped() {
    let mut t = term(20, 5);
    feed(&mut t, &sixel("#0!99999~"));
    let image = one_image(&mut t);
    assert_eq!((image.width, image.height), (MAX_DIMENSION, 6));
    assert_eq!(image.rgba.len(), (MAX_DIMENSION * 6 * 4) as usize);
}

#[test]
fn empty_and_non_sixel_dcs_place_nothing() {
    let mut t = term(20, 5);
    feed(&mut t, b"\x1b[2;2H");
    feed(&mut t, &sixel(""));
    feed(&mut t, b"\x1bP1$r0m\x1b\\"); // DECRSPS, not Sixel
    assert!(t.take_graphics().is_empty());
    assert_eq!(
        t.grid().cursor.point,
        alacritty_terminal::index::Point::new(Line(1), Column(1))
    );
    assert_eq!(graphic_at(&t, 1, 1), None);
}

/// Pixels are virtual 10 x 20 cells (VT340 / conhost). A 25 x 45 image with five
/// bands covers 3 x 3 cells; the cursor ends on the row holding the top of the
/// last band (`4 * 6 / 20 = 1` rows down) in its original column, and the image
/// row below the cursor is still placed.
#[test]
fn places_cells_and_leaves_the_cursor_on_the_last_band_row() {
    let mut t = term(10, 6);
    feed(&mut t, b"\x1b[3;4H"); // line 2, column 3
    feed(&mut t, &sixel("\"1;1;25;45#0~-~-~-~-~"));
    let image = one_image(&mut t);
    for (row, line) in [(0u16, 2i32), (1, 3), (2, 4)] {
        for (col, column) in [(0u16, 3usize), (1, 4), (2, 5)] {
            assert_eq!(
                graphic_at(&t, line, column),
                Some(GraphicCell {
                    id: image.id,
                    col,
                    row
                }),
                "line {line} column {column}"
            );
        }
        assert_eq!(
            graphic_at(&t, line, 6),
            None,
            "line {line}: nothing past the image"
        );
        assert_eq!(
            graphic_at(&t, line, 2),
            None,
            "line {line}: nothing before the image"
        );
    }
    assert_eq!(
        t.grid().cursor.point.line,
        Line(3),
        "row of the last band's top"
    );
    assert_eq!(t.grid().cursor.point.column, Column(3), "column is kept");
    assert_eq!(
        t.grid()[Line(2)][Column(3)].c,
        ' ',
        "cell text is untouched"
    );
}

#[test]
fn image_wider_than_the_grid_is_clipped_on_the_right() {
    let mut t = term(4, 5);
    feed(&mut t, b"\x1b[1;3H"); // column 2 of 4
    feed(&mut t, &sixel("\"1;1;80;8#0~"));
    let image = one_image(&mut t);
    assert_eq!(
        graphic_at(&t, 0, 3),
        Some(GraphicCell {
            id: image.id,
            col: 1,
            row: 0
        })
    );
    assert_eq!(
        t.grid().cursor.point.line,
        Line(0),
        "one band: cursor row unchanged"
    );
}

#[test]
fn image_at_the_bottom_scrolls_into_history() {
    let mut t = term(10, 3);
    feed(&mut t, b"\x1b[3;1H"); // last line
    feed(&mut t, &sixel("\"1;1;8;32#0~-~-~-~-~")); // 1 col x 2 rows, cursor 1 row down
    let image = one_image(&mut t);
    // The linefeed at the bottom pushes the top line into history: the image
    // rows end at lines 1-2 and the cursor stays on the last line.
    assert_eq!(t.total_lines(), 4, "one line scrolled into history");
    assert_eq!(
        graphic_at(&t, 1, 0),
        Some(GraphicCell {
            id: image.id,
            col: 0,
            row: 0
        })
    );
    assert_eq!(
        graphic_at(&t, 2, 0),
        Some(GraphicCell {
            id: image.id,
            col: 0,
            row: 1
        })
    );
    assert_eq!(graphic_at(&t, -1, 0), None);
    assert_eq!(t.grid().cursor.point.line, Line(2));
}

#[test]
fn erase_and_overwrite_drop_the_reference() {
    let mut t = term(10, 5);
    feed(&mut t, &sixel("\"1;1;16;16#0~"));
    assert!(graphic_at(&t, 0, 0).is_some());
    feed(&mut t, b"\x1b[1;2Hx"); // overwrite column 1
    assert_eq!(graphic_at(&t, 0, 1), None);
    assert!(graphic_at(&t, 0, 0).is_some());
    feed(&mut t, b"\x1b[2J");
    assert_eq!(graphic_at(&t, 0, 0), None);
    let _ = t.take_graphics();
    feed(&mut t, &sixel("#0~"));
    feed(&mut t, b"\x1bc"); // RIS
    assert!(t.take_graphics().is_empty(), "RIS drops pending images");
}

/// The band count, not the raster height, decides how far the cursor moves:
/// two bands (top of the last at 6 px) stay on the row, four bands (18 px) too,
/// five bands (24 px) move one row, as `bands * 6 / 20` on a VT340.
#[test]
fn band_count_decides_the_cursor_row() {
    for (body, rows_down) in [
        ("#0~-~", 0),
        ("#0~-~-~-~", 0),
        ("#0~-~-~-~-~", 1),
        ("#0~-~-~-~-~-~-~-~", 2),
    ] {
        let mut t = term(10, 8);
        feed(&mut t, &sixel(&format!("\"1;1;8;60{body}")));
        let _ = one_image(&mut t);
        assert_eq!(t.grid().cursor.point.line, Line(rows_down), "{body}");
        // Every one of the three cell rows the 60 px image covers is referenced.
        for row in 0..3 {
            assert_eq!(
                graphic_at(&t, row, 0).map(|g| g.row),
                Some(row as u16),
                "{body} row {row}"
            );
        }
    }
}

#[derive(Clone, Default)]
struct Recorder(Arc<Mutex<Vec<String>>>);

impl EventListener for Recorder {
    fn send_event(&self, event: Event) {
        if let Event::PtyWrite(text) = event {
            self.0.lock().unwrap().push(text);
        }
    }
}

#[test]
fn primary_device_attributes_advertise_sixel() {
    let recorder = Recorder::default();
    let mut t = Term::new(Config::default(), &TermSize::new(10, 3), recorder.clone());
    feed(&mut t, b"\x1b[c");
    assert_eq!(
        recorder.0.lock().unwrap().as_slice(),
        ["\x1b[?62;4c".to_string()]
    );
}
