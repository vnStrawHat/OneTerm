//! Independent re-verification of the `US-0102` rework (`IN-0038`) at
//! `6394f5b9`: the cross-chunk cluster carry (F1), the `cluster_width` base
//! guard (F11), the encoder's "no protocol, no bytes" rule (F2) and `DECSC` /
//! `DECRC` saving the shifts (F4).
//!
//! Written against the public API, and deliberately separate from the suite the
//! implementer adopted.

use std::time::Instant;

use oneterm_vt::input::{
    MouseModifiers, TerminalMouseButton, encode_mouse_move, encode_mouse_press,
    encode_mouse_release, encode_wheel_event,
};
use oneterm_vt::{Config, EventBatch, ResizePolicy, Size, Terminal};

const FAMILY: &str = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";
const SKIN: &str = "\u{1F44D}\u{1F3FD}";
const KEYCAP: &str = "1\u{FE0F}\u{20E3}";
const FLAG: &str = "\u{1F1E9}\u{1F1EA}";
const ACUTE: &str = "e\u{301}";

struct Run {
    term: Terminal,
    batch: EventBatch,
    carries: u32,
}

impl Run {
    fn new(rows: u16, cols: u16) -> Run {
        Run {
            term: Terminal::new(Size { rows, cols }, Config::default()),
            batch: EventBatch::new(),
            carries: 0,
        }
    }

    fn on(rows: u16, cols: u16) -> Run {
        let mut run = Run::new(rows, cols);
        run.feed(b"\x1b[?2027h");
        run
    }

    fn feed(&mut self, bytes: &[u8]) {
        self.batch.clear();
        let stats = self.term.feed(bytes, &mut self.batch, Instant::now());
        self.carries += stats.dropped_cluster_carries;
    }

    fn col(&self) -> u16 {
        self.term.screen().cursor().pos.col
    }

    fn row(&self) -> u16 {
        self.term.screen().cursor_row_index()
    }

    fn text(&self, index: u16) -> String {
        let id = self.term.screen().row_of_index(index);
        self.term.row_text(id).trim_end().to_owned()
    }

    fn active_charset(&self) -> oneterm_vt::grid::Charset {
        self.term.active_charset()
    }
}

/// Feed `text` as `parts` pieces at the given byte boundaries.
fn feed_split(run: &mut Run, text: &str, cuts: &[usize]) {
    let bytes = text.as_bytes();
    let mut start = 0;
    for &cut in cuts {
        run.feed(&bytes[start..cut]);
        start = cut;
    }
    run.feed(&bytes[start..]);
}

// ── F1: the cross-chunk carry ───────────────────────────────────────────────

/// Every interior split point, across two `feed` calls. This is the exact table
/// that was red at `ef9f7c32`.
#[test]
fn w1_every_two_way_split_measures_the_whole_cluster() {
    for (label, text, want_col) in [
        ("family", FAMILY, 2u16),
        ("skin tone", SKIN, 2),
        ("keycap", KEYCAP, 2),
        ("flag", FLAG, 2),
        ("acute", ACUTE, 1),
    ] {
        for cut in 1..text.len() {
            let mut run = Run::on(3, 40);
            feed_split(&mut run, text, &[cut]);
            assert_eq!(run.col(), want_col, "{label} cut after {cut} bytes");
            assert_eq!(run.text(0), text, "{label} cut after {cut} bytes");
        }
    }
}

/// And across three, which is what a slow SSH link actually does.
#[test]
fn w1_every_three_way_split_measures_the_whole_cluster() {
    for (label, text, want_col) in [
        ("family", FAMILY, 2u16),
        ("skin tone", SKIN, 2),
        ("keycap", KEYCAP, 2),
        ("flag", FLAG, 2),
    ] {
        for first in 1..text.len() {
            for second in first + 1..text.len() {
                let mut run = Run::on(3, 40);
                feed_split(&mut run, text, &[first, second]);
                assert_eq!(run.col(), want_col, "{label} cuts {first}/{second}");
                assert_eq!(run.text(0), text, "{label} cuts {first}/{second}");
            }
        }
    }
}

/// One scalar per `feed` is the worst case the carry has to survive.
#[test]
fn w1_one_scalar_per_feed_still_measures_the_whole_cluster() {
    for (label, text, want_col) in [("family", FAMILY, 2u16), ("keycap", KEYCAP, 2)] {
        let mut run = Run::on(3, 40);
        for c in text.chars() {
            let mut buf = [0u8; 4];
            run.feed(c.encode_utf8(&mut buf).as_bytes());
        }
        assert_eq!(run.col(), want_col, "{label}");
        assert_eq!(run.text(0), text, "{label}");
    }
}

