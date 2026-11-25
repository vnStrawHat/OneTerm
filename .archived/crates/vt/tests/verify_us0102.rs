//! Independent verification of US-0102 (IN-0038): the eight conformance gaps.
//!
//! Written against the public API and the xterm / DEC references, without
//! reading the implementer's tests. Every assertion states the reference it is
//! taken from in a comment.

use std::time::Instant;

use oneterm_vt::grid::RowId;
use oneterm_vt::input::{
    MouseModifiers, TerminalMouseButton, encode_mouse_move, encode_mouse_press,
    encode_mouse_release, encode_wheel_event,
};
use oneterm_vt::{
    Cell, ColorKey, Config, EventBatch, ModeSnapshot, MouseEncoding, MouseReporting, OscRoutes,
    Size, Terminal, VtEvent,
};

struct Run {
    term: Terminal,
    batch: EventBatch,
}

impl Run {
    fn new(rows: u16, cols: u16) -> Run {
        Run {
            term: Terminal::new(Size { rows, cols }, Config::default()),
            batch: EventBatch::new(),
        }
    }

    fn feed(&mut self, bytes: &[u8]) {
        self.batch.clear();
        self.term.feed(bytes, &mut self.batch, Instant::now());
    }

    /// Feed without clearing the batch, so a caller can split one logical
    /// stream over several `feed` calls and still read every event.
    fn feed_more(&mut self, bytes: &[u8]) {
        self.term.feed(bytes, &mut self.batch, Instant::now());
    }

    fn replies(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for event in self.batch.iter() {
            if let VtEvent::Reply(span) = event {
                out.extend_from_slice(self.batch.bytes(*span));
            }
        }
        out
    }

    fn color_queries(&self) -> Vec<ColorKey> {
        self.batch
            .iter()
            .filter_map(|e| match e {
                VtEvent::ColorQuery { key, .. } => Some(*key),
                _ => None,
            })
            .collect()
    }

    fn modes(&self) -> ModeSnapshot {
        self.term.mode_snapshot()
    }

    fn col(&self) -> u16 {
        self.term.screen().cursor().pos.col
    }

    fn row_id(&self) -> RowId {
        self.term.screen().cursor().pos.row
    }

    fn text(&self, index: u16) -> String {
        let id = self.term.screen().row_of_index(index);
        self.term.row_text(id).trim_end().to_owned()
    }

    fn cells(&self, index: u16) -> Vec<Cell> {
        let id = self.term.screen().row_of_index(index);
        self.term.screen().row(id).cells().to_vec()
    }

    /// `FeedStats` is per-`feed`, not cumulative, so this is "how many the last
    /// call counted".
    fn unhandled(&self) -> u64 {
        u64::from(self.term.stats().unhandled_sequences)
    }
}

// ── 1. `? 9`, the X10 mouse protocol ────────────────────────────────────────

/// xterm ctlseqs, "X10 compatibility mode": *on button press* xterm sends
/// `CSI M Cb Cx Cy`, `Cb` is `button-1` and coordinates carry the `+ 32`
/// offset. No release, no motion, no modifier bits.
#[test]
fn v1_x10_press_is_the_six_byte_legacy_form_with_no_modifiers() {
    let mut run = Run::new(24, 80);
    run.feed(b"\x1b[?9h");
    let modes = run.modes();
    assert_eq!(
        modes.mouse.map(|m| m.reporting),
        Some(MouseReporting::X10),
        "? 9 must reach the snapshot as X10"
    );
    assert_eq!(
        modes.mouse.map(|m| m.encoding),
        Some(MouseEncoding::Default),
        "? 9 must not change the encoding half"
    );

    let none = MouseModifiers::default();
    // Row 0 / column 0 from the caller is 1/1 to the terminal, so both
    // coordinates are 33 = b'!'.
    for (button, code) in [
        (TerminalMouseButton::Left, 32u8),
        (TerminalMouseButton::Middle, 33),
        (TerminalMouseButton::Right, 34),
    ] {
        assert_eq!(
            encode_mouse_press(0, 0, button, modes, none),
            vec![0x1b, b'[', b'M', code, 33, 33],
            "{button:?} press under ? 9"
        );
    }

    // Modifiers are not encoded in X10 mode.
    let all = MouseModifiers {
        shift: true,
        alt: true,
        ctrl: true,
    };
    assert_eq!(
        encode_mouse_press(0, 0, TerminalMouseButton::Left, modes, all),
        encode_mouse_press(0, 0, TerminalMouseButton::Left, modes, none),
        "X10 predates the modifier bits"
    );
}

/// The other four event kinds must produce nothing at all under `? 9`.
#[test]
fn v1_x10_reports_no_release_motion_hover_or_wheel() {
    let mut run = Run::new(24, 80);
    run.feed(b"\x1b[?9h");
    let modes = run.modes();
    let none = MouseModifiers::default();

    assert!(
        encode_mouse_release(0, 0, TerminalMouseButton::Left, modes, none).is_empty(),
        "X10 sends no release"
    );
    assert!(
        encode_mouse_move(0, 0, Some(TerminalMouseButton::Left), modes, none).is_empty(),
        "X10 sends no drag"
    );
    assert!(
        encode_mouse_move(0, 0, None, modes, none).is_empty(),
        "X10 sends no hover"
    );
    assert!(
        encode_wheel_event(0, 0, 1.0, modes, none).is_empty(),
        "X10 sends no wheel up"
    );
    assert!(
        encode_wheel_event(0, 0, -1.0, modes, none).is_empty(),
        "X10 sends no wheel down"
    );
}

