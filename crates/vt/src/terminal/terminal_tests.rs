//! Byte-feed tests for the dispatch layer.
//!
//! Every test drives real bytes through a real [`Terminal`] and asserts on
//! cells, the cursor, modes or the event batch — the shape
//! `testing-and-bench.md` § 1 names. The 45-recording parity gate is this
//! file's real exit criterion; these exist so a failure names the sequence
//! rather than the recording.

use std::time::Instant;

use super::*;
use crate::cell::{Attrs, Color, NamedColor, Rgb, Semantic};
use crate::event::{Progress, ShellMark, VtEvent};
use crate::grid::{RowFlags, Size};
use crate::input::NamedKey;
use crate::intern::HYPERLINK_TABLE_LIMIT;
use crate::render::{MouseEncoding, MouseReporting};
use crate::terminal::mode::{KEYBOARD_STACK_MAX, ModeState, TITLE_STACK_MAX};

// ── Harness ─────────────────────────────────────────────────────────────────

fn terminal(cols: u16, rows: u16) -> Terminal {
    Terminal::new(Size { rows, cols }, Config::default())
}

fn routing(cols: u16, rows: u16, codes: &[u32], route: OscRoute) -> Terminal {
    let mut routes = OscRoutes::new();
    routes.route_all(codes, route);
    with_routes(cols, rows, routes)
}

fn with_routes(cols: u16, rows: u16, routes: OscRoutes) -> Terminal {
    Terminal::new(
        Size { rows, cols },
        Config {
            osc_routes: routes,
            ..Config::default()
        },
    )
}

/// Feed and keep the batch, so a test can read the events back.
struct Session {
    term: Terminal,
    batch: EventBatch,
    now: Instant,
}

impl Session {
    fn new(cols: u16, rows: u16) -> Session {
        Session::with(terminal(cols, rows))
    }

    fn with(term: Terminal) -> Session {
        Session {
            term,
            batch: EventBatch::new(),
            now: Instant::now(),
        }
    }

    fn feed(&mut self, bytes: &[u8]) -> FeedStats {
        self.term.feed(bytes, &mut self.batch, self.now)
    }

    /// Every `Reply` payload in the last batch, concatenated.
    fn replies(&self) -> String {
        let mut out = String::new();
        for event in self.batch.iter() {
            if let VtEvent::Reply(span) = event {
                out.push_str(&String::from_utf8_lossy(self.batch.bytes(*span)));
            }
        }
        out
    }

    fn row(&self, index: u16) -> String {
        let id = self.term.screen().row_of_index(index);
        self.term.row_text(id)
    }

    fn cursor(&self) -> (u16, u16) {
        (
            self.term.screen().cursor_row_index(),
            self.term.screen().cursor().pos.col,
        )
    }

    fn cell(&self, row: u16, col: u16) -> crate::cell::Cell {
        let id = self.term.screen().row_of_index(row);
        self.term.screen().row(id).cell(col)
    }

    fn style_at(&self, row: u16, col: u16) -> crate::cell::Style {
        *self
            .term
            .interner()
            .resolve_style(self.cell(row, col).style_id())
    }
}

fn feed(term: &mut Terminal, bytes: &[u8]) -> EventBatch {
    let mut batch = EventBatch::new();
    term.feed(bytes, &mut batch, Instant::now());
    batch
}

// ── Printing and the basics ─────────────────────────────────────────────────

#[test]
fn a_terminal_prints_and_wraps() {
    let mut term = terminal(4, 2);
    feed(&mut term, b"abcdef");

    let top = term.viewport().top;
    assert_eq!(term.row_text(top), "abcd");
    assert_eq!(term.row_text(top + 1), "ef  ");
}

#[test]
fn feed_with_an_empty_slice_is_a_noop() {
    let mut session = Session::new(10, 3);
    let stats = session.feed(b"");

    assert_eq!(stats, FeedStats::default());
    assert!(session.batch.is_empty());
}

#[test]
fn chunking_is_invariant() {
    let stream = b"\x1b[3;5Hhello\x1b[1;1H\x1b[32mworld\x1b[0m\r\n\x1b]0;title\x07tail";
    let mut whole = terminal(20, 6);
    feed(&mut whole, stream);

    for chunk in [1usize, 3, 7] {
        let mut split = terminal(20, 6);
        let mut batch = EventBatch::new();
        let now = Instant::now();
        for part in stream.chunks(chunk) {
            split.feed(part, &mut batch, now);
        }
        for index in 0..6u16 {
            let id = whole.screen().row_of_index(index);
            let other = split.screen().row_of_index(index);
            assert_eq!(
                whole.row_text(id),
                split.row_text(other),
                "row {index} differs at chunk size {chunk}"
            );
        }
        assert_eq!(whole.title(), split.title());
    }
}

// ── Parameter defaults and the CSI table ────────────────────────────────────

#[test]
fn param_zero_means_default() {
    // Trap 22: `CSI 0 A` is `CSI A` is "up one".
    let mut session = Session::new(20, 6);
    session.feed(b"\x1b[6;10H\x1b[0A");
    assert_eq!(session.cursor(), (4, 9));

    session.feed(b"\x1b[6;10H\x1b[A");
    assert_eq!(session.cursor(), (4, 9));

    // And `CSI 0 SP q` is `CSI SP q` is "reset to the configured default".
    session.feed(b"\x1b[5 q");
    assert_eq!(session.term.cursor_style().shape, CursorShape::Beam);
    session.feed(b"\x1b[0 q");
    assert_eq!(session.term.cursor_style().shape, CursorShape::Block);
}

#[test]
fn csi_table_is_exhaustive() {
    // Each row: the sequence with an explicit parameter, the same sequence with
    // none, and the cursor or text both must produce. The default column of the
    // design's CSI table, asserted.
    let cases: &[(&[u8], (u16, u16))] = &[
        (b"\x1b[5;5H\x1b[2A", (2, 4)),
        (b"\x1b[5;5H\x1b[A", (3, 4)),
        (b"\x1b[5;5H\x1b[2B", (6, 4)),
        (b"\x1b[5;5H\x1b[B", (5, 4)),
        (b"\x1b[5;5H\x1b[2e", (6, 4)),
        (b"\x1b[5;5H\x1b[2C", (4, 6)),
        (b"\x1b[5;5H\x1b[C", (4, 5)),
        (b"\x1b[5;5H\x1b[2a", (4, 6)),
        (b"\x1b[5;5H\x1b[2D", (4, 2)),
        (b"\x1b[5;5H\x1b[D", (4, 3)),
        (b"\x1b[5;5H\x1b[3d", (2, 4)),
        (b"\x1b[5;5H\x1b[d", (0, 4)),
        (b"\x1b[5;5H\x1b[2E", (6, 0)),
        (b"\x1b[5;5H\x1b[E", (5, 0)),
        (b"\x1b[5;5H\x1b[2F", (2, 0)),
        (b"\x1b[5;5H\x1b[F", (3, 0)),
        (b"\x1b[5;5H\x1b[3G", (4, 2)),
        (b"\x1b[5;5H\x1b[G", (4, 0)),
        (b"\x1b[5;5H\x1b[3`", (4, 2)),
        (b"\x1b[3;7H", (2, 6)),
        (b"\x1b[H", (0, 0)),
        (b"\x1b[3;7f", (2, 6)),
        (b"\x1b[f", (0, 0)),
        // Tabs are at every eighth column, so `CHT` with no parameter is one.
        (b"\x1b[1;1H\x1b[I", (0, 8)),
        (b"\x1b[1;1H\x1b[2I", (0, 16)),
        (b"\x1b[1;20H\x1b[Z", (0, 16)),
        (b"\x1b[1;20H\x1b[2Z", (0, 8)),
    ];
    for (bytes, expected) in cases {
        let mut session = Session::new(40, 10);
        session.feed(bytes);
        assert_eq!(
            session.cursor(),
            *expected,
            "{}",
            String::from_utf8_lossy(bytes)
        );
    }

    // The editing sequences, whose default is also 1.
    let edits: &[(&[u8], &str)] = &[
        (b"abcdef\x1b[1;3H\x1b[@", "ab cdef   "),
        (b"abcdef\x1b[1;3H\x1b[2@", "ab  cdef  "),
        (b"abcdef\x1b[1;3H\x1b[P", "abdef     "),
        (b"abcdef\x1b[1;3H\x1b[2P", "abef      "),
        (b"abcdef\x1b[1;3H\x1b[X", "ab def    "),
        (b"abcdef\x1b[1;3H\x1b[2X", "ab  ef    "),
        (b"abcdef\x1b[1;3H\x1b[K", "ab        "),
        (b"abcdef\x1b[1;3H\x1b[1K", "   def    "),
        (b"abcdef\x1b[1;3H\x1b[2K", "          "),
    ];
    for (bytes, expected) in edits {
        let mut session = Session::new(10, 3);
        session.feed(bytes);
        assert_eq!(
            session.row(0),
            *expected,
            "{}",
            String::from_utf8_lossy(bytes)
        );
    }
}

#[test]
fn an_overflowed_or_over_long_sequence_is_dropped_whole() {
    let mut session = Session::new(10, 3);
    // Three intermediates: the sequence still dispatches, with `ignore` set, so
    // the handler can drop it deliberately — which is a different path from
    // `CsiIgnore` (trap 23).
    let stats = session.feed(b"\x1b[ !#p");
    assert_eq!(session.cursor(), (0, 0));
    assert_eq!(stats.unhandled_sequences, 1);

    // A private marker in the parameter list poisons the whole sequence, and
    // that one dispatches nothing at all.
    let mut session = Session::new(10, 3);
    let stats = session.feed(b"\x1b[1?2H");
    assert_eq!(session.cursor(), (0, 0));
    assert_eq!(stats.unhandled_sequences, 0);
}

// ── SGR ─────────────────────────────────────────────────────────────────────

#[test]
fn sgr_semicolon_and_colon_colour_forms() {
    // Trap 20: all five forms, plus the above-255 abort.
    let cases: &[(&[u8], Color)] = &[
        (b"\x1b[38;5;9m", Color::Palette(9)),
        (b"\x1b[38;2;1;2;3m", Color::Rgb(Rgb { r: 1, g: 2, b: 3 })),
        (b"\x1b[38:5:9m", Color::Palette(9)),
        (b"\x1b[38:2:1:2:3m", Color::Rgb(Rgb { r: 1, g: 2, b: 3 })),
        // Six sub-parameters: the colour-space id is skipped.
        (b"\x1b[38:2:7:1:2:3m", Color::Rgb(Rgb { r: 1, g: 2, b: 3 })),
    ];
    for (bytes, expected) in cases {
        let mut session = Session::new(10, 3);
        session.feed(bytes);
        session.feed(b"x");
        assert_eq!(
            session.style_at(0, 0).fg,
            *expected,
            "{}",
            String::from_utf8_lossy(bytes)
        );
    }

    // A value above 255 aborts the attribute, after the parameters it already
    // consumed: `2` and `300` are gone, so `1` and `2` are read as fresh SGR
    // parameters (bold and dim) and the foreground never moves.
    let mut session = Session::new(10, 3);
    session.feed(b"\x1b[38;2;300;1;2mx");
    let style = session.style_at(0, 0);
    assert_eq!(style.fg, Color::Named(NamedColor::Foreground));
    assert!(style.attrs.contains(Attrs::BOLD | Attrs::DIM));

    // The background and the underline colour take the same five forms.
    let mut session = Session::new(10, 3);
    session.feed(b"\x1b[48:5:4m\x1b[58;2;9;9;9mx");
    let style = session.style_at(0, 0);
    assert_eq!(style.bg, Color::Palette(4));
    assert_eq!(
        style.underline_color,
        Some(Color::Rgb(Rgb { r: 9, g: 9, b: 9 }))
    );
    session.feed(b"\x1b[59my");
    assert_eq!(session.style_at(0, 1).underline_color, None);
}

