//! `US-0108`'s byte-identity harness, adopted from the independent
//! verification of `IN-0040` so the criterion keeps being measured.
//!
//! The strong form of the byte-identity criterion: `main`'s `map_key` +
//! `encode_key` as of `980bf5da` is reproduced verbatim below and compared,
//! case by case, against this crate's `map_key` + `encode_key_event`. The
//! copied functions are frozen on purpose -- they are the baseline, so they
//! must not be refactored along with the live ones.

use gpui::{Keystroke, Modifiers};
use oneterm_terminal::{
    KeyEventKind, KeyMods, KeySpec, KeyboardFlags, ModeSnapshot, NamedKey, encode_key,
    encode_key_event,
};

use super::keys::{KeyAction, KeyContext, canonical_key, classify_key, map_key};

// ── `main`'s code at 980bf5da, copied verbatim from `.../keys.rs` ───────

fn map_key_main(ks: &Keystroke) -> Option<(KeySpec, KeyMods)> {
    let mods = ks.modifiers;
    let key_mods = KeyMods {
        shift: mods.shift,
        ctrl: mods.control,
        alt: mods.alt,
    };
    if let Some(named) = named_key_main(ks.key.as_str()) {
        return Some((KeySpec::Named(named), key_mods));
    }
    let text = ks
        .key_char
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            if ks.key == "space" {
                " ".to_string()
            } else {
                ks.key.clone()
            }
        });
    if text.chars().count() != 1 {
        return None;
    }
    Some((KeySpec::Character(text), key_mods))
}

fn named_key_main(key: &str) -> Option<NamedKey> {
    Some(match key {
        "enter" | "return" => NamedKey::Enter,
        "backspace" => NamedKey::Backspace,
        "delete" => NamedKey::Delete,
        "tab" => NamedKey::Tab,
        "escape" => NamedKey::Escape,
        "up" => NamedKey::ArrowUp,
        "down" => NamedKey::ArrowDown,
        "left" => NamedKey::ArrowLeft,
        "right" => NamedKey::ArrowRight,
        "home" => NamedKey::Home,
        "end" => NamedKey::End,
        "pageup" => NamedKey::PageUp,
        "pagedown" => NamedKey::PageDown,
        "insert" => NamedKey::Insert,
        "f1" => NamedKey::F1,
        "f2" => NamedKey::F2,
        "f3" => NamedKey::F3,
        "f4" => NamedKey::F4,
        "f5" => NamedKey::F5,
        "f6" => NamedKey::F6,
        "f7" => NamedKey::F7,
        "f8" => NamedKey::F8,
        "f9" => NamedKey::F9,
        "f10" => NamedKey::F10,
        "f11" => NamedKey::F11,
        "f12" => NamedKey::F12,
        "f13" => NamedKey::F13,
        "f14" => NamedKey::F14,
        "f15" => NamedKey::F15,
        "f16" => NamedKey::F16,
        "f17" => NamedKey::F17,
        "f18" => NamedKey::F18,
        "f19" => NamedKey::F19,
        "f20" => NamedKey::F20,
        "f21" => NamedKey::F21,
        "f22" => NamedKey::F22,
        "f23" => NamedKey::F23,
        "f24" => NamedKey::F24,
        _ => return None,
    })
}

// ── The case table ──────────────────────────────────────────────────────

/// (key name, key_char) pairs: every `named_key` row, a spread of text keys,
/// the `space` special case, and two names with no encoding at all.
fn key_table() -> Vec<(&'static str, Option<&'static str>)> {
    let named = [
        "enter",
        "return",
        "backspace",
        "delete",
        "tab",
        "escape",
        "up",
        "down",
        "left",
        "right",
        "home",
        "end",
        "pageup",
        "pagedown",
        "insert",
        "f1",
        "f2",
        "f3",
        "f4",
        "f5",
        "f6",
        "f7",
        "f8",
        "f9",
        "f10",
        "f11",
        "f12",
        "f13",
        "f14",
        "f15",
        "f16",
        "f17",
        "f18",
        "f19",
        "f20",
        "f21",
        "f22",
        "f23",
        "f24",
    ];
    let mut out: Vec<(&'static str, Option<&'static str>)> =
        named.iter().map(|k| (*k, None)).collect();
    // Named keys that GPUI also gives a `key_char` for.
    out.push(("enter", Some("\r")));
    out.push(("tab", Some("\t")));
    // Text keys, with and without the layout text GPUI supplies.
    for (key, ch) in [
        ("a", "a"),
        ("A", "A"),
        ("z", "z"),
        ("0", "0"),
        ("1", "1"),
        ("9", "9"),
        ("-", "-"),
        ("=", "="),
        ("[", "["),
        ("]", "]"),
        ("\\", "\\"),
        (";", ";"),
        ("'", "'"),
        (",", ","),
        (".", "."),
        ("/", "/"),
        ("`", "`"),
        ("!", "!"),
        ("$", "$"),
        ("?", "?"),
        ("é", "é"),
    ] {
        out.push((key, None));
        out.push((key, Some(ch)));
    }
    // The `space` translation and two names with no encoding.
    out.push(("space", None));
    out.push(("space", Some(" ")));
    out.push(("print", None));
    out.push(("f25", None));
    out
}