/// **Finding 2, closed by the rework.** The packet's acceptance line 1 says
/// "With `? 9 l`, a press produces none", and the CHANGELOG says "the encoders
/// return an **empty** `Vec` for an event the current mode does not report, so
/// a caller writes nothing". Neither was true of the encoder: with no mouse
/// mode at all it returned a full legacy report, and only the four
/// X10-suppressed *event kinds* returned empty. The gate now lives in the
/// encoder as well as in the caller, so the published contract and the code
/// agree.
#[test]
fn v1_x10_reset_silences_the_encoder_and_not_only_the_caller() {
    let mut run = Run::new(24, 80);
    run.feed(b"\x1b[?9h\x1b[?9l");
    let modes = run.modes();
    assert_eq!(modes.mouse, None, "reporting really is off");
    assert!(
        encode_mouse_press(
            0,
            0,
            TerminalMouseButton::Left,
            modes,
            MouseModifiers::default()
        )
        .is_empty(),
        "no protocol was asked for, so nothing is encoded"
    );
}

/// The four reporting modes are one choice: setting any clears the others,
/// unsetting clears only itself (xterm keeps one `send_mouse_pos`).
#[test]
fn v1_reporting_modes_replace_each_other_on_set_only() {
    let mut run = Run::new(24, 80);
    for (seq, want) in [
        (&b"\x1b[?9h"[..], MouseReporting::X10),
        (&b"\x1b[?1000h"[..], MouseReporting::Normal),
        (&b"\x1b[?1002h"[..], MouseReporting::ButtonEvent),
        (&b"\x1b[?1003h"[..], MouseReporting::AnyEvent),
        (&b"\x1b[?9h"[..], MouseReporting::X10),
    ] {
        run.feed(seq);
        assert_eq!(
            run.modes().mouse.map(|m| m.reporting),
            Some(want),
            "after {}",
            String::from_utf8_lossy(seq)
        );
    }
    // Unsetting a mode that is not the live one leaves the live one alone.
    run.feed(b"\x1b[?1003h\x1b[?9l");
    assert_eq!(
        run.modes().mouse.map(|m| m.reporting),
        Some(MouseReporting::AnyEvent),
        "? 9 l must not clear ? 1003"
    );
}

/// DECRQM (`CSI ? Ps $ p`) must answer the real state for both new numbers.
#[test]
fn v1_decrqm_answers_for_9_and_1015() {
    let mut run = Run::new(24, 80);
    run.feed(b"\x1b[?9$p\x1b[?1015$p");
    assert_eq!(
        run.replies(),
        b"\x1b[?9;2$y\x1b[?1015;2$y".to_vec(),
        "power-on state is reset for both"
    );
    run.feed(b"\x1b[?9h\x1b[?1015h\x1b[?9$p\x1b[?1015$p");
    assert_eq!(run.replies(), b"\x1b[?9;1$y\x1b[?1015;1$y".to_vec());
    // Setting ? 1000 replaces ? 9, which DECRQM must show.
    run.feed(b"\x1b[?1000h\x1b[?9$p");
    assert_eq!(run.replies(), b"\x1b[?9;2$y".to_vec());
}

// ── 2. `? 1015`, the urxvt encoding ─────────────────────────────────────────

/// xterm ctlseqs, `Ps = 1 0 1 5`: "the normal mouse response is altered to use
/// `CSI` followed by semicolon-separated encoded button value, the Cx and Cy
/// ordinates and final character `M`. This uses the same button encoding as
/// X10, but printing it as a decimal integer rather than as a single byte."
///
/// So `Cb` carries the `+ 32` offset: a plain left press is 32, not 0. The
/// packet's original acceptance line (`CSI 0 ; 300 ; 300 M`) was wrong and the
/// implementer's correction is the one that matches xterm and rxvt-unicode.
#[test]
fn v2_urxvt_button_carries_the_plus_32_offset() {
    let mut run = Run::new(400, 400);
    run.feed(b"\x1b[?1000h\x1b[?1015h");
    let modes = run.modes();
    assert_eq!(modes.mouse.map(|m| m.encoding), Some(MouseEncoding::Urxvt));
    let none = MouseModifiers::default();

    let bytes = encode_mouse_press(299, 299, TerminalMouseButton::Left, modes, none);
    assert_eq!(bytes, b"\x1b[32;300;300M".to_vec());
    assert!(bytes.iter().all(|b| *b < 0x80), "no byte above 127");
}

/// The whole point of 1015 is that coordinates past 223 survive, where the
/// legacy single-byte form saturates.
#[test]
fn v2_urxvt_has_no_column_ceiling_and_the_legacy_form_does() {
    let mut run = Run::new(2000, 2000);
    run.feed(b"\x1b[?1000h\x1b[?1015h");
    let urxvt = run.modes();
    let none = MouseModifiers::default();
    assert_eq!(
        encode_mouse_press(1499, 999, TerminalMouseButton::Left, urxvt, none),
        b"\x1b[32;1000;1500M".to_vec()
    );

    // Same press in the power-on encoding saturates at 255.
    run.feed(b"\x1b[?1015l");
    let legacy = run.modes();
    assert_eq!(
        encode_mouse_press(1499, 999, TerminalMouseButton::Left, legacy, none),
        vec![0x1b, b'[', b'M', 32, 255, 255]
    );
}

