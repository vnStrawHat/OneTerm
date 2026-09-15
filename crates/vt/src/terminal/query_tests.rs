//! The exact reply bytes for `DECRQCRA`, `DECRQSS` and `XTGETTCAP`.
//!
//! Every test here feeds the request a real program would send and asserts the
//! answer byte for byte. The reply bytes are a published contract (guide
//! chapter 12, clause 6), so a golden string is the right shape for most of
//! them -- with one deliberate exception, the SGR round trip, which is a
//! property because an ordering bug is exactly what a hand-written expectation
//! would agree with.

use std::time::Instant;

use super::query::{QUERY_MAX_BYTES, XTGETTCAP_MAX_NAME_BYTES, XTGETTCAP_MAX_NAMES};
use super::*;
use crate::event::VtEvent;
use crate::grid::Size;

struct Q {
    term: Terminal,
    batch: EventBatch,
    now: Instant,
}

impl Q {
    fn new() -> Q {
        Q::with(Config::default())
    }

    /// A terminal with the readback gate open — the state the `esctest`
    /// harness runs in, and the only place in this repository that opens it.
    fn readable() -> Q {
        Q::with(Config {
            allow_screen_readback: true,
            ..Config::default()
        })
    }

    fn with(config: Config) -> Q {
        Q {
            term: Terminal::new(Size { rows: 4, cols: 8 }, config),
            batch: EventBatch::new(),
            now: Instant::now(),
        }
    }

    fn feed(&mut self, bytes: &[u8]) -> FeedStats {
        self.batch.clear();
        self.term.feed(bytes, &mut self.batch, self.now)
    }

    /// Every `Reply` payload of the last batch, concatenated. Concatenated
    /// rather than listed because the order inside one batch is itself part of
    /// what these tests pin.
    fn replies(&self) -> String {
        let mut out = String::new();
        for event in self.batch.iter() {
            if let VtEvent::Reply(span) = event {
                out.push_str(&String::from_utf8_lossy(self.batch.bytes(*span)));
            }
        }
        out
    }

    fn reply_count(&self) -> usize {
        self.batch
            .iter()
            .filter(|event| matches!(event, VtEvent::Reply(_)))
            .count()
    }
}

// ── DECRQCRA ────────────────────────────────────────────────────────────────

/// The gate is shut by default, and shutting it is the status quo: no reply,
/// and the sequence counted exactly as it was before it was implemented.
#[test]
fn decrqcra_answers_nothing_while_the_gate_is_shut() {
    let mut q = Q::new();
    q.feed(b"AB");
    let stats = q.feed(b"\x1b[1;0;1;1;1;1*y");

    assert_eq!(q.replies(), "", "the shut gate answered");
    assert_eq!(stats.unhandled_sequences, 1);
}

/// `CSI Pid ; Pp ; Pt ; Pl ; Pb ; Pr * y` -> `DCS Pid ! ~ xxxx ST`, four
/// upper-case hex digits with the label echoed back unchanged.
#[test]
fn decrqcra_answers_one_cell_with_its_scalar_value() {
    let mut q = Q::readable();
    q.feed(b"A");
    // `A` is U+0041, so the positive, un-negated, attribute-free sum is 0x41.
    q.feed(b"\x1b[1;0;1;1;1;1*y");
    assert_eq!(q.replies(), "\x1bP1!~0041\x1b\\");
}

/// A never-written cell counts as `U+0020`, which is the `csNOTRIM` half of the
/// variant: blanks are summed rather than trimmed away.
#[test]
fn decrqcra_counts_a_blank_cell_as_a_space() {
    let mut q = Q::readable();
    q.feed(b"\x1b[7;0;1;1;1;1*y");
    assert_eq!(q.replies(), "\x1bP7!~0020\x1b\\");
}

/// A styled cell answers its text only: the video attributes contribute
/// nothing, which is the `csATTRIBS` half.
#[test]
fn decrqcra_ignores_the_video_attributes() {
    let mut plain = Q::readable();
    plain.feed(b"A");
    plain.feed(b"\x1b[0;0;1;1;1;1*y");
    let plain = plain.replies();

    let mut styled = Q::readable();
    styled.feed(b"\x1b[1;4;7;31;42mA\x1b[0m");
    styled.feed(b"\x1b[0;0;1;1;1;1*y");

    assert_eq!(styled.replies(), plain, "an attribute changed the checksum");
}