fn mod_table() -> Vec<Modifiers> {
    let mut out = Vec::new();
    for shift in [false, true] {
        for control in [false, true] {
            for alt in [false, true] {
                out.push(Modifiers {
                    shift,
                    control,
                    alt,
                    ..Default::default()
                });
            }
        }
    }
    out
}

fn stroke(key: &str, key_char: Option<&str>, modifiers: Modifiers) -> Keystroke {
    Keystroke {
        modifiers,
        key: key.to_string(),
        key_char: key_char.map(str::to_string),
    }
}

/// THE criterion, in the strong form the packet asked for and did not run:
/// for every (key x modifiers x DECCKM x modifyOtherKeys x press/repeat) case
/// with **no kitty flag pushed**, this branch writes exactly the bytes `main`
/// wrote, and agrees with `main` about which cases write nothing at all.
#[test]
fn byte_identity_with_main_when_no_kitty_flag_is_pushed() {
    let mut cases = 0usize;
    let mut sent = 0usize;
    for (key, key_char) in key_table() {
        for modifiers in mod_table() {
            let ks = stroke(key, key_char, modifiers);
            for app_cursor in [false, true] {
                for level in [0u8, 1, 2] {
                    let mut modes = ModeSnapshot::default();
                    modes.app_cursor = app_cursor;
                    modes.modify_other_keys = level;
                    assert!(
                        modes.keyboard_flags.is_empty(),
                        "the identity criterion is about the no-flag case"
                    );
                    let old = map_key_main(&ks);
                    for kind in [KeyEventKind::Press, KeyEventKind::Repeat] {
                        let new = map_key(&ks, kind);
                        assert_eq!(
                            old.is_some(),
                            new.is_some(),
                            "{key:?}/{key_char:?}/{modifiers:?}: mappability moved"
                        );
                        let (old_bytes, new_bytes) = match (&old, &new) {
                            (Some((spec, mods)), Some(event)) => {
                                assert_eq!(&event.key, spec, "{key:?}: KeySpec moved");
                                assert_eq!(event.mods, *mods, "{key:?}: KeyMods moved");
                                (
                                    encode_key(spec, *mods, &modes),
                                    encode_key_event(event, &modes),
                                )
                            }
                            _ => (None, None),
                        };
                        assert_eq!(
                            old_bytes, new_bytes,
                            "{key:?}/{key_char:?}/{modifiers:?}/app_cursor={app_cursor}/\
                             mok={level}/{kind:?}: bytes moved"
                        );
                        if new_bytes.is_some() {
                            sent += 1;
                        }
                        cases += 1;
                    }
                    // And a release, which nobody asked to hear about, writes
                    // nothing at all — so it adds no entry to the stream.
                    if let Some(event) = map_key(&ks, KeyEventKind::Release) {
                        assert_eq!(
                            encode_key_event(&event, &modes),
                            None,
                            "{key:?}: a release with no REPORT_EVENT_TYPES must be silent"
                        );
                    }
                    cases += 1;
                }
            }
        }
    }
    // Printed so the verification report can quote a number rather than a claim.
    println!("byte-identity cases compared: {cases} ({sent} of them wrote bytes)");
    assert!(cases > 3000, "the table must actually be wide: {cases}");
}