#[test]
fn sgr_21_is_cancel_bold_and_underline_styles_are_exclusive() {
    // Trap 21.
    let mut session = Session::new(10, 3);
    session.feed(b"\x1b[1m\x1b[21mx");
    assert!(!session.style_at(0, 0).attrs.contains(Attrs::BOLD));

    // Any underline attribute clears the other underline bits first.
    let forms: &[(&[u8], Attrs)] = &[
        (b"\x1b[4m", Attrs::UNDERLINE),
        (b"\x1b[4:1m", Attrs::UNDERLINE),
        (b"\x1b[4:2m", Attrs::DOUBLE_UNDERLINE),
        (b"\x1b[4:3m", Attrs::UNDERCURL),
        (b"\x1b[4:4m", Attrs::DOTTED_UNDERLINE),
        (b"\x1b[4:5m", Attrs::DASHED_UNDERLINE),
    ];
    for (bytes, expected) in forms {
        let mut session = Session::new(10, 3);
        session.feed(b"\x1b[4:3m");
        session.feed(bytes);
        session.feed(b"x");
        let attrs = session.style_at(0, 0).attrs;
        assert_eq!(
            attrs & Attrs::ALL_UNDERLINES,
            *expected,
            "{}",
            String::from_utf8_lossy(bytes)
        );
    }

    // `4:0` and `24` both cancel them all.
    for cancel in [&b"\x1b[4:0m"[..], &b"\x1b[24m"[..]] {
        let mut session = Session::new(10, 3);
        session.feed(b"\x1b[4:3m");
        session.feed(cancel);
        session.feed(b"x");
        assert!(
            !session
                .style_at(0, 0)
                .attrs
                .intersects(Attrs::ALL_UNDERLINES)
        );
    }
}

#[test]
fn blink_and_overline_are_stored() {
    // Correction C11: the reference parses SGR 5 / 6 and drops them, and has no
    // 53 / 55 at all.
    let mut session = Session::new(10, 3);
    session.feed(b"\x1b[5;53mx");
    let attrs = session.style_at(0, 0).attrs;
    assert!(attrs.contains(Attrs::BLINK_SLOW));
    assert!(attrs.contains(Attrs::OVERLINE));

    session.feed(b"\x1b[6my");
    assert!(session.style_at(0, 1).attrs.contains(Attrs::BLINK_FAST));

    session.feed(b"\x1b[25;55mz");
    let attrs = session.style_at(0, 2).attrs;
    assert!(!attrs.intersects(Attrs::BLINK_SLOW | Attrs::BLINK_FAST));
    assert!(!attrs.contains(Attrs::OVERLINE));
}

#[test]
fn sgr_reset_and_named_colours() {
    let mut session = Session::new(10, 3);
    session.feed(b"\x1b[1;31;44mx\x1b[0my");
    let first = session.style_at(0, 0);
    assert_eq!(first.fg, Color::Named(NamedColor::Red));
    assert_eq!(first.bg, Color::Named(NamedColor::Blue));
    assert_eq!(session.style_at(0, 1), crate::cell::Style::DEFAULT);

    // The bright ranges, and the two defaults.
    session.feed(b"\x1b[91;104mz\x1b[39;49mw");
    let bright = session.style_at(0, 2);
    assert_eq!(bright.fg, Color::Named(NamedColor::BrightRed));
    assert_eq!(bright.bg, Color::Named(NamedColor::BrightBlue));
    let reset = session.style_at(0, 3);
    assert_eq!(reset.fg, Color::Named(NamedColor::Foreground));
    assert_eq!(reset.bg, Color::Named(NamedColor::Background));

    // An unknown parameter is skipped; the rest of the list still processes.
    session.feed(b"\x1b[0;199;3mq");
    assert!(session.style_at(0, 4).attrs.contains(Attrs::ITALIC));
}

// ── Erase, clear and the ScreenCleared scope ────────────────────────────────

#[test]
fn screen_cleared_fires_only_for_ed2_ed3_and_ris() {
    // Trap 12.
    let cleared = |bytes: &[u8]| {
        let mut session = Session::new(10, 4);
        session.feed(b"hello\r\nworld\r\n");
        session.feed(bytes);
        session
            .batch
            .iter()
            .filter(|event| **event == VtEvent::ScreenCleared)
            .count()
    };

    assert_eq!(cleared(b"\x1b[J"), 0);
    assert_eq!(cleared(b"\x1b[0J"), 0);
    assert_eq!(cleared(b"\x1b[1J"), 0);
    assert_eq!(cleared(b"\x1b[K"), 0);
    assert_eq!(cleared(b"\x1b[2K"), 0);
    assert_eq!(cleared(b"\x1b[2J"), 1);
    assert_eq!(cleared(b"\x1b[3J"), 1);
    assert_eq!(cleared(b"\x1bc"), 1);
}

#[test]
fn ed3_with_no_history_still_fires_screen_cleared() {
    // Trap 12: the event is not gated on there being history to clear.
    let mut session = Session::new(10, 4);
    session.feed(b"\x1b[3J");

    assert_eq!(session.term.screen().history_len(), 0);
    assert!(
        session
            .batch
            .iter()
            .any(|event| *event == VtEvent::ScreenCleared)
    );
}

#[test]
fn erase_uses_the_background_but_not_the_foreground() {
    // The erase cell is the default cell plus the template background, so an
    // erase under an open underline leaves plain blanks.
    let mut session = Session::new(8, 2);
    session.feed(b"\x1b[4;31;44mabcd\x1b[K");
    let erased = session.style_at(0, 5);
    assert_eq!(erased.bg, Color::Named(NamedColor::Blue));
    assert_eq!(erased.fg, Color::Named(NamedColor::Foreground));
    assert!(!erased.attrs.contains(Attrs::UNDERLINE));
}

// ── Modes ───────────────────────────────────────────────────────────────────

#[test]
fn decrqm_answers_match_the_mode_table() {
    // A table test over every mode with a private number, in its power-on state.
    let expected = |mode: Mode| -> ModeState {
        match mode {
            Mode::DecCoLm | Mode::GraphemeClusters => ModeState::NotSupported,
            Mode::LineWrap | Mode::ShowCursor | Mode::AlternateScroll | Mode::UrgencyHints => {
                ModeState::Set
            }
            _ => ModeState::Reset,
        }
    };
    for mode in Mode::PRIVATE {
        let code = mode.private_code().expect("a private mode has a number");
        let mut session = Session::new(10, 3);
        session.feed(format!("\x1b[?{code}$p").as_bytes());
        assert_eq!(
            session.replies(),
            format!("\x1b[?{code};{}$y", expected(mode) as u8),
            "{mode:?}"
        );
    }

    // A mode that is accepted but does nothing must never answer `Set`: that
    // would tell a program it may rely on a capability the engine has not
    // implemented. The whole table is walked rather than a hand-picked pair, so
    // a mode added without a reader fails here instead of lying on the wire.
    let mut inert = 0;
    for mode in Mode::PRIVATE {
        let Some(state) = mode.inert_state() else {
            continue;
        };
        inert += 1;
        assert_ne!(state, ModeState::Set, "{mode:?} is inert and claims Set");
        let code = mode.private_code().expect("a private mode has a number");
        let mut session = Session::new(10, 3);
        session.feed(format!("\x1b[?{code}h\x1b[?{code}$p").as_bytes());
        assert_eq!(
            session.replies(),
            format!("\x1b[?{code};{}$y", state as u8),
            "mode {code} is recognised but unread, so `h` must not make it Set"
        );
    }
    assert_eq!(
        inert, 3,
        "? 3, ? 2027 and ? 9001 — and `? 45` left the table"
    );

    // The same walk over the ANSI space (`US-0087`). The rule is about readers,
    // not about which number space a mode lives in, so LNM — tracked and inert
    // by deviation D9 — answers through `inert_state` exactly like `? 9001`,
    // and an `h` must not make it `Set`.
    let mut ansi_inert = 0;
    for (mode, code) in Mode::ANSI {
        let mut session = Session::new(10, 3);
        session.feed(format!("\x1b[{code}h\x1b[{code}$p").as_bytes());
        let answer = match mode.inert_state() {
            Some(state) => {
                ansi_inert += 1;
                assert_ne!(state, ModeState::Set, "{mode:?} is inert and claims Set");
                state
            }
            // A mode with a reader answers its real state, which `h` just set.
            None => ModeState::Set,
        };
        assert_eq!(
            session.replies(),
            format!("\x1b[{code};{}$y", answer as u8),
            "ANSI mode {code} ({mode:?})"
        );
    }
    assert_eq!(ansi_inert, 1, "LNM is the only inert ANSI mode");

    // `? 45` left it at `US-0086`: `Screen::backspace` reads the mode now, so
    // the honest answer is the real state (deviation D12).
    let mut session = Session::new(10, 3);
    session.feed(b"\x1b[?45h\x1b[?45$p");
    assert_eq!(session.replies(), "\x1b[?45;1$y");
    session.feed(b"\x1b[?45l\x1b[?45$p");
    assert_eq!(session.replies(), "\x1b[?45;2$y");

    // And that the answer follows the real state.
    let mut session = Session::new(10, 3);
    session.feed(b"\x1b[?1h\x1b[?1$p");
    assert_eq!(session.replies(), "\x1b[?1;1$y");
    session.feed(b"\x1b[?1l\x1b[?1$p");
    assert_eq!(session.replies(), "\x1b[?1;2$y");

    // An unknown number in both spaces.
    let mut session = Session::new(10, 3);
    session.feed(b"\x1b[77$p");
    assert_eq!(session.replies(), "\x1b[77;0$y");
    session.feed(b"\x1b[?7777$p");
    assert_eq!(session.replies(), "\x1b[?7777;0$y");
}

#[test]
fn reverse_wrap_is_off_by_default_and_crosses_a_wrapped_row_when_set() {
    // Deviation D12 / R-08, from the wire. Four columns, so "abcde" wraps and
    // row 0 carries WRAPPED.
    let mut session = Session::new(4, 3);
    session.feed(b"abcde");
    assert_eq!(session.cursor(), (1, 1));

    // Default (reset) — trap 1: `BS` at column 0 is a complete no-op.
    session.feed(b"\x08\x08");
    assert_eq!(session.cursor(), (1, 0));
    session.feed(b"\x08");
    assert_eq!(session.cursor(), (1, 0), "? 45 is reset, so BS stops here");

    // Set: the cursor crosses into the previous row's last column, and the
    // glyph it lands on is the one it wrapped away from.
    session.feed(b"\x1b[?45h\x08");
    assert_eq!(session.cursor(), (0, 3));
    assert_eq!(session.row(0), "abcd");

    // Unsetting restores trap 1 immediately.
    session.feed(b"\x1b[?45l\x1b[2;1H\x08");
    assert_eq!(session.cursor(), (1, 0));

    // A row that is not WRAPPED is not crossed into even while the mode is set.
    let mut session = Session::new(4, 3);
    session.feed(b"ab\r\n\x1b[?45h\x08");
    assert_eq!(session.cursor(), (1, 0));
}

