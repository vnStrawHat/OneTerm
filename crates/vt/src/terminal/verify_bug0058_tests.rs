//! Independent verification tests for `BUG-0058` (DCS intermediates routing).
//!
//! Written by the verifier, not by the implementer, and kept in their own file
//! so the two suites stay independently readable. Six of the eight fail against
//! the pre-fix `dcs_hook`; the other two are the non-regression guards for the
//! Sixel half of the routing key. The verification report they come from is
//! `docs/spec-intakes/IN-0038-embeddable-vt-core/evidence/BUG-0058-verify.md`.

use std::time::Instant;

use super::*;
use crate::grid::Size;

struct Vs {
    term: Terminal,
    batch: EventBatch,
    now: Instant,
}

impl Vs {
    fn new() -> Vs {
        Vs {
            term: Terminal::new(Size { rows: 6, cols: 20 }, Config::default()),
            batch: EventBatch::new(),
            now: Instant::now(),
        }
    }

    fn feed(&mut self, bytes: &[u8]) -> FeedStats {
        self.term.feed(bytes, &mut self.batch, self.now)
    }

    fn decoder_live(&self) -> bool {
        self.term.state.graphics.parser.is_some()
    }
}

/// `DCS $ q m ST` (DECRQSS) and `DCS + q 544e ST` (XTGETTCAP) must not open the
/// Sixel decoder, must be counted once, and must place nothing.
#[test]
fn verify_intermediate_dcs_q_never_reaches_the_decoder() {
    for bytes in [&b"\x1bP$qm\x1b\\"[..], &b"\x1bP+q544e\x1b\\"[..]] {
        let mut vs = Vs::new();
        let stats = vs.feed(bytes);

        assert!(
            !vs.decoder_live(),
            "{:?} opened a Sixel decoder",
            String::from_utf8_lossy(bytes)
        );
        assert_eq!(
            stats.unhandled_sequences,
            1,
            "{:?} unhandled count",
            String::from_utf8_lossy(bytes)
        );
        assert_eq!(stats.aborted_dcs, 0);
        assert!(vs.term.state.graphics.pending.is_empty());
        assert!(vs.term.placements().is_empty());
        assert!(vs.term.take_graphics().is_empty());
        // Nothing was echoed to the grid either.
        assert_eq!(
            vs.term.row_text(vs.term.screen().row_of_index(0)).trim(),
            ""
        );
    }
}

/// The other half of the routing key: a bare `DCS q` is still Sixel.
#[test]
fn verify_bare_dcs_q_still_places_one_graphic() {
    let mut vs = Vs::new();
    let stats = vs.feed(b"\x1bPq#0;2;0;0;0#0~\x1b\\");
    assert_eq!(stats.unhandled_sequences, 0);
    assert_eq!(vs.term.placements().len(), 1);
    assert_eq!(vs.term.take_graphics().len(), 1);
}

/// A DCS with **parameters** but no intermediate is still Sixel: the fix must
/// key on intermediates only, never on "the sequence had something before `q`".
#[test]
fn verify_parameterised_dcs_q_still_decodes() {
    for bytes in [
        &b"\x1bP0;1q#0;2;0;0;0#0~\x1b\\"[..],
        &b"\x1bP0;1;0q#0;2;0;0;0#0~\x1b\\"[..],
        &b"\x1bP7;1;0q#0;2;0;0;0#0~\x1b\\"[..],
    ] {
        let mut vs = Vs::new();
        let stats = vs.feed(bytes);
        assert_eq!(
            stats.unhandled_sequences,
            0,
            "{:?} was counted unhandled",
            String::from_utf8_lossy(bytes)
        );
        assert_eq!(
            vs.term.take_graphics().len(),
            1,
            "{:?} did not decode",
            String::from_utf8_lossy(bytes)
        );
    }
}

/// An unterminated `DCS q` followed by an intermediate DCS leaves no in-flight
/// parser, and the trailing `ST` places nothing. This is the implementer's
/// case: the aborted Sixel carried no payload.
#[test]
fn verify_intermediate_dcs_aborts_an_empty_unterminated_sixel() {
    let mut vs = Vs::new();
    vs.feed(b"\x1bPq");
    assert!(vs.decoder_live(), "the bare DCS q should be in flight");

    let stats = vs.feed(b"\x1bP$qm");
    assert!(!vs.decoder_live(), "the DECRQSS left a decoder in flight");
    assert_eq!(stats.unhandled_sequences, 1);

    vs.feed(b"\x1b\\");
    assert!(!vs.decoder_live());
    assert!(vs.term.take_graphics().is_empty());
    assert!(vs.term.placements().is_empty());
}

