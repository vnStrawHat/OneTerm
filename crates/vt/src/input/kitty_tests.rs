//! The kitty keyboard encoder against the specification's own worked examples.
//!
//! Every row cites the section of
//! <https://sw.kovidgoyal.net/kitty/keyboard-protocol/> it came from, fetched
//! 2026-09-15. A row that disagrees with the specification is a bug here, not a
//! table to be edited until it passes.

use super::*;
use crate::input::encode_key_event;

const DISAMBIGUATE: KeyboardFlags = KeyboardFlags::DISAMBIGUATE_ESC_CODES;
const EVENT_TYPES: KeyboardFlags = KeyboardFlags::REPORT_EVENT_TYPES;
const ALTERNATE: KeyboardFlags = KeyboardFlags::REPORT_ALTERNATE_KEYS;
const ALL_ESC: KeyboardFlags = KeyboardFlags::REPORT_ALL_KEYS_AS_ESC;
const TEXT: KeyboardFlags = KeyboardFlags::REPORT_ASSOCIATED_TEXT;

fn modes(flags: KeyboardFlags) -> ModeSnapshot {
    ModeSnapshot {
        keyboard_flags: flags,
        ..ModeSnapshot::default()
    }
}

fn ch(text: &str) -> KeySpec {
    KeySpec::Character(text.into())
}

fn named(key: NamedKey) -> KeySpec {
    KeySpec::Named(key)
}

fn mods(shift: bool, ctrl: bool, alt: bool) -> KeyMods {
    KeyMods { shift, ctrl, alt }
}

/// The specification's own examples, byte for byte.
#[test]
fn specification_worked_examples() {
    // (label and specification section, event, flags, expected bytes)
    let rows: &[(&str, KeyEvent, KeyboardFlags, &[u8])] = &[
        (
            "Functional key definitions: ESCAPE is 27 u",
            KeyEvent::new(named(NamedKey::Escape), KeyMods::default()),
            DISAMBIGUATE,
            b"\x1b[27u",
        ),
        (
            "Report all keys as escape codes: ENTER is 13 u",
            KeyEvent::new(named(NamedKey::Enter), KeyMods::default()),
            ALL_ESC,
            b"\x1b[13u",
        ),
        (
            "Functional key definitions: TAB is 9 u",
            KeyEvent::new(named(NamedKey::Tab), KeyMods::default()),
            ALL_ESC,
            b"\x1b[9u",
        ),
        (
            "Functional key definitions: BACKSPACE is 127 u",
            KeyEvent::new(named(NamedKey::Backspace), KeyMods::default()),
            ALL_ESC,
            b"\x1b[127u",
        ),
        (
            "Legacy functional keys: the second form omits the 1 and the \
             modifier when both are absent",
            KeyEvent::new(named(NamedKey::ArrowUp), KeyMods::default()),
            DISAMBIGUATE,
            b"\x1b[A",
        ),
        (
            "Modifiers: ctrl+shift is 1 + 0b101 = 6",
            KeyEvent::new(named(NamedKey::ArrowUp), mods(true, true, false)),
            DISAMBIGUATE,
            b"\x1b[1;6A",
        ),
        (
            "Legacy functional keys: PAGE_UP keeps its CSI 5 ~ form",
            KeyEvent::new(named(NamedKey::PageUp), mods(false, true, false)),
            DISAMBIGUATE,
            b"\x1b[5;5~",
        ),
        (
            "Example encodings: ctrl+shift+i is CSI 105 ; 6 u",
            KeyEvent::new(ch("i"), mods(true, true, false)),
            DISAMBIGUATE,
            b"\x1b[105;6u",
        ),
        (
            "Key codes: the code point is always the un-shifted key, so \
             ctrl+shift+a is 97 and never 65",
            KeyEvent::new(ch("A"), mods(true, true, false)),
            DISAMBIGUATE,
            b"\x1b[97;6u",
        ),
        (
            "Event types: a press with no modifiers and no event type is bare",
            KeyEvent::new(ch("a"), KeyMods::default()),
            ALL_ESC,
            b"\x1b[97u",
        ),
        (
            "Event types: CSI key-code;modifier:2 is a repeat",
            KeyEvent {
                kind: KeyEventKind::Repeat,
                ..KeyEvent::new(ch("a"), KeyMods::default())
            },
            EVENT_TYPES | ALL_ESC,
            b"\x1b[97;1:2u",
        ),
        (
            "Event types: CSI key-code;modifier:3 is a release",
            KeyEvent {
                kind: KeyEventKind::Release,
                ..KeyEvent::new(ch("a"), KeyMods::default())
            },
            EVENT_TYPES | ALL_ESC,
            b"\x1b[97;1:3u",
        ),
        (
            "Text as code points: shift+a is CSI 97 ; 2 ; 65 u",
            KeyEvent {
                text: Some("A".into()),
                ..KeyEvent::new(ch("a"), mods(true, false, false))
            },
            ALL_ESC | TEXT,
            b"\x1b[97;2;65u",
        ),
        (
            "Key codes: if only one alternate key is present it is the \
             shifted key",
            KeyEvent {
                shifted: Some('A'),
                ..KeyEvent::new(ch("a"), mods(true, false, false))
            },
            ALL_ESC | ALTERNATE,
            b"\x1b[97:65;2u",
        ),
        (
            "Key codes: a base layout key with no shifted key uses an empty \
             sub-field",
            KeyEvent {
                base_layout: Some('a'),
                ..KeyEvent::new(ch("a"), mods(false, true, false))
            },
            ALL_ESC | ALTERNATE,
            b"\x1b[97::97;5u",
        ),
        (
            "Key codes: the shifted key is present only if shift is in the \
             modifiers",
            KeyEvent {
                shifted: Some('A'),
                ..KeyEvent::new(ch("a"), mods(false, true, false))
            },
            ALL_ESC | ALTERNATE,
            b"\x1b[97;5u",
        ),
        (
            "Legacy ctrl mapping: ctrl+space is the NULL byte with nothing \
             negotiated",
            KeyEvent::new(ch(" "), mods(false, true, false)),
            KeyboardFlags::empty(),
            b"\x00",
        ),
    ];

    for (source, event, flags, want) in rows {
        let got = encode_key_event(event, &modes(*flags));
        assert_eq!(
            got.as_deref(),
            Some(*want),
            "{source}: {event:?} under {flags:?}"
        );
    }
}

