//! Re-verification attacks for US-0108 at `db8a059b`. NOT part of the packet.

use gpui::{Keystroke, Modifiers};
use oneterm_terminal::{
    KeyEventKind, KeySpec, KeyboardFlags, ModeSnapshot, NamedKey, encode_key_event,
};

use super::keys::{KeyAction, KeyContext, canonical_key, classify_key, map_key};

fn stroke(key: &str, key_char: Option<&str>, modifiers: Modifiers) -> Keystroke {
    Keystroke {
        modifiers,
        key: key.to_string(),
        key_char: key_char.map(str::to_string),
    }
}

fn ctrl() -> Modifiers {
    Modifiers {
        control: true,
        ..Default::default()
    }
}

/// The code point the **engine** derives from a text key, read back out of the
/// bytes it emits rather than from a table copied next to it. `DISAMBIGUATE`
/// plus `ctrl` puts any text key on the `CSI <code> ; 5 u` rung.
fn engine_code_point(text: &str) -> u32 {
    let mut modes = ModeSnapshot::default();
    modes.keyboard_flags = KeyboardFlags::DISAMBIGUATE_ESC_CODES;
    let event = map_key(&stroke(text, Some(text), ctrl()), KeyEventKind::Press)
        .unwrap_or_else(|| panic!("{text} maps"));
    let bytes = encode_key_event(&event, &modes).unwrap_or_else(|| panic!("{text} encodes"));
    let rendered = String::from_utf8(bytes).expect("ASCII escape sequence");
    let digits = rendered
        .trim_start_matches("\u{1b}[")
        .split(';')
        .next()
        .expect("a code point field");
    digits.parse().expect("a numeric code point")
}

/// `canonical_key` copies the engine's private PC-101 table. The packet's own
/// test only proves the copy is self-consistent; this one proves it agrees with
/// the original, which is what a future drift would break.
#[test]
fn canonical_key_agrees_with_the_engines_own_unshifted_code_point() {
    let mut checked = 0;
    for text in [
        "~", "!", "@", "#", "$", "%", "^", "&", "*", "(", ")", "_", "+", "{", "}", "|", ":", "\"",
        "<", ">", "?", "`", "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "-", "=", "[", "]",
        "\\", ";", "'", ",", ".", "/", "a", "A", "z", "Z", " ", "e", "E",
    ] {
        let KeySpec::Character(folded) = canonical_key(&KeySpec::Character(text.into())) else {
            panic!("{text}: a text key stays a text key");
        };
        let folded_point = folded.chars().next().expect("one scalar") as u32;
        assert_eq!(
            folded_point,
            engine_code_point(text),
            "{text}: the view's canonical form and the engine's unshifted code point disagree"
        );
        checked += 1;
    }
    println!("canonical/engine code points compared: {checked}");
    assert_eq!(checked, 49);
}

/// Every PC-101 pair, in **both** directions, plus the letters the platform
/// folds for case and the named keys Windows renames. The reverse direction is
/// the one the fix's own test does not drive: press `1`, release `!`, which is
/// what a user who presses Shift *after* the digit produces.
#[test]
fn every_pairing_holds_in_both_directions() {
    let pairs = [
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
        // Case folding: caps lock or Shift gives GPUI an upper-case `key_char`,
        // and the release can arrive with either case.
        ("A", "a"),
        ("Z", "z"),
        ("E", "e"),
        // A key with no PC-101 pairing at all: only the case fold applies.
        ("\u{c9}", "\u{e9}"),
    ];
    for (shifted, plain) in pairs {
        for (down, up) in [(shifted, plain), (plain, shifted)] {
            let pressed = map_key(
                &stroke(down, Some(down), Modifiers::default()),
                KeyEventKind::Press,
            )
            .unwrap_or_else(|| panic!("{down} maps"));
            let released = map_key(
                &stroke(up, Some(up), Modifiers::default()),
                KeyEventKind::Release,
            )
            .unwrap_or_else(|| panic!("{up} maps"));
            assert_eq!(
                canonical_key(&pressed.key),
                canonical_key(&released.key),
                "press {down:?} / release {up:?} must pair"
            );
        }
    }
    // The named keys, including the two spellings Windows uses for one key and
    // `space`, which `named_key` does not cover and which becomes `" "`.
    for (down, up) in [
        ("enter", "return"),
        ("return", "enter"),
        ("escape", "escape"),
        ("tab", "tab"),
        ("backspace", "backspace"),
        ("space", "space"),
    ] {
        let pressed = map_key(
            &stroke(down, None, Modifiers::default()),
            KeyEventKind::Press,
        )
        .unwrap_or_else(|| panic!("{down} maps"));
        let released = map_key(
            &stroke(up, None, Modifiers::default()),
            KeyEventKind::Release,
        )
        .unwrap_or_else(|| panic!("{up} maps"));
        assert_eq!(
            canonical_key(&pressed.key),
            canonical_key(&released.key),
            "press {down:?} / release {up:?} must pair"
        );
    }
    // And two different keys must still not pair, or a release would clear the
    // wrong entry.
    assert_ne!(
        canonical_key(&KeySpec::Character("1".into())),
        canonical_key(&KeySpec::Character("2".into()))
    );
    assert_ne!(
        canonical_key(&KeySpec::Named(NamedKey::Enter)),
        canonical_key(&KeySpec::Named(NamedKey::Tab))
    );
    assert_ne!(
        canonical_key(&KeySpec::Character(" ".into())),
        canonical_key(&KeySpec::Named(NamedKey::Tab))
    );
}