#[test]
fn mode_2026_reports_its_real_state() {
    // Deviation D4: the reference hardcodes `Reset` and can never say "set".
    let mut session = Session::new(10, 3);
    session.feed(b"\x1b[?2026h\x1b[?2026$p");
    assert_eq!(session.replies(), "\x1b[?2026;1$y");
    assert!(session.term.sync().is_set());

    session.feed(b"\x1b[?2026l\x1b[?2026$p");
    assert_eq!(session.replies(), "\x1b[?2026;2$y");
    assert!(!session.term.sync().is_set());
}

#[test]
fn mouse_modes_are_exclusive_on_set_not_on_unset() {
    let mut session = Session::new(10, 3);
    session.feed(b"\x1b[?1000h");
    assert_eq!(
        session.term.mouse_reporting().map(|p| p.reporting),
        Some(MouseReporting::Normal)
    );

    // Setting another clears the first.
    session.feed(b"\x1b[?1003h");
    assert_eq!(
        session.term.mouse_reporting().map(|p| p.reporting),
        Some(MouseReporting::AnyEvent)
    );
    assert!(!session.term.mode(Mode::MouseClick));

    // Unsetting clears only that one, so a mode set behind it stays gone.
    session.feed(b"\x1b[?1002h\x1b[?1002l");
    assert_eq!(session.term.mouse_reporting(), None);

    // The encodings are mutually exclusive on set in the same way.
    session.feed(b"\x1b[?1000h\x1b[?1005h");
    assert_eq!(
        session.term.mouse_reporting().map(|p| p.encoding),
        Some(MouseEncoding::Utf8)
    );
    session.feed(b"\x1b[?1006h");
    assert_eq!(
        session.term.mouse_reporting().map(|p| p.encoding),
        Some(MouseEncoding::Sgr)
    );
    assert!(!session.term.mode(Mode::Utf8Mouse));
}

#[test]
fn deccolm_does_not_change_the_width() {
    // Trap 40: both `h` and `l` run the side effects, and the column count and
    // the DECRQM answer both stay put.
    for bytes in [&b"\x1b[?3h"[..], &b"\x1b[?3l"[..]] {
        let mut session = Session::new(20, 5);
        session.feed(b"\x1b[2;4rhello");
        session.feed(bytes);

        assert_eq!(session.term.size().cols, 20);
        assert_eq!(session.row(0), " ".repeat(20));
        assert_eq!(session.term.screen().region().top, 0);
        assert_eq!(session.term.screen().region().bottom, 5);
    }
}

#[test]
fn win32_input_mode_is_accepted_silently() {
    // R-36: conhost sends this unprompted at session start and re-injects it
    // after any DECRST, so it must never be counted as unhandled.
    let mut session = Session::new(10, 3);
    let stats = session.feed(b"\x1b[?9001h\x1b[?9001h\x1b[?9001l\x1b[?9001h");

    assert_eq!(stats.unhandled_sequences, 0);
    session.feed(b"\x1b[?9001$p");
    // `Reset` is the honest answer: the encoding is not implemented.
    assert_eq!(session.replies(), "\x1b[?9001;2$y");
}

#[test]
fn mode_2027_is_recognised_and_inert() {
    // R-56: `cluster_width` ships, the print path does not.
    let mut session = Session::new(10, 3);
    let stats = session.feed(b"\x1b[?2027h");
    assert_eq!(stats.unhandled_sequences, 0);

    session.feed("e\u{301}".as_bytes());
    // Still one scalar per cell, exactly as with the mode reset: the combining
    // mark joins the cell to its left rather than taking a column.
    assert_eq!(session.row(0), "e\u{301}         ");

    session.feed(b"\x1b[?2027$p");
    assert_eq!(session.replies(), "\x1b[?2027;0$y");
}

#[test]
fn encode_key_reads_the_terminals_own_decckm() {
    // The convenience form: the embedder does not fetch a snapshot, so this is
    // the only thing that pins the mode table to the encoder.
    let mut session = Session::new(10, 3);
    let up = KeySpec::Named(NamedKey::ArrowUp);
    let none = KeyMods::default();
    assert_eq!(session.term.encode_key(&up, none).unwrap(), b"\x1b[A");

    session.feed(b"\x1b[?1h");
    assert_eq!(session.term.encode_key(&up, none).unwrap(), b"\x1bOA");

    session.feed(b"\x1b[?1l");
    assert_eq!(session.term.encode_key(&up, none).unwrap(), b"\x1b[A");
}

#[test]
fn app_keypad_mode_is_reported() {
    // R-64: the keypad state `crate::input::key` needs.
    let mut session = Session::new(10, 3);
    assert!(!session.term.mode_snapshot().app_keypad);

    session.feed(b"\x1b=");
    assert!(session.term.mode_snapshot().app_keypad);

    session.feed(b"\x1b>");
    assert!(!session.term.mode_snapshot().app_keypad);
}

#[test]
fn insert_mode_shifts_the_row() {
    let mut session = Session::new(10, 2);
    session.feed(b"abcdef\x1b[1;3H\x1b[4hXY");
    assert_eq!(session.row(0), "abXYcdef  ");

    session.feed(b"\x1b[4l\x1b[1;1HZ");
    assert_eq!(session.row(0), "ZbXYcdef  ");
}

#[test]
fn origin_mode_homes_the_cursor_and_makes_goto_region_relative() {
    let mut session = Session::new(10, 10);
    session.feed(b"\x1b[3;6r");
    // A valid DECSTBM always homes the cursor.
    assert_eq!(session.cursor(), (0, 0));

    session.feed(b"\x1b[?6h");
    // Setting origin mode homes the cursor, which is now the region's top.
    assert_eq!(session.cursor(), (2, 0));

    session.feed(b"\x1b[2;1H");
    assert_eq!(session.cursor(), (3, 0));

    // And it clamps to the region's bottom.
    session.feed(b"\x1b[99;1H");
    assert_eq!(session.cursor(), (5, 0));
}

#[test]
fn an_invalid_scrolling_region_is_a_noop() {
    // Trap 15, with the reference's one-based validity test: `CSI 1;1r` is
    // invalid, because a region has to be at least two rows.
    let mut session = Session::new(10, 10);
    session.feed(b"\x1b[3;6r\x1b[5;5H");
    session.feed(b"\x1b[8;4r");
    assert_eq!(session.term.screen().region().top, 2);
    assert_eq!(session.term.screen().region().bottom, 6);
    // A rejected region does not home the cursor either.
    assert_eq!(session.cursor(), (4, 4));
}

// ── Answers ─────────────────────────────────────────────────────────────────

/// `product_name` owns both halves of the terminal's identity: the string
/// `XTVERSION` returns and the number `DA2` returns. The literals below are the
/// bytes an embedder shipping "OneTerm(0.5.2)" gets.
#[test]
fn product_name_owns_xtversion_and_da2() {
    let mut session = Session::with(Terminal::new(
        Size { rows: 24, cols: 80 },
        Config {
            product_name: Some("OneTerm(0.5.2)".into()),
            ..Config::default()
        },
    ));

    session.feed(b"\x1b[>0q");
    assert_eq!(session.replies(), "\x1bP>|OneTerm(0.5.2)\x1b\\");

    session.feed(b"\x1b[>c");
    assert_eq!(session.replies(), "\x1b[>0;502;1c");

    // A name with no version in it leaves `DA2` on the engine's own number, so
    // the reply stays a version rather than becoming zero.
    let mut plain = Session::with(Terminal::new(
        Size { rows: 24, cols: 80 },
        Config {
            product_name: Some("MyTerm".into()),
            ..Config::default()
        },
    ));
    plain.feed(b"\x1b[>0q");
    assert_eq!(plain.replies(), "\x1bP>|MyTerm\x1b\\");
    plain.feed(b"\x1b[>c");
    assert_eq!(
        plain.replies(),
        format!("\x1b[>0;{};1c", super::dispatch_version_for_tests())
    );
}

#[test]
fn da1_da2_dsr_xtversion_answers() {
    let mut session = Session::new(80, 24);

    // Deviation D13: VT220, Sixel, ANSI colour.
    session.feed(b"\x1b[c");
    assert_eq!(session.replies(), "\x1b[?62;4;22c");
    session.feed(b"\x1bZ");
    assert_eq!(session.replies(), "\x1b[?62;4;22c");
    // Only `Ps == 0` answers.
    session.feed(b"\x1b[1c");
    assert_eq!(session.replies(), "");

    let version = super::dispatch_version_for_tests();
    session.feed(b"\x1b[>c");
    assert_eq!(session.replies(), format!("\x1b[>0;{version};1c"));

    session.feed(b"\x1b[5n");
    assert_eq!(session.replies(), "\x1b[0n");

    session.feed(b"\x1b[3;7H\x1b[6n");
    assert_eq!(session.replies(), "\x1b[3;7R");

    // Deviation D7.
    session.feed(b"\x1b[?6n");
    assert_eq!(session.replies(), "\x1b[?3;7;1R");

    // Deviation D8. With no `product_name`, the engine answers for itself.
    session.feed(b"\x1b[>0q");
    assert_eq!(
        session.replies(),
        format!("\x1bP>|oneterm-vt({})\x1b\\", env!("CARGO_PKG_VERSION"))
    );

    session.feed(b"\x1b[18t");
    assert_eq!(session.replies(), "\x1b[8;24;80t");

    // `CSI 14 t` needs metrics only the embedder has; they default to zero.
    session.feed(b"\x1b[14t");
    assert_eq!(session.replies(), "\x1b[4;0;0t");
    session.term.set_cell_pixels(9, 18);
    session.feed(b"\x1b[14t");
    assert_eq!(session.replies(), "\x1b[4;432;720t");

    // Deviation D10.
    session.feed(b"\x1b[>4;2m\x1b[?4m");
    assert_eq!(session.replies(), "\x1b[>4;2m");
    assert_eq!(session.term.modify_other_keys(), 2);
}

#[test]
fn cpr_honours_origin_mode() {
    // Correction C5, trap 38: region-relative while `DECOM` is set, absolute
    // otherwise. Conhost never sets origin mode, so its handshake is unaffected.
    let mut session = Session::new(20, 10);
    session.feed(b"\x1b[3;8r\x1b[5;2H\x1b[6n");
    assert_eq!(session.replies(), "\x1b[5;2R");

    session.feed(b"\x1b[?6h\x1b[3;2H\x1b[6n");
    assert_eq!(session.replies(), "\x1b[3;2R");
    // Absolute row 5, region-relative row 3.
    assert_eq!(session.cursor().0, 4);
}

// ── Resets ──────────────────────────────────────────────────────────────────

#[test]
fn ris_resets_the_palette_and_keeps_the_row_ids() {
    // Correction C6, trap 39: the reference leaves the overrides in place.
    let mut session = Session::new(10, 4);
    session.feed(b"\x1b]4;3;#ff0000\x07\x1b]11;#00ff00\x07");
    assert_eq!(
        session.term.color(ColorKey::Palette(3)),
        Some(Rgb {
            r: 0xff,
            g: 0,
            b: 0
        })
    );

    session.feed(b"hello\r\nworld\r\n");
    let newest = session.term.screen().newest();

    session.feed(b"\x1bc");

    assert_eq!(session.term.color(ColorKey::Palette(3)), None);
    assert_eq!(session.term.color(ColorKey::Background), None);
    // `DEC-0015`: ids name a position in the output stream and are never reset.
    assert_eq!(session.term.screen().newest(), newest);
    assert_eq!(session.row(0), "          ");
    assert_eq!(session.cursor(), (0, 0));
}

