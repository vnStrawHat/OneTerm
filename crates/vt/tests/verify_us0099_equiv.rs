//! The encoders still answer exactly what they answered before they moved.
//!
//! `mod orig_key` and `mod orig_mouse` below are **frozen copies**: the
//! byte-for-byte production half of `crates/terminal/src/key_encode.rs` and
//! `mouse_encode.rs` as they stood in commit `0558fa2`, the last commit before
//! the move, pasted in by script rather than retyped. They are a fixed oracle,
//! not code under maintenance -- never "fix" them to match the engine. If the
//! engine's answer and the frozen answer ever differ, the engine changed
//! behaviour, which is the whole point of this file.
//!
//! Everything after them is the cross-product: every `NamedKey`, every
//! `KeyMods`, every mouse button / encoding / modifier combination, compared
//! byte for byte against `oneterm_vt::input`.
//!
//! Written by an independent verifier for this move and adopted here so the
//! comparison keeps running.
#![allow(dead_code)]
#![allow(clippy::all)]

#[rustfmt::skip]
mod orig_key {
//! Encode keyboard events → escape sequences for the terminal.
//!
//! Key-to-byte encoding rules:
//! - Defines neutral types (`KeySpec`/`KeyMods`/`NamedKey`) — no dependency on
//!   `keyboard_types` or GPUI. The UI crate maps GPUI key events → `KeySpec`.
//! - Returns bytes only, with NO side effects (scroll/selection/shift-tracking
//!   belong to the view/session, not the encoder).
//! - Returns `None` only when the combination has no terminal encoding
//!   (Ctrl + non-ASCII / multi-codepoint text) → the caller ignores it.

/// Modifier state when encoding a key (bit-agnostic, uses bool for clarity).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyMods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

/// Special key names (not printable characters).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedKey {
    Enter,
    Backspace,
    Delete,
    Tab,
    Escape,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
}

/// Specification of a key event — framework-agnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeySpec {
    /// A character (may be multiple codepoints, e.g. a compose key).
    Character(String),
    /// A special key.
    Named(NamedKey),
}

/// Byte CSI u / xterm modifier: `1 + shift + alt*2 + ctrl*4`.
fn modifier_byte(mods: KeyMods) -> u8 {
    1 + mods.shift as u8 + (mods.alt as u8) * 2 + (mods.ctrl as u8) * 4
}

/// Bytes for Ctrl + a printable ASCII character, following the xterm table.
///
/// Letters map to their C0 control (`byte & 0x1f`, case-insensitive). The
/// digits and punctuation that share a control byte in xterm are listed
/// explicitly; every other ASCII character (`0`, `1`, `9`, `,`, `.`, …) is
/// sent unchanged, as xterm does. Non-ASCII or multi-codepoint text has no
/// Ctrl encoding → `None`.
fn ctrl_bytes(text: &str) -> Option<Vec<u8>> {
    let mut chars = text.chars();
    let ch = chars.next()?;
    if chars.next().is_some() || !ch.is_ascii() {
        return None;
    }
    let byte = match ch {
        'a'..='z' | 'A'..='Z' => (ch as u8) & 0x1f,
        ' ' | '2' | '@' => 0x00,
        '3' | '[' => 0x1b,
        '4' | '\\' => 0x1c,
        '5' | ']' => 0x1d,
        '6' | '^' => 0x1e,
        '7' | '_' | '/' => 0x1f,
        '8' | '?' => 0x7f,
        other => other as u8,
    };
    Some(vec![byte])
}

