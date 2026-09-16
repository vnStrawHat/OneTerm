//! Final re-check of `US-0106` at `a22036db`: the cases the rework created.
//!
//! The adopted `verify_us0106.rs` covers the twelve notes; this file attacks the
//! four engine changes from angles that file does not: a rectangle wholly
//! outside the region under `DECOM`, `DECOM` off, clusters with more than one
//! combining mark, a wide character carrying marks, and the partially-outside
//! rectangle that is the one case where the clamp deviates from xterm's reject.

use std::time::Instant;

use oneterm_vt::{Config, EventBatch, Size, Terminal, VtEvent};

struct Q {
    term: Terminal,
    batch: EventBatch,
}

impl Q {
    fn open(rows: u16, cols: u16) -> Q {
        Q {
            term: Terminal::new(
                Size { rows, cols },
                Config {
                    allow_screen_readback: true,
                    ..Config::default()
                },
            ),
            batch: EventBatch::new(),
        }
    }

    fn feed(&mut self, bytes: &[u8]) -> oneterm_vt::FeedStats {
        self.batch.clear();
        self.term.feed(bytes, &mut self.batch, Instant::now())
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
}

fn cksum(id: u16, sum: u32) -> String {
    format!("\x1bP{id}!~{:04X}\x1b\\", sum & 0xffff)
}

/// A 4x8 grid with `A`,`B`,`C`,`D` down column 1 and the region set to rows 2-3.
fn rows_abcd_region_two_three(origin: bool) -> Q {
    let mut q = Q::open(4, 8);
    q.feed(b"\x1b[1;1HA\x1b[2;1HB\x1b[3;1HC\x1b[4;1HD");
    q.feed(b"\x1b[2;3r");
    if origin {
        q.feed(b"\x1b[?6h");
    }
    q
}

/// G1. A rectangle whose rows are wholly past the region, under `DECOM`, is
/// empty -- it does not spill onto the rows below the region.
#[test]
fn decom_rectangle_past_the_region_is_empty() {
    let mut q = rows_abcd_region_two_three(true);
    // Region rows 4-6 do not exist; the region is two rows tall.
    for request in [
        &b"\x1b[1;0;4;1;5;8*y"[..],
        &b"\x1b[1;0;3;1;9;8*y"[..],
        &b"\x1b[1;0;65535;1;65535;8*y"[..],
    ] {
        q.feed(request);
        assert_eq!(
            q.replies(),
            cksum(1, 0),
            "{:?} escaped the region",
            String::from_utf8_lossy(request)
        );
    }
    // Nothing below the region leaked in: `D` (0x44) never appears.
    q.feed(b"\x1b[1;0;1;1;99;99*y");
    assert_eq!(q.replies(), cksum(1, 0x42 + 0x43 + 14 * 0x20));
}

/// G2. With `DECOM` off, the same region leaves the page alone: the whole
/// screen is addressable and every row is reachable.
#[test]
fn decom_off_leaves_the_whole_screen_addressable() {
    let mut q = rows_abcd_region_two_three(false);
    let whole = 0x41 + 0x42 + 0x43 + 0x44 + 28 * 0x20;
    q.feed(b"\x1b[1;0;1;1;99;99*y");
    assert_eq!(
        q.replies(),
        cksum(1, whole),
        "clamped to the region with DECOM off"
    );
    q.feed(b"\x1b[1;0*y");
    assert_eq!(q.replies(), cksum(1, whole), "defaults are not the region");
    // Row 4 -- outside the region -- is still individually readable.
    q.feed(b"\x1b[1;0;4;1;4;1*y");
    assert_eq!(q.replies(), cksum(1, 0x44));
}

/// G3. Setting and then resetting `DECOM` restores the page in both
/// directions, and a region reset while `DECOM` is on widens the page.
#[test]
fn decom_and_the_region_move_the_page_both_ways() {
    let mut q = rows_abcd_region_two_three(true);
    let region_only = 0x42 + 0x43 + 14 * 0x20;
    let whole = 0x41 + 0x42 + 0x43 + 0x44 + 28 * 0x20;

    q.feed(b"\x1b[1;0*y");
    assert_eq!(q.replies(), cksum(1, region_only));

    // `CSI r` resets the region to the whole screen; `DECOM` is still set, so
    // the page is the screen again.
    q.feed(b"\x1b[r");
    q.feed(b"\x1b[1;0*y");
    assert_eq!(
        q.replies(),
        cksum(1, whole),
        "a reset region did not widen the page"
    );

    // Narrow it again, then drop `DECOM`.
    q.feed(b"\x1b[2;3r");
    q.feed(b"\x1b[1;0*y");
    assert_eq!(q.replies(), cksum(1, region_only));
    q.feed(b"\x1b[?6l");
    q.feed(b"\x1b[1;0*y");
    assert_eq!(q.replies(), cksum(1, whole));
}

/// G4. Three combining marks on one base character all count, in the order
/// xterm's `combData` walk would add them (order is irrelevant to a sum, which
/// is the point -- it cannot hide a dropped mark).
#[test]
fn every_combining_mark_of_a_long_cluster_counts() {
    let mut q = Q::open(4, 8);
    q.feed(b"\x1b[?2027h");
    q.feed("e\u{301}\u{302}\u{303}".as_bytes());
    q.feed(b"\x1b[1;0;1;1;1;1*y");
    assert_eq!(q.replies(), cksum(1, 0x65 + 0x301 + 0x302 + 0x303));

    // A second cell beside it is still a plain blank: the cluster did not
    // spill.
    q.feed(b"\x1b[1;0;1;2;1;2*y");
    assert_eq!(q.replies(), cksum(1, 0x20));
}

/// G5. A double-width base character with combining marks: the glyph cell
/// carries every scalar, the spacer carries a space, and the pair sums to both.
#[test]
fn a_wide_character_with_marks_sums_the_glyph_cell_only() {
    let mut q = Q::open(4, 8);
    q.feed(b"\x1b[?2027h");
    q.feed("\u{4e16}\u{301}\u{302}".as_bytes());

    let glyph = 0x4e16 + 0x301 + 0x302;
    q.feed(b"\x1b[1;0;1;1;1;1*y");
    assert_eq!(q.replies(), cksum(1, glyph), "the glyph column");
    q.feed(b"\x1b[1;0;1;2;1;2*y");
    assert_eq!(q.replies(), cksum(1, 0x20), "the spacer column");
    q.feed(b"\x1b[1;0;1;1;1;2*y");
    assert_eq!(q.replies(), cksum(1, glyph + 0x20), "both columns");
}

/// G6. A cluster that is erased goes back to counting as `U+0020`, marks and
/// all -- no residue in the interner reaches the checksum.
#[test]
fn an_erased_cluster_counts_as_a_blank_again() {
    let mut q = Q::open(4, 8);
    q.feed(b"\x1b[?2027h");
    q.feed("e\u{301}\u{302}".as_bytes());
    q.feed(b"\x1b[1;0;1;1;1;1*y");
    assert_eq!(q.replies(), cksum(1, 0x65 + 0x301 + 0x302));

    q.feed(b"\x1b[H\x1b[2J");
    q.feed(b"\x1b[1;0;1;1;1;1*y");
    assert_eq!(q.replies(), cksum(1, 0x20));
}

/// G7. The documented deviation, isolated: a **partially** outside rectangle is
/// clamped to the page and answers the part that exists, where xterm's
/// `validRect` would reject it and answer `0000`. A **wholly** outside one
/// answers `0000` in both, which is why only this case can tell them apart.
#[test]
fn a_partially_outside_rectangle_is_clamped_not_rejected() {
    let mut q = Q::open(4, 8);
    q.feed(b"ABCDEFGH");
    let row_one = (0x41..=0x48).sum::<u32>();

    // Rows 1-99 of an eight-column, four-row screen: the three blank rows below
    // row one are included, the ninety-five that do not exist are not.
    q.feed(b"\x1b[1;0;1;1;99;8*y");
    assert_eq!(
        q.replies(),
        cksum(1, row_one + 24 * 0x20),
        "a partially outside rectangle was rejected rather than clamped"
    );

    // Columns 1-99 of row one, the same story on the other axis.
    q.feed(b"\x1b[1;0;1;1;1;99*y");
    assert_eq!(q.replies(), cksum(1, row_one));

    // Wholly outside: `0000`, which agrees with xterm.
    q.feed(b"\x1b[1;0;5;9;9;12*y");
    assert_eq!(q.replies(), cksum(1, 0));
}

/// G8. The `DECRQSS` SGR round trip still holds after the rework, including the
/// underline colour that finding 9 touched.
#[test]
fn sgr_still_round_trips_after_the_rework() {
    for sgr in [
        &b"\x1b[1;38;5;196;48;2;10;20;30;4:3;58;5;99m"[..],
        &b"\x1b[4:4;58;2;1;2;3m"[..],
        &b"\x1b[0m"[..],
    ] {
        let mut a = Q::open(4, 8);
        a.feed(sgr);
        let want = a.term.style();
        a.feed(b"\x1bP$qm\x1b\\");
        let reply = a.replies();
        let payload = reply
            .strip_prefix("\x1bP1$r")
            .and_then(|body| body.strip_suffix("\x1b\\"))
            .unwrap_or_else(|| panic!("{reply:?}"));

        let mut b = Q::open(4, 8);
        b.feed(format!("\x1b[{payload}").as_bytes());
        assert_eq!(
            b.term.style(),
            want,
            "{:?} answered {payload:?}",
            String::from_utf8_lossy(sgr)
        );
    }
}