#[test]
fn ris_resets_the_modes_the_title_and_the_charset() {
    let mut session = Session::new(10, 4);
    session.feed(b"\x1b]0;title\x07\x1b[22t\x1b[?1h\x1b[?25l\x1b[4h\x1b(0\x1b=");
    assert_eq!(session.term.title(), Some("title"));

    session.feed(b"\x1bc");

    assert_eq!(session.term.title(), None);
    assert_eq!(session.term.title_depth(), 0);
    assert!(!session.term.mode(Mode::AppCursor));
    assert!(session.term.mode(Mode::ShowCursor));
    assert!(!session.term.mode(Mode::Insert));
    assert!(!session.term.mode(Mode::AppKeypad));
    assert!(session.term.mode(Mode::LineWrap));
    assert_eq!(session.term.active_charset(), crate::grid::Charset::Ascii);
}

#[test]
fn decstr_soft_reset_scope() {
    // Correction C9: the reference does not implement `CSI ! p` at all.
    let mut session = Session::new(10, 6);
    session.feed(b"\x1b]0;keep\x07\x1b]4;3;#ff0000\x07");
    session.feed(b"hello\r\n\x1b[2;5r\x1b[?6h\x1b[4h\x1b[?7l\x1b[?25l\x1b[1;31m");
    session.feed(b"\x1b[3;4H\x1b7");

    session.feed(b"\x1b[!p");

    // Reset.
    assert!(!session.term.mode(Mode::Origin));
    assert!(!session.term.mode(Mode::Insert));
    assert!(session.term.mode(Mode::LineWrap));
    assert!(session.term.mode(Mode::ShowCursor));
    assert_eq!(session.term.screen().region().top, 0);
    assert_eq!(session.term.screen().region().bottom, 6);
    assert_eq!(session.cursor(), (0, 0));
    assert_eq!(session.term.style(), crate::cell::Style::DEFAULT);
    assert_eq!(session.term.screen().saved_cursor().pos.col, 0);

    // Kept: the screen, the title and the palette.
    assert_eq!(session.row(0), "hello     ");
    assert_eq!(session.term.title(), Some("keep"));
    assert_eq!(
        session.term.color(ColorKey::Palette(3)),
        Some(Rgb {
            r: 0xff,
            g: 0,
            b: 0
        })
    );
}

// ── Screens ─────────────────────────────────────────────────────────────────

#[test]
fn alt_screen_47_and_1047_and_1048() {
    // Correction C8, trap 13: the reference ignores all three.
    for code in ["47", "1047", "1049"] {
        let mut session = Session::new(10, 3);
        session.feed(b"primary\r\n");
        session.feed(format!("\x1b[?{code}h").as_bytes());
        assert!(session.term.mode(Mode::AltScreen), "{code} h");
        assert_eq!(session.row(0), "          ");

        session.feed(b"alt");
        session.feed(format!("\x1b[?{code}l").as_bytes());
        assert!(!session.term.mode(Mode::AltScreen), "{code} l");
        assert_eq!(session.row(0), "primary   ");
    }

    // `? 1048` is `DECSC` / `DECRC` with no screen swap at all.
    let mut session = Session::new(10, 3);
    session.feed(b"\x1b[2;5H\x1b[?1048h\x1b[1;1H");
    assert!(!session.term.mode(Mode::AltScreen));
    session.feed(b"\x1b[?1048l");
    assert_eq!(session.cursor(), (1, 4));
}

#[test]
fn decsc_and_decrc_are_per_screen() {
    let mut session = Session::new(10, 4);
    session.feed(b"\x1b[2;3H\x1b7");
    session.feed(b"\x1b[?1049h");
    // The alternate screen has its own saved slot.
    session.feed(b"\x1b[4;8H\x1b7\x1b[1;1H\x1b8");
    assert_eq!(session.cursor(), (3, 7));

    session.feed(b"\x1b[?1049l");
    // Trap 14: entering took the primary `DECSC` slot, which is what `? 1049`
    // means, so the primary comes back where it was.
    assert_eq!(session.cursor(), (1, 2));
}

#[test]
fn the_keyboard_stack_swaps_with_the_screen() {
    let mut session = Session::new(10, 3);
    session.feed(b"\x1b[>5u");
    assert_eq!(session.term.keyboard_flags().bits(), 5);

    session.feed(b"\x1b[?1049h");
    assert_eq!(session.term.keyboard_flags().bits(), 0);
    session.feed(b"\x1b[>2u");
    assert_eq!(session.term.keyboard_flags().bits(), 2);

    session.feed(b"\x1b[?1049l");
    assert_eq!(session.term.keyboard_flags().bits(), 5);
}

// ── Kitty keyboard ──────────────────────────────────────────────────────────

#[test]
fn kitty_query_reads_the_stack_top() {
    // Trap 42: `CSI ? u` reports the stack, which can legitimately differ from
    // the live flags after `CSI = Ps u`.
    let mut session = Session::new(10, 3);
    session.feed(b"\x1b[>1u");
    session.feed(b"\x1b[=6;2u");

    assert_eq!(session.term.keyboard_flags().bits(), 1 | 6);
    session.feed(b"\x1b[?u");
    assert_eq!(session.replies(), "\x1b[?1u");

    // The three apply behaviours.
    let mut session = Session::new(10, 3);
    session.feed(b"\x1b[=7u");
    assert_eq!(session.term.keyboard_flags().bits(), 7);
    session.feed(b"\x1b[=1;3u");
    assert_eq!(session.term.keyboard_flags().bits(), 6);
    session.feed(b"\x1b[=8;2u");
    assert_eq!(session.term.keyboard_flags().bits(), 14);
    session.feed(b"\x1b[=2u");
    assert_eq!(session.term.keyboard_flags().bits(), 2);
}

#[test]
fn kitty_pop_beyond_len_resets_the_stack() {
    // Deviation D15: the reference pops the *title* stack on overflow, and a
    // huge pop count is a denial-of-service vector.
    let mut session = Session::new(10, 3);
    session.feed(b"\x1b]0;title\x07\x1b[22t\x1b[22t");
    assert_eq!(session.term.title_depth(), 2);

    for _ in 0..KEYBOARD_STACK_MAX + 4 {
        session.feed(b"\x1b[>1u");
    }
    // The title stack is untouched, which is the fix.
    assert_eq!(session.term.title_depth(), 2);

    session.feed(b"\x1b[<65535u");
    session.feed(b"\x1b[?u");
    assert_eq!(session.replies(), "\x1b[?0u");
    assert_eq!(session.term.keyboard_flags().bits(), 0);

    // The default pop count is 1.
    let mut session = Session::new(10, 3);
    session.feed(b"\x1b[>1u\x1b[>3u\x1b[<u\x1b[?u");
    assert_eq!(session.replies(), "\x1b[?1u");
}

// ── Title stack ─────────────────────────────────────────────────────────────

#[test]
fn title_stack_caps_at_sixteen_dropping_the_oldest() {
    // Deviation D14: the reference caps at 4096, which is a cheap memory sink.
    let mut session = Session::new(10, 3);
    for index in 0..TITLE_STACK_MAX + 4 {
        session.feed(format!("\x1b]0;t{index}\x07\x1b[22t").as_bytes());
    }
    assert_eq!(session.term.title_depth(), TITLE_STACK_MAX);

    // The newest entries survived, so popping walks back down from the top.
    session.feed(b"\x1b[23t");
    assert_eq!(session.term.title(), Some("t19"));
    for _ in 1..TITLE_STACK_MAX {
        session.feed(b"\x1b[23t");
    }
    // The oldest kept entry is `t4`, not `t0`.
    assert_eq!(session.term.title(), Some("t4"));
    assert_eq!(session.term.title_depth(), 0);

    // Popping an empty stack applies nothing.
    session.feed(b"\x1b[23t");
    assert_eq!(session.term.title(), Some("t4"));
}

// ── OSC ─────────────────────────────────────────────────────────────────────

#[test]
fn osc_0_and_2_set_the_title() {
    for code in ["0", "2"] {
        let mut session = Session::new(10, 3);
        session.feed(format!("\x1b]{code};  hello  \x07").as_bytes());
        assert_eq!(session.term.title(), Some("hello"));
        assert!(
            session
                .batch
                .iter()
                .any(|event| matches!(event, VtEvent::Title(_)))
        );
    }

    // A `;` inside the title is rejoined, and `ESC \` terminates too.
    let mut session = Session::new(10, 3);
    session.feed(b"\x1b]0;a;b\x1b\\");
    assert_eq!(session.term.title(), Some("a;b"));

    // With no second parameter it is dropped and counted.
    let mut session = Session::new(10, 3);
    let stats = session.feed(b"\x1b]0\x07");
    assert_eq!(session.term.title(), None);
    assert_eq!(stats.unhandled_sequences, 1);
}

#[test]
fn osc_4_applies_complete_pairs() {
    // Correction C7, trap 26: the reference rejects an even parameter count
    // wholesale; here every complete pair is applied and a trailing odd
    // parameter is ignored.
    let mut session = Session::new(10, 3);
    session.feed(b"\x1b]4;1;#ff0000;2;#00ff00;3\x07");

    assert_eq!(
        session.term.color(ColorKey::Palette(1)),
        Some(Rgb {
            r: 0xff,
            g: 0,
            b: 0
        })
    );
    assert_eq!(
        session.term.color(ColorKey::Palette(2)),
        Some(Rgb {
            r: 0,
            g: 0xff,
            b: 0
        })
    );
    assert_eq!(session.term.color(ColorKey::Palette(3)), None);

    // An index above 255 is still rejected.
    let mut session = Session::new(10, 3);
    let stats = session.feed(b"\x1b]4;300;#ff0000\x07");
    assert!(stats.unhandled_sequences > 0);

    // Both colour syntaxes, and the `?` query.
    let mut session = Session::new(10, 3);
    session.feed(b"\x1b]4;5;rgb:ff/00/80\x07");
    assert_eq!(
        session.term.color(ColorKey::Palette(5)),
        Some(Rgb {
            r: 0xff,
            g: 0,
            b: 0x80
        })
    );
    session.feed(b"\x1b]4;5;?\x07");
    assert!(session.batch.iter().any(|event| matches!(
        event,
        VtEvent::ColorQuery {
            key: ColorKey::Palette(5),
            ..
        }
    )));
}

#[test]
fn osc_104_does_not_reset_the_special_colours() {
    // Trap 26: with no argument it resets 0..=255 and nothing else.
    let mut session = Session::new(10, 3);
    session.feed(b"\x1b]4;7;#ff0000\x07\x1b]10;#00ff00\x07\x1b]11;#0000ff\x07\x1b]12;#ffffff\x07");

    session.feed(b"\x1b]104\x07");

    assert_eq!(session.term.color(ColorKey::Palette(7)), None);
    assert!(session.term.color(ColorKey::Foreground).is_some());
    assert!(session.term.color(ColorKey::Background).is_some());
    assert!(session.term.color(ColorKey::Cursor).is_some());

    // With arguments it resets exactly those indices.
    session.feed(b"\x1b]4;7;#ff0000;8;#00ff00\x07\x1b]104;7\x07");
    assert_eq!(session.term.color(ColorKey::Palette(7)), None);
    assert!(session.term.color(ColorKey::Palette(8)).is_some());

    // And 110 / 111 / 112 reset the three that 104 will not.
    session.feed(b"\x1b]110\x07\x1b]111\x07\x1b]112\x07");
    assert_eq!(session.term.color(ColorKey::Foreground), None);
    assert_eq!(session.term.color(ColorKey::Background), None);
    assert_eq!(session.term.color(ColorKey::Cursor), None);
}

