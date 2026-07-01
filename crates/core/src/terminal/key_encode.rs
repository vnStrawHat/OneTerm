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
/// Conventions:
/// - `Character` + `ctrl` + 1 byte → `& 0x1f` (control code).
/// - `Enter` shift/ctrl → CSI u; plain → `\r`.
/// - `Backspace`: ctrl → `0x08`, alt → `ESC DEL`, plain → `0x7f`.
/// - Arrow + (shift|ctrl) → `CSI 1;{mod}{ch}`; plain → `ESC [ {ch}`.
/// - `Home`/`End` + (shift|ctrl) → `CSI 1;{mod}H/F`; plain → `ESC [ H/F`.
pub fn encode_key(key: &KeySpec, mods: KeyMods) -> Option<Vec<u8>> {
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
            if shift || ctrl {
                format!("\x1b[1;{}{ch}", modifier_byte(mods)).into_bytes()
            } else {
                vec![0x1b, b'[', ch as u8]
            }
        }

        KeySpec::Named(NamedKey::Home) if shift || ctrl => {
            format!("\x1b[1;{}H", modifier_byte(mods)).into_bytes()
        }
        KeySpec::Named(NamedKey::Home) => b"\x1b[H".to_vec(),

        KeySpec::Named(NamedKey::End) if shift || ctrl => {
            format!("\x1b[1;{}F", modifier_byte(mods)).into_bytes()
        }
        KeySpec::Named(NamedKey::End) => b"\x1b[F".to_vec(),

        KeySpec::Named(NamedKey::PageUp) => b"\x1b[5~".to_vec(),
        KeySpec::Named(NamedKey::PageDown) => b"\x1b[6~".to_vec(),
        KeySpec::Named(NamedKey::Insert) => b"\x1b[2~".to_vec(),
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
        let s = encode_key(&KeySpec::Character("c".into()), m(false, true, false)).unwrap();
        assert_eq!(s, vec![0x03]);
    }

    #[test]
    fn enter_plain_cr() {
        let s = encode_key(&KeySpec::Named(NamedKey::Enter), m(false, false, false)).unwrap();
        assert_eq!(s, b"\r");
    }

    #[test]
    fn enter_shift_csiu() {
        let s = encode_key(&KeySpec::Named(NamedKey::Enter), m(true, false, false)).unwrap();
        assert_eq!(s, b"\x1b[13;2u");
    }

    #[test]
    fn backspace_plain_del() {
        let s = encode_key(&KeySpec::Named(NamedKey::Backspace), m(false, false, false)).unwrap();
        assert_eq!(s, vec![0x7f]);
    }

    #[test]
    fn backspace_ctrl_bs() {
        let s = encode_key(&KeySpec::Named(NamedKey::Backspace), m(false, true, false)).unwrap();
        assert_eq!(s, vec![0x08]);
    }

    #[test]
    fn arrow_up_plain() {
        let s = encode_key(&KeySpec::Named(NamedKey::ArrowUp), m(false, false, false)).unwrap();
        assert_eq!(s, b"\x1b[A");
    }

    #[test]
    fn arrow_up_ctrl() {
        let s = encode_key(&KeySpec::Named(NamedKey::ArrowUp), m(false, true, false)).unwrap();
        // modifier_byte(ctrl only) = 1+0+0+4 = 5
        assert_eq!(s, b"\x1b[1;5A");
    }

    #[test]
    fn tab_shift_backtab() {
        let s = encode_key(&KeySpec::Named(NamedKey::Tab), m(true, false, false)).unwrap();
        assert_eq!(s, b"\x1b[Z");
    }

    #[test]
    fn home_plain() {
        let s = encode_key(&KeySpec::Named(NamedKey::Home), m(false, false, false)).unwrap();
        assert_eq!(s, b"\x1b[H");
    }

    #[test]
    fn delete_ctrl_csiu_tilde() {
        let s = encode_key(&KeySpec::Named(NamedKey::Delete), m(false, true, false)).unwrap();
        // modifier_byte(ctrl) = 5
        assert_eq!(s, b"\x1b[3;5~");
    }

    #[test]
    fn alt_char_prefix() {
        let s = encode_key(&KeySpec::Character("a".into()), m(false, false, true)).unwrap();
        assert_eq!(s, b"\x1ba");
    }
}