/// The label is echoed verbatim, including a value the engine never chose.
#[test]
fn decrqcra_echoes_the_requester_label() {
    let mut q = Q::readable();
    q.feed(b"\x1b[65535;0;1;1;1;1*y");
    assert_eq!(q.replies(), "\x1bP65535!~0020\x1b\\");
}

/// A rectangle larger than the screen is the screen: saturating clamps, never
/// a panic and never a read past the grid.
#[test]
fn decrqcra_clamps_a_rectangle_from_outside_the_grid() {
    let mut q = Q::readable();
    q.feed(b"AB");
    let stats = q.feed(b"\x1b[1;0;1;1;65535;65535*y");

    // Four rows of eight, all blank but the two written cells:
    // 0x41 + 0x42 + 30 * 0x20 = 1 091 = 0x443.
    assert_eq!(q.replies(), "\x1bP1!~0443\x1b\\");
    assert_eq!(stats.unhandled_sequences, 0);
}

/// A reversed rectangle (`Pb < Pt`, or `Pr < Pl`) is empty rather than an
/// underflow, and an empty rectangle answers zero.
#[test]
fn decrqcra_reads_a_reversed_rectangle_as_empty() {
    let mut q = Q::readable();
    q.feed(b"AAAAAAAA");
    for request in [
        &b"\x1b[1;0;4;1;2;8*y"[..], // bottom above top
        &b"\x1b[1;0;1;8;1;2*y"[..], // right left of left
    ] {
        q.feed(request);
        assert_eq!(
            q.replies(),
            "\x1bP1!~0000\x1b\\",
            "{:?} was not empty",
            String::from_utf8_lossy(request)
        );
    }
}

/// `DECOM`: while origin mode is set the rows are relative to the scrolling
/// region, the same rule `CPR` follows. This is the bug Contour had to fix.
#[test]
fn decrqcra_rows_are_region_relative_under_origin_mode() {
    let mut q = Q::readable();
    // Row 3 of the screen carries `Z`; the region starts there.
    q.feed(b"\x1b[3;1HZ\x1b[3;4r\x1b[?6h");
    q.feed(b"\x1b[1;0;1;1;1;1*y");
    assert_eq!(q.replies(), "\x1bP1!~005A\x1b\\");

    // The same request with origin mode reset reads the top of the screen,
    // which is blank.
    q.feed(b"\x1b[?6l");
    q.feed(b"\x1b[1;0;1;1;1;1*y");
    assert_eq!(q.replies(), "\x1bP1!~0020\x1b\\");
}

/// The scrollback is not addressable: a row that scrolled off is gone from the
/// checksum's reach, so the sequence cannot be used to read back history.
#[test]
fn decrqcra_never_reaches_the_scrollback() {
    let mut q = Q::readable();
    q.feed(b"SECRET\r\n\r\n\r\n\r\n\r\n");
    // The whole grid, clamped: `SECRET` has scrolled off the visible screen.
    q.feed(b"\x1b[1;0;1;1;65535;65535*y");
    // 4 rows x 8 columns of blanks: 32 * 0x20 = 0x400.
    assert_eq!(q.replies(), "\x1bP1!~0400\x1b\\");
}

/// A grapheme cluster contributes its **first** scalar, so a combining tail
/// cannot silently change a checksum a harness compares against `ord(char)`.
#[test]
fn decrqcra_sums_the_first_scalar_of_a_cluster() {
    let mut q = Q::readable();
    q.feed(b"\x1b[?2027h");
    q.feed("e\u{301}".as_bytes());
    q.feed(b"\x1b[1;0;1;1;1;1*y");
    assert_eq!(q.replies(), "\x1bP1!~0065\x1b\\");
}

// ── DECRQSS ─────────────────────────────────────────────────────────────────