/// The widened gate, checked against the encoder it now mirrors: for every flag
/// state, the view's decision and the encoder's rung agree about Ctrl+C.
#[test]
fn the_ctrl_c_gate_mirrors_the_encoders_rung() {
    let ks = stroke("c", None, ctrl());
    for (flags, expect_key) in [
        (KeyboardFlags::empty(), false),
        (KeyboardFlags::DISAMBIGUATE_ESC_CODES, true),
        (KeyboardFlags::REPORT_ALL_KEYS_AS_ESC, true),
        (
            KeyboardFlags::DISAMBIGUATE_ESC_CODES | KeyboardFlags::REPORT_ALL_KEYS_AS_ESC,
            true,
        ),
        // The flags that are pure enhancements of a rung some other flag chose
        // must NOT move Ctrl+C on their own.
        (KeyboardFlags::REPORT_EVENT_TYPES, false),
        (KeyboardFlags::REPORT_ALTERNATE_KEYS, false),
        (KeyboardFlags::REPORT_ASSOCIATED_TEXT, false),
    ] {
        let ctx = KeyContext {
            ctrl_c_is_a_key: flags.intersects(
                KeyboardFlags::DISAMBIGUATE_ESC_CODES | KeyboardFlags::REPORT_ALL_KEYS_AS_ESC,
            ),
            ..KeyContext::default()
        };
        let KeyAction::Interrupt(encoded) = classify_key(&ks, false, KeyEventKind::Press, ctx)
        else {
            panic!("{flags:?}: Ctrl+C is always the interrupt row");
        };
        let mut modes = ModeSnapshot::default();
        modes.keyboard_flags = flags;
        // What the encoder would answer for the same key and the same snapshot.
        let engine =
            map_key(&ks, KeyEventKind::Press).and_then(|event| encode_key_event(&event, &modes));
        if expect_key {
            let event = encoded.unwrap_or_else(|| panic!("{flags:?}: the view must carry the key"));
            assert_eq!(
                encode_key_event(&event, &modes),
                Some(b"\x1b[99;5u".to_vec()),
                "{flags:?}"
            );
            assert_eq!(
                engine,
                Some(b"\x1b[99;5u".to_vec()),
                "{flags:?}: and the encoder agrees, so there is no second handling"
            );
        } else {
            assert!(encoded.is_none(), "{flags:?}: still a signal");
            assert_eq!(
                engine,
                Some(b"\x03".to_vec()),
                "{flags:?}: and the legacy byte the signal stands in for is unchanged"
            );
        }
    }
}

/// The ceiling the canonical form buys: two *different physical* keys that
/// share one unshifted code point (a numpad digit and the digit row, if a
/// backend names both `1`) collapse to one held entry. Recorded so the
/// degradation is known: one release short, never a stuck key.
#[test]
fn two_physical_keys_sharing_a_code_point_collapse_to_one_entry() {
    let a = map_key(
        &stroke("1", Some("1"), Modifiers::default()),
        KeyEventKind::Press,
    )
    .unwrap();
    let b = map_key(
        &stroke("!", None, Modifiers::default()),
        KeyEventKind::Press,
    )
    .unwrap();
    assert_eq!(canonical_key(&a.key), canonical_key(&b.key));
}