/// Encode a single key event → escape sequence.
///
/// Returns `None` only when the combination has no terminal encoding — today
/// that is Ctrl + a non-ASCII or multi-codepoint `Character` (there is no
/// control byte for `Ctrl+é`); the caller drops the event.
///
/// `app_cursor` mirrors the terminal's DECCKM state (`TermMode::APP_CURSOR`):
/// when the program has enabled Application Cursor Keys (e.g. vim/less/man
/// send `CSI ?1h`), the plain cursor keys (arrows, Home, End) must use the
/// `ESC O{ch}` form instead of `ESC [{ch}` so the program recognizes them.
///
/// Conventions:
/// - `Character` + `ctrl` → xterm control table (see `ctrl_bytes`).
/// - `Character` + `alt` → `ESC` prefix; combined with `ctrl` the prefix is
///   applied to the control byte (`Ctrl+Alt+a` → `ESC 0x01`).
/// - `Enter` shift/ctrl → CSI u; plain → `\r`.
/// - `Backspace`: ctrl → `0x08`, alt → `ESC DEL`, plain → `0x7f`.
/// - Arrow + (shift|ctrl) → `CSI 1;{mod}{ch}`; plain → `ESC O{ch}` when
///   `app_cursor`, else `ESC [{ch}`.
/// - `Home`/`End` + (shift|ctrl) → `CSI 1;{mod}H/F`; plain → `ESC OH/F`
///   when `app_cursor`, else `ESC [H/F`.
pub fn encode_key(key: &KeySpec, mods: KeyMods, app_cursor: bool) -> Option<Vec<u8>> {
    let shift = mods.shift;
    let ctrl = mods.ctrl;
    let alt = mods.alt;

    let seq: Vec<u8> = match key {
        KeySpec::Character(ch) => {
            let mut bytes = if ctrl {
                ctrl_bytes(ch)?
            } else {
                ch.as_bytes().to_vec()
            };
            if alt {
                // Alt (Meta) prefixes ESC — also for Ctrl+Alt chords.
                bytes.insert(0, 0x1b);
            }
            bytes
        }

        KeySpec::Named(NamedKey::Enter) if shift || ctrl => {
            format!("\x1b[13;{}u", modifier_byte(mods)).into_bytes()
        }
        KeySpec::Named(NamedKey::Enter) => b"\r".to_vec(),

        KeySpec::Named(NamedKey::Backspace) if ctrl => vec![0x08],
        KeySpec::Named(NamedKey::Backspace) if alt => vec![0x1b, 0x7f],
        KeySpec::Named(NamedKey::Backspace) => vec![0x7f],

        KeySpec::Named(NamedKey::Delete) if alt || ctrl || shift => {
            format!("\x1b[3;{}~", modifier_byte(mods)).into_bytes()
        }
        KeySpec::Named(NamedKey::Delete) => b"\x1b[3~".to_vec(),

        KeySpec::Named(NamedKey::Tab) if shift => b"\x1b[Z".to_vec(),
        KeySpec::Named(NamedKey::Tab) => b"\t".to_vec(),

        KeySpec::Named(NamedKey::Escape) => vec![0x1b],

        KeySpec::Named(
            dir @ (NamedKey::ArrowUp
            | NamedKey::ArrowDown
            | NamedKey::ArrowLeft
            | NamedKey::ArrowRight),
        ) => {
            let ch = match dir {
                NamedKey::ArrowUp => 'A',
                NamedKey::ArrowDown => 'B',
                NamedKey::ArrowRight => 'C',
                NamedKey::ArrowLeft => 'D',
                _ => unreachable!(),
            };
            if shift || ctrl || alt {
                format!("\x1b[1;{}{ch}", modifier_byte(mods)).into_bytes()
            } else if app_cursor {
                vec![0x1b, b'O', ch as u8]
            } else {
                vec![0x1b, b'[', ch as u8]
            }
        }

        KeySpec::Named(NamedKey::Home) if shift || ctrl || alt => {
            format!("\x1b[1;{}H", modifier_byte(mods)).into_bytes()
        }
        KeySpec::Named(NamedKey::Home) if app_cursor => b"\x1bOH".to_vec(),
        KeySpec::Named(NamedKey::Home) => b"\x1b[H".to_vec(),

        KeySpec::Named(NamedKey::End) if shift || ctrl || alt => {
            format!("\x1b[1;{}F", modifier_byte(mods)).into_bytes()
        }
        KeySpec::Named(NamedKey::End) if app_cursor => b"\x1bOF".to_vec(),
        KeySpec::Named(NamedKey::End) => b"\x1b[F".to_vec(),

        KeySpec::Named(NamedKey::PageUp) if shift || ctrl || alt => {
            format!("\x1b[5;{}~", modifier_byte(mods)).into_bytes()
        }
        KeySpec::Named(NamedKey::PageUp) => b"\x1b[5~".to_vec(),
        KeySpec::Named(NamedKey::PageDown) if shift || ctrl || alt => {
            format!("\x1b[6;{}~", modifier_byte(mods)).into_bytes()
        }
        KeySpec::Named(NamedKey::PageDown) => b"\x1b[6~".to_vec(),
        KeySpec::Named(NamedKey::Insert) => b"\x1b[2~".to_vec(),
        // F1–F4: xterm SS3 sequences (ESC O P/Q/R/S).
        KeySpec::Named(NamedKey::F1) => b"\x1bOP".to_vec(),
        KeySpec::Named(NamedKey::F2) => b"\x1bOQ".to_vec(),
        KeySpec::Named(NamedKey::F3) => b"\x1bOR".to_vec(),
        KeySpec::Named(NamedKey::F4) => b"\x1bOS".to_vec(),
        // F5–F12: CSI ~ with parameter codes 15–24.
        KeySpec::Named(NamedKey::F5) => b"\x1b[15~".to_vec(),
        KeySpec::Named(NamedKey::F6) => b"\x1b[17~".to_vec(),
        KeySpec::Named(NamedKey::F7) => b"\x1b[18~".to_vec(),
        KeySpec::Named(NamedKey::F8) => b"\x1b[19~".to_vec(),
        KeySpec::Named(NamedKey::F9) => b"\x1b[20~".to_vec(),
        KeySpec::Named(NamedKey::F10) => b"\x1b[21~".to_vec(),
        KeySpec::Named(NamedKey::F11) => b"\x1b[23~".to_vec(),
        KeySpec::Named(NamedKey::F12) => b"\x1b[24~".to_vec(),
        // F13–F24: shifted F1–F12 (CSI ~ with modifier 2 = shift).
        // These are rarely used but xterm maps them to 2P/2Q/.../~ forms.
        KeySpec::Named(NamedKey::F13) => b"\x1b[1;2P".to_vec(),
        KeySpec::Named(NamedKey::F14) => b"\x1b[1;2Q".to_vec(),
        KeySpec::Named(NamedKey::F15) => b"\x1b[1;2R".to_vec(),
        KeySpec::Named(NamedKey::F16) => b"\x1b[1;2S".to_vec(),
        KeySpec::Named(NamedKey::F17) => b"\x1b[15;2~".to_vec(),
        KeySpec::Named(NamedKey::F18) => b"\x1b[17;2~".to_vec(),
        KeySpec::Named(NamedKey::F19) => b"\x1b[18;2~".to_vec(),
        KeySpec::Named(NamedKey::F20) => b"\x1b[19;2~".to_vec(),
        KeySpec::Named(NamedKey::F21) => b"\x1b[20;2~".to_vec(),
        KeySpec::Named(NamedKey::F22) => b"\x1b[21;2~".to_vec(),
        KeySpec::Named(NamedKey::F23) => b"\x1b[23;2~".to_vec(),
        KeySpec::Named(NamedKey::F24) => b"\x1b[24;2~".to_vec(),
    };

    Some(seq)
}
}

