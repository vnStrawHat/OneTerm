//! `US-0087`'s two cleanup rows, driven from the wire.
//!
//! Written by the packet's independent verifier and adopted on their finding:
//! every case here goes through the public API with real bytes, so none of it
//! reuses the unit suites' fixtures, and the cases the unit tests reach only
//! indirectly — a `DECSTBM` top that is not row 0, a cursor parked *above* the
//! region, `DECRQM` consistency across `h` and `l`, and that `LNM` really is
//! inert — are pinned here instead of argued in a comment.

use std::time::Instant;

use oneterm_vt::{Config, EventBatch, Mode, Size, Terminal, VtEvent};

struct Wire {
    term: Terminal,
    batch: EventBatch,
    now: Instant,
}

impl Wire {
    fn new(cols: u16, rows: u16) -> Wire {
        Wire {
            term: Terminal::new(Size { rows, cols }, Config::default()),
            batch: EventBatch::new(),
            now: Instant::now(),
        }
    }

    fn feed(&mut self, bytes: &[u8]) {
        self.term.feed(bytes, &mut self.batch, self.now);
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

    fn cursor(&self) -> (u16, u16) {
        (
            self.term.screen().cursor_row_index(),
            self.term.screen().cursor().pos.col,
        )
    }

    fn row(&self, index: u16) -> String {
        let id = self.term.screen().row_of_index(index);
        self.term.row_text(id)
    }
}

/// (a) A `DECSTBM` region whose top is not row 0: `BS` with `? 45` set at the
/// region's top row, column 0, must not cross into the row above the region.
#[test]
fn verify_reverse_wrap_blocked_at_a_non_zero_region_top() {
    let mut w = Wire::new(8, 6);
    w.feed(b"\x1b[?45h");
    // Nine glyphs at eight columns: row 0 is WRAPPED and row 1 holds `i`.
    w.feed(b"abcdefghi");
    assert_eq!(w.row(0).trim_end(), "abcdefgh");
    assert_eq!(w.row(1).trim_end(), "i");

    // Rows 2..5 (1-based) == indices 1..5. Row 1 is the region's top.
    w.feed(b"\x1b[2;5r");
    w.feed(b"\x1b[2;1H"); // index 1, column 0
    assert_eq!(w.cursor(), (1, 0));
    w.feed(b"\x08");
    assert_eq!(
        w.cursor(),
        (1, 0),
        "BS at the region top must not cross above the region"
    );
}

/// (b) With the region dropped back to the whole screen the same `BS` crosses.
#[test]
fn verify_reverse_wrap_crosses_once_the_region_is_dropped() {
    let mut w = Wire::new(8, 6);
    w.feed(b"\x1b[?45h");
    w.feed(b"abcdefghi");
    w.feed(b"\x1b[2;5r");
    w.feed(b"\x1b[2;1H");
    w.feed(b"\x08");
    assert_eq!(w.cursor(), (1, 0), "still blocked while the region stands");

    w.feed(b"\x1b[r"); // DECSTBM reset: full screen
    w.feed(b"\x1b[2;1H");
    w.feed(b"\x08");
    assert_eq!(
        w.cursor(),
        (0, 7),
        "with no region the crossing lands on the wrapped row's last column"
    );
}

/// Probe, not a promise: a cursor parked ABOVE the region cannot reverse-wrap
/// either. Pins the behaviour the new guard actually has.
#[test]
fn verify_reverse_wrap_above_the_region_is_also_blocked() {
    let mut w = Wire::new(8, 6);
    w.feed(b"\x1b[?45h");
    w.feed(b"abcdefghi");
    w.feed(b"\x1b[3;6r"); // indices 2..6; row 1 is OUTSIDE, above the region
    w.feed(b"\x1b[2;1H");
    w.feed(b"\x08");
    assert_eq!(
        w.cursor(),
        (1, 0),
        "outside-the-region cursor does not reverse-wrap under the new guard"
    );
}

/// (c) `DECRQM` for LNM answers the same thing after `h` and after `l` — the
/// inert-mode contract: never `Set` for a mode nothing reads.
#[test]
fn verify_decrqm_lnm_is_consistent_and_never_claims_set() {
    let mut w = Wire::new(10, 3);
    w.feed(b"\x1b[20$p");
    assert_eq!(w.replies(), "\x1b[20;2$y", "LNM before any h/l");

    let mut w = Wire::new(10, 3);
    w.feed(b"\x1b[20h\x1b[20$p");
    assert_eq!(w.replies(), "\x1b[20;2$y", "LNM after `CSI 20 h`");

    let mut w = Wire::new(10, 3);
    w.feed(b"\x1b[20l\x1b[20$p");
    assert_eq!(w.replies(), "\x1b[20;2$y", "LNM after `CSI 20 l`");

    // The mode that DOES have a reader still answers its real state.
    let mut w = Wire::new(10, 3);
    w.feed(b"\x1b[4h\x1b[4$p");
    assert_eq!(w.replies(), "\x1b[4;1$y", "IRM has a reader");
}

/// (c, second half) The answer is honest: LNM set does NOT turn `LF` into
/// `CR`+`LF` (deviation D9), which is exactly why `DECRQM` must not say `Set`.
#[test]
fn verify_lnm_does_not_make_lf_imply_cr() {
    let mut w = Wire::new(10, 3);
    w.feed(b"\x1b[20h");
    w.feed(b"ab\n");
    assert_eq!(
        w.cursor(),
        (1, 2),
        "LNM is inert: LF moves down only, the column is kept"
    );
}

/// Nothing was lost when LNM joined the table: every code `Mode::from_ansi`
/// recognises is in the `Mode::ANSI` walk the DECRQM test drives.
#[test]
fn verify_mode_ansi_walk_covers_every_recognised_ansi_code() {
    for code in 0u16..=2000 {
        if let Some(mode) = Mode::from_ansi(code) {
            assert!(
                Mode::ANSI.iter().any(|&(m, c)| m == mode && c == code),
                "ANSI mode {code} ({mode:?}) is recognised but not in the Mode::ANSI walk"
            );
        }
    }
    for (mode, code) in Mode::ANSI {
        assert_eq!(
            Mode::from_ansi(code),
            Some(mode),
            "Mode::ANSI lists {code} but from_ansi disagrees"
        );
    }
}