#[test]
fn osc_10_11_12_set_query_and_advance() {
    let mut session = Session::new(10, 3);
    session.feed(b"\x1b]10;#010203\x07");
    assert_eq!(
        session.term.color(ColorKey::Foreground),
        Some(Rgb { r: 1, g: 2, b: 3 })
    );

    // A multi-parameter form advances the key.
    session.feed(b"\x1b]10;#040506;#070809\x07");
    assert_eq!(
        session.term.color(ColorKey::Foreground),
        Some(Rgb { r: 4, g: 5, b: 6 })
    );
    assert_eq!(
        session.term.color(ColorKey::Background),
        Some(Rgb { r: 7, g: 8, b: 9 })
    );

    // And stops past `Cursor`.
    let mut session = Session::new(10, 3);
    let stats = session.feed(b"\x1b]12;#010101;#020202\x07");
    assert_eq!(
        session.term.color(ColorKey::Cursor),
        Some(Rgb { r: 1, g: 1, b: 1 })
    );
    assert!(stats.unhandled_sequences > 0);

    // A query leaves the answer to the embedder, terminated as it was asked.
    let mut session = Session::new(10, 3);
    session.feed(b"\x1b]11;?\x1b\\");
    assert!(session.batch.iter().any(|event| matches!(
        event,
        VtEvent::ColorQuery {
            key: ColorKey::Background,
            terminator: crate::event::StringTerm::St,
        }
    )));
}

#[test]
fn osc_52_selection_byte_validation() {
    // Trap 25: the engine reports; the policy stays in the embedder.
    let mut session = Session::new(10, 3);
    session.feed(b"\x1b]52;c;aGVsbG8=\x07");
    let stored = session
        .batch
        .iter()
        .find_map(|event| match event {
            VtEvent::ClipboardStore { selection, text } => {
                Some((*selection, session.batch.str(*text).to_owned()))
            }
            _ => None,
        })
        .expect("a valid store reaches the batch");
    assert_eq!(
        stored,
        (crate::event::ClipboardKind::Clipboard, "hello".to_owned())
    );

    // `p` and `s` are the other two legal selections.
    for (byte, expected) in [
        (b'p', crate::event::ClipboardKind::Primary),
        (b's', crate::event::ClipboardKind::Secondary),
    ] {
        let mut session = Session::new(10, 3);
        session.feed(format!("\x1b]52;{};aGk=\x07", byte as char).as_bytes());
        assert!(session.batch.iter().any(
            |event| matches!(event, VtEvent::ClipboardStore { selection, .. } if *selection == expected)
        ));
    }

    // Anything else drops the request.
    let mut session = Session::new(10, 3);
    let stats = session.feed(b"\x1b]52;q;aGk=\x07");
    assert!(
        session.batch.is_empty()
            || !session
                .batch
                .iter()
                .any(|event| matches!(event, VtEvent::ClipboardStore { .. }))
    );
    assert!(stats.unhandled_sequences > 0);

    // Undecodable base64 and invalid UTF-8 are dropped silently.
    for payload in [
        &b"\x1b]52;c;not base64!\x07"[..],
        &b"\x1b]52;c;/w==\x07"[..],
    ] {
        let mut session = Session::new(10, 3);
        let stats = session.feed(payload);
        assert!(
            !session
                .batch
                .iter()
                .any(|event| matches!(event, VtEvent::ClipboardStore { .. }))
        );
        assert!(stats.malformed_sequences > 0);
    }

    // A read is a load event; the embedder formats the reply.
    let mut session = Session::new(10, 3);
    session.feed(b"\x1b]52;c;?\x07");
    assert!(
        session
            .batch
            .iter()
            .any(|event| matches!(event, VtEvent::ClipboardLoad { .. }))
    );
}

#[test]
fn osc_8_sets_and_clears_the_hyperlink() {
    let mut session = Session::new(20, 3);
    session.feed(b"\x1b]8;;https://example.com/a;b\x07link\x1b]8;;\x07plain");

    let linked = session.cell(0, 0);
    let link = session
        .term
        .interner()
        .resolve_extras(linked.extras_id())
        .hyperlink
        .expect("the cell carries a link");
    let resolved = session
        .term
        .interner()
        .hyperlinks
        .resolve(link)
        .expect("the link resolves");
    // The URI is rejoined, because `;` is not special inside it.
    assert_eq!(&*resolved.uri, "https://example.com/a;b");
    assert!(resolved.implicit);

    // The empty URI cleared it.
    let plain = session.cell(0, 4);
    assert!(
        session
            .term
            .interner()
            .resolve_extras(plain.extras_id())
            .hyperlink
            .is_none()
    );

    // An explicit `id=` groups occurrences, an implicit one does not.
    let mut session = Session::new(20, 3);
    session.feed(b"\x1b]8;id=g;http://a\x07x\x1b]8;;\x07\x1b]8;id=g;http://a\x07y");
    let first = session.cell(0, 0).extras_id();
    let second = session.cell(0, 1).extras_id();
    assert_eq!(first, second);

    let mut session = Session::new(20, 3);
    session.feed(b"\x1b]8;;http://a\x07x\x1b]8;;\x07\x1b]8;;http://a\x07y");
    assert_ne!(
        session.cell(0, 0).extras_id(),
        session.cell(0, 1).extras_id()
    );
}

#[test]
fn hyperlink_table_exhaustion_drops_the_attribute_and_logs_once() {
    // The ladder: reuse, insert, drop. The text still renders; the link is
    // simply not clickable.
    let mut session = Session::new(4, 2);
    for index in 0..HYPERLINK_TABLE_LIMIT {
        // Straight into the table, without paying for a print per link.
        session
            .term
            .state_for_tests()
            .interner
            .hyperlinks
            .intern(None, &format!("http://{index}"));
    }
    assert_eq!(
        session.term.interner().hyperlinks.len(),
        HYPERLINK_TABLE_LIMIT
    );

    let stats = session.feed(b"\x1b]8;;http://one-more\x07ok");

    assert_eq!(stats.hyperlink_table_exhausted, 1);
    assert_eq!(session.row(0), "ok  ");
    assert!(
        session
            .term
            .interner()
            .resolve_extras(session.cell(0, 0).extras_id())
            .hyperlink
            .is_none()
    );
}

#[test]
fn ris_clears_the_hyperlink_table() {
    let mut session = Session::new(20, 3);
    session.feed(b"\x1b]8;;http://a\x07x");
    assert_eq!(session.term.interner().hyperlinks.len(), 1);

    session.feed(b"\x1bc");
    assert!(session.term.interner().hyperlinks.is_empty());

    // The implicit counter is recycled too, so a clear-and-restart cycle cannot
    // accumulate.
    session.feed(b"\x1b]8;;http://b\x07y");
    let id = session
        .term
        .interner()
        .resolve_extras(session.cell(0, 0).extras_id())
        .hyperlink
        .expect("a fresh link");
    assert_eq!(
        &*session
            .term
            .interner()
            .hyperlinks
            .resolve(id)
            .expect("resolves")
            .id,
        "1"
    );
}

#[test]
fn osc_133_marks_reach_the_cells_and_the_anchor_list() {
    let mut session = Session::new(20, 4);
    let before = session.term.grid().anchors().live();

    session.feed(b"\x1b]133;A\x07$ \x1b]133;B\x07ls\x1b]133;C\x07\r\nout\x1b]133;D;0\x07");

    assert_eq!(session.cell(0, 0).semantic(), Semantic::Prompt);
    assert_eq!(session.cell(0, 2).semantic(), Semantic::Input);
    assert_eq!(session.cell(1, 0).semantic(), Semantic::Output);
    // The prompt and the command output each registered a tracked mark.
    assert_eq!(session.term.grid().anchors().live(), before + 2);
}

/// Collect the parameters of the first forwarded OSC carrying `code`.
fn forwarded_osc(session: &Session, code: u32) -> Option<Vec<Vec<u8>>> {
    session.batch.iter().find_map(|event| match event {
        VtEvent::Osc {
            code: seen, params, ..
        } if *seen == code => Some(session.batch.params(*params).map(<[u8]>::to_vec).collect()),
        _ => None,
    })
}

fn osc_events(session: &Session) -> usize {
    session
        .batch
        .iter()
        .filter(|event| matches!(event, VtEvent::Osc { .. }))
        .count()
}

/// All six rows of the routing truth table, for a number the engine
/// implements, one below the bitmap's 2048 bits that it does not, and one
/// above the bitmap that it does not.
#[test]
fn the_routing_table_answers_every_row() {
    const BUILTIN: u32 = 7;
    const PRIVATE: u32 = 633;
    const SPILL: u32 = 20308;

    // Rows 1 and 2: nothing said about the number.
    let routes = OscRoutes::new();
    assert_eq!(routes.get(BUILTIN), OscRoute::Builtin);
    assert_eq!(routes.get(PRIVATE), OscRoute::Drop);
    assert_eq!(routes.get(SPILL), OscRoute::Drop);
    assert_eq!(routes.overrides().count(), 0);

    // Rows 3 and 4: forwarded, with the built-in kept where there is one.
    let mut routes = OscRoutes::new();
    routes.route_all(&[BUILTIN, PRIVATE, SPILL], OscRoute::BuiltinAndForward);
    assert_eq!(routes.get(BUILTIN), OscRoute::BuiltinAndForward);
    assert_eq!(routes.get(PRIVATE), OscRoute::Forward);
    assert_eq!(routes.get(SPILL), OscRoute::Forward);

    // Row 5: dropped.
    let mut routes = OscRoutes::new();
    routes.route_all(&[BUILTIN, PRIVATE, SPILL], OscRoute::Drop);
    assert_eq!(routes.get(BUILTIN), OscRoute::Drop);
    assert_eq!(routes.get(PRIVATE), OscRoute::Drop);
    assert_eq!(routes.get(SPILL), OscRoute::Drop);
    // Only the built-in moved off its default, so only it is an override.
    assert_eq!(
        routes.overrides().collect::<Vec<_>>(),
        vec![(BUILTIN, OscRoute::Drop)]
    );

    // Row 6: forwarded with the built-in skipped.
    let mut routes = OscRoutes::new();
    routes.route_all(&[BUILTIN, PRIVATE, SPILL], OscRoute::Forward);
    assert_eq!(routes.get(BUILTIN), OscRoute::Forward);
    assert_eq!(routes.get(PRIVATE), OscRoute::Forward);
    assert_eq!(routes.get(SPILL), OscRoute::Forward);
    assert_eq!(
        routes.overrides().collect::<Vec<_>>(),
        vec![
            (BUILTIN, OscRoute::Forward),
            (PRIVATE, OscRoute::Forward),
            (SPILL, OscRoute::Forward),
        ]
    );

    // A later call wins, and the published built-in set is the one the engine
    // implements.
    let mut routes = OscRoutes::new();
    routes.route(BUILTIN, OscRoute::Forward);
    routes.route(BUILTIN, OscRoute::Builtin);
    assert_eq!(routes.get(BUILTIN), OscRoute::Builtin);
    for code in OscRoutes::BUILTIN {
        assert!(OscRoutes::has_builtin(code), "OSC {code}");
    }
    for code in [3, 6, 13, 633, 777, 1337, 20308] {
        assert!(!OscRoutes::has_builtin(code), "OSC {code}");
    }
}

