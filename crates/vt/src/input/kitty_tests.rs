//! The kitty keyboard encoder against the specification's own worked examples.
//!
//! Every row cites the section of
//! <https://sw.kovidgoyal.net/kitty/keyboard-protocol/> it came from, fetched
//! 2026-09-15. A row that disagrees with the specification is a bug here, not a
//! table to be edited until it passes.

use super::*;
use crate::input::{KeyMods, encode_key_event};

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
///
/// xterm(1): level `1` "enables this feature for keys **except** for those with
/// well-known behavior, e.g., Tab, Backarrow and some special control character
/// cases which are built into the X11 library, e.g., Control-Space to make a
/// NUL, or Control-3 to make an Escape character"; level `2` "enables this
/// feature for keys including the exceptions listed". So the level-1 rule is
/// "the chord already produces a control byte", and every row below is one side
/// of it.
#[test]
fn modify_other_keys_levels() {
    let ctrl = mods(false, true, false);
    let level = |level: u8| ModeSnapshot {
        modify_other_keys: level,
        ..ModeSnapshot::default()
    };
    let cases: &[(KeySpec, &[u8], &[u8])] = &[
        // (key with ctrl, level 1, level 2)
        (ch("a"), b"\x01", b"\x1b[27;5;97~"),
        (ch("2"), b"\x00", b"\x1b[27;5;50~"),
        (ch("3"), b"\x1b", b"\x1b[27;5;51~"),
        (ch(" "), b"\x00", b"\x1b[27;5;32~"),
        // No control byte, so level 1 already escapes it. This is the chord
        // modifyOtherKeys exists for.
        (ch(";"), b"\x1b[27;5;59~", b"\x1b[27;5;59~"),
        (ch("9"), b"\x1b[27;5;57~", b"\x1b[27;5;57~"),
        (named(NamedKey::Enter), b"\x1b[27;5;13~", b"\x1b[27;5;13~"),
        (named(NamedKey::Tab), b"\x09", b"\x1b[27;5;9~"),
        (named(NamedKey::Backspace), b"\x08", b"\x1b[27;5;127~"),
    ];
    for (key, one, two) in cases {
        let event = KeyEvent::new(key.clone(), ctrl);
        assert_eq!(
            encode_key_event(&event, &level(1)).as_deref(),
            Some(*one),
            "{key:?} at level 1"
        );
        assert_eq!(
            encode_key_event(&event, &level(2)).as_deref(),
            Some(*two),
            "{key:?} at level 2"
        );
    }
    // A functional key already has an unambiguous form at either level.
    let up = KeyEvent::new(named(NamedKey::ArrowUp), ctrl);
    assert_eq!(
        encode_key_event(&up, &level(2)).as_deref(),
        Some(b"\x1b[1;5A".as_slice())
    );
    // An unmodified key is never an "other" key -- and neither is a shifted
    // one, because the layout has already consumed shift to make the character.
    for mods in [KeyMods::default(), mods(true, false, false)] {
        let event = KeyEvent::new(ch("A"), mods);
        assert_eq!(
            encode_key_event(&event, &level(2)).as_deref(),
            Some(b"A".as_slice()),
            "{mods:?} must not turn typing into escape sequences"
        );
    }
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

/// `F13`-`F24` take the specification's private-use codes on the kitty rung and
/// keep xterm's shifted `F1`-`F12` forms on the legacy one.
///
/// The two rungs disagree on purpose. Spelling `F17` as `shift+F5` under the
/// kitty flags would set a modifier bit the user never pressed, so a program
/// matching `shift+F5` would fire on a bare `F17`; the legacy rung has no code
/// point to use instead and is frozen anyway.
#[test]
fn shifted_function_keys_take_the_private_use_codes() {
    let kitty = modes(DISAMBIGUATE);
    let legacy = modes(KeyboardFlags::empty());
    for (key, under_flags, legacy_form) in [
        (
            NamedKey::F13,
            b"\x1b[57376u".as_slice(),
            b"\x1b[1;2P".as_slice(),
        ),
        (NamedKey::F15, b"\x1b[57378u", b"\x1b[1;2R"),
        (NamedKey::F17, b"\x1b[57380u", b"\x1b[15;2~"),
        (NamedKey::F24, b"\x1b[57387u", b"\x1b[24;2~"),
    ] {
        let event = KeyEvent::new(named(key), KeyMods::default());
        assert_eq!(
            encode_key_event(&event, &kitty).as_deref(),
            Some(under_flags),
            "{key:?} under the kitty flags"
        );
        assert_eq!(
            encode_key_event(&event, &legacy).as_deref(),
            Some(legacy_form),
            "{key:?} on the legacy rung"
        );
    }
}

/// `F3` never takes the `R` final byte: "`CSI R` conflicts with the Cursor
/// Position Report, so it was removed", and `R` is not in the
/// `[~ABCDEFHPQS]` set the disambiguate section permits.
#[test]
fn f3_never_collides_with_the_cursor_position_report() {
    for bits in 1u8..32 {
        // Only the two flags that move a functional key onto the kitty rung;
        // the others are enhancements of a form some flag already chose. The
        // legacy rung still spells `F15` as `shift+F3` (`CSI 1 ; 2 R`), which
        // collides with a CPR and is frozen by `US-0099`'s equivalence bar --
        // recorded in guide chapter 6 rather than fixed inside this packet.
        if bits & 0b1001 == 0 {
            continue;
        }
        let modes = modes(KeyboardFlags::from_bits_truncate(bits));
        for mods in [
            KeyMods::default(),
            mods(true, true, false),
            mods(false, false, true),
        ] {
            for key in [NamedKey::F3, NamedKey::F15] {
                let event = KeyEvent::new(named(key), mods);
                let bytes = encode_key_event(&event, &modes).expect("F-keys always encode");
                assert_ne!(
                    bytes.last(),
                    Some(&b'R'),
                    "{key:?} {mods:?} under flags {bits} is {:?}",
                    String::from_utf8_lossy(&bytes)
                );
            }
        }
    }
    assert_eq!(
        encode_key_event(
            &KeyEvent::new(named(NamedKey::F3), KeyMods::default()),
            &modes(ALL_ESC)
        )
        .as_deref(),
        Some(b"\x1b[13~".as_slice())
    );
}

/// A held letter key keeps typing that letter under `REPORT_EVENT_TYPES` alone.
///
/// "Key events that result in text are reported as plain UTF-8 text, so events
/// are not supported for them, unless the application requests key report
/// mode."
#[test]
fn a_text_key_has_no_event_types_without_report_all_keys() {
    for flags in [
        EVENT_TYPES,
        DISAMBIGUATE | EVENT_TYPES,
        EVENT_TYPES | ALTERNATE,
    ] {
        let modes = modes(flags);
        let press = KeyEvent::new(ch("a"), KeyMods::default());
        let repeat = KeyEvent {
            kind: KeyEventKind::Repeat,
            ..press.clone()
        };
        let release = KeyEvent {
            kind: KeyEventKind::Release,
            ..press.clone()
        };
        assert_eq!(
            encode_key_event(&repeat, &modes).as_deref(),
            Some(b"a".as_slice()),
            "a repeat under {flags:?} must still type the letter"
        );
        assert_eq!(
            encode_key_event(&release, &modes),
            None,
            "a release under {flags:?} must send nothing"
        );
    }
    // A non-text key does get its event types from the flag alone.
    let release = KeyEvent {
        kind: KeyEventKind::Release,
        ..KeyEvent::new(named(NamedKey::ArrowUp), KeyMods::default())
    };
    assert_eq!(
        encode_key_event(&release, &modes(EVENT_TYPES)).as_deref(),
        Some(b"\x1b[1;1:3A".as_slice())
    );
}

/// The associated-text field is never invented for a chord that produces no
/// text: `Ctrl+A` must not tell the program an `a` was inserted.
#[test]
fn the_text_fallback_respects_the_modifiers() {
    let modes = modes(ALL_ESC | TEXT);
    for (mods, want) in [
        (KeyMods::default(), b"\x1b[97;;97u".as_slice()),
        (mods(true, false, false), b"\x1b[97;2;97u"),
        (mods(false, true, false), b"\x1b[97;5u"),
        (mods(false, false, true), b"\x1b[97;3u"),
        (mods(false, true, true), b"\x1b[97;7u"),
    ] {
        let event = KeyEvent::new(ch("a"), mods);
        assert_eq!(
            encode_key_event(&event, &modes).as_deref(),
            Some(want),
            "{mods:?}"
        );
    }
    // An embedder that knows the text still wins.
    let event = KeyEvent {
        text: Some("\u{e5}".into()),
        ..KeyEvent::new(ch("a"), mods(false, false, true))
    };
    assert_eq!(
        encode_key_event(&event, &modes).as_deref(),
        Some(b"\x1b[97;3;229u".as_slice())
    );
}
