//! Encode mouse events → CSI escape sequences (X10 / X11 / SGR-1006).
//!
//! Reference: `freya-terminal/parser.rs`, refined + added modifier support.
//! Mode flags (`TermMode::MOUSE_REPORT_CLICK` / `MOUSE_DRAG` / `MOUSE_MOTION` /
//! `SGR_MOUSE`) decide whether the caller sends; this module only handles encoding.

use alacritty_terminal::term::TermMode;

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

/// Encode a single mouse event → the escape sequence the running app receives.
///
/// `sgr_code`: code for SGR (1006); `x11_code`: code for classic X11 (before the
/// mandatory +32 offset). `release_in_sgr` only affects SGR (`m` vs `M`);
/// X11 release uses a fixed button byte = 3.
fn encode(
    sgr_code: u8,
    x11_code: u8,
    row: usize,
    col: usize,
    mode: TermMode,
    mods: MouseModifiers,
    release_in_sgr: bool,
) -> String {
    // row/col are 0-indexed from the caller → terminal is 1-indexed.
    let row = row.saturating_add(1);
    let col = col.saturating_add(1);
    let mod_mask = mods.mask();
    if mode.contains(TermMode::SGR_MOUSE) {
        let action = if release_in_sgr { 'm' } else { 'M' };
        format!("\x1b[<{};{};{}{}", sgr_code + mod_mask, col, row, action)
    } else {
        // X10/X11: button+32, col+32, row+32 (capped at 255 due to single byte).
        let button_byte = x11_code.saturating_add(32) + mod_mask;
        let col_byte = (col + 32).min(255) as u8;
        let row_byte = (row + 32).min(255) as u8;
        format!(
            "\x1b[M{}{}{}",
            button_byte as char, col_byte as char, row_byte as char
        )
    }
}

/// Mouse press (button down).
pub fn encode_mouse_press(
    row: usize,
    col: usize,
    button: TerminalMouseButton,
    mode: TermMode,
    mods: MouseModifiers,
) -> String {
    encode(button.code(), button.code(), row, col, mode, mods, false)
}

/// Mouse release (button up).
pub fn encode_mouse_release(
    row: usize,
    col: usize,
    button: TerminalMouseButton,
    mode: TermMode,
    mods: MouseModifiers,
) -> String {
    // X11 collapses release into a fixed button byte = 3; SGR keeps the original
    // button code but changes `M` → `m`.
    encode(button.code(), 3, row, col, mode, mods, true)
}

/// Mouse motion. `button = None` → hover (no button, code 3).
pub fn encode_mouse_move(
    row: usize,
    col: usize,
    button: Option<TerminalMouseButton>,
    mode: TermMode,
    mods: MouseModifiers,
) -> String {
    let code = button.map_or(3, TerminalMouseButton::code) + 32;
    encode(code, code, row, col, mode, mods, false)
}

/// Wheel. `delta_y > 0` = scroll up (code 64), `< 0` = scroll down (code 65).
pub fn encode_wheel_event(
    row: usize,
    col: usize,
    delta_y: f64,
    mode: TermMode,
    mods: MouseModifiers,
) -> String {
    let code = if delta_y > 0.0 { 64 } else { 65 };
    encode(code, code, row, col, mode, mods, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sgr_mode() -> TermMode {
        TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE
    }

    fn x11_mode() -> TermMode {
        TermMode::MOUSE_REPORT_CLICK
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
        assert_eq!(s, "\x1b[<0;1;1M");
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
        assert_eq!(s, "\x1b[<0;4;3m");
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
        assert_eq!(s, "\x1b[<16;1;1M");
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
        assert_eq!(s, "\x1b[M\x20\x21\x21");
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
        assert_eq!(s, "\x1b[M\x23\x21\x21");
    }

    #[test]
    fn wheel_up_sgr() {
        let s = encode_wheel_event(5, 5, 1.0, sgr_mode(), MouseModifiers::default());
        assert_eq!(s, "\x1b[<64;6;6M");
    }

    #[test]
    fn wheel_down_sgr() {
        let s = encode_wheel_event(5, 5, -1.0, sgr_mode(), MouseModifiers::default());
        assert_eq!(s, "\x1b[<65;6;6M");
    }

    #[test]
    fn move_hover_code_35() {
        let s = encode_mouse_move(0, 0, None, sgr_mode(), MouseModifiers::default());
        // hover: code 3+32 = 35
        assert_eq!(s, "\x1b[<35;1;1M");
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
        assert_eq!(s, "\x1b[M0!!");
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
        assert_eq!(s, "\x1b[M.+&");
    }

    #[test]
    fn x11_coordinates_above_127() {
        // Coordinates above 95 (1-indexed) still fit in a byte after +32.
        // col=200 → 201+32 = 233.
        let s = encode_mouse_press(
            200,
            200,
            TerminalMouseButton::Left,
            x11_mode(),
            MouseModifiers::default(),
        );
        // button = 32 (space), col = 201+32 = 233, row = 201+32 = 233
        let expected: String = format!(
            "\x1b[M{}{}{}",
            ' ',
            char::from_u32(233).unwrap(),
            char::from_u32(233).unwrap()
        );
        assert_eq!(s, expected);
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
        assert_eq!(s, "\x1b[M\x20\x7f\x7f");
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
        assert_eq!(s, "\x1b[<0;1001;501M");
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
        assert_eq!(s, "\x1b[<22;8;4m");
    }

    #[test]
    fn wheel_x11_mode() {
        let s = encode_wheel_event(5, 5, 1.0, x11_mode(), MouseModifiers::default());
        // code = 64+32 = 96 ('`'), col = 6+32 = 38 ('&'), row = 6+32 = 38 ('&')
        assert_eq!(s, "\x1b[M`&&");
    }

    #[test]
    fn move_hover_x11() {
        let s = encode_mouse_move(2, 3, None, x11_mode(), MouseModifiers::default());
        // code = 3+32+32 = 67 ('C'), col = 4+32 = 36 ('$'), row = 3+32 = 35 ('#')
        assert_eq!(s, "\x1b[MC$#");
    }
}
