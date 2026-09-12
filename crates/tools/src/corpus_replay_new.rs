//! Replay a corpus recording through `oneterm-vt`, the new engine.
//!
//! It produces the **same two expectation forms** `corpus_replay` produces from
//! the engine being replaced, so the frozen files never move: the new engine is
//! measured against them, and it never blesses (R-58).
//!
//! The encoding therefore speaks the reference's vocabulary in three places
//! where the new engine's model is deliberately different:
//!
//! * the wrap flag is a row flag here (deviation G1) and a flag on the last
//!   cell there, so it is written back onto the last cell;
//! * `Attrs` has bits the reference does not have (`BLINK_*`, `OVERLINE`,
//!   correction C11) and no bit for the wide-cell classes, which are a `CellWidth`
//!   enum here, so the two are recombined into the reference's `Flags` layout;
//! * an implicit hyperlink id is a bare decimal counter here and `<n>_alacritty`
//!   there, so both are renumbered by first appearance — which keeps link
//!   *identity* (which cells share a link) while making the value reproducible.
//!
//! Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/testing-and-bench.md` § 2.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::time::Instant;

use oneterm_vt::cell::{Attrs, Cell, CellContent, CellWidth, Color};
use oneterm_vt::event::VtEvent;
use oneterm_vt::grid::{RowId, Screen, Size};
use oneterm_vt::intern::{HyperlinkId, Interner};
use oneterm_vt::render::MouseEncoding;
use oneterm_vt::terminal::{CursorShape, KeyboardFlags, Mode};
use oneterm_vt::{Config, EventBatch, Terminal};

use crate::corpus::{GridExpect, Recording, RowExpect, StateExpect};
use crate::corpus_replay::{encode_flags, percent_encode};

// The reference's `Flags` bits, in the order `corpus_replay::FLAG_NAMES` spells
// them. Reproduced here rather than imported so the encoding survives the fork's
// deletion at `US-0087`.
const INVERSE: u16 = 0x0001;
const BOLD: u16 = 0x0002;
const ITALIC: u16 = 0x0004;
const UNDERLINE: u16 = 0x0008;
const WRAPLINE: u16 = 0x0010;
const WIDE_CHAR: u16 = 0x0020;
const WIDE_CHAR_SPACER: u16 = 0x0040;
const DIM: u16 = 0x0080;
const HIDDEN: u16 = 0x0100;
const STRIKEOUT: u16 = 0x0200;
const LEADING_WIDE_CHAR_SPACER: u16 = 0x0400;
const DOUBLE_UNDERLINE: u16 = 0x0800;
const UNDERCURL: u16 = 0x1000;
const DOTTED_UNDERLINE: u16 = 0x2000;
const DASHED_UNDERLINE: u16 = 0x4000;

/// Build a terminal at the recording's captured geometry.
pub fn terminal_for(recording: &Recording) -> Terminal {
    let size = Size {
        rows: recording.screen_lines as u16,
        cols: recording.columns as u16,
    };
    Terminal::new(
        size,
        Config {
            scrollback_limit: recording.history_size as u32,
            ..Config::default()
        },
    )
}

/// Replay `recording` through the new engine and produce both expectation forms.
pub fn replay_new(recording: &Recording) -> (GridExpect, StateExpect) {
    let mut term = terminal_for(recording);
    let mut batch = EventBatch::new();
    let mut titles = Vec::new();
    // One `feed`, exactly as the pump would hand a whole chunk over; the title
    // events are the only thing the grid cannot show.
    let now = Instant::now();
    term.feed(&recording.bytes, &mut batch, now);
    collect_titles(&batch, &mut titles);

    let grid = encode_grid(&term, recording);
    let state = encode_state(&term, &titles, recording);
    (grid, state)
}

fn collect_titles(batch: &EventBatch, out: &mut Vec<String>) {
    for event in batch.iter() {
        match event {
            VtEvent::Title(span) => out.push(format!("set:{}", batch.str(*span))),
            VtEvent::TitleReset => out.push("reset".to_owned()),
            _ => {}
        }
    }
}

