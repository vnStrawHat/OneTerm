//! Encode keyboard events → escape sequences for the terminal.
//!
//! Reference: `freya-terminal/handle.rs::write_key`, refined:
//! - Defines neutral types (`KeySpec`/`KeyMods`/`NamedKey`) — no dependency on
//!   `keyboard_types` or GPUI. The UI crate maps GPUI key events → `KeySpec`.
//! - Returns bytes only, with NO side effects (scroll/selection/shift-tracking
//!   belong to the view/session, not the encoder).
//! - Returns `None` when unrecognized → the caller decides to ignore it.

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

/// Encode a single key event → escape sequence. Returns `None` if unrecognized.
///
/// `app_cursor` mirrors the terminal's DECCKM state (`TermMode::APP_CURSOR`):
/// when the program has enabled Application Cursor Keys (e.g. vim/less/man
/// send `CSI ?1h`), the plain cursor keys (arrows, Home, End) must use the
/// `ESC O{ch}` form instead of `ESC [{ch}` so the program recognizes them.
///
/// Conventions:
/// - `Character` + `ctrl` + 1 byte → `& 0x1f` (control code).
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
        KeySpec::Character(ch) if ctrl && ch.len() == 1 => vec![ch.as_bytes()[0] & 0x1f],
        KeySpec::Character(ch) if alt => {
            // Alt-prefix: ESC + character (approximation for ASCII).
            let mut v = vec![0x1b];
            v.extend_from_slice(ch.as_bytes());
            v
        }
        KeySpec::Character(ch) => ch.as_bytes().to_vec(),

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

#[cfg(test)]
mod tests {
    use super::*;

    fn m(shift: bool, ctrl: bool, alt: bool) -> KeyMods {
        KeyMods { shift, ctrl, alt }
    }

    #[test]
    fn ctrl_c_is_0x03() {
        let s = encode_key(
            &KeySpec::Character("c".into()),
            m(false, true, false),
            false,
        )
        .unwrap();
        assert_eq!(s, vec![0x03]);
    }

    #[test]
    fn enter_plain_cr() {
        let s = encode_key(
            &KeySpec::Named(NamedKey::Enter),
            m(false, false, false),
            false,
        )
        .unwrap();
        assert_eq!(s, b"\r");
    }

    #[test]
    fn enter_shift_csiu() {
        let s = encode_key(
            &KeySpec::Named(NamedKey::Enter),
            m(true, false, false),
            false,
        )
        .unwrap();
        assert_eq!(s, b"\x1b[13;2u");
    }

    #[test]
    fn backspace_plain_del() {
        let s = encode_key(
            &KeySpec::Named(NamedKey::Backspace),
            m(false, false, false),
            false,
        )
        .unwrap();
        assert_eq!(s, vec![0x7f]);
    }

    #[test]
    fn backspace_ctrl_bs() {
        let s = encode_key(
            &KeySpec::Named(NamedKey::Backspace),
            m(false, true, false),
            false,
        )
        .unwrap();
        assert_eq!(s, vec![0x08]);
    }

    #[test]
    fn arrow_up_plain() {
        let s = encode_key(
            &KeySpec::Named(NamedKey::ArrowUp),
            m(false, false, false),
            false,
        )
        .unwrap();
        assert_eq!(s, b"\x1b[A");
    }

    #[test]
    fn arrow_up_app_cursor() {
        // DECCKM on (e.g. vim) → application-mode arrow keys use `ESC O{ch}`.
        let s = encode_key(
            &KeySpec::Named(NamedKey::ArrowUp),
            m(false, false, false),
            true,
        )
        .unwrap();
        assert_eq!(s, b"\x1bOA");
    }

    #[test]
    fn arrow_up_ctrl() {
        // Modifiers force the numeric CSI form regardless of DECCKM.
        let s = encode_key(
            &KeySpec::Named(NamedKey::ArrowUp),
            m(false, true, false),
            true,
        )
        .unwrap();
        // modifier_byte(ctrl only) = 1+0+0+4 = 5
        assert_eq!(s, b"\x1b[1;5A");
    }