/// Anything that is not a print breaks the cluster, so the continuation starts
/// a cluster of its own instead of reaching back into a screen that has moved.
#[test]
fn w1_every_non_print_dispatch_breaks_the_carry() {
    // The head is a wide emoji; the tail is `ZWJ + emoji`, which would have
    // joined it. After a break it must take its own cells.
    let head = "\u{1F468}";
    let tail = "\u{200D}\u{1F469}";
    let joined = {
        let mut run = Run::on(4, 40);
        run.feed(head.as_bytes());
        run.feed(tail.as_bytes());
        run.col()
    };
    assert_eq!(joined, 2, "without a break the two halves are one cluster");

    // Breakers that leave the cursor where it is: the tail's emoji takes two
    // more columns of its own, so the cursor ends at 4 rather than 2.
    for (label, breaker) in [
        ("BEL", &b"\x07"[..]),
        ("CSI SGR", &b"\x1b[0m"[..]),
        ("ESC 7", &b"\x1b7"[..]),
        ("OSC", &b"\x1b]0;t\x07"[..]),
        ("DCS", &b"\x1bP0q\x1b\\"[..]),
        ("APC", &b"\x1b_x\x1b\\"[..]),
        ("mode change", &b"\x1b[?7h"[..]),
        ("alt screen round trip", &b"\x1b[?1049h\x1b[?1049l"[..]),
    ] {
        let mut run = Run::on(4, 40);
        run.feed(head.as_bytes());
        run.feed(breaker);
        run.feed(tail.as_bytes());
        assert_eq!(run.col(), 4, "{label} did not break the carry");
    }

    // Breakers that move the cursor to column 0: the tail starts a cluster
    // there, so it cannot have joined a head two columns to its right.
    for (label, breaker) in [("CR", &b"\r"[..]), ("CUP", &b"\x1b[1;1H"[..])] {
        let mut run = Run::on(4, 40);
        run.feed(head.as_bytes());
        run.feed(breaker);
        run.feed(tail.as_bytes());
        assert_eq!(run.col(), 2, "{label}: the tail is its own cluster at 0");
        assert_eq!(
            run.text(0),
            "\u{1F469}",
            "{label}: the tail overwrote the head rather than joining it"
        );
    }

    // LF moves the cursor, so the tail lands on the next row entirely.
    let mut run = Run::on(4, 40);
    run.feed(head.as_bytes());
    run.feed(b"\n");
    run.feed(tail.as_bytes());
    assert_eq!(run.row(), 1, "LF broke the carry and moved on");
    assert_eq!(run.text(0), head);

    // `resize` is not a dispatch at all, and must break it too.
    let mut run = Run::on(4, 40);
    run.feed(head.as_bytes());
    run.term
        .resize(Size { rows: 4, cols: 30 }, ResizePolicy::BottomAnchor);
    run.feed(tail.as_bytes());
    assert_eq!(run.col(), 4, "resize must break the carry");
}

/// The next chunk starting with a base that does **not** continue: nothing is
/// reprinted and nothing is lost.
#[test]
fn w1_a_non_continuing_next_chunk_reprints_nothing() {
    let mut run = Run::on(3, 40);
    run.feed("\u{1F468}".as_bytes());
    assert_eq!(run.col(), 2);
    run.feed(b"abc");
    assert_eq!(run.col(), 5);
    assert_eq!(run.text(0), "\u{1F468}abc");

    // The same with a second cluster that is itself carried.
    let mut run = Run::on(3, 40);
    run.feed(ACUTE.as_bytes());
    run.feed("o".as_bytes());
    run.feed("\u{308}".as_bytes());
    assert_eq!(run.col(), 2);
    assert_eq!(run.text(0), "e\u{301}o\u{308}");
}

/// A cluster that grows while sitting in the last column, with DECAWM on and
/// off: the reprint must not leave the head behind and must not lose the cell.
#[test]
fn w1_a_growing_cluster_at_the_last_column() {
    // Five columns, four filled, so the head lands in the last one.
    let mut on = Run::on(3, 5);
    on.feed(b"abcd");
    on.feed(b"1");
    assert_eq!(on.text(0), "abcd1");
    on.feed("\u{FE0F}\u{20E3}".as_bytes());
    let rows: Vec<String> = (0..3).map(|i| on.text(i)).collect();
    let seen: String = rows.join("|");
    assert!(
        seen.contains(KEYCAP),
        "the grown keycap must exist exactly once, saw {seen:?}"
    );
    assert_eq!(
        seen.matches('1').count(),
        1,
        "the head must not be left behind as a stray '1': {seen:?}"
    );

    let mut off = Run::on(3, 5);
    off.feed(b"\x1b[?7l");
    off.feed(b"abcd");
    off.feed(b"1");
    off.feed("\u{FE0F}\u{20E3}".as_bytes());
    let seen: String = (0..3).map(|i| off.text(i)).collect::<Vec<_>>().join("|");
    assert_eq!(
        seen.matches('1').count(),
        1,
        "DECAWM off: no stray head either: {seen:?}"
    );
}