/// "The only exceptions are the Enter, Tab and Backspace keys which still
/// generate the same bytes as in legacy mode ... to allow the user to type and
/// execute commands in the shell such as `reset`."
#[test]
fn enter_tab_and_backspace_survive_a_crashed_program() {
    for flags in [
        DISAMBIGUATE,
        DISAMBIGUATE | EVENT_TYPES,
        DISAMBIGUATE | EVENT_TYPES | ALTERNATE | TEXT,
    ] {
        let modes = modes(flags);
        for (key, want) in [
            (NamedKey::Enter, b"\r".as_slice()),
            (NamedKey::Tab, b"\t"),
            (NamedKey::Backspace, b"\x7f"),
        ] {
            let event = KeyEvent::new(named(key), KeyMods::default());
            assert_eq!(
                encode_key_event(&event, &modes).as_deref(),
                Some(want),
                "{key:?} under {flags:?}"
            );
        }
    }
}

/// "The Enter, Tab and Backspace keys will not have release events unless
/// Report all keys as escape codes is also set."
#[test]
fn the_three_legacy_keys_have_no_release_without_report_all() {
    for key in [NamedKey::Enter, NamedKey::Tab, NamedKey::Backspace] {
        let event = KeyEvent {
            kind: KeyEventKind::Release,
            ..KeyEvent::new(named(key), KeyMods::default())
        };
        assert_eq!(encode_key_event(&event, &modes(EVENT_TYPES)), None);
        assert!(encode_key_event(&event, &modes(EVENT_TYPES | ALL_ESC)).is_some());
    }
}

