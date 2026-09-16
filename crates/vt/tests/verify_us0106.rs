//! Independent verification of `US-0106` (DECRQCRA, DECRQSS, XTGETTCAP).
//!
//! Written by the verifier, against the public API only: every number below is
//! either hand-computed from xterm's `xtermCheckRect` at `checksumExtension: 7`
//! or taken from `ctlseqs` / the terminfo capability definition.
//!
//! Some tests here are **characterisation** tests: they record what the engine
//! does where it differs from xterm, so the difference is a pinned number
//! rather than a surprise. Those are named `characterise_*`.
//!
//! Two of them stopped being characterisations when the findings they recorded
//! were fixed: the engine now sums every scalar of a grapheme cluster and now
//! clamps the rectangle to the scrolling region under `DECOM`, so both tests
//! assert xterm's answer and keep the old number as the thing that must not
//! come back.

use std::time::Instant;

use oneterm_vt::{Config, EventBatch, Size, Terminal, VtEvent};

struct Q {
    term: Terminal,
    batch: EventBatch,
}

impl Q {
    fn sized(rows: u16, cols: u16, readback: bool) -> Q {
        Q {
            term: Terminal::new(
                Size { rows, cols },
                Config {
                    allow_screen_readback: readback,
                    ..Config::default()
                },
            ),
            batch: EventBatch::new(),
        }
    }

    fn open(rows: u16, cols: u16) -> Q {
        Q::sized(rows, cols, true)
    }