/// A cluster that gets **narrower**: a wide base plus VS15. The orphaned half
/// of the wide pair must not survive as a blank hole with a spacer in it.
#[test]
fn w1_a_shrinking_cluster_at_the_last_column() {
    for cols in [5u16, 6] {
        let mut run = Run::on(3, cols);
        let filler = "x".repeat(usize::from(cols) - 2);
        run.feed(filler.as_bytes());
        run.feed("\u{231A}".as_bytes()); // watch, wide by default
        run.feed("\u{FE0E}".as_bytes()); // VS15: ask for the narrow form
        let seen: String = (0..3).map(|i| run.text(i)).collect::<Vec<_>>().join("|");
        assert!(
            seen.contains("\u{231A}\u{FE0E}"),
            "cols {cols}: the narrowed cluster must be whole, saw {seen:?}"
        );
    }
}

/// A wide cluster whose head wrapped: the reprint has to follow it to the row
/// it actually landed on, not the row the run started on.
#[test]
fn w1_a_carry_that_wrapped_is_replaced_where_it_landed() {
    let mut run = Run::on(3, 5);
    run.feed(b"abcd");
    run.feed("\u{1F468}".as_bytes()); // wraps to row 1
    assert_eq!(run.row(), 1);
    run.feed("\u{200D}\u{1F469}".as_bytes());
    assert_eq!(run.text(0), "abcd");
    assert_eq!(run.text(1), "\u{1F468}\u{200D}\u{1F469}");
    assert_eq!(run.col(), 2);
}

/// Scrollback and viewport motion between the two halves must not panic and
/// must not corrupt the row.
#[test]
fn w1_scrollback_and_viewport_changes_between_halves_are_safe() {
    let mut run = Run::on(3, 20);
    run.feed(b"one\r\ntwo\r\nthree\r\nfour\r\n");
    run.feed("\u{1F468}".as_bytes());
    run.term.set_scrollback_limit(0);
    run.feed("\u{200D}\u{1F469}".as_bytes());
    let seen: String = (0..3).map(|i| run.text(i)).collect::<Vec<_>>().join("|");
    assert!(
        seen.contains("\u{1F468}"),
        "the head must still be somewhere: {seen:?}"
    );
}

/// The carry is capped, counted, and linear. A hostile stream can feed one
/// unbounded cluster a scalar at a time.
#[test]
fn w1_the_carry_is_capped_counted_and_linear() {
    fn marks(n: usize) -> (Run, std::time::Duration) {
        let mut run = Run::on(3, 40);
        run.feed(b"a");
        let start = Instant::now();
        for _ in 0..n {
            run.feed("\u{301}".as_bytes());
        }
        let elapsed = start.elapsed();
        (run, elapsed)
    }

    marks(500);
    let (small, small_time) = marks(10_000);
    assert_eq!(small.col(), 1, "one base, one column");
    assert!(small.carries > 0, "the cap must be reported");
    assert!(
        small_time.as_secs() < 10,
        "10k scalar-per-chunk marks took {small_time:?}"
    );

    let (large, large_time) = marks(20_000);
    assert_eq!(large.col(), 1);
    let ratio = large_time.as_secs_f64() / small_time.as_secs_f64().max(1e-6);
    assert!(ratio < 8.0, "20k/10k ratio {ratio:.1} looks super-linear");

    // The cap is 32 scalars: a 32-scalar cluster still carries, a longer one
    // does not.
    let mut short = Run::on(3, 40);
    short.feed(b"a");
    for _ in 0..30 {
        short.feed("\u{301}".as_bytes());
    }
    assert_eq!(short.carries, 0, "31 scalars is inside the cap");
}