#[rustfmt::skip]
mod orig_mouse {
//! Encode mouse events → CSI escape sequences (X10 / X11 / 1005 UTF-8 / SGR-1006).
//!
//! Mouse-event encoding with modifier support. `ModeSnapshot::mouse` decides
//! whether the caller sends at all (`? 1000` / `? 1002` / `? 1003`); this module
//! reads only the **encoding** half of it (`? 1005` / `? 1006`).

use oneterm_vt::ModeSnapshot;
use oneterm_vt::MouseEncoding;

/// Mouse button for terminal encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalMouseButton {
    Left,
    Middle,
    Right,
}

impl TerminalMouseButton {
    /// X11/SGR code (before modifier bits).
    fn code(self) -> u8 {
        match self {
            Self::Left => 0,
            Self::Middle => 1,
            Self::Right => 2,
        }
    }
}

/// Modifiers accompanying a mouse event — added to the button code.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MouseModifiers {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
}

impl MouseModifiers {
    /// Bit mask: shift=4, alt=8, ctrl=16 (per the XTerm standard).
    fn mask(self) -> u8 {
        let mut m = 0;
        if self.shift {
            m += 4;
        }
        if self.alt {
            m += 8;
        }
        if self.ctrl {
            m += 16;
        }
        m
    }
}