    fn shut() -> Q {
        Q::sized(4, 8, false)
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

    fn reply_list(&self) -> Vec<String> {
        self.batch
            .iter()
            .filter_map(|event| match event {
                VtEvent::Reply(span) => {
                    Some(String::from_utf8_lossy(self.batch.bytes(*span)).into_owned())
                }
                _ => None,
            })
            .collect()
    }
}

/// `DCS <id> ! ~ <4 hex> ST` for a hand-computed sum.
fn cksum(id: u16, sum: u32) -> String {
    format!("\x1bP{id}!~{:04X}\x1b\\", sum & 0xffff)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

// -- DECRQCRA ---------------------------------------------------------------

/// F1. A rectangle wider than one cell, summed by hand.
///
/// Grid 4x8. Row 1 is `AB`, row 2 is `CD`. The rectangle is rows 1-2,
/// columns 1-3, so it covers `A B <blank>` and `C D <blank>`:
/// 0x41 + 0x42 + 0x20 + 0x43 + 0x44 + 0x20 = 330 = 0x014A.
#[test]
fn decrqcra_multi_cell_rectangle_matches_a_hand_sum() {
    let mut q = Q::open(4, 8);
    q.feed(b"AB\r\nCD");
    q.feed(b"\x1b[1;0;1;1;2;3*y");
    assert_eq!(
        q.replies(),
        cksum(1, 0x41 + 0x42 + 0x20 + 0x43 + 0x44 + 0x20)
    );
}

/// F2. Omitted parameters and explicit zeroes both mean "the whole page".
///
/// Grid 4x8 with a single `A`: 0x41 + 31 * 0x20 = 1057 = 0x0421, label 0.
#[test]
fn decrqcra_defaults_are_the_whole_page() {
    for request in [&b"\x1b[*y"[..], &b"\x1b[0;0;0;0;0;0*y"[..]] {
        let mut q = Q::open(4, 8);
        q.feed(b"A");
        q.feed(request);
        assert_eq!(
            q.replies(),
            cksum(0, 0x41 + 31 * 0x20),
            "{:?}",
            String::from_utf8_lossy(request)
        );
    }
}

/// F3. `Pp` (page) is parsed and ignored: every page answers the same.
#[test]
fn decrqcra_ignores_the_page_parameter() {
    let mut answers = Vec::new();
    for page in [0u16, 1, 2, 5, 65535] {
        let mut q = Q::open(4, 8);
        q.feed(b"A");
        q.feed(format!("\x1b[9;{page};1;1;4;8*y").as_bytes());
        answers.push(q.replies());
    }
    let first = answers[0].clone();
    assert_eq!(first, cksum(9, 0x41 + 31 * 0x20));
    assert!(
        answers.iter().all(|a| *a == first),
        "page changed the answer: {answers:?}"
    );
}

/// F4. The sum is masked to 16 bits, not saturated and not widened.
///
/// 4 rows x 8 columns of `U+4E16`: each is one wide cell (0x4E16) plus one
/// spacer, so 16 wide cells and 16 spacers.
#[test]
fn decrqcra_masks_to_sixteen_bits() {
    let mut q = Q::open(4, 8);
    for _ in 0..4 {
        q.feed("\u{4e16}\u{4e16}\u{4e16}\u{4e16}".as_bytes());
    }
    q.feed(b"\x1b[1;0;1;1;4;8*y");
    let expected = 16 * 0x4e16u32 + 16 * 0x20;
    assert!(expected > 0xffff, "the test no longer exercises the mask");
    assert_eq!(q.replies(), cksum(1, expected));
}

/// F5. Characterisation: the spacer column of a double-width character.
///
/// xterm's `xtermCheckRect` reads `charSeen[col]` for every drawn column; this
/// engine reads the cell's content, and a `WideSpacer` cell's content is a
/// space. Pinned: the glyph column contributes 0x4E16, the spacer 0x20.
#[test]
fn characterise_decrqcra_wide_character_spacer() {
    let mut q = Q::open(4, 8);
    q.feed("\u{4e16}".as_bytes());

    q.feed(b"\x1b[1;0;1;1;1;1*y");
    assert_eq!(q.replies(), cksum(1, 0x4e16), "the glyph column");

    q.feed(b"\x1b[1;0;1;2;1;2*y");
    assert_eq!(q.replies(), cksum(1, 0x20), "the spacer column");

    q.feed(b"\x1b[1;0;1;1;1;2*y");
    assert_eq!(q.replies(), cksum(1, 0x4e16 + 0x20), "both columns");
}

/// F6. A combining mark contributes its own scalar.
///
/// xterm at `checksumExtension: 7` (`csBYTE` clear) adds every `combData`
/// entry into the total, so xterm answers 0x0065 + 0x0301 = 0x0366 here. This
/// test found the engine summing the cluster's first scalar only; it now sums
/// them all, so the two agree.
#[test]
fn decrqcra_sums_every_scalar_of_a_cluster() {
    let mut q = Q::open(4, 8);
    q.feed(b"\x1b[?2027h");
    q.feed("e\u{301}".as_bytes());
    q.feed(b"\x1b[1;0;1;1;1;1*y");
    // xterm at extension 7 has `csBYTE` clear, so `xtermCheckRect` walks
    // `combData` and adds every combining scalar on top of the base character.
    // The engine now does the same; the verification that produced this test
    // found it summing only the first scalar.
    assert_eq!(
        q.replies(),
        cksum(1, 0x65 + 0x301),
        "base plus combining mark"
    );
    assert_ne!(
        q.replies(),
        cksum(1, 0x65),
        "first scalar only is the old bug"
    );
}

/// F7. The alternate screen is what is read while it is active, and the
/// primary screen is untouched by it.
#[test]
fn decrqcra_reads_the_active_screen() {
    let mut q = Q::open(4, 8);
    q.feed(b"P");
    q.feed(b"\x1b[?1049h");
    q.feed(b"\x1b[1;0;1;1;1;1*y");
    assert_eq!(q.replies(), cksum(1, 0x20), "alt screen starts blank");
    q.feed(b"\x1b[HZ");
    q.feed(b"\x1b[1;0;1;1;1;1*y");
    assert_eq!(q.replies(), cksum(1, 0x5a));
    q.feed(b"\x1b[?1049l");
    q.feed(b"\x1b[1;0;1;1;1;1*y");
    assert_eq!(q.replies(), cksum(1, 0x50), "the primary screen came back");
}

/// F8. The scrollback cannot be reached, with content in it and with a
/// rectangle that asks for far more rows than the screen has.
#[test]
fn decrqcra_cannot_read_scrollback() {
    let mut q = Q::open(4, 8);
    // Eight rows of `Q` pushed through a four-row screen: four are history.
    for _ in 0..8 {
        q.feed(b"QQQQQQQQ\r\n");
    }
    // Only the last three rows still hold `Q`; the cursor's row is blank.
    // 3 * 8 * 0x51 + 8 * 0x20 = 1272 = 0x04F8.
    q.feed(b"\x1b[1;0;1;1;65535;65535*y");
    let whole = q.replies();
    assert_eq!(whole, cksum(1, 3 * 8 * 0x51 + 8 * 0x20));

    // The same rectangle stated exactly. If history were reachable the two
    // would differ.
    q.feed(b"\x1b[1;0;1;1;4;8*y");
    assert_eq!(q.replies(), whole);
}

/// F9. The gate is shut by default, answers nothing and is counted once.
#[test]
fn decrqcra_gate_shut_answers_nothing_and_counts() {
    let mut q = Q::shut();
    q.feed(b"ABCD");
    let stats = q.feed(b"\x1b[1;0;1;1;4;8*y");
    assert_eq!(q.replies(), "");
    assert_eq!(stats.unhandled_sequences, 1);

    let stats = q.feed(b"\x1b[1;0;1;1;4;8*y\x1b[2;0;1;1;4;8*y");
    assert_eq!(q.replies(), "");
    assert_eq!(stats.unhandled_sequences, 2);
}

/// F10. The gate is a construction-time value: `Terminal::config` hands out a
/// shared reference and there is no setter, so a program inside the terminal
/// cannot open it.
#[test]
fn decrqcra_gate_has_no_runtime_setter() {
    let q = Q::shut();
    assert!(!q.term.config().allow_screen_readback);
    let open = Q::open(4, 8);
    assert!(open.term.config().allow_screen_readback);
    // The real proof is the snapshot: `crates/vt/public-api.*.txt` lists
    // `method config` and no `config_mut` / `set_config`.
}

/// F11. Characterisation: under `DECOM` the rectangle is **not** confined to
/// the scrolling region.
///
/// xterm's `xtermParseRect` defaults `top` / `bottom` to `minRectRow` /
/// `maxRectRow`, which under origin mode are the scrolling region's margins,
/// and `limitedParseRow` clamps an explicit row into the same span. So xterm
/// reading "the whole page" under a region of rows 2-3 reads rows 2-3 only.
///
/// This engine offsets by the region's top and then clamps to the **screen**,
/// so the same request reads rows 2, 3 and 4.
#[test]
fn decrqcra_origin_mode_clamps_to_the_region() {
    let mut q = Q::open(4, 8);
    // Rows: 1 `A`, 2 `B`, 3 `C`, 4 `D`. Region rows 2-3, origin mode on.
    q.feed(b"\x1b[1;1HA\x1b[2;1HB\x1b[3;1HC\x1b[4;1HD");
    q.feed(b"\x1b[2;3r\x1b[?6h");

    // Row 1 of the region is screen row 2: `B`.
    q.feed(b"\x1b[1;0;1;1;1;1*y");
    assert_eq!(q.replies(), cksum(1, 0x42), "origin offset not applied");

    // Whole page, defaults omitted. Under `DECOM` the page **is** the region,
    // so this is rows 2-3 only: 0x42 + 0x43 + 14 * 0x20 = 581 = 0x245. xterm
    // reaches the same number through `minRectRow` / `maxRectRow`.
    let region_only = 0x42 + 0x43 + 14 * 0x20;
    // Rows 2-4 -- 0x42 + 0x43 + 0x44 + 21 * 0x20 = 873 -- is what the engine
    // answered before this verification: the offset applied, the clamp missing.
    let through_row_four = 0x42 + 0x43 + 0x44 + 21 * 0x20;
    q.feed(b"\x1b[7;0*y");
    assert_eq!(
        q.replies(),
        cksum(7, region_only),
        "defaults are the region"
    );
    assert_ne!(
        q.replies(),
        cksum(7, through_row_four),
        "read past the region"
    );

    // An explicit bottom past the region is clamped to it as well.
    q.feed(b"\x1b[7;0;1;1;99;99*y");
    assert_eq!(
        q.replies(),
        cksum(7, region_only),
        "explicit bottom not clamped"
    );

    // Resetting origin mode restores the whole screen as the page.
    q.feed(b"\x1b[?6l");
    q.feed(b"\x1b[7;0;1;1;99;99*y");
    let whole_screen = 0x41 + 0x42 + 0x43 + 0x44 + 28 * 0x20;
    assert_eq!(q.replies(), cksum(7, whole_screen));
}

/// F12. A reversed rectangle is empty, in both axes, and never panics.
#[test]
fn decrqcra_reversed_rectangle_is_empty() {
    let mut q = Q::open(4, 8);
    q.feed(b"ABCDEFGH\r\nIJKLMNOP");
    for request in [
        &b"\x1b[1;0;3;1;2;8*y"[..],
        &b"\x1b[1;0;1;6;2;2*y"[..],
        &b"\x1b[1;0;4;8;1;1*y"[..],
    ] {
        q.feed(request);
        assert_eq!(
            q.replies(),
            cksum(1, 0),
            "{:?}",
            String::from_utf8_lossy(request)
        );
    }
}

/// F13. A rectangle that starts past the last row or column is empty rather
/// than a read out of bounds.
#[test]
fn decrqcra_rectangle_entirely_outside_the_grid() {
    let mut q = Q::open(4, 8);
    q.feed(b"\x1b[1;0;9;9;12;12*y");
    assert_eq!(q.replies(), cksum(1, 0));
    q.feed(b"\x1b[1;0;65535;65535;65535;65535*y");
    assert_eq!(q.replies(), cksum(1, 0));
}

// -- DECRQSS ----------------------------------------------------------------

/// The payload of a `DCS 1 $ r ... ST` reply, or `None` for the invalid form.
fn decrqss_payload(reply: &str) -> Option<String> {
    let body = reply.strip_prefix("\x1bP1$r")?.strip_suffix("\x1b\\")?;
    Some(body.to_owned())
}

/// F14. The SGR answer round-trips: replayed into a fresh terminal it
/// reproduces the very style it described.
#[test]
fn decrqss_sgr_round_trips_a_mixed_style() {
    let states: &[&[u8]] = &[
        b"\x1b[0m",
        b"\x1b[1m",
        b"\x1b[1;38;5;196;48;2;10;20;30;4:3;58;5;99m",
        b"\x1b[3;9;53;38;2;1;2;3;48;5;17;58;2;255;0;255m",
        b"\x1b[2;5;7;8;4:2m",
        b"\x1b[31;44m",
        b"\x1b[91;104m",
        b"\x1b[6;4:4;58;5;1m",
        b"\x1b[4:5m",
    ];
    for sgr in states {
        let mut a = Q::open(4, 8);
        a.feed(sgr);
        let want = a.term.style();
        a.feed(b"\x1bP$qm\x1b\\");
        let payload = decrqss_payload(&a.replies()).unwrap_or_else(|| panic!("{:?}", a.replies()));
        assert!(
            payload.ends_with('m'),
            "the reply must repeat the request's final byte: {payload:?}"
        );
        assert!(
            payload.starts_with('0'),
            "the reply must reset first: {payload:?}"
        );

        let mut b = Q::open(4, 8);
        b.feed(format!("\x1b[{payload}").as_bytes());
        assert_eq!(
            b.term.style(),
            want,
            "{:?} answered {payload:?}, which does not reproduce it",
            String::from_utf8_lossy(sgr)
        );
    }
}

/// F15. `DECSTBM`, with and without a region set.
#[test]
fn decrqss_answers_the_scrolling_region() {
    let mut q = Q::open(6, 8);
    q.feed(b"\x1bP$qr\x1b\\");
    assert_eq!(q.replies(), "\x1bP1$r1;6r\x1b\\", "the default region");

    q.feed(b"\x1b[2;5r");
    q.feed(b"\x1bP$qr\x1b\\");
    assert_eq!(q.replies(), "\x1bP1$r2;5r\x1b\\");

    q.feed(b"\x1b[r");
    q.feed(b"\x1bP$qr\x1b\\");
    assert_eq!(q.replies(), "\x1bP1$r1;6r\x1b\\", "after a reset region");
}

/// F16. `DECSCUSR` 1-6 round-trip, and hiding the cursor does not change the
/// shape the stream selected.
#[test]
fn decrqss_answers_every_cursor_shape() {
    for id in 1u8..=6 {
        let mut q = Q::open(4, 8);
        q.feed(format!("\x1b[{id} q").as_bytes());
        q.feed(b"\x1bP$q q\x1b\\");
        assert_eq!(q.replies(), format!("\x1bP1$r{id} q\x1b\\"), "shape {id}");

        q.feed(b"\x1b[?25l");
        q.feed(b"\x1bP$q q\x1b\\");
        assert_eq!(
            q.replies(),
            format!("\x1bP1$r{id} q\x1b\\"),
            "DECTCEM changed the reported shape for {id}"
        );
    }
    // `CSI 0 SP q` is "the default", reported as the engine's own default
    // cursor style rather than as 0.
    let mut q = Q::open(4, 8);
    q.feed(b"\x1b[0 q");
    q.feed(b"\x1bP$q q\x1b\\");
    let reply = q.replies();
    assert!(
        reply.starts_with("\x1bP1$r") && reply.ends_with(" q\x1b\\"),
        "{reply:?}"
    );
}

/// F17. `DECSCA` and `DECSCL`.
#[test]
fn decrqss_answers_decsca_and_decscl() {
    let mut q = Q::open(4, 8);
    q.feed(b"\x1bP$q\"q\x1b\\");
    assert_eq!(q.replies(), "\x1bP1$r0\"q\x1b\\");
    // `CSI 1 " q` is not dispatched, so the answer stays 0 -- recorded, not
    // asserted as correct behaviour.
    q.feed(b"\x1b[1\"q");
    q.feed(b"\x1bP$q\"q\x1b\\");
    assert_eq!(q.replies(), "\x1bP1$r0\"q\x1b\\");

    q.feed(b"\x1bP$q\"p\x1b\\");
    assert_eq!(q.replies(), "\x1bP1$r62;1\"p\x1b\\");
    // The level must agree with DA1's `? 62`.
    q.feed(b"\x1b[c");
    assert!(q.replies().starts_with("\x1b[?62;"), "{:?}", q.replies());
}

/// F18. Everything else takes the invalid reply, including the settings the
/// packet deliberately refuses and an empty request.
#[test]
fn decrqss_refuses_everything_else() {
    let mut q = Q::open(4, 8);
    for request in [
        &b"\x1bP$qs\x1b\\"[..],  // DECSLRM
        &b"\x1bP$q$}\x1b\\"[..], // DECSASD
        &b"\x1bP$q*x\x1b\\"[..], // DECSACE
        &b"\x1bP$q$|\x1b\\"[..], // DECSCPP
        &b"\x1bP$q*|\x1b\\"[..], // DECSNLS
        &b"\x1bP$qt\x1b\\"[..],  // DECSLPP
        &b"\x1bP$q\x1b\\"[..],   // empty
        &b"\x1bP$qM\x1b\\"[..],  // case matters
        &b"\x1bP$qmm\x1b\\"[..], // not a setting
        &b"\x1bP$q m\x1b\\"[..], // wrong intermediate
    ] {
        q.feed(request);
        assert_eq!(
            q.replies(),
            "\x1bP0$r\x1b\\",
            "{:?}",
            String::from_utf8_lossy(request)
        );
    }
}

/// F19. A 9 KiB request is over the payload ceiling: nothing is answered and
/// the attempt is counted once.
#[test]
fn decrqss_over_long_request_answers_nothing() {
    let mut q = Q::open(4, 8);
    let mut bytes = b"\x1bP$q".to_vec();
    bytes.extend(std::iter::repeat_n(b'm', 9 * 1024));
    bytes.extend_from_slice(b"\x1b\\");
    let stats = q.feed(&bytes);
    assert_eq!(q.replies(), "");
    assert_eq!(stats.unhandled_sequences, 1);
    assert_eq!(stats.aborted_dcs, 0);

    // The terminal still answers the next, well-formed request.
    q.feed(b"\x1bP$qm\x1b\\");
    assert_eq!(q.replies(), "\x1bP1$r0m\x1b\\");
}

/// F20. A request whose `ST` is split across feeds is answered exactly once,
/// and the `\` half of the terminator never reaches the screen as text.
///
/// Characterisation: the answer is emitted on the `ESC`, not on the `\` -- the
/// parser unhooks a DCS as soon as `ESC` arrives, the same rule that lets an
/// unterminated Sixel be finished by the sequence behind it.
#[test]
fn decrqss_survives_a_split_string_terminator() {
    let mut q = Q::open(4, 8);
    q.feed(b"\x1bP$q");
    assert_eq!(q.replies(), "");
    q.feed(b"m");
    assert_eq!(q.replies(), "");
    q.feed(b"\x1b");
    assert_eq!(q.replies(), "\x1bP1$r0m\x1b\\", "unhooked on ESC");
    q.feed(b"\\");
    assert_eq!(q.replies(), "", "the terminator was answered twice");
    assert_eq!(
        q.term.row_text(oneterm_vt::RowId(0)).trim_end(),
        "",
        "the ST tail was printed as text"
    );
}

/// F21. `CAN` aborts the request: nothing answered, `aborted_dcs` counted.
#[test]
fn decrqss_can_aborts_without_answering() {
    let mut q = Q::open(4, 8);
    let stats = q.feed(b"\x1bP$qm\x18");
    assert_eq!(q.replies(), "");
    assert_eq!(stats.aborted_dcs, 1);
    q.feed(b"\x1bP$qm\x1b\\");
    assert_eq!(
        q.replies(),
        "\x1bP1$r0m\x1b\\",
        "the buffer was not poisoned"
    );
}

// -- XTGETTCAP --------------------------------------------------------------

fn tcap(name: &str) -> Vec<u8> {
    format!("\x1bP+q{}\x1b\\", hex(name.as_bytes())).into_bytes()
}

/// F22. The documented names, each decoded back to the value a terminfo entry
/// would hold.
#[test]
fn xtgettcap_answers_the_documented_names() {
    let known: &[(&str, &str)] = &[
        ("TN", "xterm-256color"),
        ("Co", "256"),
        ("colors", "256"),
        ("RGB", ""),
        ("Ms", "\x1b]52;%p1%s;%p2%s\x07"),
        ("Se", "\x1b[2 q"),
        ("Ss", "\x1b[%p1%d q"),
        ("Su", ""),
    ];
    for (name, value) in known {
        let mut q = Q::open(4, 8);
        q.feed(&tcap(name));
        let want = format!(
            "\x1bP1+r{}={}\x1b\\",
            hex(name.as_bytes()),
            hex(value.as_bytes())
        );
        assert_eq!(q.replies(), want, "{name}");
    }

    // `kbs` (backspace) is not in the table, so it is honestly unknown.
    for name in ["kbs", "bce", "u8", "XT"] {
        let mut q = Q::open(4, 8);
        q.feed(&tcap(name));
        assert_eq!(
            q.replies(),
            format!("\x1bP0+r{}\x1b\\", hex(name.as_bytes())),
            "{name}"
        );
    }
}

/// F23. The echo preserves the request's own hex spelling, including case.
#[test]
fn xtgettcap_echoes_the_request_spelling() {
    let mut q = Q::open(4, 8);
    q.feed(b"\x1bP+q544e\x1b\\");
    assert_eq!(
        q.replies(),
        "\x1bP1+r544e=787465726D2D323536636F6C6F72\x1b\\"
    );
    q.feed(b"\x1bP+q544E\x1b\\");
    assert_eq!(
        q.replies(),
        "\x1bP1+r544E=787465726D2D323536636F6C6F72\x1b\\",
        "the echo is the request, the value is upper-case"
    );
}

/// F24. Capability names are case-sensitive, which is what a terminfo lookup
/// is: `tn` is not `TN`.
#[test]
fn xtgettcap_names_are_case_sensitive() {
    for name in ["tn", "co", "rgb", "su", "ms", "SS", "SE", "COLORS"] {
        let mut q = Q::open(4, 8);
        q.feed(&tcap(name));
        assert_eq!(
            q.replies(),
            format!("\x1bP0+r{}\x1b\\", hex(name.as_bytes())),
            "{name} matched something it should not"
        );
    }
}

/// F25. Several names in one request get one reply each, in order.
#[test]
fn xtgettcap_answers_each_name_separately() {
    let mut q = Q::open(4, 8);
    let request = format!(
        "\x1bP+q{};{};{}\x1b\\",
        hex(b"Co"),
        hex(b"kbs"),
        hex(b"RGB")
    );
    q.feed(request.as_bytes());
    assert_eq!(
        q.reply_list(),
        vec![
            format!("\x1bP1+r{}={}\x1b\\", hex(b"Co"), hex(b"256")),
            format!("\x1bP0+r{}\x1b\\", hex(b"kbs")),
            format!("\x1bP1+r{}=\x1b\\", hex(b"RGB")),
        ]
    );
}

/// F26. Odd-length and non-hex names are answered unknown, and a name that
/// could end the reply early is never echoed.
#[test]
fn xtgettcap_refuses_malformed_names() {
    let cases: &[(&str, &str)] = &[
        ("544", "\x1bP0+r544\x1b\\"),
        ("54ZZ", "\x1bP0+r\x1b\\"),
        ("zz", "\x1bP0+r\x1b\\"),
        // `54 1b 5c 4e` decodes to `T ESC \ N`: a known-looking name that is
        // not in the table, echoed only because the request itself was hex.
        ("541b5c4e", "\x1bP0+r541b5c4e\x1b\\"),
        ("", "\x1bP0+r\x1b\\"),
    ];
    for (name, want) in cases {
        let mut q = Q::open(4, 8);
        q.feed(format!("\x1bP+q{name}\x1b\\").as_bytes());
        assert_eq!(q.replies(), *want, "{name:?}");
    }

    // A raw `ESC \` in the payload terminates the DCS: the reply describes only
    // the bytes that arrived before it, and it must not carry a terminator of
    // its own in the middle.
    let mut q = Q::open(4, 8);
    q.feed(b"\x1bP+q54\x1b\\4e\x1b\\");
    let replies = q.replies();
    assert_eq!(
        replies, "\x1bP0+r54\x1b\\",
        "the truncated name was mis-answered"
    );
}

/// F27. The name-count ceiling: 16 answered, the 17th dropped and counted once
/// for the whole request.
#[test]
fn xtgettcap_name_ceiling_drops_and_counts_once() {
    let mut q = Q::open(4, 8);
    let sixteen = vec![hex(b"Co"); 16].join(";");
    let stats = q.feed(format!("\x1bP+q{sixteen}\x1b\\").as_bytes());
    assert_eq!(q.reply_list().len(), 16);
    assert_eq!(stats.unhandled_sequences, 0);

    let seventeen = vec![hex(b"Co"); 17].join(";");
    let stats = q.feed(format!("\x1bP+q{seventeen}\x1b\\").as_bytes());
    assert_eq!(q.reply_list().len(), 16);
    assert_eq!(stats.unhandled_sequences, 1);

    // Four thousand names still counts once, not four thousand times.
    let many = vec![hex(b"Co"); 4000].join(";");
    let stats = q.feed(format!("\x1bP+q{many}\x1b\\").as_bytes());
    assert_eq!(stats.unhandled_sequences, 1);
    assert!(q.reply_list().len() <= 16);
}

/// F28. The per-name byte ceiling drops rather than truncating the echo.
#[test]
fn xtgettcap_long_name_is_dropped_not_truncated() {
    let mut q = Q::open(4, 8);
    let at = "41".repeat(128);
    let over = "41".repeat(129);
    let stats = q.feed(format!("\x1bP+q{at}\x1b\\").as_bytes());
    assert_eq!(q.reply_list().len(), 1, "128 bytes is answerable");
    assert_eq!(stats.unhandled_sequences, 0);

    let stats = q.feed(format!("\x1bP+q{over}\x1b\\").as_bytes());
    assert_eq!(q.reply_list().len(), 0, "129 bytes must be dropped");
    assert_eq!(stats.unhandled_sequences, 1);
    assert!(
        !q.replies().contains(&over[..8]),
        "a dropped name was echoed truncated"
    );
}

/// F29. `product_name` overrides `TN`, and only its name half.
#[test]
fn xtgettcap_reports_the_product_name() {
    let mut term = Terminal::new(
        Size { rows: 4, cols: 8 },
        Config {
            product_name: Some("OneTerm(1.2.3)".into()),
            ..Config::default()
        },
    );
    let mut batch = EventBatch::new();
    term.feed(b"\x1bP+q544e\x1b\\", &mut batch, Instant::now());
    let mut out = String::new();
    for event in batch.iter() {
        if let VtEvent::Reply(span) = event {
            out.push_str(&String::from_utf8_lossy(batch.bytes(*span)));
        }
    }
    assert_eq!(out, format!("\x1bP1+r544e={}\x1b\\", hex(b"OneTerm")));
}

// -- Routing (BUG-0058 must still hold) -------------------------------------

/// F30. `DCS q` with no intermediate is still Sixel, and the two query forms
/// never open the image decoder.
#[test]
fn dcs_routing_is_unchanged() {
    let mut q = Q::open(4, 8);
    let stats = q.feed(b"\x1bPq#0;2;100;0;0#0~~@@vv@@~~@@~~$\x1b\\");
    assert_eq!(q.replies(), "", "Sixel answered a query reply");
    assert_eq!(stats.unhandled_sequences, 0);
    assert!(!q.term.placements().is_empty(), "the Sixel did not decode");

    let mut q = Q::open(4, 8);
    q.feed(b"\x1bP$qm\x1b\\");
    assert_eq!(q.replies(), "\x1bP1$r0m\x1b\\");
    assert!(q.term.placements().is_empty());
    q.feed(b"\x1bP+q436f\x1b\\");
    assert!(q.term.placements().is_empty());

    // Every other DCS is still counted unhandled and answers nothing.
    let mut q = Q::open(4, 8);
    for request in [
        &b"\x1bP0;1|61/1b5b32334d\x1b\\"[..], // DECUDK
        &b"\x1bP$rm\x1b\\"[..],               // a reply shape, not a request
        &b"\x1bP+p54\x1b\\"[..],              // XTSETTCAP
        &b"\x1bP!|\x1b\\"[..],
    ] {
        let stats = q.feed(request);
        assert_eq!(
            q.replies(),
            "",
            "{:?} was answered",
            String::from_utf8_lossy(request)
        );
        assert!(
            stats.unhandled_sequences >= 1,
            "{:?} was not counted",
            String::from_utf8_lossy(request)
        );
    }
}

/// F31. DA1, DA2 and DECRQM are byte-identical to what they were.
#[test]
fn device_attributes_and_decrqm_are_unchanged() {
    let mut q = Q::open(4, 8);
    q.feed(b"\x1b[c");
    assert_eq!(q.replies(), "\x1b[?62;4;22c");
    q.feed(b"\x1b[>c");
    assert!(q.replies().starts_with("\x1b[>0;"), "{:?}", q.replies());
    q.feed(b"\x1b[?1049$p");
    assert_eq!(q.replies(), "\x1b[?1049;2$y");
    q.feed(b"\x1b[?6$p");
    assert_eq!(q.replies(), "\x1b[?6;2$y");
}

/// F32. Two queries in one feed answer in order.
#[test]
fn queries_answer_in_order() {
    let mut q = Q::open(4, 8);
    q.feed(b"\x1bP$qm\x1b\\\x1bP+q436f\x1b\\");
    assert_eq!(
        q.reply_list(),
        vec![
            "\x1bP1$r0m\x1b\\".to_owned(),
            "\x1bP1+r436f=323536\x1b\\".to_owned(),
        ]
    );
}

/// F33. Characterisation: a bare `ESC` ends an in-flight query and the engine
/// answers from what arrived, exactly as it finishes a partial Sixel.
///
/// So `RIS`'s own buffer clear is unreachable in practice, which is what the
/// implementation's comment claims; the buffer is nonetheless left clean for
/// the next request.
#[test]
fn a_bare_escape_finishes_an_in_flight_query() {
    let mut q = Q::open(4, 8);
    q.feed(b"\x1bP+q436f");
    q.feed(b"\x1bc"); // RIS: the ESC unhooks the DCS first
    assert_eq!(q.replies(), "\x1bP1+r436f=323536\x1b\\");

    q.feed(b"\x1bP+q436f\x1b\\");
    assert_eq!(
        q.replies(),
        "\x1bP1+r436f=323536\x1b\\",
        "the buffer carried stale bytes"
    );

    // A `DECSTR` after a complete query leaves nothing behind either.
    q.feed(b"\x1b[!p");
    q.feed(b"\x1bP$qm\x1b\\");
    assert_eq!(q.replies(), "\x1bP1$r0m\x1b\\");
}
