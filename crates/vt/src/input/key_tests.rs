use super::*;

/// The signature `encode_key` had while it lived in the adapter, expressed in
/// the one the engine publishes. Every case below is written against the old
/// three-argument form and runs against the new one through this line, which
/// is what pins "the boolean was `ModeSnapshot::app_cursor` and nothing else".
/// It shadows the glob-imported `encode_key`; `only_app_cursor_is_read` is the
/// test that holds the other half of the claim.
fn encode_key(key: &KeySpec, mods: KeyMods, app_cursor: bool) -> Option<Vec<u8>> {
    super::encode_key(
        key,
        mods,
        &ModeSnapshot {
            app_cursor,
            ..ModeSnapshot::default()
        },
    )
}

/// Every `NamedKey` there is, and every one of them encodes to something.
///
/// The `match` is exhaustive on purpose and this is the only place it can be:
/// `NamedKey` is `#[non_exhaustive]`, which does not apply inside the defining
/// crate, so out-of-crate callers -- `tests/verify_us0099_equiv.rs` among them
/// -- are forced into a `_` arm and cannot notice a new variant at all. Adding
/// one is a compile error *here*, which is the signal to extend `NEW_NAMED` in
/// that file and give the new key its own case.
#[test]
fn every_named_key_is_known_here() {
    const ALL: [NamedKey; 38] = [
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

    // Do not add a `_` arm. Its absence is the whole guard.
    fn listed_above(k: NamedKey) -> bool {
        match k {
            NamedKey::Enter
            | NamedKey::Backspace
            | NamedKey::Delete
            | NamedKey::Tab
            | NamedKey::Escape
            | NamedKey::ArrowUp
            | NamedKey::ArrowDown
            | NamedKey::ArrowLeft
            | NamedKey::ArrowRight
            | NamedKey::Home
            | NamedKey::End
            | NamedKey::PageUp
            | NamedKey::PageDown
            | NamedKey::Insert
            | NamedKey::F1
            | NamedKey::F2
            | NamedKey::F3
            | NamedKey::F4
            | NamedKey::F5
            | NamedKey::F6
            | NamedKey::F7
            | NamedKey::F8
            | NamedKey::F9
            | NamedKey::F10
            | NamedKey::F11
            | NamedKey::F12
            | NamedKey::F13
            | NamedKey::F14
            | NamedKey::F15
            | NamedKey::F16
            | NamedKey::F17
            | NamedKey::F18
            | NamedKey::F19
            | NamedKey::F20
            | NamedKey::F21
            | NamedKey::F22
            | NamedKey::F23
            | NamedKey::F24 => true,
        }
    }

    assert_eq!(ALL.len(), 38, "extend ALL and the match together");
    for k in ALL {
        assert!(listed_above(k));
        assert!(
            encode_key(&KeySpec::Named(k), m(false, false, false), false).is_some(),
            "{k:?} has no encoding"
        );
    }
}

/// Every other field of the snapshot is noise to the encoder: flip them all and
/// the bytes do not move.
#[test]
fn only_app_cursor_is_read() {
    let loud = ModeSnapshot {
        app_cursor: true,
        alt_screen: true,
        app_keypad: true,
        bracketed_paste: true,
        show_cursor: false,
        insert: true,
        alternate_scroll: true,
        mouse: Some(crate::snapshot::MouseProtocol {
            reporting: crate::snapshot::MouseReporting::AnyEvent,
            encoding: crate::snapshot::MouseEncoding::Sgr,
        }),
    };
    let quiet = ModeSnapshot {
        app_cursor: true,
        ..ModeSnapshot::default()
    };
    for key in [
        KeySpec::Named(NamedKey::ArrowUp),
        KeySpec::Named(NamedKey::Home),
        KeySpec::Named(NamedKey::F5),
        KeySpec::Character("a".into()),
    ] {
        for mods in [m(false, false, false), m(true, true, true)] {
            assert_eq!(
                super::encode_key(&key, mods, &loud),
                super::encode_key(&key, mods, &quiet),
                "{key:?} {mods:?}"
            );
        }
    }
}

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
fn ctrl_space_is_nul() {
    let s = encode_key(
        &KeySpec::Character(" ".into()),
        m(false, true, false),
        false,
    )
    .unwrap();
    assert_eq!(s, vec![0x00]);
}

#[test]
fn ctrl_uppercase_letter_uses_same_control_byte() {
    // Ctrl+Shift+A arrives as "A"; the control byte is case-insensitive.
    let s = encode_key(&KeySpec::Character("A".into()), m(true, true, false), false).unwrap();
    assert_eq!(s, vec![0x01]);
}

#[test]
fn ctrl_punctuation_and_digits_follow_xterm_table() {
    // xterm: Ctrl+2/@ → NUL, 3/[ → ESC, 4/\ → FS, 5/] → GS, 6/^ → RS,
    // 7/_ → US, 8/? → DEL.
    let table: &[(&str, u8)] = &[
        ("2", 0x00),
        ("@", 0x00),
        ("3", 0x1b),
        ("[", 0x1b),
        ("4", 0x1c),
        ("\\", 0x1c),
        ("5", 0x1d),
        ("]", 0x1d),
        ("6", 0x1e),
        ("^", 0x1e),
        ("7", 0x1f),
        ("_", 0x1f),
        ("/", 0x1f),
        ("8", 0x7f),
        ("?", 0x7f),
    ];
    for (ch, expected) in table {
        let s = encode_key(
            &KeySpec::Character((*ch).into()),
            m(false, true, false),
            false,
        )
        .unwrap();
        assert_eq!(s, vec![*expected], "Ctrl+{ch}");
    }
}

#[test]
fn ctrl_unmapped_ascii_passes_through() {
    // xterm sends the digit itself for Ctrl+0/1/9 (no control byte exists).
    for ch in ["0", "1", "9", ",", "."] {
        let s = encode_key(&KeySpec::Character(ch.into()), m(false, true, false), false).unwrap();
        assert_eq!(s, ch.as_bytes(), "Ctrl+{ch}");
    }
}

#[test]
fn ctrl_non_ascii_has_no_encoding() {
    assert_eq!(
        encode_key(
            &KeySpec::Character("é".into()),
            m(false, true, false),
            false
        ),
        None
    );
    // Multi-codepoint text (IME/compose) with Ctrl is dropped as well.
    assert_eq!(
        encode_key(
            &KeySpec::Character("ab".into()),
            m(false, true, false),
            false
        ),
        None
    );
}

#[test]
fn ctrl_alt_letter_prefixes_esc_to_control_byte() {
    let s = encode_key(&KeySpec::Character("a".into()), m(false, true, true), false).unwrap();
    assert_eq!(s, vec![0x1b, 0x01]);
}

#[test]
fn ctrl_alt_bracket_prefixes_esc_to_esc() {
    let s = encode_key(&KeySpec::Character("[".into()), m(false, true, true), false).unwrap();
    assert_eq!(s, vec![0x1b, 0x1b]);
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