/// Which terminator byte an SGR (1006) mouse report uses.
///
/// SGR distinguishes press (`M`) from release (`m`); classic X10/X11 encoding
/// ignores this (release collapses to a fixed button byte).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SgrTerminator {
    /// Button press / motion / wheel — terminator `M`.
    Press,
    /// Button release — terminator `m`.
    Release,
}

/// Encode a single mouse event → the bytes the running app receives.
///
/// `sgr_code`: code for SGR (1006); `x11_code`: code for classic X11 (before the
/// mandatory +32 offset). `terminator` only affects SGR (`m` vs `M`);
/// X11 release uses a fixed button byte = 3.
fn encode(
    sgr_code: u8,
    x11_code: u8,
    row: usize,
    col: usize,
    modes: ModeSnapshot,
    mods: MouseModifiers,
    terminator: SgrTerminator,
) -> Vec<u8> {
    // row/col are 0-indexed from the caller → terminal is 1-indexed.
    let row = row.saturating_add(1);
    let col = col.saturating_add(1);
    let mod_mask = mods.mask();
    let encoding = modes.mouse.map(|mouse| mouse.encoding).unwrap_or_default();
    if encoding == MouseEncoding::Sgr {
        let action = match terminator {
            SgrTerminator::Release => 'm',
            SgrTerminator::Press => 'M',
        };
        return format!("[<{};{};{}{}", sgr_code + mod_mask, col, row, action).into_bytes();
    }

    // X10/X11: `CSI M` followed by button+32, col+32, row+32. The legacy
    // format is raw single bytes (values ≥ 0x80 are sent as-is, not
    // UTF-8 encoded); only when the app enabled DECSET 1005 (`UTF8_MOUSE`)
    // are the coordinates UTF-8 encoded so positions above 223 are
    // representable (xterm ctlseqs, "UTF-8 Mouse Mode").
    let button_byte = x11_code.saturating_add(32) + mod_mask;
    let utf8 = encoding == MouseEncoding::Utf8;
    let mut bytes = vec![0x1b, b'[', b'M', button_byte];
    push_x11_coordinate(&mut bytes, col, utf8);
    push_x11_coordinate(&mut bytes, row, utf8);
    bytes
}

/// Append one 1-indexed X11 coordinate (+32 offset) to `out`.
///
/// Legacy mode is one raw byte, capped at 255. DECSET 1005 (UTF-8 mouse mode)
/// encodes the offset value as UTF-8; xterm and alacritty only ever emit the
/// two-byte form, so the value is capped at 2047 (U+07FF).
fn push_x11_coordinate(out: &mut Vec<u8>, value: usize, utf8: bool) {
    let offset = value.saturating_add(32);
    if !utf8 {
        out.push(offset.min(255) as u8);
        return;
    }
    let capped = offset.min(0x7ff) as u32;
    // `capped` ≤ 0x7FF is always a valid scalar value; the fallback never fires.
    let ch = char::from_u32(capped).unwrap_or(' ');
    let mut buf = [0u8; 4];
    out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
}

/// Mouse press (button down).
pub fn encode_mouse_press(
    row: usize,
    col: usize,
    button: TerminalMouseButton,
    modes: ModeSnapshot,
    mods: MouseModifiers,
) -> Vec<u8> {
    encode(
        button.code(),
        button.code(),
        row,
        col,
        modes,
        mods,
        SgrTerminator::Press,
    )
}

/// Mouse release (button up).
pub fn encode_mouse_release(
    row: usize,
    col: usize,
    button: TerminalMouseButton,
    modes: ModeSnapshot,
    mods: MouseModifiers,
) -> Vec<u8> {
    // X11 collapses release into a fixed button byte = 3; SGR keeps the original
    // button code but changes `M` → `m`.
    encode(
        button.code(),
        3,
        row,
        col,
        modes,
        mods,
        SgrTerminator::Release,
    )
}

/// Mouse motion. `button = None` → hover (no button, code 3).
pub fn encode_mouse_move(
    row: usize,
    col: usize,
    button: Option<TerminalMouseButton>,
    modes: ModeSnapshot,
    mods: MouseModifiers,
) -> Vec<u8> {
    let code = button.map_or(3, TerminalMouseButton::code) + 32;
    encode(code, code, row, col, modes, mods, SgrTerminator::Press)
}