/// A repeat under `REPORT_EVENT_TYPES` alone is still the plain press bytes for
/// a **text** key — the specification exempts text keys — and the escape form
/// only for a key that generates none.
#[test]
fn repeat_under_report_event_types_follows_the_specification() {
    let mut modes = ModeSnapshot::default();
    modes.keyboard_flags = KeyboardFlags::REPORT_EVENT_TYPES;
    let text = map_key(
        &stroke("a", Some("a"), Modifiers::default()),
        KeyEventKind::Repeat,
    )
    .expect("a letter maps");
    assert_eq!(
        encode_key_event(&text, &modes),
        Some(b"a".to_vec()),
        "a held letter keeps typing the letter"
    );
    let arrow = map_key(
        &stroke("up", None, Modifiers::default()),
        KeyEventKind::Repeat,
    )
    .expect("an arrow maps");
    assert_eq!(
        encode_key_event(&arrow, &modes),
        Some(b"\x1b[1;1:2A".to_vec())
    );
    // `Enter`, `Tab` and `Backspace` are the specification's explicit exception.
    for (key, bytes) in [
        ("enter", b"\r".to_vec()),
        ("tab", b"\t".to_vec()),
        ("backspace", b"\x7f".to_vec()),
    ] {
        let event = map_key(
            &stroke(key, None, Modifiers::default()),
            KeyEventKind::Repeat,
        )
        .expect("maps");
        assert_eq!(encode_key_event(&event, &modes), Some(bytes), "{key}");
        let release = map_key(
            &stroke(key, None, Modifiers::default()),
            KeyEventKind::Release,
        )
        .expect("maps");
        assert_eq!(encode_key_event(&release, &modes), None, "{key} release");
    }
    // A text key's release is silent under REPORT_EVENT_TYPES alone.
    let release = map_key(
        &stroke("a", Some("a"), Modifiers::default()),
        KeyEventKind::Release,
    )
    .expect("maps");
    assert_eq!(encode_key_event(&release, &modes), None);
}

/// With `REPORT_ALL_KEYS_AS_ESC` a text key does get event types, and a repeat
/// carries its associated text when the program also asked for text.
#[test]
fn a_text_key_repeat_carries_text_under_all_keys_as_esc() {
    let mut modes = ModeSnapshot::default();
    modes.keyboard_flags = KeyboardFlags::REPORT_EVENT_TYPES
        | KeyboardFlags::REPORT_ALL_KEYS_AS_ESC
        | KeyboardFlags::REPORT_ASSOCIATED_TEXT;
    let ks = stroke("a", Some("a"), Modifiers::default());
    assert_eq!(
        encode_key_event(&map_key(&ks, KeyEventKind::Press).unwrap(), &modes),
        // The modifier field is dropped when both it and the event type are
        // at their defaults, leaving the empty placeholder before the text.
        Some(b"\x1b[97;;97u".to_vec())
    );
    assert_eq!(
        encode_key_event(&map_key(&ks, KeyEventKind::Repeat).unwrap(), &modes),
        Some(b"\x1b[97;1:2;97u".to_vec()),
        "a repeat inserts text, so it carries it"
    );
    assert_eq!(
        encode_key_event(&map_key(&ks, KeyEventKind::Release).unwrap(), &modes),
        Some(b"\x1b[97;1:3u".to_vec()),
        "a release inserts nothing, so it carries no text"
    );
}

/// Ctrl+C, the three states.
///
/// `F4` of the verification asked which way the view's gate should point under
/// `DISAMBIGUATE_ESC_CODES` alone. The kitty specification answers it: "Turning
/// on this flag will cause the terminal to report the Esc, alt+key, ctrl+key,
/// ctrl+alt+key, shift+alt+key keys using `CSI u` sequences instead of legacy
/// ones", with Enter, Tab and Backspace the only exceptions. So the view widened
/// its gate to match the encoder's rung rather than the encoder narrowing.
#[test]
fn ctrl_c_across_the_flag_states() {
    let ctrl = Modifiers {
        control: true,
        ..Default::default()
    };
    let ks = stroke("c", None, ctrl);
    let encoded = |ctx: KeyContext, flags: KeyboardFlags| {
        let KeyAction::Interrupt(Some(event)) = classify_key(&ks, false, KeyEventKind::Press, ctx)
        else {
            panic!("Ctrl+C must reach the encoder once the program negotiated the CSI u rung");
        };
        let mut modes = ModeSnapshot::default();
        modes.keyboard_flags = flags;
        encode_key_event(&event, &modes)
    };

    // 1. Nothing negotiated: unchanged, the signal, and no encoded key with it.
    assert_eq!(
        classify_key(&ks, false, KeyEventKind::Press, KeyContext::default()),
        KeyAction::Interrupt(None)
    );

    // 2. REPORT_ALL_KEYS_AS_ESC: the encoded key, `CSI 99;5u`.
    let all_esc = KeyContext {
        ctrl_c_is_a_key: true,
        ..KeyContext::default()
    };
    assert_eq!(
        encoded(all_esc, KeyboardFlags::REPORT_ALL_KEYS_AS_ESC),
        Some(b"\x1b[99;5u".to_vec())
    );

    // 3. DISAMBIGUATE_ESC_CODES alone: the same, because `kitty_applies` puts a
    //    ctrl chord on the kitty rung under it and the specification says so.
    //    The view no longer hands a signal to a program promised bytes.
    assert_eq!(
        encoded(all_esc, KeyboardFlags::DISAMBIGUATE_ESC_CODES),
        Some(b"\x1b[99;5u".to_vec())
    );

    // 4. REPORT_EVENT_TYPES alone puts nothing on the kitty rung for a press,
    //    so the view must still make a signal -- which is what the view's own
    //    flag test produces, and is asserted at the view level in
    //    `view_tests::verify_ctrl_c_fans_an_interrupt_whatever_the_origin_negotiated`.
    let mut modes = ModeSnapshot::default();
    modes.keyboard_flags = KeyboardFlags::REPORT_EVENT_TYPES;
    let would_be = map_key(&ks, KeyEventKind::Press).expect("Ctrl+C maps");
    assert_eq!(
        encode_key_event(&would_be, &modes),
        Some(b"\x03".to_vec()),
        "the encoder's own answer is the legacy byte, so a signal agrees with it"
    );
}