/// `DCS $ q m ST` on a fresh terminal. The plain `0` is the correct answer and
/// is easy to get wrong by emitting nothing at all.
#[test]
fn decrqss_answers_sgr_on_a_fresh_terminal() {
    let mut q = Q::new();
    q.feed(b"\x1bP$qm\x1b\\");
    assert_eq!(q.replies(), "\x1bP1$r0m\x1b\\");
}

/// `DCS $ q r ST` reports the scrolling region, 1-based and inclusive.
#[test]
fn decrqss_answers_the_scrolling_region() {
    let mut q = Q::new();
    q.feed(b"\x1bP$qr\x1b\\");
    assert_eq!(q.replies(), "\x1bP1$r1;4r\x1b\\");

    q.feed(b"\x1b[2;3r");
    q.feed(b"\x1bP$qr\x1b\\");
    assert_eq!(q.replies(), "\x1bP1$r2;3r\x1b\\");
}

/// `DCS $ q SP q ST` reports the `DECSCUSR` selector, blink included.
#[test]
fn decrqss_answers_the_cursor_style() {
    let mut q = Q::new();
    q.feed(b"\x1bP$q q\x1b\\");
    assert_eq!(q.replies(), "\x1bP1$r2 q\x1b\\", "power-on steady block");

    for (set, expected) in [
        (&b"\x1b[1 q"[..], "1"),
        (&b"\x1b[3 q"[..], "3"),
        (&b"\x1b[4 q"[..], "4"),
        (&b"\x1b[5 q"[..], "5"),
        (&b"\x1b[6 q"[..], "6"),
    ] {
        q.feed(set);
        q.feed(b"\x1bP$q q\x1b\\");
        assert_eq!(q.replies(), format!("\x1bP1$r{expected} q\x1b\\"));
    }
}

/// Hiding the cursor does not change which shape `DECSCUSR` selected, so
/// `DECRQSS` must not report the folded-in `Hidden` a renderer sees.
#[test]
fn decrqss_reports_the_shape_a_hidden_cursor_still_has() {
    let mut q = Q::new();
    q.feed(b"\x1b[5 q\x1b[?25l");
    q.feed(b"\x1bP$q q\x1b\\");
    assert_eq!(q.replies(), "\x1bP1$r5 q\x1b\\");
}

/// `DECSCA` and `DECSCL`. The protected bit is stored and set by nothing, so
/// `0` is the whole truth; the level is the one `DA1` claims.
#[test]
fn decrqss_answers_decsca_and_decscl() {
    let mut q = Q::new();
    q.feed(b"\x1bP$q\"q\x1b\\");
    assert_eq!(q.replies(), "\x1bP1$r0\"q\x1b\\");

    q.feed(b"\x1bP$q\"p\x1b\\");
    assert_eq!(q.replies(), "\x1bP1$r62;1\"p\x1b\\");

    // The level `DECSCL` reports and the one `DA1` claims are the same number.
    q.feed(b"\x1b[c");
    assert_eq!(q.replies(), "\x1b[?62;4;22c");
}

/// A setting the engine does not have takes the invalid reply, which is the
/// honest one: answering would claim a capability that does not exist.
#[test]
fn decrqss_refuses_every_setting_the_engine_does_not_have() {
    let mut q = Q::new();
    for request in [
        &b"\x1bP$qs\x1b\\"[..],    // DECSLRM, no left-right margins here
        &b"\x1bP$q$}\x1b\\"[..],   // DECSASD
        &b"\x1bP$q*x\x1b\\"[..],   // DECSACE
        &b"\x1bP$q$|\x1b\\"[..],   // DECSCPP
        &b"\x1bP$q*|\x1b\\"[..],   // DECSNLS
        &b"\x1bP$q\x1b\\"[..],     // empty
        &b"\x1bP$qmm\x1b\\"[..],   // not a setting, close to one
        &b"\x1bP$q\xff\x1b\\"[..], // not UTF-8
    ] {
        q.feed(request);
        assert_eq!(
            q.replies(),
            "\x1bP0$r\x1b\\",
            "{:?} was answered",
            String::from_utf8_lossy(request)
        );
    }
}

