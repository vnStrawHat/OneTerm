use crate::render::{MouseProtocol, MouseReporting};

use super::*;

/// `? 1000` plus the encoding under test. The engine's encoding is one
/// value, not a set of bits: `? 1005` and `? 1006` replace each other
/// instead of overlapping, which is what `sgr_wins_over_utf8` now pins.
fn reporting(encoding: MouseEncoding) -> ModeSnapshot {
    ModeSnapshot {
        mouse: Some(MouseProtocol {
            reporting: MouseReporting::Normal,
            encoding,
        }),
        ..ModeSnapshot::default()
    }
}

fn sgr_mode() -> ModeSnapshot {
    reporting(MouseEncoding::Sgr)
}

fn x11_mode() -> ModeSnapshot {
    reporting(MouseEncoding::Default)
}

fn utf8_mode() -> ModeSnapshot {
    reporting(MouseEncoding::Utf8)
}

#[test]
fn sgr_press_left() {
    let s = encode_mouse_press(
        0,
        0,
        TerminalMouseButton::Left,
        sgr_mode(),
        MouseModifiers::default(),
    );
    assert_eq!(s, b"\x1b[<0;1;1M");
}

#[test]
fn sgr_release_left() {
    let s = encode_mouse_release(
        2,
        3,
        TerminalMouseButton::Left,
        sgr_mode(),
        MouseModifiers::default(),
    );
    assert_eq!(s, b"\x1b[<0;4;3m");
}

#[test]
fn sgr_press_with_ctrl() {
    let s = encode_mouse_press(
        0,
        0,
        TerminalMouseButton::Left,
        sgr_mode(),
        MouseModifiers {
            ctrl: true,
            ..Default::default()
        },
    );
    assert_eq!(s, b"\x1b[<16;1;1M");
}

#[test]
fn x11_press_left() {
    let s = encode_mouse_press(
        0,
        0,
        TerminalMouseButton::Left,
        x11_mode(),
        MouseModifiers::default(),
    );
    // button = 0+32 = 32 (space), col = 1+32 = 33 ('!'), row = 1+32 = 33 ('!')
    assert_eq!(s, b"\x1b[M\x20\x21\x21");
}

#[test]
fn x11_release_uses_button_3() {
    let s = encode_mouse_release(
        0,
        0,
        TerminalMouseButton::Left,
        x11_mode(),
        MouseModifiers::default(),
    );
    // x11_code = 3 → 3+32 = 35 = '#', col = 1+32 = 33 ('!'), row = 1+32 = 33 ('!')
    assert_eq!(s, b"\x1b[M\x23\x21\x21");
}

#[test]
fn wheel_up_sgr() {
    let s = encode_wheel_event(5, 5, 1.0, sgr_mode(), MouseModifiers::default());
    assert_eq!(s, b"\x1b[<64;6;6M");
}

#[test]
fn wheel_down_sgr() {
    let s = encode_wheel_event(5, 5, -1.0, sgr_mode(), MouseModifiers::default());
    assert_eq!(s, b"\x1b[<65;6;6M");
}

#[test]
fn move_hover_code_35() {
    let s = encode_mouse_move(0, 0, None, sgr_mode(), MouseModifiers::default());
    // hover: code 3+32 = 35
    assert_eq!(s, b"\x1b[<35;1;1M");
}

#[test]
fn x11_press_with_ctrl() {
    let s = encode_mouse_press(
        0,
        0,
        TerminalMouseButton::Left,
        x11_mode(),
        MouseModifiers {
            ctrl: true,
            ..Default::default()
        },
    );
    // button = 0+32+16(ctrl) = 48 ('0'), col = 1+32 = 33 ('!'), row = 1+32 = 33 ('!')
    assert_eq!(s, b"\x1b[M0!!");
}

#[test]
fn x11_press_with_shift_alt() {
    let s = encode_mouse_press(
        5,
        10,
        TerminalMouseButton::Right,
        x11_mode(),
        MouseModifiers {
            shift: true,
            alt: true,
            ..Default::default()
        },
    );
    // button = 2+32+4(shift)+8(alt) = 46 ('.'), col = 11+32 = 43 ('+'), row = 6+32 = 38 ('&')
    assert_eq!(s, b"\x1b[M.+&");
}