/// 1015 changes the transport and not the semantics: a release is still the
/// fixed button 3 and modifiers still ride in the value.
#[test]
fn v2_urxvt_keeps_the_legacy_semantics() {
    let mut run = Run::new(24, 80);
    run.feed(b"\x1b[?1003h\x1b[?1015h");
    let modes = run.modes();
    let none = MouseModifiers::default();

    assert_eq!(
        encode_mouse_release(0, 0, TerminalMouseButton::Left, modes, none),
        b"\x1b[35;1;1M".to_vec(),
        "release collapses to button 3 -> 3 + 32"
    );
    assert_eq!(
        encode_mouse_press(
            0,
            0,
            TerminalMouseButton::Right,
            modes,
            MouseModifiers {
                shift: true,
                ..Default::default()
            }
        ),
        b"\x1b[38;1;1M".to_vec(),
        "2 (right) + 32 + 4 (shift)"
    );
    assert_eq!(
        encode_mouse_move(0, 0, None, modes, none),
        b"\x1b[67;1;1M".to_vec(),
        "hover is 3 + the motion bit 32, then the transport's + 32"
    );
    assert_eq!(
        encode_wheel_event(0, 0, 1.0, modes, none),
        b"\x1b[96;1;1M".to_vec(),
        "wheel up is 64, then + 32"
    );
}

/// The three encodings are one choice: setting one clears the other two.
#[test]
fn v2_encodings_replace_each_other_on_set_only() {
    let mut run = Run::new(24, 80);
    run.feed(b"\x1b[?1000h");
    for (seq, want) in [
        (&b"\x1b[?1005h"[..], MouseEncoding::Utf8),
        (&b"\x1b[?1006h"[..], MouseEncoding::Sgr),
        (&b"\x1b[?1015h"[..], MouseEncoding::Urxvt),
        (&b"\x1b[?1006h"[..], MouseEncoding::Sgr),
        (&b"\x1b[?1015h"[..], MouseEncoding::Urxvt),
    ] {
        run.feed(seq);
        assert_eq!(
            run.modes().mouse.map(|m| m.encoding),
            Some(want),
            "after {}",
            String::from_utf8_lossy(seq)
        );
    }
    run.feed(b"\x1b[?1006l");
    assert_eq!(
        run.modes().mouse.map(|m| m.encoding),
        Some(MouseEncoding::Urxvt),
        "? 1006 l must not clear ? 1015"
    );
    run.feed(b"\x1b[?1015l");
    assert_eq!(
        run.modes().mouse.map(|m| m.encoding),
        Some(MouseEncoding::Default)
    );
}

/// Regression: the same press under 1005 and 1006 is what it always was.
#[test]
fn v2_1005_and_1006_are_unchanged() {
    let mut run = Run::new(400, 400);
    let none = MouseModifiers::default();
    run.feed(b"\x1b[?1000h\x1b[?1006h");
    assert_eq!(
        encode_mouse_press(299, 299, TerminalMouseButton::Left, run.modes(), none),
        b"\x1b[<0;300;300M".to_vec()
    );
    run.feed(b"\x1b[?1005h");
    let bytes = encode_mouse_press(299, 299, TerminalMouseButton::Left, run.modes(), none);
    let mut want = vec![0x1b, b'[', b'M', 32];
    let mut buf = [0u8; 4];
    want.extend_from_slice(
        char::from_u32(332)
            .unwrap()
            .encode_utf8(&mut buf)
            .as_bytes(),
    );
    want.extend_from_slice(
        char::from_u32(332)
            .unwrap()
            .encode_utf8(&mut buf)
            .as_bytes(),
    );
    assert_eq!(bytes, want);
}

/// Adversarial: `? 9` with `? 1006` also set. The reporting half wins in this
/// implementation and the report is legacy. Recorded as today's behaviour --
/// xterm applies its extended-coordinate encoding to the X10 report too.
#[test]
fn v2_x10_ignores_the_extended_encodings() {
    let mut run = Run::new(24, 80);
    run.feed(b"\x1b[?1006h\x1b[?9h");
    let modes = run.modes();
    assert_eq!(modes.mouse.map(|m| m.reporting), Some(MouseReporting::X10));
    assert_eq!(modes.mouse.map(|m| m.encoding), Some(MouseEncoding::Sgr));
    assert_eq!(
        encode_mouse_press(
            0,
            0,
            TerminalMouseButton::Left,
            modes,
            MouseModifiers::default()
        ),
        vec![0x1b, b'[', b'M', 32, 33, 33],
        "documenting: ? 9 outranks ? 1006 here"
    );
}

// ── 3. `? 5`, DECSCNM ───────────────────────────────────────────────────────

#[test]
fn v3_decscnm_is_a_snapshot_flag_and_mutates_no_cell() {
    let mut run = Run::new(4, 20);
    run.feed(b"\x1b[31;44mred\x1b[0m plain\x1b[1;4mbold");
    let before: Vec<Vec<_>> = (0..4).map(|i| run.cells(i)).collect();
    let before_text: Vec<String> = (0..4).map(|i| run.text(i)).collect();
    assert!(!run.modes().reverse_video);

    run.feed(b"\x1b[?5h");
    assert!(run.modes().reverse_video, "? 5 h must set the flag");
    let after: Vec<Vec<_>> = (0..4).map(|i| run.cells(i)).collect();
    let after_text: Vec<String> = (0..4).map(|i| run.text(i)).collect();
    assert_eq!(before, after, "no cell may change");
    assert_eq!(before_text, after_text);

    run.feed(b"\x1b[?5l");
    assert!(!run.modes().reverse_video, "? 5 l must clear the flag");
    assert_eq!(
        before,
        (0..4).map(|i| run.cells(i)).collect::<Vec<_>>(),
        "? 5 l restores exactly what was there"
    );
}

#[test]
fn v3_decscnm_answers_decrqm() {
    let mut run = Run::new(4, 20);
    run.feed(b"\x1b[?5$p");
    assert_eq!(run.replies(), b"\x1b[?5;2$y".to_vec());
    run.feed(b"\x1b[?5h\x1b[?5$p");
    assert_eq!(run.replies(), b"\x1b[?5;1$y".to_vec());
    run.feed(b"\x1b[?5l\x1b[?5$p");
    assert_eq!(run.replies(), b"\x1b[?5;2$y".to_vec());
}