/// Wheel. `delta_y > 0` = scroll up (code 64), `< 0` = scroll down (code 65).
pub fn encode_wheel_event(
    row: usize,
    col: usize,
    delta_y: f64,
    modes: ModeSnapshot,
    mods: MouseModifiers,
) -> Vec<u8> {
    let code = if delta_y > 0.0 { 64 } else { 65 };
    encode(code, code, row, col, modes, mods, SgrTerminator::Press)
}
}

use oneterm_vt::input::{
    KeyMods, KeySpec, MouseModifiers, NamedKey, TerminalMouseButton, encode_key, encode_mouse_move,
    encode_mouse_press, encode_mouse_release, encode_wheel_event,
};
use oneterm_vt::{ModeSnapshot, MouseEncoding, MouseProtocol, MouseReporting};

const NEW_NAMED: [NamedKey; 38] = [
    NamedKey::Enter,
    NamedKey::Backspace,
    NamedKey::Delete,
    NamedKey::Tab,
    NamedKey::Escape,
    NamedKey::ArrowUp,
    NamedKey::ArrowDown,
    NamedKey::ArrowLeft,
    NamedKey::ArrowRight,
    NamedKey::Home,
    NamedKey::End,
    NamedKey::PageUp,
    NamedKey::PageDown,
    NamedKey::Insert,
    NamedKey::F1,
    NamedKey::F2,
    NamedKey::F3,
    NamedKey::F4,
    NamedKey::F5,
    NamedKey::F6,
    NamedKey::F7,
    NamedKey::F8,
    NamedKey::F9,
    NamedKey::F10,
    NamedKey::F11,
    NamedKey::F12,
    NamedKey::F13,
    NamedKey::F14,
    NamedKey::F15,
    NamedKey::F16,
    NamedKey::F17,
    NamedKey::F18,
    NamedKey::F19,
    NamedKey::F20,
    NamedKey::F21,
    NamedKey::F22,
    NamedKey::F23,
    NamedKey::F24,
];

/// Every variant of the new enum mapped onto the frozen one. `NamedKey` is
/// `#[non_exhaustive]`, so from out here the `_` arm is mandatory and the
/// compiler can no longer catch a new variant; `named_key_variant_sets_are_identical`
/// holds that line instead, by counting.
fn to_orig_named(k: NamedKey) -> orig_key::NamedKey {
    use orig_key::NamedKey as O;
    match k {
        NamedKey::Enter => O::Enter,
        NamedKey::Backspace => O::Backspace,
        NamedKey::Delete => O::Delete,
        NamedKey::Tab => O::Tab,
        NamedKey::Escape => O::Escape,
        NamedKey::ArrowUp => O::ArrowUp,
        NamedKey::ArrowDown => O::ArrowDown,
        NamedKey::ArrowLeft => O::ArrowLeft,
        NamedKey::ArrowRight => O::ArrowRight,
        NamedKey::Home => O::Home,
        NamedKey::End => O::End,
        NamedKey::PageUp => O::PageUp,
        NamedKey::PageDown => O::PageDown,
        NamedKey::Insert => O::Insert,
        NamedKey::F1 => O::F1,
        NamedKey::F2 => O::F2,
        NamedKey::F3 => O::F3,
        NamedKey::F4 => O::F4,
        NamedKey::F5 => O::F5,
        NamedKey::F6 => O::F6,
        NamedKey::F7 => O::F7,
        NamedKey::F8 => O::F8,
        NamedKey::F9 => O::F9,
        NamedKey::F10 => O::F10,
        NamedKey::F11 => O::F11,
        NamedKey::F12 => O::F12,
        NamedKey::F13 => O::F13,
        NamedKey::F14 => O::F14,
        NamedKey::F15 => O::F15,
        NamedKey::F16 => O::F16,
        NamedKey::F17 => O::F17,
        NamedKey::F18 => O::F18,
        NamedKey::F19 => O::F19,
        NamedKey::F20 => O::F20,
        NamedKey::F21 => O::F21,
        NamedKey::F22 => O::F22,
        NamedKey::F23 => O::F23,
        NamedKey::F24 => O::F24,
        other => panic!(
            "{other:?} was added to NamedKey after the move. The frozen oracle \
             predates it and has nothing to compare against: extend NEW_NAMED \
             and its count, and cover the new key with its own test."
        ),
    }
}