/// Rows newest first, over the same row count the reference's `initialize_all`
/// produces: the scrollback limit plus the screen, or just the screen while the
/// alternate screen is active, which has no history.
pub fn encode_grid(term: &Terminal, recording: &Recording) -> GridExpect {
    let screen = term.screen();
    let interner = term.interner();
    let columns = screen.cols() as usize;
    let lines = screen.rows() as usize;
    let total = if term.grid().alt_active() {
        lines
    } else {
        recording.history_size + lines
    };

    // Every row older than the oldest live one reads as blanks, which is what
    // `initialize_all` fills the ring with.
    let live = screen.newest().distance(screen.oldest()) + 1;
    let newest = screen.newest();

    let mut links = NewLinkIds::default();
    let mut rows = Vec::with_capacity(total);
    for index in 0..total {
        let row = if (index as u64) < live {
            Some(screen.row(newest - index as u64))
        } else {
            None
        };
        let wrap = row.as_ref().is_some_and(|row| row.wrapped());
        let blank = Cell::EMPTY;
        let cells: Vec<String> = (0..columns)
            .map(|column| {
                let cell = row.as_ref().map_or(blank, |row| row.cell(column as u16));
                let last = column + 1 == columns;
                encode_cell(cell, wrap && last, interner, &mut links)
            })
            .collect();
        rows.push(RowExpect { wrap, cells });
    }

    GridExpect {
        columns,
        lines,
        display_offset: screen.scroll_offset() as usize,
        rows,
    }
}

/// Implicit ids are a bare decimal counter here and `<n>_alacritty` there, so
/// both sides renumber them by first appearance. An explicit id is kept
/// verbatim in both, and the two id spaces overlap — `id=42` and the
/// forty-second implicit link both spell their id `42` — which is why
/// `Hyperlink::implicit` exists.
#[derive(Default)]
struct NewLinkIds {
    seen: HashMap<u32, usize>,
}

impl NewLinkIds {
    fn normalize(&mut self, id: HyperlinkId, interner: &Interner) -> String {
        let Some(link) = interner.hyperlinks.resolve(id) else {
            return "-".to_owned();
        };
        if !link.implicit {
            return percent_encode(&link.id);
        }
        let next = self.seen.len();
        let index = *self.seen.entry(id.0).or_insert(next);
        format!("#{index}")
    }
}