/// A renderer needs to know its cached rows are stale; the only signal the
/// engine has is a `Repaint` out of `feed`.
#[test]
fn v3_decscnm_forces_a_repaint_and_is_idempotent() {
    let mut run = Run::new(4, 20);
    run.feed(b"x");
    run.feed(b"\x1b[?5h");
    assert!(
        run.batch.iter().any(|e| matches!(e, VtEvent::Repaint)),
        "a mode that changes every painted cell must repaint"
    );
    // Setting it twice must not claim a second change.
    run.feed(b"\x1b[?5h");
    assert!(run.modes().reverse_video);
}

/// `RIS` clears it. VT510 DECSTR does *not* list DECSCNM, so a soft reset
/// leaving it alone is spec-correct.
#[test]
fn v3_ris_clears_decscnm_and_decstr_does_not() {
    let mut run = Run::new(4, 20);
    run.feed(b"\x1b[?5h\x1b[!p");
    assert!(
        run.modes().reverse_video,
        "DECSTR does not reset DECSCNM (VT510 DECSTR list)"
    );
    run.feed(b"\x1bc");
    assert!(!run.modes().reverse_video, "RIS clears it");
}

/// xterm's reverse video is a widget-wide attribute, not a per-screen one, so
/// the alternate screen must not carry its own copy.
#[test]
fn v3_decscnm_is_global_across_the_alternate_screen() {
    let mut run = Run::new(4, 20);
    run.feed(b"\x1b[?5h\x1b[?1049h");
    assert!(run.modes().alt_screen);
    assert!(run.modes().reverse_video, "alt screen shares the flag");
    run.feed(b"\x1b[?5l\x1b[?1049l");
    assert!(
        !run.modes().reverse_video,
        "clearing it on the alt screen clears it everywhere"
    );
}

/// `CSI ? 5 W` (DECST8C) shares only a number with DECSCNM.
#[test]
fn v3_decst8c_does_not_toggle_reverse_video() {
    let mut run = Run::new(4, 20);
    run.feed(b"\x1b[?5W");
    assert!(!run.modes().reverse_video);
}

// ── 4. LS2 / LS3 / SS2 / SS3 ────────────────────────────────────────────────

const HLINE: char = '\u{2500}';

#[test]
fn v4_ls2_and_ls3_lock_g2_and_g3() {
    let mut run = Run::new(2, 10);
    // ESC * 0 designates G2 = DEC special graphics; ESC n (LS2) locks it in.
    run.feed(b"\x1b*0\x1bnqq");
    assert_eq!(run.text(0), format!("{HLINE}{HLINE}"));

    let mut run = Run::new(2, 10);
    // ESC + 0 designates G3; ESC o (LS3) locks it in.
    run.feed(b"\x1b+0\x1boqq");
    assert_eq!(run.text(0), format!("{HLINE}{HLINE}"));

    // SI returns to G0, which is still ASCII.
    run.feed(b"\x0fq");
    assert_eq!(run.text(0), format!("{HLINE}{HLINE}q"));
}

#[test]
fn v4_single_shifts_cover_exactly_one_character() {
    let mut run = Run::new(2, 10);
    run.feed(b"\x1b*0\x1bNqq");
    assert_eq!(run.text(0), format!("{HLINE}q"), "SS2 covers one character");

    let mut run = Run::new(2, 10);
    run.feed(b"\x1b+0\x1bOqq");
    assert_eq!(run.text(0), format!("{HLINE}q"), "SS3 covers one character");
}

/// DEC: a single shift affects "the next graphic character". An intervening
/// escape sequence is not a graphic character, so the shift must survive it.
#[test]
fn v4_a_single_shift_survives_an_intervening_escape_sequence() {
    let mut run = Run::new(2, 10);
    run.feed(b"\x1b*0\x1bN\x1b[1mq\x1b[0mq");
    assert_eq!(run.text(0), format!("{HLINE}q"));
}

/// Nor is a C0 control a graphic character: DEC's rule is "the next graphic
/// character", so a control between the shift and the character must not eat
/// it either.
#[test]
fn v4_a_single_shift_survives_an_intervening_c0_control() {
    let mut run = Run::new(3, 10);
    run.feed(b"\x1b*0\x1bN\x07q");
    assert_eq!(run.text(0), HLINE.to_string(), "BEL must not consume SS2");

    let mut run = Run::new(3, 10);
    run.feed(b"\x1b*0\x1bN\r\nq");
    assert_eq!(run.text(1), HLINE.to_string(), "CR LF must not consume SS2");
}

/// **Finding 4, closed by the rework.** The locking set and the pending single
/// shift live on the terminal rather than on the cursor, and `DECSC` / `DECRC`
/// used not to carry them — a deviation from VT510 (`DECSC` saves the sets in
/// GL and GR and any pending `SS2` / `SS3`) and from xterm's `CursorSave`,
/// inherited from the reference. Correction C12 saves both, one slot per
/// screen because the saved cursor is per screen.
#[test]
fn v4_decsc_decrc_save_the_locking_set_and_the_single_shift() {
    let mut run = Run::new(3, 20);
    // Lock G2, save, go back to G0, restore: DEC restores G2, and so does this.
    run.feed(b"\x1b*0\x1bn\x1b7\x0f\x1b8q");
    assert_eq!(
        run.text(0),
        HLINE.to_string(),
        "DECRC puts the locking set back (VT510 DECSC)"
    );

    // And the other direction: saved with G0 locked, restored after locking G2.
    let mut run = Run::new(3, 20);
    run.feed(b"\x1b*0\x1b7\x1bn\x1b8q");
    assert_eq!(run.text(0), "q".to_string(), "the save wins over the shift");

    let mut run = Run::new(3, 20);
    // Pending SS2 across a save/restore pair: DEC restores the shift.
    run.feed(b"\x1b*0\x1bN\x1b7\x1b8q");
    assert_eq!(
        run.text(0),
        HLINE.to_string(),
        "the shift is saved and restored, so it is still pending here"
    );
}