/// With nothing negotiated a release sends nothing, and a repeat is a press.
#[test]
fn release_is_silent_and_repeat_is_a_press_by_default() {
    let modes = modes(KeyboardFlags::empty());
    let press = KeyEvent::new(ch("a"), KeyMods::default());
    let repeat = KeyEvent {
        kind: KeyEventKind::Repeat,
        ..press.clone()
    };
    let release = KeyEvent {
        kind: KeyEventKind::Release,
        ..press.clone()
    };
    assert_eq!(encode_key_event(&release, &modes), None);
    assert_eq!(
        encode_key_event(&repeat, &modes),
        encode_key_event(&press, &modes)
    );
}

/// Once a kitty flag applies, `app_cursor` is not read: the kitty form is not
/// the cursor key form, and DECCKM only ever covered the unmodified legacy one.
#[test]
fn app_cursor_is_not_read_once_the_kitty_rung_applies() {
    let modes = ModeSnapshot {
        app_cursor: true,
        keyboard_flags: DISAMBIGUATE,
        ..ModeSnapshot::default()
    };
    let up = KeyEvent::new(named(NamedKey::ArrowUp), KeyMods::default());
    assert_eq!(
        encode_key_event(&up, &modes).as_deref(),
        Some(b"\x1b[A".as_slice())
    );

    // ...and it is still read when no flag applies.
    let legacy = ModeSnapshot {
        app_cursor: true,
        ..ModeSnapshot::default()
    };
    assert_eq!(
        encode_key_event(&up, &legacy).as_deref(),
        Some(b"\x1bOA".as_slice())
    );
}

/// A text-producing key is untouched by `DISAMBIGUATE_ESC_CODES`; alt or ctrl
/// on the same key is not.
#[test]
fn disambiguate_covers_only_the_keys_that_produce_no_text() {
    let modes = modes(DISAMBIGUATE);
    for (mods, want) in [
        (KeyMods::default(), b"a".as_slice()),
        (mods(true, false, false), b"a"),
        (mods(false, true, false), b"\x1b[97;5u"),
        (mods(false, false, true), b"\x1b[97;3u"),
        (mods(true, false, true), b"\x1b[97;4u"),
        (mods(false, true, true), b"\x1b[97;7u"),
    ] {
        let event = KeyEvent::new(ch("a"), mods);
        assert_eq!(
            encode_key_event(&event, &modes).as_deref(),
            Some(want),
            "{mods:?}"
        );
    }
}

/// "only if a key event was already going to be represented as an escape code
/// due to one of the other enhancements will this enhancement affect it."
#[test]
fn alternate_keys_and_text_are_inert_on_their_own() {
    for flags in [ALTERNATE, TEXT, ALTERNATE | TEXT] {
        let event = KeyEvent {
            shifted: Some('A'),
            base_layout: Some('a'),
            text: Some("a".into()),
            ..KeyEvent::new(ch("a"), KeyMods::default())
        };
        assert_eq!(
            encode_key_event(&event, &modes(flags)).as_deref(),
            Some(b"a".as_slice()),
            "{flags:?}"
        );
    }
}

/// "The associated text must not contain control codes", and a hostile payload
/// costs a bounded allocation.
#[test]
fn the_text_field_is_dropped_rather_than_sent_unsafely() {
    let modes = modes(ALL_ESC | TEXT);
    for text in ["\u{1}", "a\u{7f}", "a\u{9b}b", &"x".repeat(100_000)] {
        let event = KeyEvent {
            text: Some(text.into()),
            ..KeyEvent::new(ch("a"), KeyMods::default())
        };
        assert_eq!(
            encode_key_event(&event, &modes).as_deref(),
            Some(b"\x1b[97u".as_slice()),
            "{text:.16?}"
        );
    }
    // Multiple code points are separated by colons.
    let event = KeyEvent {
        text: Some("ab".into()),
        ..KeyEvent::new(ch("a"), KeyMods::default())
    };
    assert_eq!(
        encode_key_event(&event, &modes).as_deref(),
        Some(b"\x1b[97;;97:98u".as_slice())
    );
}