fn encode_cell(cell: Cell, wrapline: bool, interner: &Interner, links: &mut NewLinkIds) -> String {
    let mut content = String::new();
    match cell.content() {
        CellContent::Scalar(c) => {
            let _ = write!(content, "{:04x}", c as u32);
        }
        CellContent::Grapheme(id) => {
            let cluster = interner.resolve_grapheme(id);
            let _ = write!(
                content,
                "{:04x}",
                cluster.first().copied().unwrap_or(' ') as u32
            );
            for extra in &cluster[1.min(cluster.len())..] {
                let _ = write!(content, "+{:04x}", *extra as u32);
            }
        }
    }

    let style = interner.resolve_style(cell.style_id());
    let mut bits = 0u16;
    let set = |bits: &mut u16, condition: bool, bit: u16| {
        if condition {
            *bits |= bit;
        }
    };
    set(&mut bits, style.attrs.contains(Attrs::INVERSE), INVERSE);
    set(&mut bits, style.attrs.contains(Attrs::BOLD), BOLD);
    set(&mut bits, style.attrs.contains(Attrs::ITALIC), ITALIC);
    set(&mut bits, style.attrs.contains(Attrs::UNDERLINE), UNDERLINE);
    set(&mut bits, wrapline, WRAPLINE);
    set(&mut bits, cell.width() == CellWidth::Wide, WIDE_CHAR);
    set(
        &mut bits,
        cell.width() == CellWidth::WideSpacer,
        WIDE_CHAR_SPACER,
    );
    set(&mut bits, style.attrs.contains(Attrs::DIM), DIM);
    set(&mut bits, style.attrs.contains(Attrs::HIDDEN), HIDDEN);
    set(&mut bits, style.attrs.contains(Attrs::STRIKEOUT), STRIKEOUT);
    set(
        &mut bits,
        cell.width() == CellWidth::LeadingWideSpacer,
        LEADING_WIDE_CHAR_SPACER,
    );
    set(
        &mut bits,
        style.attrs.contains(Attrs::DOUBLE_UNDERLINE),
        DOUBLE_UNDERLINE,
    );
    set(&mut bits, style.attrs.contains(Attrs::UNDERCURL), UNDERCURL);
    set(
        &mut bits,
        style.attrs.contains(Attrs::DOTTED_UNDERLINE),
        DOTTED_UNDERLINE,
    );
    set(
        &mut bits,
        style.attrs.contains(Attrs::DASHED_UNDERLINE),
        DASHED_UNDERLINE,
    );

    let attrs = encode_flags(bits);
    let fg = encode_color(style.fg);
    let bg = encode_color(style.bg);
    let underline = style
        .underline_color
        .map_or_else(|| "-".to_owned(), encode_color);
    let hyperlink = match interner.resolve_extras(cell.extras_id()).hyperlink {
        None => "-".to_owned(),
        Some(id) => {
            let uri = interner
                .hyperlinks
                .resolve(id)
                .map(|link| percent_encode(&link.uri))
                .unwrap_or_default();
            format!("{}~{uri}", links.normalize(id, interner))
        }
    };

    format!("{content};{attrs};{fg};{bg};{underline};{hyperlink}")
}

fn encode_color(color: Color) -> String {
    match color {
        Color::Named(named) => format!("n{named:?}"),
        Color::Palette(index) => format!("i{index}"),
        Color::Rgb(rgb) => format!("#{:02x}{:02x}{:02x}", rgb.r, rgb.g, rgb.b),
    }
}

pub fn encode_state(term: &Terminal, titles: &[String], recording: &Recording) -> StateExpect {
    let mut state = StateExpect::default();
    let screen = term.screen();
    let cursor = screen.cursor();

    state.push(
        "cursor",
        format!(
            "row={} col={} shape={:?}",
            screen.cursor_row_index(),
            cursor.pos.col,
            cursor_shape(term)
        ),
    );
    state.push("pending_wrap", u8::from(cursor.pending_wrap).to_string());
    state.push("modes", mode_names(term));

    for (index, rgb) in term.colors().iter() {
        state.push(
            format!("palette.{index}"),
            format!("#{:02x}{:02x}{:02x}", rgb.r, rgb.g, rgb.b),
        );
    }

    state.push("title_events", titles.len().to_string());
    for (index, event) in titles.iter().enumerate() {
        state.push(format!("title_event.{index}"), percent_encode(event));
    }

    let (tab_stops, scroll_region) = probe(recording);
    state.push("tab_stops", tab_stops);
    state.push("scroll_region", scroll_region);

    state
}

fn cursor_shape(term: &Terminal) -> CursorShape {
    term.cursor_style().shape
}