/// **The packet's best test.** Feed an SGR state, ask for it back, replay the
/// answer into a fresh terminal, and assert the two cell templates match. A
/// property, not a golden string: it catches an ordering or default-omission
/// bug that a hand-written expectation would have agreed with.
#[test]
fn decrqss_sgr_round_trips_through_a_second_terminal() {
    let states: &[&[u8]] = &[
        b"\x1b[m",
        b"\x1b[1m",
        b"\x1b[1;2;3;5;6;7;8;9;53m",
        b"\x1b[4m",
        b"\x1b[4:2m",
        b"\x1b[4:3m",
        b"\x1b[4:4m",
        b"\x1b[4:5m",
        b"\x1b[31;42m",
        b"\x1b[91;104m",
        b"\x1b[38;5;123;48;5;7m",
        b"\x1b[38;2;1;2;3;48;2;250;251;252m",
        b"\x1b[38:2::10:20:30m",
        b"\x1b[58;5;9;4:3m",
        b"\x1b[58;2;7;8;9m",
        b"\x1b[1;3;4:4;38;5;200;48;2;9;9;9;58;5;12;53m",
    ];

    for state in states {
        let mut source = Q::new();
        source.feed(state);
        source.feed(b"\x1bP$qm\x1b\\");
        let answer = source.replies();

        let body = answer
            .strip_prefix("\x1bP1$r")
            .and_then(|rest| rest.strip_suffix("\x1b\\"))
            .unwrap_or_else(|| panic!("{:?} was refused", String::from_utf8_lossy(state)));

        let mut replay = Q::new();
        replay.feed(format!("\x1b[{body}").as_bytes());

        assert_eq!(
            replay.term.state.grid.screen().cursor().template(),
            source.term.state.grid.screen().cursor().template(),
            "{:?} did not round-trip through {body:?}",
            String::from_utf8_lossy(state)
        );
    }
}

// ── XTGETTCAP ───────────────────────────────────────────────────────────────

/// `DCS + q <hex> ST` -> `DCS 1 + r <hex> = <hex> ST`, upper-case hex pairs,
/// with the requested name echoed exactly as it was sent.
#[test]
fn xtgettcap_answers_a_known_capability() {
    let mut q = Q::new();
    // `544e` is `TN`.
    q.feed(b"\x1bP+q544e\x1b\\");
    assert_eq!(
        q.replies(),
        "\x1bP1+r544e=787465726D2D323536636F6C6F72\x1b\\",
        "TN is xterm-256color"
    );

    // `436f` is `Co`, the colour count tmux keys its handling off.
    q.feed(b"\x1bP+q436f\x1b\\");
    assert_eq!(q.replies(), "\x1bP1+r436f=323536\x1b\\", "Co is 256");
}

/// An embedder that tells programs what it is through `XTVERSION` must not tell
/// them it is xterm here. Only the name half is reported: `TN` is a terminfo
/// entry name and a version in parentheses is not part of one.
#[test]
fn xtgettcap_reports_the_embedders_product_name() {
    let mut q = Q::with(Config {
        product_name: Some("oneterm(1.2.3)".into()),
        ..Config::default()
    });
    q.feed(b"\x1bP+q544e\x1b\\");
    assert_eq!(q.replies(), "\x1bP1+r544e=6F6E657465726D\x1b\\");
}

/// A flag capability answers present with an empty value, which is what the
/// `RGB` and `Tc` truecolour probes look for.
#[test]
fn xtgettcap_answers_a_flag_with_an_empty_value() {
    let mut q = Q::new();
    // `524742` is `RGB`.
    q.feed(b"\x1bP+q524742\x1b\\");
    assert_eq!(q.replies(), "\x1bP1+r524742=\x1b\\");
}

/// Several names in one request are answered in **separate** replies, in
/// order, which is what xterm does and what tmux parses.
#[test]
fn xtgettcap_answers_each_name_in_its_own_reply() {
    let mut q = Q::new();
    // `TN` ; `Co` ; `nope`
    q.feed(b"\x1bP+q544e;436f;6e6f7065\x1b\\");
    assert_eq!(q.reply_count(), 3);
    assert_eq!(
        q.replies(),
        "\x1bP1+r544e=787465726D2D323536636F6C6F72\x1b\\\
         \x1bP1+r436f=323536\x1b\\\
         \x1bP0+r6e6f7065\x1b\\"
    );
}