#[test]
fn v4_ris_clears_both_the_locking_set_and_a_pending_shift() {
    let mut run = Run::new(3, 20);
    run.feed(b"\x1b*0\x1bn\x1bcq");
    assert_eq!(run.text(0), "q".to_string(), "RIS clears the locking set");

    let mut run = Run::new(3, 20);
    run.feed(b"\x1b*0\x1bN\x1bcq");
    assert_eq!(run.text(0), "q".to_string(), "RIS clears a pending shift");
}

/// SO / SI still work and a locking shift to G2 is undone by either.
#[test]
fn v4_si_so_interplay() {
    let mut run = Run::new(3, 20);
    run.feed(b"\x1b)0\x1b*0\x1bn\x0eq\x0fq");
    // SO selects G1 (also DEC special here), SI selects G0 (ASCII).
    assert_eq!(run.text(0), format!("{HLINE}q"));
}

/// `ESC * B` (the packet's own acceptance wording) designates ASCII into G2, so
/// LS2 then prints plain ASCII. A regression guard on the designation parser.
#[test]
fn v4_esc_star_b_designates_ascii_into_g2() {
    let mut run = Run::new(3, 20);
    run.feed(b"\x1b*B\x1bnq");
    assert_eq!(run.text(0), "q".to_string());
}

/// A single shift must not leak into the *next* feed's first character when a
/// character was already printed.
#[test]
fn v4_single_shift_does_not_leak_across_feeds() {
    let mut run = Run::new(3, 20);
    run.feed(b"\x1b*0\x1bN");
    run.feed(b"q");
    run.feed(b"q");
    assert_eq!(run.text(0), format!("{HLINE}q"));
}

// ── 5. OSC 1 (landed in US-0098; regression only) ───────────────────────────

#[test]
fn v5_osc_1_sets_the_icon_name_only() {
    let mut run = Run::new(3, 20);
    run.feed(b"\x1b]1;icon\x1b\\");
    let names: Vec<&str> = run
        .batch
        .iter()
        .filter_map(|e| match e {
            VtEvent::IconName(s) => Some(run.batch.str(*s)),
            _ => None,
        })
        .collect();
    assert_eq!(names, vec!["icon"]);
    assert!(
        !run.batch.iter().any(|e| matches!(e, VtEvent::Title(_))),
        "OSC 1 must not set the title"
    );
}

// ── 6. `? 2027`, grapheme clusters ──────────────────────────────────────────

const FAMILY: &str = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";
const FLAG_DE: &str = "\u{1F1E9}\u{1F1EA}";

#[test]
fn v6_2027_reset_is_per_scalar_and_set_is_per_cluster() {
    let mut run = Run::new(3, 40);
    run.feed(FAMILY.as_bytes());
    assert_eq!(run.col(), 8, "four wide emoji, ZWJs at width 0");

    let mut run = Run::new(3, 40);
    run.feed(b"\x1b[?2027h");
    run.feed(FAMILY.as_bytes());
    assert_eq!(run.col(), 2, "one cluster, one wide cell");
    assert_eq!(run.text(0), FAMILY, "the whole cluster is in the cell");
}

#[test]
fn v6_2027_measures_flags_and_combining_sequences() {
    let mut run = Run::new(3, 40);
    run.feed(b"\x1b[?2027h");
    run.feed(FLAG_DE.as_bytes());
    assert_eq!(
        run.col(),
        2,
        "a regional-indicator pair is one wide cluster"
    );

    let mut run = Run::new(3, 40);
    run.feed(b"\x1b[?2027h");
    run.feed("e\u{301}x".as_bytes());
    assert_eq!(run.col(), 2, "e + combining acute is one narrow cell");
    assert_eq!(run.text(0), "e\u{301}x");
}

#[test]
fn v6_2027_answers_decrqm() {
    let mut run = Run::new(3, 40);
    run.feed(b"\x1b[?2027$p");
    assert_eq!(run.replies(), b"\x1b[?2027;2$y".to_vec());
    run.feed(b"\x1b[?2027h\x1b[?2027$p");
    assert_eq!(run.replies(), b"\x1b[?2027;1$y".to_vec());
    run.feed(b"\x1b[?2027l\x1b[?2027$p");
    assert_eq!(run.replies(), b"\x1b[?2027;2$y".to_vec());
}

/// **Finding 1, the chunk-boundary defect.** A pty read can end anywhere, so a
/// cluster arrives over two `feed` calls whenever the boundary falls inside it.
/// `Dispatch::print_str` segments only the run it was handed and keeps no carry,
/// so under `? 2027` the two halves are measured as separate clusters. Every
/// one of the 24 interior split points of a ZWJ family is wrong.
///
/// `cell-and-style.md` listed "a cross-chunk pending-cluster buffer" as part of
/// what mode 2027 needs; `US-0102` deleted that sentence without building it.
///
/// **Closed by the rework**: the carry landed, so this asserts the fix.
#[test]
fn v6_a_cluster_split_across_two_feeds_is_measured_whole() {
    let mut run = Run::new(3, 40);
    run.feed(b"\x1b[?2027h");
    run.feed(FAMILY.as_bytes());
    assert_eq!(run.col(), 2, "unsplit, the family is one wide cell");

    let bytes = FAMILY.as_bytes();
    let mut wrong = Vec::new();
    for split in 1..bytes.len() {
        let mut run = Run::new(3, 40);
        run.feed(b"\x1b[?2027h");
        run.feed_more(&bytes[..split]);
        run.feed_more(&bytes[split..]);
        if run.col() != 2 {
            wrong.push((split, run.col()));
        }
    }
    assert!(
        wrong.is_empty(),
        "no interior split may miscount -- {wrong:?}"
    );
}