/// The charset the head was mapped through is the one the tail is replaced
/// with, even when the locking set changes in between -- and a single shift is
/// consumed exactly once.
#[test]
fn w1_the_carry_remembers_its_charset() {
    // `q` in DEC special graphics is a horizontal line; `U+0301` joins it.
    let mut run = Run::on(3, 20);
    run.feed(b"\x1b(0");
    run.feed(b"q");
    run.feed("\u{301}".as_bytes());
    assert_eq!(run.text(0), "\u{2500}\u{301}", "the head keeps its mapping");
    assert_eq!(run.col(), 1);

    // A single shift covers the head; the tail must not consume a second one.
    let mut run = Run::on(3, 20);
    run.feed(b"\x1b*0\x1bN");
    run.feed(b"q");
    run.feed("\u{301}".as_bytes());
    run.feed(b"q");
    assert_eq!(
        run.text(0),
        "\u{2500}\u{301}q",
        "SS2 covered only the head; the next q is plain ASCII"
    );
    assert_eq!(run.active_charset(), oneterm_vt::grid::Charset::Ascii);
}

/// With the mode reset there is no carry at all, and turning it off mid-stream
/// must not leave one behind.
#[test]
fn w1_no_carry_while_the_mode_is_reset() {
    let mut run = Run::new(3, 40);
    run.feed("\u{1F468}".as_bytes());
    run.feed("\u{200D}\u{1F469}".as_bytes());
    assert_eq!(run.col(), 4, "per scalar, as before");

    // Set, print a head, clear the mode, then send the tail.
    let mut run = Run::on(3, 40);
    run.feed("\u{1F468}".as_bytes());
    run.feed(b"\x1b[?2027l");
    run.feed("\u{200D}\u{1F469}".as_bytes());
    assert_eq!(run.col(), 4, "the tail is measured per scalar");
}

// ── F11: a presentation selector needs a base ───────────────────────────────

#[test]
fn w11_a_bare_variation_selector_is_zero_width() {
    for text in [
        "\u{FE0F}",
        "\u{FE0E}",
        "\u{301}",
        "\u{200D}",
        "\u{20E3}",
        "\u{FE0F}\u{20E3}",
    ] {
        let mut run = Run::on(3, 20);
        run.feed(text.as_bytes());
        run.feed(b"a");
        assert_eq!(run.col(), 1, "{text:?} must take no column of its own");
    }
}

#[test]
fn w11_a_selector_still_decides_the_width_of_a_base_it_follows() {
    // U+2714 HEAVY CHECK MARK is text-default (narrow); VS16 widens it.
    let mut wide = Run::on(3, 20);
    wide.feed("\u{2714}\u{FE0F}".as_bytes());
    assert_eq!(wide.col(), 2, "VS16 widens a text-default base");

    // U+231A WATCH is emoji-default (wide); VS15 narrows it.
    let mut narrow = Run::on(3, 20);
    narrow.feed("\u{231A}\u{FE0E}".as_bytes());
    assert_eq!(narrow.col(), 1, "VS15 narrows an emoji-default base");

    // The documented limitation: VS16 after a non-emoji base widens it too.
    let mut odd = Run::on(3, 20);
    odd.feed("a\u{FE0F}".as_bytes());
    assert_eq!(odd.col(), 2, "known limitation, recorded in width.rs");
}

// ── F2: no protocol, no bytes ───────────────────────────────────────────────

#[test]
fn w2_every_encoder_is_empty_with_no_protocol() {
    for seq in [
        &b""[..],
        &b"\x1b[?9h\x1b[?9l"[..],
        &b"\x1b[?1003h\x1b[?1003l"[..],
        // An encoding without a reporting mode is still no protocol.
        &b"\x1b[?1006h"[..],
        &b"\x1b[?1015h"[..],
    ] {
        let mut run = Run::new(24, 80);
        run.feed(seq);
        let modes = run.term.mode_snapshot();
        assert_eq!(modes.mouse, None, "{:?}", String::from_utf8_lossy(seq));
        let none = MouseModifiers::default();
        let all = MouseModifiers {
            shift: true,
            alt: true,
            ctrl: true,
        };
        for mods in [none, all] {
            assert!(encode_mouse_press(3, 4, TerminalMouseButton::Left, modes, mods).is_empty());
            assert!(encode_mouse_release(3, 4, TerminalMouseButton::Right, modes, mods).is_empty());
            assert!(encode_mouse_move(3, 4, None, modes, mods).is_empty());
            assert!(
                encode_mouse_move(3, 4, Some(TerminalMouseButton::Left), modes, mods).is_empty()
            );
            assert!(encode_wheel_event(3, 4, 1.0, modes, mods).is_empty());
            assert!(encode_wheel_event(3, 4, -1.0, modes, mods).is_empty());
        }
    }
}