/// An unknown name, odd-length hex and non-hex input are all answered unknown
/// with the request echoed back. No panic, no partial decode, and no `String`
/// built from untrusted bytes.
///
/// A request that is not hex at all echoes **nothing**: the echo is spliced
/// back into a DCS string, so a "name" carrying `ESC \` would end the reply
/// early and spill the rest onto the program's input as text.
#[test]
fn xtgettcap_answers_unknown_for_anything_it_cannot_decode_or_find() {
    let mut q = Q::new();
    for (request, echo) in [
        (&b"\x1bP+q6e6f7065\x1b\\"[..], "6e6f7065"), // "nope"
        (&b"\x1bP+q544\x1b\\"[..], "544"),           // odd length, still hex
        (&b"\x1bP+qZZZZ\x1b\\"[..], ""),             // not hex: not echoed
        (&b"\x1bP+q\x1b\\"[..], ""),                 // empty
        (&b"\x1bP+qfffe\x1b\\"[..], "fffe"),         // decodes to invalid UTF-8
    ] {
        let stats = q.feed(request);
        assert_eq!(
            q.replies(),
            format!("\x1bP0+r{echo}\x1b\\"),
            "{:?}",
            String::from_utf8_lossy(request)
        );
        assert_eq!(stats.unhandled_sequences, 0, "an answer is not a drop");
    }
}

/// The name-count ceiling, at the boundary: 16 answered, the 17th dropped and
/// the request counted once.
#[test]
fn xtgettcap_answers_sixteen_names_and_drops_the_seventeenth() {
    let mut q = Q::new();
    // `544e` (TN) repeated: every one is answerable, so only the ceiling can
    // reduce the count.
    let names: Vec<&str> = std::iter::repeat_n("544e", XTGETTCAP_MAX_NAMES).collect();

    let request = format!("\x1bP+q{}\x1b\\", names.join(";"));
    let stats = q.feed(request.as_bytes());
    assert_eq!(q.reply_count(), XTGETTCAP_MAX_NAMES);
    assert_eq!(stats.unhandled_sequences, 0, "nothing was dropped");

    let request = format!("\x1bP+q{};544e\x1b\\", names.join(";"));
    let stats = q.feed(request.as_bytes());
    assert_eq!(
        q.reply_count(),
        XTGETTCAP_MAX_NAMES,
        "the 17th was answered"
    );
    assert_eq!(stats.unhandled_sequences, 1, "the drop was not counted");
}

/// The per-name ceiling: a name too long to match any table entry is dropped
/// rather than echoed back truncated, because a truncated echo would be a lie
/// about what was asked.
#[test]
fn xtgettcap_drops_an_over_long_name_rather_than_truncating_it() {
    let mut q = Q::new();
    let long = "41".repeat(XTGETTCAP_MAX_NAME_BYTES + 1);

    let request = format!("\x1bP+q{long};544e\x1b\\");
    let stats = q.feed(request.as_bytes());

    assert_eq!(q.reply_count(), 1, "only the answerable name replies");
    assert!(q.replies().starts_with("\x1bP1+r544e="));
    assert_eq!(stats.unhandled_sequences, 1);
}

/// A hostile request of pure separators answers nothing useful, allocates
/// nothing unbounded, and is counted **once** -- not once per name, or 4 000
/// semicolons would drive the counter by 4 000.
#[test]
fn xtgettcap_survives_four_thousand_empty_names() {
    let mut q = Q::new();
    let request = format!("\x1bP+q{}\x1b\\", ";".repeat(4_000));
    let stats = q.feed(request.as_bytes());

    assert_eq!(q.reply_count(), XTGETTCAP_MAX_NAMES);
    assert_eq!(stats.unhandled_sequences, 1);
}

// ── The payload buffer ──────────────────────────────────────────────────────