#[test]
fn x11_coordinates_above_127_are_raw_bytes() {
    // Legacy X10/X11 reports are raw bytes: col=200 → 201+32 = 233 must be
    // sent as the single byte 0xE9, never as its 2-byte UTF-8 form
    // (that would be the DECSET 1005 encoding).
    let s = encode_mouse_press(
        200,
        200,
        TerminalMouseButton::Left,
        x11_mode(),
        MouseModifiers::default(),
    );
    assert_eq!(s, [0x1b, b'[', b'M', 0x20, 233, 233]);
}

#[test]
fn x11_coordinates_beyond_byte_range_are_capped() {
    // Without 1005 the wire format is one byte per coordinate: 500+1+32
    // does not fit → capped at 255.
    let s = encode_mouse_press(
        500,
        500,
        TerminalMouseButton::Left,
        x11_mode(),
        MouseModifiers::default(),
    );
    assert_eq!(s, [0x1b, b'[', b'M', 0x20, 255, 255]);
}

#[test]
fn utf8_mouse_encodes_large_coordinates_as_utf8() {
    // DECSET 1005: col=200 → 233 = U+00E9 → UTF-8 0xC3 0xA9.
    let s = encode_mouse_press(
        200,
        200,
        TerminalMouseButton::Left,
        utf8_mode(),
        MouseModifiers::default(),
    );
    assert_eq!(s, [0x1b, b'[', b'M', 0x20, 0xc3, 0xa9, 0xc3, 0xa9]);
}

#[test]
fn utf8_mouse_small_coordinates_match_legacy() {
    // Below 95 the 1005 and legacy encodings are byte-identical.
    let s = encode_mouse_press(
        0,
        0,
        TerminalMouseButton::Left,
        utf8_mode(),
        MouseModifiers::default(),
    );
    assert_eq!(s, b"[M !!");
}

#[test]
fn utf8_mouse_caps_at_two_byte_form() {
    // 1005 only defines the 2-byte form (≤ U+07FF).
    let s = encode_mouse_press(
        5000,
        5000,
        TerminalMouseButton::Left,
        utf8_mode(),
        MouseModifiers::default(),
    );
    assert_eq!(s, [0x1b, b'[', b'M', 0x20, 0xdf, 0xbf, 0xdf, 0xbf]);
}

#[test]
fn sgr_wins_over_utf8() {
    // SGR (1006) replaces 1005 in the engine's one-value encoding, so the
    // output stays decimal ASCII.
    let s = encode_mouse_press(
        200,
        200,
        TerminalMouseButton::Left,
        sgr_mode(),
        MouseModifiers::default(),
    );
    assert_eq!(s, b"[<0;201;201M");
}

#[test]
fn x11_coordinates_at_boundary_95() {
    // col=94 → 95+32 = 127 (DEL) — last coordinate before overflow.
    let s = encode_mouse_press(
        94,
        94,
        TerminalMouseButton::Left,
        x11_mode(),
        MouseModifiers::default(),
    );
    // button = 32, col = 95+32 = 127, row = 95+32 = 127
    assert_eq!(s, b"\x1b[M\x20\x7f\x7f");
}

#[test]
fn sgr_preserves_large_coordinates() {
    // SGR has no +32 offset and supports large coordinates as decimal.
    let s = encode_mouse_press(
        500,
        1000,
        TerminalMouseButton::Left,
        sgr_mode(),
        MouseModifiers::default(),
    );
    assert_eq!(s, b"\x1b[<0;1001;501M");
}

#[test]
fn sgr_release_right_with_modifiers() {
    let s = encode_mouse_release(
        3,
        7,
        TerminalMouseButton::Right,
        sgr_mode(),
        MouseModifiers {
            shift: true,
            ctrl: true,
            ..Default::default()
        },
    );
    // button = 2+4(shift)+16(ctrl) = 22, col = 8, row = 4
    assert_eq!(s, b"\x1b[<22;8;4m");
}

#[test]
fn wheel_x11_mode() {
    let s = encode_wheel_event(5, 5, 1.0, x11_mode(), MouseModifiers::default());
    // code = 64+32 = 96 ('`'), col = 6+32 = 38 ('&'), row = 6+32 = 38 ('&')
    assert_eq!(s, b"\x1b[M`&&");
}

#[test]
fn move_hover_x11() {
    let s = encode_mouse_move(2, 3, None, x11_mode(), MouseModifiers::default());
    // code = 3+32+32 = 67 ('C'), col = 4+32 = 36 ('$'), row = 3+32 = 35 ('#')
    assert_eq!(s, b"\x1b[MC$#");
}