/// An empty `Character` names no key, so the enhanced protocols cannot carry
/// it and the event sends nothing. The legacy rung keeps its pre-existing
/// answer, an empty write, which is the asymmetry this packet did not touch.
#[test]
fn an_empty_character_has_no_kitty_encoding() {
    let plain = KeyEvent::new(ch(""), KeyMods::default());
    let with_alt = KeyEvent::new(ch(""), mods(false, false, true));
    for flags in 0u8..32 {
        let flags = KeyboardFlags::from_bits_truncate(flags);
        let modes = modes(flags);
        if flags.contains(ALL_ESC) {
            assert_eq!(encode_key_event(&plain, &modes), None, "{flags:?}");
        } else {
            assert_eq!(
                encode_key_event(&plain, &modes),
                Some(Vec::new()),
                "{flags:?}"
            );
        }
        // With alt held it no longer produces text, so the disambiguate flag
        // reaches it too.
        let want = if flags.intersects(ALL_ESC | DISAMBIGUATE) {
            None
        } else {
            Some(vec![0x1b])
        };
        assert_eq!(encode_key_event(&with_alt, &modes), want, "{flags:?}");
    }
}

/// `modifyOtherKeys`, which the encoder reaches only when no kitty flag did.
#[test]
fn modify_other_keys_levels() {
    let ctrl = mods(false, true, false);
    let level = |level: u8| ModeSnapshot {
        modify_other_keys: level,
        ..ModeSnapshot::default()
    };
    let cases: &[(KeySpec, &[u8], &[u8])] = &[
        // (key, level 1, level 2)
        (ch("a"), b"\x01", b"\x1b[27;5;97~"),
        (ch("2"), b"\x1b[27;5;50~", b"\x1b[27;5;50~"),
        (named(NamedKey::Enter), b"\x1b[27;5;13~", b"\x1b[27;5;13~"),
        (named(NamedKey::Tab), b"\x1b[27;5;9~", b"\x1b[27;5;9~"),
    ];
    for (key, one, two) in cases {
        let event = KeyEvent::new(key.clone(), ctrl);
        assert_eq!(encode_key_event(&event, &level(1)).as_deref(), Some(*one));
        assert_eq!(encode_key_event(&event, &level(2)).as_deref(), Some(*two));
    }
    // A functional key already has an unambiguous form at either level.
    let up = KeyEvent::new(named(NamedKey::ArrowUp), ctrl);
    assert_eq!(
        encode_key_event(&up, &level(2)).as_deref(),
        Some(b"\x1b[1;5A".as_slice())
    );
    // An unmodified key is never an "other" key.
    let plain = KeyEvent::new(ch("a"), KeyMods::default());
    assert_eq!(
        encode_key_event(&plain, &level(2)).as_deref(),
        Some(b"a".as_slice())
    );
}

/// The kitty flags supersede `modifyOtherKeys` when both are on.
#[test]
fn kitty_wins_over_modify_other_keys() {
    let both = ModeSnapshot {
        keyboard_flags: DISAMBIGUATE,
        modify_other_keys: 2,
        ..ModeSnapshot::default()
    };
    let event = KeyEvent::new(ch("a"), mods(false, true, false));
    assert_eq!(
        encode_key_event(&event, &both).as_deref(),
        Some(b"\x1b[97;5u".as_slice())
    );
}

/// `F13`-`F24` stay on xterm's shifted `F1`-`F12` forms, which is the recorded
/// deviation from the specification's private-use codes.
#[test]
fn shifted_function_keys_keep_the_xterm_form() {
    let modes = modes(DISAMBIGUATE);
    for (key, want) in [
        (NamedKey::F13, b"\x1b[1;2P".as_slice()),
        (NamedKey::F17, b"\x1b[15;2~"),
        (NamedKey::F24, b"\x1b[24;2~"),
    ] {
        let event = KeyEvent::new(named(key), KeyMods::default());
        assert_eq!(
            encode_key_event(&event, &modes).as_deref(),
            Some(want),
            "{key:?}"
        );
    }
}
