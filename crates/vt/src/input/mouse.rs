//! Encode mouse events → CSI escape sequences (X10 / X11 / 1005 UTF-8 / SGR-1006).
//!
//! Mouse-event encoding with modifier support. `ModeSnapshot::mouse` decides
//! whether the caller sends at all (`? 1000` / `? 1002` / `? 1003`); this module
//! reads only the **encoding** half of it (`? 1005` / `? 1006`).

use crate::render::{ModeSnapshot, MouseEncoding};

/// Mouse button for terminal encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalMouseButton {
    /// Primary button.
    Left,
    /// Wheel / middle button.
    Middle,
    /// Secondary button.
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
    /// Shift is held.
    pub shift: bool,
    /// Alt (Meta / Option) is held.
    pub alt: bool,
    /// Control is held.
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

#[cfg(test)]
#[path = "mouse_tests.rs"]
mod tests;