/// Exhaustive on the OLD enum: a variant dropped in the move fails to compile.
fn from_orig_named(k: orig_key::NamedKey) -> NamedKey {
    use orig_key::NamedKey as O;
    match k {
        O::Enter => NamedKey::Enter,
        O::Backspace => NamedKey::Backspace,
        O::Delete => NamedKey::Delete,
        O::Tab => NamedKey::Tab,
        O::Escape => NamedKey::Escape,
        O::ArrowUp => NamedKey::ArrowUp,
        O::ArrowDown => NamedKey::ArrowDown,
        O::ArrowLeft => NamedKey::ArrowLeft,
        O::ArrowRight => NamedKey::ArrowRight,
        O::Home => NamedKey::Home,
        O::End => NamedKey::End,
        O::PageUp => NamedKey::PageUp,
        O::PageDown => NamedKey::PageDown,
        O::Insert => NamedKey::Insert,
        O::F1 => NamedKey::F1,
        O::F2 => NamedKey::F2,
        O::F3 => NamedKey::F3,
        O::F4 => NamedKey::F4,
        O::F5 => NamedKey::F5,
        O::F6 => NamedKey::F6,
        O::F7 => NamedKey::F7,
        O::F8 => NamedKey::F8,
        O::F9 => NamedKey::F9,
        O::F10 => NamedKey::F10,
        O::F11 => NamedKey::F11,
        O::F12 => NamedKey::F12,
        O::F13 => NamedKey::F13,
        O::F14 => NamedKey::F14,
        O::F15 => NamedKey::F15,
        O::F16 => NamedKey::F16,
        O::F17 => NamedKey::F17,
        O::F18 => NamedKey::F18,
        O::F19 => NamedKey::F19,
        O::F20 => NamedKey::F20,
        O::F21 => NamedKey::F21,
        O::F22 => NamedKey::F22,
        O::F23 => NamedKey::F23,
        O::F24 => NamedKey::F24,
    }
}

const MODS: [KeyMods; 8] = [
    KeyMods {
        shift: false,
        ctrl: false,
        alt: false,
    },
    KeyMods {
        shift: true,
        ctrl: false,
        alt: false,
    },
    KeyMods {
        shift: false,
        ctrl: true,
        alt: false,
    },
    KeyMods {
        shift: false,
        ctrl: false,
        alt: true,
    },
    KeyMods {
        shift: true,
        ctrl: true,
        alt: false,
    },
    KeyMods {
        shift: true,
        ctrl: false,
        alt: true,
    },
    KeyMods {
        shift: false,
        ctrl: true,
        alt: true,
    },
    KeyMods {
        shift: true,
        ctrl: true,
        alt: true,
    },
];

fn orig_mods(m: KeyMods) -> orig_key::KeyMods {
    orig_key::KeyMods {
        shift: m.shift,
        ctrl: m.ctrl,
        alt: m.alt,
    }
}

/// Every mouse protocol state a ModeSnapshot can hold.
fn mouse_states() -> Vec<Option<MouseProtocol>> {
    let mut v = vec![None];
    for reporting in [
        MouseReporting::Normal,
        MouseReporting::ButtonEvent,
        MouseReporting::AnyEvent,
    ] {
        for encoding in [
            MouseEncoding::Default,
            MouseEncoding::Utf8,
            MouseEncoding::Sgr,
        ] {
            v.push(Some(MouseProtocol {
                reporting,
                encoding,
            }));
        }
    }
    v
}

/// The full snapshot space: 2^7 boolean combinations x 10 mouse states = 1280.
fn all_snapshots() -> Vec<ModeSnapshot> {
    let mut out = Vec::new();
    for bits in 0u32..128 {
        for mouse in mouse_states() {
            out.push(ModeSnapshot {
                app_cursor: bits & 1 != 0,
                alt_screen: bits & 2 != 0,
                app_keypad: bits & 4 != 0,
                bracketed_paste: bits & 8 != 0,
                show_cursor: bits & 16 != 0,
                insert: bits & 32 != 0,
                alternate_scroll: bits & 64 != 0,
                mouse,
            });
        }
    }
    out
}