    #[test]
    fn tab_shift_backtab() {
        let s = encode_key(&KeySpec::Named(NamedKey::Tab), m(true, false, false), false).unwrap();
        assert_eq!(s, b"\x1b[Z");
    }

    #[test]
    fn home_plain() {
        let s = encode_key(
            &KeySpec::Named(NamedKey::Home),
            m(false, false, false),
            false,
        )
        .unwrap();
        assert_eq!(s, b"\x1b[H");
    }

    #[test]
    fn home_app_cursor() {
        // DECCKM on (e.g. vim) → Home must be `ESC OH` so vim recognizes it.
        let s = encode_key(
            &KeySpec::Named(NamedKey::Home),
            m(false, false, false),
            true,
        )
        .unwrap();
        assert_eq!(s, b"\x1bOH");
    }

    #[test]
    fn home_ctrl_ignores_app_cursor() {
        // Modifiers force the numeric CSI form regardless of DECCKM.
        let s = encode_key(&KeySpec::Named(NamedKey::Home), m(false, true, false), true).unwrap();
        assert_eq!(s, b"\x1b[1;5H");
    }

    #[test]
    fn end_plain() {
        let s = encode_key(
            &KeySpec::Named(NamedKey::End),
            m(false, false, false),
            false,
        )
        .unwrap();
        assert_eq!(s, b"\x1b[F");
    }

    #[test]
    fn end_app_cursor() {
        // DECCKM on (e.g. vim) → End must be `ESC OF` so vim recognizes it.
        let s = encode_key(&KeySpec::Named(NamedKey::End), m(false, false, false), true).unwrap();
        assert_eq!(s, b"\x1bOF");
    }

    #[test]
    fn delete_ctrl_csiu_tilde() {
        let s = encode_key(
            &KeySpec::Named(NamedKey::Delete),
            m(false, true, false),
            false,
        )
        .unwrap();
        // modifier_byte(ctrl) = 5
        assert_eq!(s, b"\x1b[3;5~");
    }

    #[test]
    fn alt_char_prefix() {
        let s = encode_key(
            &KeySpec::Character("a".into()),
            m(false, false, true),
            false,
        )
        .unwrap();
        assert_eq!(s, b"\x1ba");
    }

    #[test]
    fn f1_plain() {
        let s = encode_key(&KeySpec::Named(NamedKey::F1), m(false, false, false), false).unwrap();
        assert_eq!(s, b"\x1bOP");
    }

    #[test]
    fn f5_plain() {
        let s = encode_key(&KeySpec::Named(NamedKey::F5), m(false, false, false), false).unwrap();
        assert_eq!(s, b"\x1b[15~");
    }

    #[test]
    fn f12_plain() {
        let s = encode_key(
            &KeySpec::Named(NamedKey::F12),
            m(false, false, false),
            false,
        )
        .unwrap();
        assert_eq!(s, b"\x1b[24~");
    }

    #[test]
    fn alt_arrow_up() {
        let s = encode_key(
            &KeySpec::Named(NamedKey::ArrowUp),
            m(false, false, true),
            true,
        )
        .unwrap();
        // modifier_byte(alt only) = 1+0+2+0 = 3
        assert_eq!(s, b"\x1b[1;3A");
    }

    #[test]
    fn alt_home() {
        let s = encode_key(&KeySpec::Named(NamedKey::Home), m(false, false, true), true).unwrap();
        // modifier_byte(alt only) = 3
        assert_eq!(s, b"\x1b[1;3H");
    }

    #[test]
    fn ctrl_pageup() {
        let s = encode_key(
            &KeySpec::Named(NamedKey::PageUp),
            m(false, true, false),
            false,
        )
        .unwrap();
        // modifier_byte(ctrl only) = 5
        assert_eq!(s, b"\x1b[5;5~");
    }

    #[test]
    fn alt_pageup() {
        let s = encode_key(
            &KeySpec::Named(NamedKey::PageUp),
            m(false, false, true),
            false,
        )
        .unwrap();
        // modifier_byte(alt only) = 3
        assert_eq!(s, b"\x1b[5;3~");
    }
}
