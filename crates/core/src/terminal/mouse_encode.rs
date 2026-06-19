//! Encode mouse events → CSI escape sequences (X10 / X11 / SGR-1006).
//!
//! Tham chiếu: `freya-terminal/parser.rs`, thuần hoá + thêm modifier support.
//! Mode flag (`TermMode::MOUSE_REPORT_CLICK` / `MOUSE_DRAG` / `MOUSE_MOTION` /
//! `SGR_MOUSE`) quyết định caller có gửi không; module này chỉ lo encoding.

use alacritty_terminal::term::TermMode;

/// Nút chuột cho terminal encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalMouseButton {
    Left,
    Middle,
    Right,
}

impl TerminalMouseButton {
    /// Mã X11/SGR (chưa tính modifier bits).
    fn code(self) -> u8 {
        match self {
            Self::Left => 0,
            Self::Middle => 1,
            Self::Right => 2,
        }
    }
}

/// Modifier kèm theo sự kiện chuột — cộng thêm vào button code.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MouseModifiers {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
}

impl MouseModifiers {
    /// Mask bit: shift=4, alt=8, ctrl=16 (theo chuẩn XTerm).
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

/// Encode một sự kiện chuột → chuỗi escape mà app đang chạy nhận được.
///
/// `sgr_code`: mã cho SGR (1006); `x11_code`: mã cho classic X11 (trước khi
/// cộng offset +32 bắt buộc). `release_in_sgr` chỉ ảnh hưởng SGR (chữ `m` vs
/// `M`); X11 release dùng button byte cố định = 3.
fn encode(
    sgr_code: u8,
    x11_code: u8,
    row: usize,
    col: usize,
    mode: TermMode,
    mods: MouseModifiers,
    release_in_sgr: bool,
) -> String {
    // row/col là 0-indexed từ caller → terminal 1-indexed.
    let row = row.saturating_add(1);
    let col = col.saturating_add(1);
    let mod_mask = mods.mask();
    if mode.contains(TermMode::SGR_MOUSE) {
        let action = if release_in_sgr { 'm' } else { 'M' };
        format!("\x1b[<{};{};{}{}", sgr_code + mod_mask, col, row, action)
    } else {
        // X10/X11: button+32, col+32, row+32 (giới hạn 255 do byte đơn).
        let button_byte = x11_code.saturating_add(32) + mod_mask;
        let col_byte = col.min(255) as u8;
        let row_byte = row.min(255) as u8;
        format!("\x1b[M{}{}{}", button_byte as char, col_byte as char, row_byte as char)
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
    // X11 gộp release thành một button byte cố định = 3; SGR giữ mã nút gốc
    // nhưng đổi `M` → `m`.
    encode(button.code(), 3, row, col, mode, mods, true)
}

/// Mouse motion. `button = None` → hover (không nút, mã 3).
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

/// Wheel. `delta_y > 0` = scroll up (mã 64), `< 0` = scroll down (mã 65).
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
        let s = encode_mouse_press(0, 0, TerminalMouseButton::Left, sgr_mode(), MouseModifiers::default());
        assert_eq!(s, "\x1b[<0;1;1M");
    }

    #[test]
    fn sgr_release_left() {
        let s = encode_mouse_release(2, 3, TerminalMouseButton::Left, sgr_mode(), MouseModifiers::default());
        assert_eq!(s, "\x1b[<0;4;3m");
    }

    #[test]
    fn sgr_press_with_ctrl() {
        let s = encode_mouse_press(0, 0, TerminalMouseButton::Left, sgr_mode(), MouseModifiers { ctrl: true, ..Default::default() });
        assert_eq!(s, "\x1b[<16;1;1M");
    }

    #[test]
    fn x11_press_left() {
        let s = encode_mouse_press(0, 0, TerminalMouseButton::Left, x11_mode(), MouseModifiers::default());
        // button = 0+32 = 32 (space), col/row = 1+... wait col=1 → 1 as char
        assert_eq!(s, "\x1b[M\x20\x01\x01");
    }

    #[test]
    fn x11_release_uses_button_3() {
        let s = encode_mouse_release(0, 0, TerminalMouseButton::Left, x11_mode(), MouseModifiers::default());
        // x11_code = 3 → 3+32 = 35 = '#'
        assert_eq!(s, "\x1b[M\x23\x01\x01");
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
}