/// One `feed` per row, so the table's answer and the engine's behaviour are
/// asserted to be the same thing.
#[test]
fn every_route_behaves_the_way_the_table_says() {
    // `Builtin`: the title is set and nothing is forwarded.
    let mut session = Session::new(20, 4);
    session.feed(b"\x1b]0;hello\x07");
    assert_eq!(session.term.title(), Some("hello"));
    assert_eq!(osc_events(&session), 0);

    // `Forward` on a built-in is an override: the raw sequence arrives and the
    // engine does nothing with it.
    let mut session = Session::with(routing(20, 4, &[0], OscRoute::Forward));
    session.feed(b"\x1b]0;hello\x07");
    assert_eq!(session.term.title(), None);
    assert_eq!(osc_events(&session), 1);
    assert!(
        !session
            .batch
            .iter()
            .any(|event| matches!(event, VtEvent::Title(_)))
    );
    assert_eq!(
        forwarded_osc(&session, 0).expect("the raw sequence"),
        vec![b"0".to_vec(), b"hello".to_vec()]
    );

    // `BuiltinAndForward` is a wrap: two events, the typed one first. That
    // order is the contract.
    let mut session = Session::with(routing(20, 4, &[0], OscRoute::BuiltinAndForward));
    session.feed(b"\x1b]0;hello\x07");
    assert_eq!(session.term.title(), Some("hello"));
    let kinds: Vec<&VtEvent> = session
        .batch
        .iter()
        .filter(|event| !matches!(event, VtEvent::Repaint))
        .collect();
    assert_eq!(kinds.len(), 2);
    assert!(matches!(kinds[0], VtEvent::Title(_)));
    assert!(matches!(kinds[1], VtEvent::Osc { code: 0, .. }));

    // `Drop` suppresses a built-in: no event, no effect, one count.
    let mut session = Session::with(routing(20, 4, &[8], OscRoute::Drop));
    let stats = session.feed(b"\x1b]8;;https://example.invalid\x07x");
    assert_eq!(stats.unhandled_sequences, 1);
    assert_eq!(osc_events(&session), 0);
    assert!(
        session
            .term
            .interner()
            .resolve_extras(session.cell(0, 0).extras_id())
            .hyperlink
            .is_none(),
        "a dropped OSC 8 leaves no link on the cell"
    );
}

/// The escape hatch: a table that forwards everything turns the engine into a
/// pure parser.
#[test]
fn a_table_that_forwards_everything_makes_the_engine_a_parser() {
    let every: Vec<u32> = (0..2048).collect();
    let mut routes = OscRoutes::new();
    routes.route_all(&every, OscRoute::Forward);
    let mut session = Session::with(with_routes(20, 4, routes));

    session.feed(b"\x1b]0;hello\x07");
    assert_eq!(session.term.title(), None, "no engine state changed");
    assert_eq!(
        forwarded_osc(&session, 0).expect("the raw sequence"),
        vec![b"0".to_vec(), b"hello".to_vec()]
    );
}

/// The payload ceiling is bought per number and is orthogonal to the route: a
/// `Forward` route on its own cannot be used to buy memory.
#[test]
fn a_large_ceiling_is_opt_in_per_number() {
    let payload = "x".repeat(3 * 1024 * 1024);

    let mut routes = OscRoutes::new();
    routes.route(20308, OscRoute::Forward).large(20308, true);
    let mut session = Session::with(with_routes(20, 4, routes));
    session.feed(format!("\x1b]20308;1;{payload}\x07").as_bytes());
    let event = session
        .batch
        .iter()
        .find_map(|event| match event {
            VtEvent::Osc {
                code: 20308,
                params,
                truncated,
                ..
            } => Some((*params, *truncated)),
            _ => None,
        })
        .expect("a large payload reaches the batch whole");
    assert!(!event.1, "not truncated");
    assert_eq!(
        session
            .batch
            .params(event.0)
            .nth(2)
            .unwrap_or_default()
            .len(),
        payload.len()
    );

    // The same input without the ceiling stops at `OSC_INLINE`.
    let mut session = Session::with(routing(20, 4, &[20308], OscRoute::Forward));
    let stats = session.feed(format!("\x1b]20308;1;{payload}\x07").as_bytes());
    assert_eq!(stats.truncated_osc, 1);
    let (params, truncated) = session
        .batch
        .iter()
        .find_map(|event| match event {
            VtEvent::Osc {
                code: 20308,
                params,
                truncated,
                ..
            } => Some((*params, *truncated)),
            _ => None,
        })
        .expect("a capped payload still reaches the batch");
    assert!(truncated);
    assert!(
        session.batch.params(params).map(<[u8]>::len).sum::<usize>() <= crate::parser::OSC_INLINE
    );
}

/// The two ways a table can be wrong on purpose. Debug only — a release build
/// of an embedder never dies over a configuration mistake, and `get` reports
/// what actually happens.
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "has no built-in handler")]
fn routing_a_number_with_no_builtin_to_builtin_asserts() {
    OscRoutes::new().route(633, OscRoute::Builtin);
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "is dropped")]
fn buying_a_ceiling_for_a_dropped_number_asserts() {
    OscRoutes::new().large(633, true);
}

// ── The parsers that moved in from the adapter ──────────────────────────────
//
// Every input literal below was an input literal of the adapter's own OSC
// tests, with the same expectation, so the move can be diffed rather than
// trusted.

fn cwd_of(session: &Session) -> Option<(String, String)> {
    session.batch.iter().find_map(|event| match event {
        VtEvent::Cwd { host, path } => Some((
            session.batch.str(*host).to_owned(),
            session.batch.str(*path).to_owned(),
        )),
        _ => None,
    })
}

fn cwd(url: &str) -> (String, String) {
    let mut session = Session::new(20, 4);
    session.feed(format!("\x1b]7;{url}\x07").as_bytes());
    cwd_of(&session).expect("OSC 7 reports a directory")
}

#[test]
fn osc_7_reports_the_host_and_the_path_unresolved() {
    assert_eq!(cwd("file:///home/marc").1, "/home/marc");
    assert_eq!(
        cwd("file://host/var/log"),
        ("host".into(), "/var/log".into())
    );
    assert_eq!(cwd("/tmp/x"), (String::new(), "/tmp/x".into()));
}

/// Percent escapes are decoded and a Windows drive URL loses the URL's leading
/// slash; a malformed escape stays verbatim.
#[test]
fn osc_7_decodes_percent_escapes_and_the_windows_drive_slash() {
    assert_eq!(cwd("file://host/home/me/My%20Docs").1, "/home/me/My Docs");
    assert_eq!(cwd("file:///C:/Users/me/src").1, "C:/Users/me/src");
    assert_eq!(cwd("file:///C:").1, "C:");
    assert_eq!(cwd("file:///tmp/100%25/x%zz").1, "/tmp/100%/x%zz");
    assert_eq!(cwd("file:///home/%C3%A9t%C3%A9").1, "/home/été");
    // A plain absolute path that happens to start with `/C:` is left alone.
    assert_eq!(cwd("/Cx/y").1, "/Cx/y");
}

/// A cut path is a different path, not a shorter one.
#[test]
fn osc_7_refuses_a_truncated_payload() {
    let mut session = Session::new(20, 4);
    let url = "x".repeat(crate::parser::OSC_INLINE * 2);
    let stats = session.feed(format!("\x1b]7;file:///{url}\x07").as_bytes());
    assert_eq!(stats.truncated_osc, 1);
    assert_eq!(stats.unhandled_sequences, 1);
    assert_eq!(cwd_of(&session), None);
}

fn progress_of(session: &Session) -> Option<Progress> {
    session.batch.iter().find_map(|event| match event {
        VtEvent::Progress(progress) => Some(*progress),
        _ => None,
    })
}

fn progress(sequence: &str) -> Option<Progress> {
    let mut session = Session::new(20, 4);
    session.feed(format!("\x1b]{sequence}\x07").as_bytes());
    progress_of(&session)
}

#[test]
fn osc_9_4_reports_every_progress_state() {
    assert_eq!(progress("9;4;1;42"), Some(Progress::Set(42)));
    assert_eq!(progress("9;4;0"), Some(Progress::Remove));
    assert_eq!(progress("9;4;2;80"), Some(Progress::Error(80)));
    assert_eq!(progress("9;4;3"), Some(Progress::Indeterminate));
    assert_eq!(progress("9;4;4;10"), Some(Progress::Paused(10)));
    // The percentage is clamped, and an unknown state is dropped and counted.
    assert_eq!(progress("9;4;1;250"), Some(Progress::Set(100)));
    let mut session = Session::new(20, 4);
    let stats = session.feed(b"\x1b]9;4;9;50\x07");
    assert_eq!(progress_of(&session), None);
    assert_eq!(stats.unhandled_sequences, 1);
}

fn notification(sequence: &str) -> Option<(String, String)> {
    let mut session = Session::new(20, 4);
    session.feed(format!("\x1b]{sequence}\x07").as_bytes());
    session.batch.iter().find_map(|event| match event {
        VtEvent::Notification { title, body } => Some((
            session.batch.str(*title).to_owned(),
            session.batch.str(*body).to_owned(),
        )),
        _ => None,
    })
}

#[test]
fn osc_9_reports_a_notification_with_its_semicolons_rejoined() {
    assert_eq!(
        notification("9;Build finished"),
        Some((String::new(), "Build finished".into()))
    );
    assert_eq!(
        notification("9;done: 3 tests; 0 failed"),
        Some((String::new(), "done: 3 tests; 0 failed".into()))
    );
    // A message that merely starts with `7` is a message. The engine knows
    // nothing about any sub-code here.
    assert_eq!(
        notification("9;71 bottles"),
        Some((String::new(), "71 bottles".into()))
    );
}

fn shell_mark(sequence: &str) -> Option<ShellMark> {
    let mut session = Session::new(20, 4);
    session.feed(format!("\x1b]{sequence}\x07").as_bytes());
    session.batch.iter().find_map(|event| match event {
        VtEvent::ShellMark(mark) => Some(*mark),
        _ => None,
    })
}

#[test]
fn osc_133_reports_every_marker() {
    assert_eq!(shell_mark("133;A"), Some(ShellMark::PromptStart));
    assert_eq!(shell_mark("133;B"), Some(ShellMark::PromptEnd));
    assert_eq!(shell_mark("133;C"), Some(ShellMark::OutputStart));
    assert_eq!(
        shell_mark("133;D"),
        Some(ShellMark::OutputEnd { exit_code: None })
    );
    assert_eq!(
        shell_mark("133;D;0"),
        Some(ShellMark::OutputEnd { exit_code: Some(0) })
    );
    assert_eq!(
        shell_mark("133;D;127"),
        Some(ShellMark::OutputEnd {
            exit_code: Some(127)
        })
    );
    // A `D` with something that is not a number keeps the marker and loses the
    // code, which is what the adapter did.
    assert_eq!(
        shell_mark("133;D;not-a-number"),
        Some(ShellMark::OutputEnd { exit_code: None })
    );
    // An unknown sub-code is dropped and counted.
    assert_eq!(shell_mark("133;X"), None);
    assert_eq!(shell_mark("133;Z;foo"), None);
    let mut session = Session::new(20, 4);
    assert_eq!(session.feed(b"\x1b]133;X\x07").unhandled_sequences, 1);
}

