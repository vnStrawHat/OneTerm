//! Replay a corpus recording through the engine being replaced.
//!
//! This is the only place that blesses: `US-0072` freezes what the vendored
//! `alacritty_terminal` fork produces, and the new engine is later measured
//! against it. The parse path is byte-for-byte the one `crates/terminal` runs
//! (`crates/terminal/src/backend/pump.rs`): a
//! `vte::ansi::Processor<StdSyncHandler>` advanced over a `Term`.
//!
//! Grid extraction reproduces upstream's own ref harness
//! (`alacritty_terminal/tests/ref.rs`): clone the grid, `initialize_all()` to
//! fill history to `max_scroll_limit + screen_lines`, then `truncate()` to
//! rezero the ring — without which `Storage::PartialEq` panics (trap 45).
//!
//! Tab stops and the scroll region have no accessor on `Term`, so they are
//! recovered observationally on a second replay: `CR` plus repeated `HT` reads
//! the stops out of the cursor column, and origin mode plus `CUP` to row 1 and
//! to row 999 reads the region bounds out of the cursor row.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::rc::Rc;

use alacritty_terminal::event::{Event, EventListener, VoidListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::color::COUNT as COLOR_COUNT;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::{Color, Processor, StdSyncHandler};

use crate::corpus::{GridExpect, Recording, RowExpect, StateExpect};

/// Primitive cell flags in bit order.
///
/// Spelled out rather than taken from `Flags::iter_names()` because that also
/// yields the composite aliases (`BOLD_ITALIC`, `DIM_BOLD`, `ALL_UNDERLINES`),
/// which would make the encoding depend on which alias bitflags happens to
/// match first.
const FLAG_NAMES: [(u16, &str); 15] = [
    (0x0001, "INVERSE"),
    (0x0002, "BOLD"),
    (0x0004, "ITALIC"),
    (0x0008, "UNDERLINE"),
    (0x0010, "WRAPLINE"),
    (0x0020, "WIDE_CHAR"),
    (0x0040, "WIDE_CHAR_SPACER"),
    (0x0080, "DIM"),
    (0x0100, "HIDDEN"),
    (0x0200, "STRIKEOUT"),
    (0x0400, "LEADING_WIDE_CHAR_SPACER"),
    (0x0800, "DOUBLE_UNDERLINE"),
    (0x1000, "UNDERCURL"),
    (0x2000, "DOTTED_UNDERLINE"),
    (0x4000, "DASHED_UNDERLINE"),
];

/// Grid geometry for `Term::new`. `term::test::TermSize` lives behind the
/// crate's test module, so the corpus carries its own two-line equivalent, the
/// same way `crates/terminal` does with `GridSize`.
#[derive(Debug, Clone, Copy)]
struct CaptureSize {
    columns: usize,
    screen_lines: usize,
}

impl Dimensions for CaptureSize {
    fn total_lines(&self) -> usize {
        self.screen_lines
    }

    fn screen_lines(&self) -> usize {
        self.screen_lines
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

/// Records the title events the grid comparison cannot see. `Term` keeps no
/// public title, so the event stream is the only observation point.
#[derive(Clone, Default)]
struct TitleRecorder {
    events: Rc<RefCell<Vec<String>>>,
}

impl EventListener for TitleRecorder {
    fn send_event(&self, event: Event) {
        match event {
            Event::Title(title) => self.events.borrow_mut().push(format!("set:{title}")),
            Event::ResetTitle => self.events.borrow_mut().push("reset".to_owned()),
            _ => {}
        }
    }
}

/// Replay `recording` through the vendored engine and produce both frozen
/// expectation files.
pub fn replay_old(recording: &Recording) -> (GridExpect, StateExpect) {
    let recorder = TitleRecorder::default();
    let mut term = new_term(recording, recorder.clone());
    let mut processor = Processor::<StdSyncHandler>::new();
    processor.advance(&mut term, &recording.bytes);

    let grid = encode_grid(&term);
    let state = encode_state(&term, &recorder, recording);
    (grid, state)
}

fn new_term<T: EventListener>(recording: &Recording, listener: T) -> Term<T> {
    let config = Config {
        scrolling_history: recording.history_size,
        ..Default::default()
    };
    let size = CaptureSize {
        columns: recording.columns,
        screen_lines: recording.screen_lines,
    };
    Term::new(config, &size, listener)
}

fn encode_grid<T>(term: &Term<T>) -> GridExpect {
    // Exactly upstream's ref harness: fill history to its configured maximum,
    // then rezero the ring so the row order is the serialized one.
    let mut grid = term.grid().clone();
    grid.initialize_all();
    grid.truncate();

    let columns = grid.columns();
    let screen_lines = grid.screen_lines();
    let total = grid.total_lines();

    // `Storage` maps `inner[i]` to `Line(screen_lines - i - 1)` once `zero` is
    // 0, so walking i upwards walks the grid newest row first.
    let mut links = LinkIds::default();
    let mut rows = Vec::with_capacity(total);
    for index in 0..total {
        let line = Line(screen_lines as i32 - 1 - index as i32);
        let row = &grid[line];
        let cells: Vec<String> = (0..columns)
            .map(|column| encode_cell(&row[Column(column)], &mut links))
            .collect();
        let wrap = row
            .last()
            .is_some_and(|cell| cell.flags.contains(Flags::WRAPLINE));
        rows.push(RowExpect { wrap, cells });
    }

    GridExpect {
        columns,
        lines: screen_lines,
        display_offset: grid.display_offset(),
        rows,
    }
}

/// Auto-generated hyperlink ids are a process-global counter
/// (`Hyperlink::new(None, ..)` appends `_alacritty` to an `AtomicU32`), so they
/// depend on how many recordings replayed earlier in the process. Renumbering
/// them per grid, in first-appearance order, keeps link *identity* — which
/// cells share a link — while making the expectation reproducible.
#[derive(Default)]
pub(crate) struct LinkIds {
    seen: HashMap<String, usize>,
}

impl LinkIds {
    pub(crate) fn normalize(&mut self, id: &str) -> String {
        let generated = id
            .strip_suffix("_alacritty")
            .is_some_and(|prefix| !prefix.is_empty() && prefix.bytes().all(|b| b.is_ascii_digit()));
        if !generated {
            return percent_encode(id);
        }
        let next = self.seen.len();
        let index = *self.seen.entry(id.to_owned()).or_insert(next);
        format!("#{index}")
    }
}

fn encode_cell(cell: &Cell, links: &mut LinkIds) -> String {
    let mut content = format!("{:04x}", cell.c as u32);
    for zero_width in cell.zerowidth().unwrap_or(&[]) {
        let _ = write!(content, "+{:04x}", *zero_width as u32);
    }

    let attrs = encode_flags(cell.flags.bits());
    let fg = encode_color(cell.fg);
    let bg = encode_color(cell.bg);
    let underline = cell
        .underline_color()
        .map_or_else(|| "-".to_owned(), encode_color);
    let hyperlink = cell.hyperlink().map_or_else(
        || "-".to_owned(),
        |link| {
            format!(
                "{}~{}",
                links.normalize(link.id()),
                percent_encode(link.uri())
            )
        },
    );

    format!("{content};{attrs};{fg};{bg};{underline};{hyperlink}")
}

pub(crate) fn encode_flags(bits: u16) -> String {
    if bits == 0 {
        return "-".to_owned();
    }
    let names: Vec<&str> = FLAG_NAMES
        .iter()
        .filter(|(bit, _)| bits & bit != 0)
        .map(|(_, name)| *name)
        .collect();
    names.join(".")
}

fn encode_color(color: Color) -> String {
    match color {
        Color::Named(named) => format!("n{named:?}"),
        Color::Indexed(index) => format!("i{index}"),
        Color::Spec(rgb) => format!("#{:02x}{:02x}{:02x}", rgb.r, rgb.g, rgb.b),
    }
}

/// Percent-encode everything that is not an unreserved URI character, so no
/// encoded value can contain a field (`;`), run (`|`, `*`) or link (`~`)
/// separator.
pub fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'/' | b':') {
            out.push(byte as char);
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

fn encode_state<T>(term: &Term<T>, recorder: &TitleRecorder, recording: &Recording) -> StateExpect
where
    T: EventListener,
{
    let mut state = StateExpect::default();

    let cursor = &term.grid().cursor;
    let shape = term.renderable_content().cursor.shape;
    state.push(
        "cursor",
        format!(
            "row={} col={} shape={shape:?}",
            cursor.point.line.0, cursor.point.column.0
        ),
    );
    state.push(
        "pending_wrap",
        u8::from(cursor.input_needs_wrap).to_string(),
    );

    let mut modes: Vec<&str> = term.mode().iter_names().map(|(name, _)| name).collect();
    modes.sort_unstable();
    state.push(
        "modes",
        if modes.is_empty() {
            "-".to_owned()
        } else {
            modes.join(" ")
        },
    );

    let colors = term.colors();
    for index in 0..COLOR_COUNT {
        if let Some(rgb) = colors[index] {
            state.push(
                format!("palette.{index}"),
                format!("#{:02x}{:02x}{:02x}", rgb.r, rgb.g, rgb.b),
            );
        }
    }

    let events = recorder.events.borrow();
    state.push("title_events", events.len().to_string());
    for (index, event) in events.iter().enumerate() {
        state.push(format!("title_event.{index}"), percent_encode(event));
    }

    let (tab_stops, scroll_region) = probe(recording);
    state.push("tab_stops", tab_stops);
    state.push("scroll_region", scroll_region);

    state
}

/// Second replay, used only to read back state `Term` does not expose.
///
/// The probe mutates the terminal, which is why it runs on its own copy: the
/// grid and the state above are taken from the untouched replay.
fn probe(recording: &Recording) -> (String, String) {
    let mut term = new_term(recording, VoidListener);
    let mut processor = Processor::<StdSyncHandler>::new();
    processor.advance(&mut term, &recording.bytes);

    // Tab stops: from column 0, every column `HT` can reach. The last entry may
    // be the clamp to the final column rather than a real stop; both engines
    // produce it identically, which is what the gate compares.
    processor.advance(&mut term, b"\r");
    let mut stops = Vec::new();
    let mut previous = term.grid().cursor.point.column.0;
    for _ in 0..recording.columns {
        processor.advance(&mut term, b"\t");
        let column = term.grid().cursor.point.column.0;
        if column == previous {
            break;
        }
        stops.push(column.to_string());
        previous = column;
    }
    let tab_stops = if stops.is_empty() {
        "-".to_owned()
    } else {
        stops.join(",")
    };

    // Scroll region: under origin mode `CUP` is region-relative and clamped to
    // the region, so row 1 lands on its top and row 999 on its bottom.
    processor.advance(&mut term, b"\x1b[?6h\x1b[1;1H");
    let top = term.grid().cursor.point.line.0;
    processor.advance(&mut term, b"\x1b[999;1H");
    let bottom = term.grid().cursor.point.line.0;

    (tab_stops, format!("top={top} bottom={bottom}"))
}