/// The Windows key-name instability that `F1` of the verification found: a
/// digit or an OEM punctuation key is named by its **shifted** glyph while
/// Shift is down and by its unshifted glyph once Shift is up (gpui-pre-windows
/// `get_keystroke_key` / `need_to_convert_to_shifted_key`), so a key-down and
/// its key-up carry different names and a set keyed on the raw name strands the
/// key. `canonical_key` collapses them, which is what pairs the two events.
#[test]
fn a_digit_keeps_one_identity_when_shift_is_released_first() {
    // What the platform produces: down while Shift is held, up after Shift.
    let down = stroke("!", None, Modifiers::default()); // shift already consumed
    let up = stroke("1", None, Modifiers::default());
    assert_ne!(down.key, up.key, "the raw names really do differ");

    let pressed = map_key(&down, KeyEventKind::Press).expect("maps");
    let released = map_key(&up, KeyEventKind::Release).expect("maps");
    assert_ne!(pressed.key, released.key, "and so do the specs");
    assert_eq!(
        canonical_key(&pressed.key),
        canonical_key(&released.key),
        "but the held set pairs them, because both unshift to `1`"
    );
}

/// The rest of the shift relation, and the cases that must **not** collapse.
#[test]
fn the_canonical_key_mirrors_the_engines_shift_table() {
    for (shifted, unshifted) in [
        ("~", "`"),
        ("!", "1"),
        ("@", "2"),
        ("#", "3"),
        ("$", "4"),
        ("%", "5"),
        ("^", "6"),
        ("&", "7"),
        ("*", "8"),
        ("(", "9"),
        (")", "0"),
        ("_", "-"),
        ("+", "="),
        ("{", "["),
        ("}", "]"),
        ("|", "\\"),
        (":", ";"),
        ("\"", "'"),
        ("<", ","),
        (">", "."),
        ("?", "/"),
        ("A", "a"),
        ("Z", "z"),
    ] {
        assert_eq!(
            canonical_key(&KeySpec::Character(shifted.into())),
            canonical_key(&KeySpec::Character(unshifted.into())),
            "{shifted} and {unshifted} are one key"
        );
    }
    // Different keys stay different, or a release would pair with the wrong one.
    assert_ne!(
        canonical_key(&KeySpec::Character("a".into())),
        canonical_key(&KeySpec::Character("b".into()))
    );
    assert_ne!(
        canonical_key(&KeySpec::Named(NamedKey::ArrowUp)),
        canonical_key(&KeySpec::Named(NamedKey::ArrowDown))
    );
    // `enter` and `return` are one key, which the spec already collapses.
    assert_eq!(
        canonical_key(
            &map_key(
                &stroke("enter", None, Modifiers::default()),
                KeyEventKind::Press
            )
            .unwrap()
            .key
        ),
        canonical_key(
            &map_key(
                &stroke("return", None, Modifiers::default()),
                KeyEventKind::Press
            )
            .unwrap()
            .key
        )
    );
}

/// `F5`: the text a key would insert is reported only when the modifiers would
/// have let it reach the program. `Ctrl+A` produces `0x01`, so claiming it
/// inserted an `a` would be a lie to the program.
#[test]
fn a_ctrl_or_alt_chord_carries_no_associated_text() {
    for modifiers in [
        Modifiers {
            control: true,
            ..Default::default()
        },
        Modifiers {
            alt: true,
            ..Default::default()
        },
    ] {
        let event = map_key(&stroke("a", Some("a"), modifiers), KeyEventKind::Press).expect("maps");
        assert_eq!(event.text, None, "{modifiers:?}");
    }
    // Shift alone still inserts, so it still reports.
    let shifted = map_key(
        &stroke(
            "A",
            Some("A"),
            Modifiers {
                shift: true,
                ..Default::default()
            },
        ),
        KeyEventKind::Press,
    )
    .expect("maps");
    assert_eq!(shifted.text.as_deref(), Some("A"));
}