/// The reference's `TermMode::iter_names()` output, sorted. Spelled out because
/// the new engine's mode table is a typed enum and the frozen files carry the
/// old names.
fn mode_names(term: &Terminal) -> String {
    let mouse = term.mouse_reporting();
    let flags = term.keyboard_flags();
    let mut names: Vec<&str> = Vec::new();
    let push = |names: &mut Vec<&str>, on: bool, name: &'static str| {
        if on {
            names.push(name);
        }
    };
    push(&mut names, term.mode(Mode::ShowCursor), "SHOW_CURSOR");
    push(&mut names, term.mode(Mode::AppCursor), "APP_CURSOR");
    push(&mut names, term.mode(Mode::AppKeypad), "APP_KEYPAD");
    push(
        &mut names,
        term.mode(Mode::MouseClick),
        "MOUSE_REPORT_CLICK",
    );
    push(
        &mut names,
        term.mode(Mode::BracketedPaste),
        "BRACKETED_PASTE",
    );
    push(
        &mut names,
        mouse.is_some_and(|protocol| protocol.encoding == MouseEncoding::Sgr)
            || term.mode(Mode::SgrMouse),
        "SGR_MOUSE",
    );
    push(&mut names, term.mode(Mode::MouseMotion), "MOUSE_MOTION");
    push(&mut names, term.mode(Mode::LineWrap), "LINE_WRAP");
    push(
        &mut names,
        term.mode(Mode::LineFeedNewLine),
        "LINE_FEED_NEW_LINE",
    );
    push(&mut names, term.mode(Mode::Origin), "ORIGIN");
    push(&mut names, term.mode(Mode::Insert), "INSERT");
    push(&mut names, term.mode(Mode::FocusInOut), "FOCUS_IN_OUT");
    push(&mut names, term.mode(Mode::AltScreen), "ALT_SCREEN");
    push(&mut names, term.mode(Mode::MouseDrag), "MOUSE_DRAG");
    push(&mut names, term.mode(Mode::Utf8Mouse), "UTF8_MOUSE");
    push(
        &mut names,
        term.mode(Mode::AlternateScroll),
        "ALTERNATE_SCROLL",
    );
    push(&mut names, term.mode(Mode::UrgencyHints), "URGENCY_HINTS");
    push(
        &mut names,
        flags.contains(KeyboardFlags::DISAMBIGUATE_ESC_CODES),
        "DISAMBIGUATE_ESC_CODES",
    );
    push(
        &mut names,
        flags.contains(KeyboardFlags::REPORT_EVENT_TYPES),
        "REPORT_EVENT_TYPES",
    );
    push(
        &mut names,
        flags.contains(KeyboardFlags::REPORT_ALTERNATE_KEYS),
        "REPORT_ALTERNATE_KEYS",
    );
    push(
        &mut names,
        flags.contains(KeyboardFlags::REPORT_ALL_KEYS_AS_ESC),
        "REPORT_ALL_KEYS_AS_ESC",
    );
    push(
        &mut names,
        flags.contains(KeyboardFlags::REPORT_ASSOCIATED_TEXT),
        "REPORT_ASSOCIATED_TEXT",
    );

    names.sort_unstable();
    names.dedup();
    if names.is_empty() {
        "-".to_owned()
    } else {
        names.join(" ")
    }
}

/// Second replay, reading back state the grid cannot show. Byte-for-byte the
/// probe `corpus_replay` runs against the engine being replaced, so the two
/// answers are comparable.
fn probe(recording: &Recording) -> (String, String) {
    let mut term = terminal_for(recording);
    let mut batch = EventBatch::new();
    let now = Instant::now();
    let mut feed = |term: &mut Terminal, bytes: &[u8]| {
        term.feed(bytes, &mut batch, now);
    };
    feed(&mut term, &recording.bytes);

    feed(&mut term, b"\r");
    let mut stops = Vec::new();
    let mut previous = column(&term);
    for _ in 0..recording.columns {
        feed(&mut term, b"\t");
        let column = column(&term);
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

    feed(&mut term, b"\x1b[?6h\x1b[1;1H");
    let top = row(&term);
    feed(&mut term, b"\x1b[999;1H");
    let bottom = row(&term);

    (tab_stops, format!("top={top} bottom={bottom}"))
}

fn column(term: &Terminal) -> u16 {
    term.screen().cursor().pos.col
}

fn row(term: &Terminal) -> u16 {
    term.screen().cursor_row_index()
}

/// The text of one row, for the differential's context lines.
pub fn row_text(screen: &Screen, interner: &Interner, id: RowId) -> String {
    let mut out = String::new();
    screen.row_text(id, &interner.graphemes, &mut out);
    out
}