fn key_specs() -> Vec<KeySpec> {
    let mut v: Vec<KeySpec> = NEW_NAMED.iter().copied().map(KeySpec::Named).collect();
    for t in [
        "a",
        "A",
        "z",
        "Z",
        "0",
        "1",
        "9",
        " ",
        "2",
        "@",
        "3",
        "[",
        "4",
        "\\",
        "5",
        "]",
        "6",
        "^",
        "7",
        "_",
        "/",
        "8",
        "?",
        ",",
        ".",
        "~",
        "\x7f",
        "\u{0}",
        "",
        "e\u{301}",
        "\u{e9}",
        "\u{1f600}",
        "\u{10ffff}",
        "ab",
        "\u{4e2d}\u{6587}",
        "\r\n",
        "\t",
    ] {
        v.push(KeySpec::Character(t.into()));
    }
    v
}

fn orig_spec(k: &KeySpec) -> orig_key::KeySpec {
    match k {
        KeySpec::Named(n) => orig_key::KeySpec::Named(to_orig_named(*n)),
        KeySpec::Character(s) => orig_key::KeySpec::Character(s.clone()),
        // `KeySpec` is `#[non_exhaustive]`; see `to_orig_named`.
        other => panic!("{other:?} was added to KeySpec after the move"),
    }
}

#[test]
fn named_key_variant_sets_are_identical() {
    assert_eq!(NEW_NAMED.len(), 38);
    for k in NEW_NAMED {
        assert_eq!(from_orig_named(to_orig_named(k)), k);
    }
}

#[test]
fn encode_key_is_byte_identical_to_main() {
    let specs = key_specs();
    let snaps = all_snapshots();
    let mut n = 0u64;
    let mut bad = 0u64;
    for spec in &specs {
        let o_spec = orig_spec(spec);
        for mods in MODS {
            let o_mods = orig_mods(mods);
            for snap in &snaps {
                let got = encode_key(spec, mods, snap);
                let want = orig_key::encode_key(&o_spec, o_mods, snap.app_cursor);
                if got != want {
                    bad += 1;
                    if bad < 20 {
                        println!(
                            "MISMATCH spec={spec:?} mods={mods:?} snap={snap:?} new={got:?} old={want:?}"
                        );
                    }
                }
                n += 1;
            }
        }
    }
    println!(
        "encode_key cases compared: {n} (specs {} x mods {} x snapshots {})",
        specs.len(),
        MODS.len(),
        snaps.len()
    );
    assert_eq!(bad, 0, "{bad} mismatches");
}

const MBUTTONS: [TerminalMouseButton; 3] = [
    TerminalMouseButton::Left,
    TerminalMouseButton::Middle,
    TerminalMouseButton::Right,
];

fn orig_button(b: TerminalMouseButton) -> orig_mouse::TerminalMouseButton {
    match b {
        TerminalMouseButton::Left => orig_mouse::TerminalMouseButton::Left,
        TerminalMouseButton::Middle => orig_mouse::TerminalMouseButton::Middle,
        TerminalMouseButton::Right => orig_mouse::TerminalMouseButton::Right,
    }
}

fn from_orig_button(b: orig_mouse::TerminalMouseButton) -> TerminalMouseButton {
    match b {
        orig_mouse::TerminalMouseButton::Left => TerminalMouseButton::Left,
        orig_mouse::TerminalMouseButton::Middle => TerminalMouseButton::Middle,
        orig_mouse::TerminalMouseButton::Right => TerminalMouseButton::Right,
    }
}

const MMODS: [MouseModifiers; 8] = [
    MouseModifiers {
        shift: false,
        alt: false,
        ctrl: false,
    },
    MouseModifiers {
        shift: true,
        alt: false,
        ctrl: false,
    },
    MouseModifiers {
        shift: false,
        alt: true,
        ctrl: false,
    },
    MouseModifiers {
        shift: false,
        alt: false,
        ctrl: true,
    },
    MouseModifiers {
        shift: true,
        alt: true,
        ctrl: false,
    },
    MouseModifiers {
        shift: true,
        alt: false,
        ctrl: true,
    },
    MouseModifiers {
        shift: false,
        alt: true,
        ctrl: true,
    },
    MouseModifiers {
        shift: true,
        alt: true,
        ctrl: true,
    },
];