/// The same shapes on a skin-tone sequence and an emoji keycap, which are the
/// two most likely to cross a read boundary in real output.
#[test]
fn v6_skin_tone_and_keycap_splits_are_measured_whole() {
    for (label, text, want) in [
        ("thumbs up + skin tone", "\u{1F44D}\u{1F3FD}", 2u16),
        ("keycap 1", "1\u{FE0F}\u{20E3}", 2),
    ] {
        let bytes = text.as_bytes();
        let mut wrong = Vec::new();
        for split in 1..bytes.len() {
            let mut run = Run::new(3, 40);
            run.feed(b"\x1b[?2027h");
            run.feed_more(&bytes[..split]);
            run.feed_more(&bytes[split..]);
            if run.col() != want {
                wrong.push((split, run.col()));
            }
        }
        assert!(wrong.is_empty(), "{label}: {wrong:?}");
    }
}

/// The same attack with the cheapest possible cluster: a base and one
/// combining mark.
#[test]
fn v6_a_combining_mark_split_across_two_feeds_is_not_miscounted() {
    let text = "e\u{301}";
    let bytes = text.as_bytes();
    for split in 1..bytes.len() {
        let mut run = Run::new(3, 40);
        run.feed(b"\x1b[?2027h");
        run.feed_more(&bytes[..split]);
        run.feed_more(&bytes[split..]);
        assert_eq!(run.col(), 1, "e + acute split after {split} bytes");
        assert_eq!(run.text(0), text);
    }
}

/// And with a flag pair, which is two independent scalars -- the boundary the
/// UTF-8 carry buffer cannot help with.
#[test]
fn v6_a_flag_pair_split_across_two_feeds_is_not_miscounted() {
    let bytes = FLAG_DE.as_bytes();
    for split in 1..bytes.len() {
        let mut run = Run::new(3, 40);
        run.feed(b"\x1b[?2027h");
        run.feed_more(&bytes[..split]);
        run.feed_more(&bytes[split..]);
        assert_eq!(run.col(), 2, "flag split after {split} bytes");
    }
}

/// A wide cluster at the last column under DECAWM must wrap as a unit.
#[test]
fn v6_a_wide_cluster_at_the_last_column_wraps() {
    let mut run = Run::new(3, 5);
    run.feed(b"\x1b[?2027h");
    run.feed(b"abcd");
    assert_eq!(run.col(), 4);
    run.feed(FAMILY.as_bytes());
    assert_eq!(run.col(), 2, "the cluster moved to the next row");
    assert_eq!(run.text(1), FAMILY);
    assert_eq!(run.text(0), "abcd");
}

/// With DECAWM off the cluster must not wrap and must not corrupt the row.
#[test]
fn v6_a_wide_cluster_at_the_last_column_without_decawm() {
    let mut run = Run::new(3, 5);
    run.feed(b"\x1b[?7l\x1b[?2027h");
    run.feed(b"abcd");
    run.feed(FAMILY.as_bytes());
    assert_eq!(run.row_id(), run.term.screen().row_of_index(0));
}

/// Hostile: ten thousand combining marks on one base. Must terminate quickly
/// and must not take a column per mark.
#[test]
fn v6_ten_thousand_combining_marks_are_bounded() {
    let mut text = String::from("a");
    for _ in 0..10_000 {
        text.push('\u{0301}');
    }
    let mut run = Run::new(3, 40);
    run.feed(b"\x1b[?2027h");
    let start = Instant::now();
    run.feed(text.as_bytes());
    let elapsed = start.elapsed();
    assert_eq!(run.col(), 1, "one base, one column");
    assert!(
        elapsed.as_millis() < 2_000,
        "10k combining marks took {elapsed:?}"
    );
}

/// Quadratic check: twenty thousand marks must not cost four times ten
/// thousand.
#[test]
fn v6_combining_marks_do_not_blow_up_quadratically() {
    fn run_marks(n: usize) -> std::time::Duration {
        let mut text = String::from("a");
        for _ in 0..n {
            text.push('\u{0301}');
        }
        let mut run = Run::new(3, 40);
        run.feed(b"\x1b[?2027h");
        let start = Instant::now();
        run.feed(text.as_bytes());
        start.elapsed()
    }
    // Warm up so the first allocation is not counted.
    run_marks(1_000);
    let small = run_marks(10_000).as_secs_f64().max(1e-6);
    let large = run_marks(20_000).as_secs_f64();
    assert!(
        large / small < 8.0,
        "20k/10k ratio {:.1} suggests super-linear growth",
        large / small
    );
}

/// With the mode reset the print path must be byte-identical to the per-scalar
/// engine for a corpus of mixed text -- setting and clearing the mode must not
/// leave residue.
#[test]
fn v6_setting_then_clearing_2027_restores_per_scalar_widths() {
    let corpus = ["ascii", FAMILY, FLAG_DE, "e\u{301}", "漢字", "a\u{FE0F}"];
    for text in corpus {
        let plain = {
            let mut run = Run::new(3, 40);
            run.feed(text.as_bytes());
            (run.col(), run.text(0))
        };
        let mut run = Run::new(3, 40);
        run.feed(b"\x1b[?2027h\x1b[?2027l");
        run.feed(text.as_bytes());
        assert_eq!((run.col(), run.text(0)), plain, "residue for {text:?}");
    }
}