/// The same shape with a **non-empty** Sixel payload, which pins what the
/// engine actually does: the `ESC` that introduces the next DCS ends the prior
/// one *normally* (`crates/vt/src/parser/state.rs:247` calls
/// `dcs_unhook(false)`), so the partial image is finished and placed -- it is
/// not aborted, and `dcs_hook` never sees a live parser. What matters for
/// `BUG-0058` is that the DECRQSS behind it adds **no second** graphic and
/// leaves no decoder. Behaviour is identical on `main`; this test is what
/// retired the older claim that a non-Sixel DCS "aborts the prior unterminated
/// one", and what lets `dcs_hook` drop the dead `parser = None` store.
#[test]
fn verify_intermediate_dcs_after_a_nonempty_unterminated_sixel() {
    let mut vs = Vs::new();
    vs.feed(b"\x1bPq#0;2;0;0;0#0~");
    assert!(vs.decoder_live());

    let stats = vs.feed(b"\x1bP$qm");
    assert!(!vs.decoder_live(), "the DECRQSS left a decoder in flight");
    assert_eq!(stats.unhandled_sequences, 1);
    assert_eq!(stats.aborted_dcs, 0);
    // The first Sixel was finished by the ESC, not aborted.
    assert_eq!(vs.term.placements().len(), 1);

    vs.feed(b"\x1b\\");
    assert!(!vs.decoder_live());
    assert_eq!(
        vs.term.take_graphics().len(),
        1,
        "the DECRQSS must not add a graphic of its own"
    );
    assert_eq!(vs.term.placements().len(), 1);
}

/// A 1 MiB payload behind an intermediate DCS grows no image buffer and does
/// not panic. The decoder is checked mid-stream, so a buffer that filled and
/// was later dropped would still be caught.
#[test]
fn verify_one_mib_intermediate_payload_buffers_nothing() {
    let mut vs = Vs::new();
    let mut stats = vs.feed(b"\x1bP$q");
    assert!(!vs.decoder_live(), "the hook already opened a decoder");

    let chunk = vec![b'~'; 64 * 1024];
    for _ in 0..16 {
        let s = vs.feed(&chunk);
        stats.unhandled_sequences += s.unhandled_sequences;
        stats.aborted_dcs += s.aborted_dcs;
        assert!(!vs.decoder_live(), "a decoder appeared mid-payload");
        assert!(vs.term.state.graphics.pending.is_empty());
    }

    let s = vs.feed(b"\x1b\\");
    stats.unhandled_sequences += s.unhandled_sequences;
    stats.aborted_dcs += s.aborted_dcs;

    assert_eq!(stats.unhandled_sequences, 1, "counted once, not per byte");
    assert_eq!(stats.aborted_dcs, 0, "1 MiB is under DCS_MAX_BYTES");
    assert!(!vs.decoder_live());
    assert!(vs.term.state.graphics.pending.is_empty());
    assert!(vs.term.placements().is_empty());
    assert!(vs.term.take_graphics().is_empty());
    assert_eq!(
        vs.term.row_text(vs.term.screen().row_of_index(0)).trim(),
        ""
    );
}

/// 8-bit `ST` (`0x9C`) ends an intermediate DCS just as `ESC \` does, and the
/// terminal is back in ground state afterwards.
#[test]
fn verify_eight_bit_st_ends_an_intermediate_dcs() {
    let mut vs = Vs::new();
    let stats = vs.feed(b"\x1bP$qm\x9c");
    assert_eq!(stats.unhandled_sequences, 1);
    assert_eq!(stats.aborted_dcs, 0);
    assert!(!vs.decoder_live());

    // Ground state: the next printable byte lands on the grid instead of being
    // swallowed as DCS payload.
    vs.feed(b"A");
    assert_eq!(
        vs.term.row_text(vs.term.screen().row_of_index(0)).trim(),
        "A"
    );
    assert!(vs.term.take_graphics().is_empty());
}

/// An intermediate DCS whose intermediates overflow the parser's cap must not
/// fall back to the Sixel branch: the collected slice stays non-empty.
#[test]
fn verify_overflowed_intermediates_do_not_fall_back_to_sixel() {
    let mut vs = Vs::new();
    let stats = vs.feed(b"\x1bP$+!q~~~\x1b\\");
    assert!(!vs.decoder_live());
    assert_eq!(stats.unhandled_sequences, 1);
    assert!(vs.term.take_graphics().is_empty());
}