#[test]
fn w2_x10_still_reports_the_press_and_only_the_press() {
    let mut run = Run::new(24, 80);
    run.feed(b"\x1b[?9h");
    let modes = run.term.mode_snapshot();
    let none = MouseModifiers::default();
    assert_eq!(
        encode_mouse_press(0, 0, TerminalMouseButton::Left, modes, none),
        vec![0x1b, b'[', b'M', 32, 33, 33]
    );
    assert!(encode_mouse_release(0, 0, TerminalMouseButton::Left, modes, none).is_empty());
    assert!(encode_mouse_move(0, 0, None, modes, none).is_empty());
    assert!(encode_wheel_event(0, 0, 1.0, modes, none).is_empty());

    // And a live protocol still encodes, so the new guard did not silence it.
    run.feed(b"\x1b[?1000h");
    let modes = run.term.mode_snapshot();
    assert!(!encode_mouse_release(0, 0, TerminalMouseButton::Left, modes, none).is_empty());
}

// ── F4: DECSC / DECRC carry the shifts ──────────────────────────────────────

const HLINE: &str = "\u{2500}";

/// VT510 DECSC saves the sets invoked in GL and GR and any pending single
/// shift; DECRC restores them.
#[test]
fn w4_decsc_and_decrc_save_and_restore_the_locking_set() {
    for (save, restore) in [
        (&b"\x1b7"[..], &b"\x1b8"[..]),
        (&b"\x1b[s"[..], &b"\x1b[u"[..]),
        (&b"\x1b[?1048h"[..], &b"\x1b[?1048l"[..]),
    ] {
        let mut run = Run::new(3, 20);
        // Designate G2 as line drawing, lock it, save, drop back to G0,
        // restore: the locking set must come back.
        run.feed(b"\x1b*0\x1bn");
        run.feed(save);
        run.feed(b"\x0f");
        run.feed(restore);
        run.feed(b"q");
        assert_eq!(
            run.text(0),
            HLINE,
            "{} did not restore the locking set",
            String::from_utf8_lossy(save)
        );
    }
}

#[test]
fn w4_decsc_and_decrc_save_and_restore_a_pending_single_shift() {
    // Save with a shift pending, print through it, restore, then step off the
    // restored cursor position (CUP does not consume a shift) and print again:
    // the first character must come from G2 and the second must not.
    let mut run = Run::new(3, 20);
    run.feed(b"\x1b*0\x1bN\x1b7");
    run.feed(b"q"); // consumes the pending shift
    assert_eq!(run.text(0), HLINE);
    run.feed(b"\x1b8"); // DECRC: cursor home again, shift restored
    run.feed(b"\x1b[2;1H");
    run.feed(b"qq");
    assert_eq!(run.text(1), format!("{HLINE}q"), "the shift came back");

    // And saving with none pending restores none.
    let mut run = Run::new(3, 20);
    run.feed(b"\x1b*0\x1b7"); // saved with no shift pending
    run.feed(b"\x1bNq"); // a shift used after the save
    assert_eq!(run.text(0), HLINE);
    run.feed(b"\x1b8");
    run.feed(b"\x1b[2;1H");
    run.feed(b"qq");
    assert_eq!(run.text(1), "qq", "no shift was saved, so none is restored");
}

/// The save slot is per screen: the alternate screen has its own.
#[test]
fn w4_the_save_slot_is_per_screen() {
    let mut run = Run::new(3, 20);
    run.feed(b"\x1b*0\x1bn\x1b7"); // primary saves G2
    run.feed(b"\x1b[?1049h"); // alt screen
    run.feed(b"\x1b(0\x0f\x1b7"); // alt saves G0-as-line-drawing
    run.feed(b"\x1b[?1049l"); // back to primary
    run.feed(b"\x0f"); // SI: G0, which is ASCII here
    run.feed(b"\x1b8q"); // primary's DECRC must restore G2
    assert_eq!(
        run.text(0),
        HLINE,
        "the alternate screen's save must not overwrite the primary's"
    );
}

/// DECRC with no prior DECSC applies the power-on defaults, which for the
/// charset axis means ASCII in GL.
#[test]
fn w4_decrc_without_a_prior_decsc_restores_the_defaults() {
    let mut run = Run::new(3, 20);
    run.feed(b"\x1b*0\x1bn");
    run.feed(b"\x1b8");
    run.feed(b"q");
    assert_eq!(run.text(0), "q", "DECRC with no save falls back to G0");
    assert_eq!(run.active_charset(), oneterm_vt::grid::Charset::Ascii);
}

/// `RIS` clears the save slots too, so a restore after it cannot resurrect a
/// set from before the reset.
#[test]
fn w4_ris_clears_the_save_slots() {
    let mut run = Run::new(3, 20);
    run.feed(b"\x1b*0\x1bn\x1b7");
    run.feed(b"\x1bc");
    run.feed(b"\x1b8q");
    assert_eq!(run.text(0), "q");
}