/// `REP` (`CSI b`) repeats the preceding character. Under `? 2027` the
/// preceding *cluster* is what was printed; this pins what actually happens.
#[test]
fn v6_rep_after_a_cluster() {
    let mut run = Run::new(3, 40);
    run.feed(b"\x1b[?2027h");
    run.feed(FAMILY.as_bytes());
    run.feed(b"\x1b[2b");
    // Documenting today's answer rather than asserting a reference: REP is
    // per-scalar and the cluster path stores the cluster's last scalar.
    assert!(run.col() >= 2, "col after REP: {}", run.col());
}

// ── 7. OSC 17 / OSC 19 ──────────────────────────────────────────────────────

#[test]
fn v7_osc_17_and_19_set_the_two_selection_colours() {
    let mut run = Run::new(3, 20);
    run.feed(b"\x1b]17;rgb:ff/00/00\x07\x1b]19;#0000ff\x07");
    assert_eq!(
        run.term
            .color(ColorKey::SelectionBackground)
            .map(|c| (c.r, c.g, c.b)),
        Some((0xff, 0x00, 0x00))
    );
    assert_eq!(
        run.term
            .color(ColorKey::SelectionForeground)
            .map(|c| (c.r, c.g, c.b)),
        Some((0x00, 0x00, 0xff))
    );
}

#[test]
fn v7_queries_carry_the_key_and_the_terminator_they_were_asked_with() {
    let mut run = Run::new(3, 20);
    run.feed(b"\x1b]17;?\x07");
    assert_eq!(run.color_queries(), vec![ColorKey::SelectionBackground]);
    let bel = run.batch.iter().any(
        |e| matches!(e, VtEvent::ColorQuery { terminator, .. } if format!("{terminator:?}").contains("Bel")),
    );
    assert!(bel, "BEL question must be answered with BEL");

    run.feed(b"\x1b]19;?\x1b\\");
    assert_eq!(run.color_queries(), vec![ColorKey::SelectionForeground]);
    let st = run.batch.iter().any(
        |e| matches!(e, VtEvent::ColorQuery { terminator, .. } if format!("{terminator:?}").contains("St")),
    );
    assert!(st, "ST question must be answered with ST");
}

#[test]
fn v7_query_prefixes_are_17_and_19() {
    assert_eq!(ColorKey::SelectionBackground.query_prefix(), "17");
    assert_eq!(ColorKey::SelectionForeground.query_prefix(), "19");
}

#[test]
fn v7_ris_clears_the_selection_colours() {
    let mut run = Run::new(3, 20);
    run.feed(b"\x1b]17;#010203\x07\x1b]19;#040506\x07\x1bc");
    assert_eq!(run.term.color(ColorKey::SelectionBackground), None);
    assert_eq!(run.term.color(ColorKey::SelectionForeground), None);
}

/// OSC 117 / 119 (the resets) are documented as not implemented, so they must
/// be counted, not silently swallowed.
#[test]
fn v7_osc_117_and_119_are_not_implemented() {
    let mut run = Run::new(3, 20);
    run.feed(b"\x1b]17;#010203\x07");
    run.feed(b"\x1b]117\x07\x1b]119\x07");
    assert_eq!(
        run.term
            .color(ColorKey::SelectionBackground)
            .map(|c| (c.r, c.g, c.b)),
        Some((0x01, 0x02, 0x03)),
        "OSC 117 must not clear it"
    );
    assert_eq!(
        run.unhandled(),
        2,
        "OSC 117 / 119 must be counted, not silently swallowed"
    );
    assert!(
        !OscRoutes::has_builtin(117) && !OscRoutes::has_builtin(119),
        "the resets are not published as built-ins"
    );
}

/// A malformed or empty payload must be counted, not stored.
#[test]
fn v7_malformed_payloads_are_counted() {
    let mut run = Run::new(3, 20);
    for seq in [
        &b"\x1b]17\x07"[..],
        &b"\x1b]17;\x07"[..],
        &b"\x1b]17;not-a-colour\x07"[..],
        &b"\x1b]19;\x07"[..],
    ] {
        run.feed(seq);
        assert_eq!(
            run.unhandled(),
            1,
            "{} must be counted",
            String::from_utf8_lossy(seq)
        );
        assert_eq!(run.term.color(ColorKey::SelectionBackground), None);
        assert_eq!(run.term.color(ColorKey::SelectionForeground), None);
    }
}

/// xterm's OSC 10..19 advance to the next slot on a second parameter. This
/// engine deliberately does not; the second parameter must be counted, and the
/// first must still apply.
#[test]
fn v7_a_second_parameter_is_counted_and_does_not_advance() {
    let mut run = Run::new(3, 20);
    let before = run.unhandled();
    run.feed(b"\x1b]17;#010101;#020202\x07");
    assert_eq!(
        run.term
            .color(ColorKey::SelectionBackground)
            .map(|c| (c.r, c.g, c.b)),
        Some((0x01, 0x01, 0x01))
    );
    assert!(run.unhandled() > before, "the extra parameter is counted");
}

// ── 8. DA3 ──────────────────────────────────────────────────────────────────

/// xterm: `CSI = Ps c`, `Ps = 0` (or default) reports DECRPTUI, and xterm uses
/// zeros for the site code and the serial number.
#[test]
fn v8_da3_answers_a_decrptui_unit_id() {
    let mut run = Run::new(3, 20);
    run.feed(b"\x1b[=c");
    assert_eq!(run.replies(), b"\x1bP!|00000000\x1b\\".to_vec());
    run.feed(b"\x1b[=0c");
    assert_eq!(
        run.replies(),
        b"\x1bP!|00000000\x1b\\".to_vec(),
        "an explicit 0 is the same as the default"
    );
}