/// A payload past the query ceiling is not a request anyone could answer: it
/// answers nothing and is counted, and the buffer never grows past the ceiling.
#[test]
fn an_over_long_query_payload_answers_nothing() {
    let mut q = Q::new();
    let request = format!("\x1bP+q{}\x1b\\", "41".repeat(QUERY_MAX_BYTES));
    let stats = q.feed(request.as_bytes());

    assert_eq!(q.replies(), "");
    assert_eq!(stats.unhandled_sequences, 1);
    assert_eq!(stats.aborted_dcs, 0);
    assert!(q.term.state.dcs_payload.len() <= QUERY_MAX_BYTES);
}

/// A `CAN` mid-payload aborts: nothing is answered, and `aborted_dcs` -- which
/// the Sixel half already moved -- counts it.
#[test]
fn an_aborted_query_answers_nothing() {
    let mut q = Q::new();
    let stats = q.feed(b"\x1bP+q544e\x18");

    assert_eq!(q.replies(), "");
    assert_eq!(stats.aborted_dcs, 1);
    assert!(q.term.state.dcs_query.is_none());
    assert!(q.term.state.dcs_payload.is_empty());
}

/// The buffer is reused, not reallocated: a program polling `XTGETTCAP` in a
/// loop allocates once. Asserted on the capacity, which cannot stay put across
/// 10 000 requests unless the `Vec` is `clear()`ed rather than dropped.
#[test]
fn ten_thousand_queries_reuse_one_buffer() {
    let mut q = Q::new();
    q.feed(b"\x1bP+q544e;436f;524742\x1b\\");
    let capacity = q.term.state.dcs_payload.capacity();
    assert!(capacity > 0, "nothing was buffered");

    for _ in 0..10_000 {
        q.feed(b"\x1bP+q544e;436f;524742\x1b\\");
    }
    assert_eq!(q.term.state.dcs_payload.capacity(), capacity);
}

/// `RIS` and `DECSTR` clear the buffer with the rest of the terminal state. A
/// DCS cannot in fact be in flight across either, so this pins the invariant
/// rather than a reachable bug.
#[test]
fn a_reset_clears_the_query_buffer() {
    for reset in [&b"\x1bc"[..], &b"\x1b[!p"[..]] {
        let mut q = Q::new();
        q.feed(b"\x1bP+q544e\x1b\\");
        q.feed(reset);
        assert!(q.term.state.dcs_query.is_none());
        assert!(q.term.state.dcs_payload.is_empty());
    }
}

/// Two queries in one `feed` produce two replies, in order. The batch already
/// supported this; what is new is that both sequences have an answer.
#[test]
fn two_queries_in_one_feed_answer_in_order() {
    let mut q = Q::new();
    q.feed(b"\x1bP$qm\x1b\\\x1bP+q436f\x1b\\");
    assert_eq!(q.replies(), "\x1bP1$r0m\x1b\\\x1bP1+r436f=323536\x1b\\");
}

/// The other half of the routing key is untouched: a bare `DCS q` is still an
/// image and never a query.
#[test]
fn a_bare_dcs_q_is_still_sixel() {
    let mut q = Q::new();
    q.feed(b"\x1bPq#0;2;0;0;0#0~\x1b\\");
    assert_eq!(q.replies(), "");
    assert_eq!(q.term.placements().len(), 1);
    assert!(q.term.state.dcs_payload.is_empty());
}

/// `DECRQM` and `DA` are unaffected: this packet added answers, it did not
/// change any that existed.
#[test]
fn the_existing_reports_still_answer_what_they_did() {
    let mut q = Q::new();
    for (request, expected) in [
        (&b"\x1b[c"[..], "\x1b[?62;4;22c"),
        (&b"\x1b[=c"[..], "\x1bP!|00000000\x1b\\"),
        (&b"\x1b[?3$p"[..], "\x1b[?3;0$y"),
        (&b"\x1b[?25$p"[..], "\x1b[?25;1$y"),
        (&b"\x1b[20$p"[..], "\x1b[20;2$y"),
    ] {
        q.feed(request);
        assert_eq!(
            q.replies(),
            expected,
            "{:?}",
            String::from_utf8_lossy(request)
        );
    }
}