#[test]
fn osc_1_reports_the_icon_name() {
    let mut session = Session::new(20, 4);
    let stats = session.feed(b"\x1b]1;  shell  \x07");
    assert_eq!(stats.unhandled_sequences, 0);
    assert!(session.batch.iter().any(
        |event| matches!(event, VtEvent::IconName(span) if session.batch.str(*span) == "shell")
    ));
    assert_eq!(session.term.title(), None, "an icon name is not a title");
}

#[test]
fn osc_50_reports_that_the_cursor_shape_changed() {
    let mut session = Session::new(20, 4);
    session.feed(b"\x1b]50;CursorShape=1\x07");
    assert_eq!(session.term.cursor_style().shape, CursorShape::Beam);
    assert!(
        session
            .batch
            .iter()
            .any(|event| matches!(event, VtEvent::CursorStyleChanged))
    );
}

/// The worked example, and the packet's real proof: OneTerm's agent channel and
/// its deprecated `OSC 9;7` alias both work through the routing table alone,
/// with **no** knowledge of either number in this crate.
#[test]
fn an_embedder_private_osc_number_and_a_wrapped_builtin_both_work() {
    // What `crates/terminal`'s `adapter_config` builds, spelled out.
    let mut routes = OscRoutes::new();
    routes.route(20308, OscRoute::Forward).large(20308, true);
    routes.route(9, OscRoute::BuiltinAndForward).large(9, true);
    let config = Config {
        osc_routes: routes,
        ..Config::default()
    };

    // The private number: forwarded raw, sub-code and all, and never parsed.
    let mut session = Session::with(Terminal::new(Size { rows: 4, cols: 20 }, config.clone()));
    session.feed(b"\x1b]20308;1;agent-status\x07");
    assert_eq!(
        forwarded_osc(&session, 20308).expect("a forwarded OSC reaches the batch"),
        vec![b"20308".to_vec(), b"1".to_vec(), b"agent-status".to_vec()]
    );
    let mut session = Session::with(Terminal::new(Size { rows: 4, cols: 20 }, config.clone()));
    session.feed(b"\x1b]20308;0\x07");
    assert_eq!(
        forwarded_osc(&session, 20308).expect("the support query reaches the batch"),
        vec![b"20308".to_vec(), b"0".to_vec()]
    );

    // The wrapped built-in: the engine parses `9;7` as the notification it
    // looks like *and* hands over the bytes, so the embedder can recognise its
    // own alias and discard the notification. The engine has no opinion.
    let mut session = Session::with(Terminal::new(Size { rows: 4, cols: 20 }, config));
    session.feed(b"\x1b]9;7;agent-status\x07");
    assert_eq!(
        forwarded_osc(&session, 9).expect("the wrapped sequence reaches the batch"),
        vec![b"9".to_vec(), b"7".to_vec(), b"agent-status".to_vec()]
    );
    let kinds: Vec<&VtEvent> = session
        .batch
        .iter()
        .filter(|event| !matches!(event, VtEvent::Repaint))
        .collect();
    assert_eq!(kinds.len(), 2);
    assert!(matches!(kinds[0], VtEvent::Notification { .. }));
    assert!(matches!(kinds[1], VtEvent::Osc { code: 9, .. }));
}

/// Hostile bytes plus a hostile table: neither may panic, and the counters a
/// `feed` reports may only go up within the batch it reports on.
#[test]
fn arbitrary_bytes_and_an_arbitrary_table_never_panic() {
    let mut rng: u64 = 0x5EED_1234_ABCD_0003;
    let mut next = move || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    const ROUTES: [OscRoute; 4] = [
        OscRoute::Builtin,
        OscRoute::BuiltinAndForward,
        OscRoute::Forward,
        OscRoute::Drop,
    ];
    const BIAS: &[u8] = b"\x1b[]P_X;:?0123456789m\x07\\";

    for round in 0..64 {
        let mut routes = OscRoutes::new();
        for &code in OscRoutes::BUILTIN.iter().chain([633, 1337, 20308].iter()) {
            let mut route = ROUTES[(next() % 4) as usize];
            // `Builtin` on a number with no built-in is the documented debug
            // assertion, not a table an embedder can build by accident.
            if route == OscRoute::Builtin && !OscRoutes::has_builtin(code) {
                route = OscRoute::Drop;
            }
            routes.route(code, route);
            if next() % 2 == 0 && routes.get(code) != OscRoute::Drop {
                routes.large(code, true);
            }
        }
        let mut session = Session::with(with_routes(20, 4, routes));
        let mut input = vec![0u8; 4096];
        for (index, byte) in input.iter_mut().enumerate() {
            let value = (next() >> 24) as u8;
            *byte = if index % 3 == 0 {
                BIAS[usize::from(value) % BIAS.len()]
            } else {
                value
            };
        }
        for chunk in input.chunks(1 + round * 7) {
            let stats = session.feed(chunk);
            // Counters are per `feed` and count sequences, so none of them can
            // have outrun the bytes this call was given.
            let bytes = chunk.len() as u32;
            assert_eq!(stats.bytes, chunk.len());
            assert!(stats.unhandled_sequences <= bytes);
            assert!(stats.malformed_sequences <= bytes);
            assert!(stats.truncated_osc <= bytes);
            assert!(stats.aborted_dcs <= bytes);
        }
    }
}

#[test]
fn osc_50_sets_the_cursor_shape() {
    for (value, expected) in [
        (b'0', CursorShape::Block),
        (b'1', CursorShape::Beam),
        (b'2', CursorShape::Underline),
    ] {
        let mut session = Session::new(10, 3);
        session.feed(format!("\x1b]50;CursorShape={}\x07", value as char).as_bytes());
        assert_eq!(session.term.cursor_style().shape, expected);
    }
}

#[test]
fn osc_22_reports_the_pointer_shape_by_name() {
    let mut session = Session::new(10, 3);
    let stats = session.feed(b"\x1b]22;pointer\x07");
    assert_eq!(stats.unhandled_sequences, 0);
    assert!(session.batch.iter().any(
        |event| matches!(event, VtEvent::Pointer(span) if session.batch.str(*span) == "pointer")
    ));
    // A shape nobody named is still counted, as it was.
    let mut session = Session::new(10, 3);
    assert_eq!(session.feed(b"\x1b]22\x07").unhandled_sequences, 1);
}

#[test]
fn osc_parameters_past_the_sixteenth_are_re_split() {
    // P9: the parser joins everything past `MAX_OSC_PARAMS` into the last slot
    // rather than discarding it, and `OSC 4` is the sequence that reaches
    // sixteen — nine colours is enough.
    let mut session = Session::new(10, 3);
    let mut request = String::from("\x1b]4");
    for index in 0..12u8 {
        let _ = std::fmt::Write::write_fmt(
            &mut request,
            format_args!(";{index};#{:02x}0000", index + 1),
        );
    }
    request.push('\x07');
    session.feed(request.as_bytes());

    for index in 0..12u8 {
        assert_eq!(
            session.term.color(ColorKey::Palette(index)),
            Some(Rgb {
                r: index + 1,
                g: 0,
                b: 0
            }),
            "index {index}"
        );
    }
}

// ── Charsets, tabs and REP ──────────────────────────────────────────────────

#[test]
fn charsets_are_designated_and_selected() {
    let mut session = Session::new(10, 2);
    // G1 as line drawing, then `SO` selects it.
    session.feed(b"\x1b)0\x0eqqq\x0fqqq");
    assert_eq!(session.row(0), "───qqq    ");

    // G0 as line drawing is the common case.
    let mut session = Session::new(10, 2);
    session.feed(b"\x1b(0lqk\x1b(Blqk");
    assert_eq!(session.row(0), "┌─┐lqk    ");

    // `DECSC` and `DECRC` save the designations but not which one is selected.
    let mut session = Session::new(10, 2);
    session.feed(b"\x1b(0\x1b7\x1b(B\x1b8q");
    assert_eq!(session.row(0), "─         ");
}

#[test]
fn tab_stops_are_set_cleared_and_restored() {
    let mut session = Session::new(24, 2);
    session.feed(b"\x1b[1;4H\x1bH\x1b[1;1H\t");
    assert_eq!(session.cursor(), (0, 3));

    // `TBC 0` clears the stop under the cursor.
    session.feed(b"\x1b[g\x1b[1;1H\t");
    assert_eq!(session.cursor(), (0, 8));

    // `TBC 3` clears them all, so a tab walks to the last column.
    session.feed(b"\x1b[3g\x1b[1;1H\t");
    assert_eq!(session.cursor(), (0, 23));

    // Correction C10: `CSI ? 5 W` puts the every-eighth-column defaults back.
    session.feed(b"\x1b[?5W\x1b[1;1H\t");
    assert_eq!(session.cursor(), (0, 8));
}

#[test]
fn rep_replays_through_the_print_path() {
    // Trap 43: it wraps, honours insert mode and re-triggers wide handling.
    let mut session = Session::new(4, 3);
    session.feed(b"ab\x1b[4b");
    assert_eq!(session.row(0), "abbb");
    assert_eq!(session.row(1), "bb  ");

    // The preceding character survives intervening escape sequences.
    let mut session = Session::new(10, 2);
    session.feed(b"x\x1b[1;5H\x1b[32m\x1b[3b");
    assert_eq!(session.row(0), "x   xxx   ");

    // With nothing printed yet it does nothing.
    let mut session = Session::new(10, 2);
    session.feed(b"\x1b[3b");
    assert_eq!(session.row(0), "          ");
}

// ── Unhandled input ─────────────────────────────────────────────────────────

#[test]
fn unhandled_sequences_are_counted_not_echoed() {
    // R-35: there is no passthrough and no echo buffer.
    let cases: &[&[u8]] = &[
        b"\x1b[99999999z",    // unknown CSI final byte
        b"\x1b\x7b",          // unknown ESC
        b"\x1b]987;body\x07", // unclaimed OSC
        b"\x1bPz;1\x1b\\",    // unknown DCS
        b"\x1b_body\x1b\\",   // APC — no consumer in v1
        b"\x1bXbody\x1b\\",   // SOS
        b"\x1b^body\x1b\\",   // PM
    ];
    for bytes in cases {
        let mut session = Session::new(10, 3);
        let stats = session.feed(bytes);
        assert!(
            stats.unhandled_sequences > 0,
            "{}",
            String::from_utf8_lossy(bytes)
        );
        assert_eq!(session.row(0), "          ");
        assert!(
            !session
                .batch
                .iter()
                .any(|event| matches!(event, VtEvent::Reply(_))),
            "{} produced a reply",
            String::from_utf8_lossy(bytes)
        );
    }

    // `DEL` is an execute no-op, not an unhandled sequence and not a glyph.
    let mut session = Session::new(10, 3);
    let stats = session.feed(b"a\x7fb");
    assert_eq!(session.row(0), "ab        ");
    assert_eq!(stats.unhandled_sequences, 0);
}