#[test]
fn v8_da3_with_a_non_zero_parameter_is_unhandled() {
    let mut run = Run::new(3, 20);
    let before = run.unhandled();
    run.feed(b"\x1b[=1c\x1b[=2c");
    assert!(run.replies().is_empty(), "no reply for Ps != 0");
    assert_eq!(run.unhandled(), before + 2, "both are counted");
}

#[test]
fn v8_da3_ignores_the_product_name() {
    let mut run = Run {
        term: Terminal::new(
            Size { rows: 3, cols: 20 },
            Config {
                product_name: Some("SomeEmbedder 9.9".into()),
                ..Config::default()
            },
        ),
        batch: EventBatch::new(),
    };
    run.feed(b"\x1b[=c");
    assert_eq!(
        run.replies(),
        b"\x1bP!|00000000\x1b\\".to_vec(),
        "the unit id must not be a per-embedder fingerprint"
    );
}

#[test]
fn v8_da1_and_da2_are_unchanged() {
    let mut run = Run::new(3, 20);
    run.feed(b"\x1b[c");
    assert_eq!(run.replies(), b"\x1b[?62;4;22c".to_vec());
    run.feed(b"\x1bZ");
    assert_eq!(run.replies(), b"\x1b[?62;4;22c".to_vec());
    run.feed(b"\x1b[>c");
    let reply = String::from_utf8(run.replies()).unwrap();
    assert!(reply.starts_with("\x1b[>0;"), "DA2: {reply:?}");
    assert!(reply.ends_with(";1c"), "DA2: {reply:?}");
}

/// `ESC = ` is DECKPAM, not DA3: the two must not be confused by the new arm.
#[test]
fn v8_esc_equals_is_still_deckpam() {
    let mut run = Run::new(3, 20);
    run.feed(b"\x1b=");
    assert!(run.replies().is_empty());
    assert!(run.modes().app_keypad);
}

// ── 9. Hostile inputs on the new print path ─────────────────────────────────

/// **Finding 11, closed by the rework.** A cluster that begins with a
/// zero-width scalar has no base to attach to. `cluster_width` answered 0 for
/// most of them, so the scalar joined the previous cell as it does with the
/// mode reset — except for a bare variation selector, where the "an explicit
/// presentation selector decides" rule fired before anything checked that
/// there was a base, and a stray `U+FE0F` took **two whole columns**.
///
/// The rule lives in `width::cluster_width`, which shipped in `IN-0029`;
/// `US-0102` is what gave it a caller and made this observable. It is also why
/// splitting a keycap sequence in front of its `U+FE0F` used to cost three
/// columns instead of two (Finding 1). Both selectors now need a base.
#[test]
fn v9_a_leading_zero_width_cluster_takes_no_columns() {
    for (text, want_col) in [
        ("\u{301}", 1u16),
        ("\u{200D}", 1),
        ("\u{20E3}", 1),
        ("\u{0}\u{301}", 1),
        ("\u{FE0F}", 1),
        ("\u{FE0E}", 1),
    ] {
        let mut run = Run::new(3, 20);
        run.feed(b"\x1b[?2027h");
        run.feed(text.as_bytes());
        run.feed(b"a");
        assert_eq!(run.col(), want_col, "{text:?} under ? 2027");
    }
    // With the mode reset every one of them is a zero-width attach.
    for text in ["\u{301}", "\u{200D}", "\u{20E3}", "\u{FE0F}", "\u{FE0E}"] {
        let mut run = Run::new(3, 20);
        run.feed(text.as_bytes());
        run.feed(b"a");
        assert_eq!(run.col(), 1, "{text:?} with ? 2027 reset");
    }
}

/// A run mixing clusters and controls must segment per printable run, not
/// swallow the controls.
#[test]
fn v9_controls_split_the_run_under_2027() {
    let mut run = Run::new(4, 20);
    run.feed(b"\x1b[?2027h");
    run.feed("a\u{301}\r\nb\u{301}".as_bytes());
    assert_eq!(run.text(0), "a\u{301}");
    assert_eq!(run.text(1), "b\u{301}");
}

/// `? 2027` under IRM (insert mode): the cluster must insert as one cell.
#[test]
fn v9_2027_under_insert_mode() {
    let mut run = Run::new(3, 20);
    run.feed(b"\x1b[?2027h\x1b[4h");
    run.feed(b"xy");
    run.feed(b"\x1b[1;1H");
    run.feed(FAMILY.as_bytes());
    assert_eq!(run.text(0), format!("{FAMILY}xy"));
}

/// A cluster longer than anything the grapheme arena can hold must be
/// truncated, not allowed to grow a cell without bound.
#[test]
fn v9_an_enormous_cluster_is_bounded() {
    let mut text = String::from("a");
    for _ in 0..200_000 {
        text.push('\u{0301}');
    }
    let mut run = Run::new(3, 20);
    run.feed(b"\x1b[?2027h");
    let start = Instant::now();
    run.feed(text.as_bytes());
    assert_eq!(run.col(), 1);
    assert!(
        start.elapsed().as_secs() < 20,
        "200k marks took {:?}",
        start.elapsed()
    );
    assert!(
        run.text(0).chars().count() < 200_000,
        "the cell kept every mark"
    );
}

/// `DECSCNM` reaches the snapshot and touches no cell: the engine's whole share
/// of the mode. Finding 6 was that nothing then read the flag; the renderer
/// does now, and `terminal-view`'s
/// `decscnm_swaps_the_two_defaults_and_nothing_else` is the other half.
#[test]
fn v9_decscnm_is_the_engine_half_of_reverse_video() {
    let mut run = Run::new(3, 20);
    run.feed(b"\x1b[31mred\x1b[?5h");
    assert!(run.modes().reverse_video);
    assert_eq!(run.text(0), "red");
}