fn orig_mmods(m: MouseModifiers) -> orig_mouse::MouseModifiers {
    orig_mouse::MouseModifiers {
        shift: m.shift,
        alt: m.alt,
        ctrl: m.ctrl,
    }
}

#[test]
fn mouse_button_variant_sets_are_identical() {
    for b in MBUTTONS {
        assert_eq!(from_orig_button(orig_button(b)), b);
    }
}

fn report(label: &str, got: Vec<u8>, want: Vec<u8>, ctx: &str, n: &mut u64, bad: &mut u64) {
    if got != want {
        *bad += 1;
        if *bad < 20 {
            println!("MISMATCH {label} {ctx} new={got:?} old={want:?}");
        }
    }
    *n += 1;
}

#[test]
fn encode_mouse_is_byte_identical_to_main() {
    let coords: [usize; 10] = [0, 1, 79, 190, 191, 222, 223, 2014, 65535, usize::MAX];
    let snaps = all_snapshots();
    let mut n = 0u64;
    let mut bad = 0u64;
    for snap in &snaps {
        for mods in MMODS {
            let om = orig_mmods(mods);
            for row in coords {
                for col in coords {
                    for b in MBUTTONS {
                        let ob = orig_button(b);
                        let ctx =
                            format!("row={row} col={col} b={b:?} mods={mods:?} snap={snap:?}");
                        report(
                            "press",
                            encode_mouse_press(row, col, b, *snap, mods),
                            orig_mouse::encode_mouse_press(row, col, ob, *snap, om),
                            &ctx,
                            &mut n,
                            &mut bad,
                        );
                        report(
                            "release",
                            encode_mouse_release(row, col, b, *snap, mods),
                            orig_mouse::encode_mouse_release(row, col, ob, *snap, om),
                            &ctx,
                            &mut n,
                            &mut bad,
                        );
                        report(
                            "move",
                            encode_mouse_move(row, col, Some(b), *snap, mods),
                            orig_mouse::encode_mouse_move(row, col, Some(ob), *snap, om),
                            &ctx,
                            &mut n,
                            &mut bad,
                        );
                    }
                    let ctx = format!("row={row} col={col} mods={mods:?} snap={snap:?}");
                    report(
                        "hover",
                        encode_mouse_move(row, col, None, *snap, mods),
                        orig_mouse::encode_mouse_move(row, col, None, *snap, om),
                        &ctx,
                        &mut n,
                        &mut bad,
                    );
                    for d in [
                        1.0f64,
                        -1.0,
                        0.0,
                        0.5,
                        -0.5,
                        f64::NAN,
                        f64::INFINITY,
                        f64::NEG_INFINITY,
                    ] {
                        let c2 = format!("{ctx} d={d}");
                        report(
                            "wheel",
                            encode_wheel_event(row, col, d, *snap, mods),
                            orig_mouse::encode_wheel_event(row, col, d, *snap, om),
                            &c2,
                            &mut n,
                            &mut bad,
                        );
                    }
                }
            }
        }
    }
    println!("encode_mouse cases compared: {n}");
    assert_eq!(bad, 0, "{bad} mismatches");
}

/// Hostile inputs must not panic (they go through the new path only).
#[test]
fn hostile_inputs_do_not_panic() {
    let snaps = all_snapshots();
    for spec in key_specs() {
        for mods in MODS {
            for snap in &snaps {
                let _ = encode_key(&spec, mods, snap);
            }
        }
    }
    let huge = "x".repeat(100_000);
    for t in ["\u{1f600}", "", "\u{0}", "\u{10ffff}", huge.as_str()] {
        for mods in MODS {
            let _ = encode_key(
                &KeySpec::Character(t.into()),
                mods,
                &ModeSnapshot::default(),
            );
        }
    }
    let m = ModeSnapshot {
        mouse: Some(MouseProtocol {
            reporting: MouseReporting::AnyEvent,
            encoding: MouseEncoding::Utf8,
        }),
        ..ModeSnapshot::default()
    };
    let _ = encode_mouse_press(
        usize::MAX,
        usize::MAX,
        TerminalMouseButton::Left,
        m,
        MouseModifiers::default(),
    );
    let _ = encode_wheel_event(
        usize::MAX,
        usize::MAX,
        f64::NAN,
        m,
        MouseModifiers::default(),
    );
}