// `BUG-0058`: the intermediates are part of the DCS routing key. DECRQSS
// (`DCS $ q`) and XTGETTCAP (`DCS + q`) share the final byte `q` with Sixel,
// and clients such as tmux, neovim and kitty are documented to send the
// latter — routing on the final byte alone fed their payloads to the image
// decoder. This asserts the event batch as well; the wider routing suite is
// `verify_bug0058_tests.rs`.
#[test]
fn an_intermediate_dcs_q_is_not_sixel() {
    for bytes in [&b"\x1bP$qm\x1b\\"[..], &b"\x1bP+q544e\x1b\\"[..]] {
        let mut session = Session::new(10, 3);
        let stats = session.feed(bytes);

        assert!(
            session.term.state.graphics.parser.is_none(),
            "{} opened the Sixel decoder",
            String::from_utf8_lossy(bytes)
        );
        assert_eq!(stats.unhandled_sequences, 1);
        assert_eq!(stats.aborted_dcs, 0);
        assert!(session.term.take_graphics().is_empty());

        let events: Vec<&VtEvent> = session.batch.iter().collect();
        assert!(
            matches!(events.as_slice(), [VtEvent::Repaint]),
            "{} produced {events:?}",
            String::from_utf8_lossy(bytes)
        );
    }
}

#[test]
fn a_bell_reaches_the_batch() {
    let mut session = Session::new(10, 3);
    session.feed(b"a\x07b");
    assert!(session.batch.iter().any(|event| *event == VtEvent::Bell));
    assert_eq!(session.row(0), "ab        ");
}

// ── Line counting and the wrap flag ─────────────────────────────────────────

#[test]
fn lines_produced_counts_output_lines_not_wraps() {
    // R-05: the gutter contract.
    let mut session = Session::new(4, 3);
    session.feed(b"abcdefgh");
    assert_eq!(session.term.lines_produced(), 0);

    session.feed(b"\r\n\r\n");
    assert_eq!(session.term.lines_produced(), 2);

    // `IND` and `NEL` are output lines; an alternate-screen swap is not.
    session.feed(b"\x1bD\x1bE");
    assert_eq!(session.term.lines_produced(), 4);
    session.feed(b"\x1b[?1049h\x1b[?1049l");
    assert_eq!(session.term.lines_produced(), 4);
}

#[test]
fn overwriting_the_last_cell_clears_the_wrap_flag() {
    // Deviation G1 keeps the flag on the row, where the reference keeps it on
    // the last cell and wipes it with any write there. Found by the
    // old-versus-new differential on the `tui_redraw` bench fixture.
    let mut session = Session::new(4, 3);
    session.feed(b"abcdef");
    let first = session.term.screen().row_of_index(0);
    assert!(session.term.screen().row(first).wrapped());

    session.feed(b"\x1b[1;4Hz");
    assert!(!session.term.screen().row(first).wrapped());

    // An erase that reaches the last column clears it too.
    let mut session = Session::new(4, 3);
    session.feed(b"abcdef\x1b[1;2H\x1b[K");
    let first = session.term.screen().row_of_index(0);
    assert!(!session.term.screen().row(first).wrapped());
    assert!(
        !session
            .term
            .screen()
            .row(first)
            .flags()
            .contains(RowFlags::WRAPPED)
    );
}

#[test]
fn releasing_a_leading_wide_spacer_keeps_the_row_above_wrapped() {
    // The `US-0076` verification's M2. A wide glyph that does not fit leaves a
    // `LeadingWideSpacer` in the last column and wraps, so the row is wrapped
    // *and* its last cell is the spacer. Overwriting the pair at the start of
    // the next row releases that spacer — and the reference releases it by
    // clearing one flag bit, which leaves `WRAPLINE` alone. Clearing the row's
    // wrap flag instead splits a wrapped CJK line at the next reflow.
    let mut session = Session::new(4, 3);
    session.feed("abc\u{3042}".as_bytes());

    let first = session.term.screen().row_of_index(0);
    assert_eq!(
        session.cell(0, 3).width(),
        crate::cell::CellWidth::LeadingWideSpacer
    );
    assert!(session.term.screen().row(first).wrapped());
    assert_eq!(session.cell(1, 0).width(), crate::cell::CellWidth::Wide);

    // Repaint over the wide pair at the head of the wrapped row.
    session.feed(b"\x1b[2;1HX");

    assert_eq!(
        session.cell(0, 3).width(),
        crate::cell::CellWidth::Narrow,
        "the spacer is released"
    );
    assert!(
        session.term.screen().row(first).wrapped(),
        "but the row above stays wrapped"
    );
}

#[test]
fn a_fresh_row_is_materialised_blank_not_background_erased() {
    // The `US-0076` verification's M3. `Screen::row_mut` materialised an
    // unwritten ring slot with the cursor's **erase** cell, so one glyph landing
    // on a fresh row repainted every untouched column with the live
    // background-erase colour. Only the `sgr` recording caught it; this pins it
    // where a trimmed corpus cannot lose it.
    let mut session = Session::new(6, 3);
    session.feed(b"\x1b[41m\x1b[2;1HX");

    assert_eq!(
        session.style_at(1, 0).bg,
        Color::Named(NamedColor::Red),
        "the glyph itself carries the template background"
    );
    for col in 1..6 {
        assert_eq!(
            session.style_at(1, col),
            crate::cell::Style::DEFAULT,
            "column {col} was never written and must stay default"
        );
    }

    // A deliberate background erase still fills with the template background,
    // which is the behaviour the fix must not have broken.
    session.feed(b"\x1b[K");
    for col in 1..6 {
        assert_eq!(session.style_at(1, col).bg, Color::Named(NamedColor::Red));
    }
}

// ── Cursor style ────────────────────────────────────────────────────────────

#[test]
fn decscusr_sets_the_shape_and_the_blink() {
    let cases: &[(&[u8], CursorShape, bool)] = &[
        (b"\x1b[1 q", CursorShape::Block, true),
        (b"\x1b[2 q", CursorShape::Block, false),
        (b"\x1b[3 q", CursorShape::Underline, true),
        (b"\x1b[4 q", CursorShape::Underline, false),
        (b"\x1b[5 q", CursorShape::Beam, true),
        (b"\x1b[6 q", CursorShape::Beam, false),
    ];
    for (bytes, shape, blinking) in cases {
        let mut session = Session::new(10, 3);
        session.feed(bytes);
        assert_eq!(
            session.term.cursor_style(),
            CursorStyle {
                shape: *shape,
                blinking: *blinking
            },
            "{}",
            String::from_utf8_lossy(bytes)
        );
    }

    // `DECTCEM` reset reports `Hidden` without losing the configured shape.
    let mut session = Session::new(10, 3);
    session.feed(b"\x1b[4 q\x1b[?25l");
    assert_eq!(session.term.cursor_style().shape, CursorShape::Hidden);
    session.feed(b"\x1b[?25h");
    assert_eq!(session.term.cursor_style().shape, CursorShape::Underline);

    // `? 12` is the blink half, and it survives a shape change.
    let mut session = Session::new(10, 3);
    session.feed(b"\x1b[?12h");
    assert!(session.term.cursor_style().blinking);
    session.feed(b"\x1b[?12$p");
    assert_eq!(session.replies(), "\x1b[?12;1$y");
}

// ── Selection ───────────────────────────────────────────────────────────────

#[test]
fn the_selection_wrappers_carry_the_config_escape_set() {
    let mut session = Session::new(20, 4);
    session.feed(b"hello world\r\nsecond line");

    let top = session.term.viewport().top;
    session.term.selection_start(
        Pos { row: top, col: 0 },
        Side::Left,
        crate::selection::SelectionKind::Simple,
    );
    session
        .term
        .selection_update(Pos { row: top, col: 4 }, Side::Right);

    assert!(session.term.has_selection());
    assert_eq!(session.term.selection_text().as_deref(), Some("hello"));

    // `Semantic` reads the escape set off `Config`, which is where the module's
    // `&str` parameter moves in this packet.
    session.term.selection_start(
        Pos { row: top, col: 8 },
        Side::Left,
        crate::selection::SelectionKind::Semantic,
    );
    assert_eq!(session.term.selection_text().as_deref(), Some("world"));

    session.term.selection_clear();
    assert!(!session.term.has_selection());
    assert_eq!(session.term.selection_range(), None);

    // `select_all` spans the whole live range.
    session.term.select_all();
    let text = session.term.selection_text().expect("all is selected");
    assert!(text.contains("hello world"));
    assert!(text.contains("second line"));
}

#[test]
fn hit_test_maps_a_pointer_to_a_cell_and_a_side() {
    let session = Session::new(20, 4);
    let top = session.term.viewport().top;

    assert_eq!(
        session.term.hit_test(2.0, 3.2),
        (
            Pos {
                row: top + 2,
                col: 3
            },
            Side::Left
        )
    );
    assert_eq!(
        session.term.hit_test(2.9, 3.7),
        (
            Pos {
                row: top + 2,
                col: 3
            },
            Side::Right
        )
    );
    // Out of range clamps rather than panicking: the coordinate comes from a
    // hit test on a viewport that may since have been resized. The side is the
    // fraction of the raw coordinate, which is `selection::hit_test`'s rule —
    // this wrapper delegates rather than keeping a second opinion (M5).
    assert_eq!(
        session.term.hit_test(-5.0, 999.0),
        (Pos { row: top, col: 19 }, Side::Left)
    );
    assert_eq!(
        session.term.hit_test(0.0, 3.2),
        crate::selection::hit_test(session.term.grid(), 0.0, 3.2)
    );
}

#[test]
fn the_invalidation_predicate_runs_before_the_operation() {
    // The matrix is stated against the **pre-operation** grid, so an erase that
    // covers the selection must clear it — which is only decidable before the
    // rows are blanked.
    let select_then = |bytes: &[u8]| -> bool {
        let mut session = Session::new(20, 4);
        session.feed(b"hello\r\nworld\r\n");
        let top = session.term.viewport().top;
        session.term.selection_start(
            Pos { row: top, col: 0 },
            Side::Left,
            crate::selection::SelectionKind::Simple,
        );
        session
            .term
            .selection_update(Pos { row: top, col: 4 }, Side::Right);
        assert!(session.term.has_selection());
        session.feed(bytes);
        session.term.has_selection()
    };

    // The selection is on row 0; the cursor is on row 2.
    assert!(!select_then(b"\x1b[2J"), "ED 2 must clear it");
    assert!(!select_then(b"\x1bc"), "RIS must clear it");
    assert!(!select_then(b"\x1b[?1049h"), "an alt swap must clear it");
    assert!(!select_then(b"\x1b#8"), "DECALN must clear it");
    assert!(!select_then(b"\x1b[?3h"), "DECCOLM must clear it");
    // `ED 1` clears from the top of the screen down to the cursor, so it covers
    // row 0 as well.
    assert!(!select_then(b"\x1b[1J"), "ED 1 must clear it");
    // An erase that cannot reach row 0 leaves it alone.
    assert!(select_then(b"\x1b[K"), "EL on row 2 must not clear it");
    assert!(select_then(b"\x1b[0J"), "ED 0 from row 2 must not clear it");
    assert!(select_then(b"\x1b[3J"), "ED 3 has no history to clear");
}

// ── DECALN ──────────────────────────────────────────────────────────────────

#[test]
fn decaln_fills_the_screen_with_default_styled_e() {
    let mut session = Session::new(4, 2);
    session.feed(b"\x1b[41m\x1b[2;3H\x1b#8");

    assert_eq!(session.row(0), "EEEE");
    assert_eq!(session.row(1), "EEEE");
    // The reference's `Cell::default()`: the SGR template is deliberately
    // ignored, and the cursor does not move.
    assert_eq!(session.style_at(0, 0), crate::cell::Style::DEFAULT);
    assert_eq!(session.cursor(), (1, 2));
}
