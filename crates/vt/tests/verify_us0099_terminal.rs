//! `Terminal::encode_key` against the live mode table, driven only through
//! `feed` -- the convenience form reads the terminal's own modes, so real bytes
//! are the only honest way to set them.
//!
//! Also pins the axes that are deliberately **not** wired: the Kitty keyboard
//! flags and `modifyOtherKeys` reach the mode table and do not reach the
//! encoder. Those assertions are a record of today's behaviour, not a wish; the
//! packet that connects them will rewrite them.
//!
//! Written by an independent verifier for the encoder move and adopted here.

use std::time::Instant;

use oneterm_vt::input::{KeyMods, KeySpec, NamedKey};
use oneterm_vt::{Config, EventBatch, Size, Terminal};

fn term() -> (Terminal, EventBatch) {
    let t = Terminal::new(Size { rows: 24, cols: 80 }, Config::default());
    (t, EventBatch::new())
}

fn feed(t: &mut Terminal, b: &mut EventBatch, bytes: &[u8]) {
    t.feed(bytes, b, Instant::now());
}

const ARROWS: [(NamedKey, &[u8], &[u8]); 4] = [
    (NamedKey::ArrowUp, b"\x1b[A", b"\x1bOA"),
    (NamedKey::ArrowDown, b"\x1b[B", b"\x1bOB"),
    (NamedKey::ArrowRight, b"\x1b[C", b"\x1bOC"),
    (NamedKey::ArrowLeft, b"\x1b[D", b"\x1bOD"),
];

#[test]
fn decckm_is_read_live_from_the_mode_table() {
    let (mut t, mut b) = term();
    let none = KeyMods::default();
    for (k, normal, app) in ARROWS {
        let spec = KeySpec::Named(k);
        assert_eq!(t.encode_key(&spec, none).unwrap(), normal, "{k:?} power-on");
        feed(&mut t, &mut b, b"\x1b[?1h");
        assert_eq!(t.encode_key(&spec, none).unwrap(), app, "{k:?} DECCKM set");
        feed(&mut t, &mut b, b"\x1b[?1l");
        assert_eq!(
            t.encode_key(&spec, none).unwrap(),
            normal,
            "{k:?} DECCKM reset"
        );
    }
    // Home / End follow the same flag.
    feed(&mut t, &mut b, b"\x1b[?1h");
    assert_eq!(
        t.encode_key(&KeySpec::Named(NamedKey::Home), none).unwrap(),
        b"\x1bOH"
    );
    assert_eq!(
        t.encode_key(&KeySpec::Named(NamedKey::End), none).unwrap(),
        b"\x1bOF"
    );
    feed(&mut t, &mut b, b"\x1b[?1l");
    assert_eq!(
        t.encode_key(&KeySpec::Named(NamedKey::Home), none).unwrap(),
        b"\x1b[H"
    );
}

#[test]
fn decckm_survives_the_alt_screen_round_trip() {
    let (mut t, mut b) = term();
    let none = KeyMods::default();
    let up = KeySpec::Named(NamedKey::ArrowUp);
    feed(&mut t, &mut b, b"\x1b[?1h");
    feed(&mut t, &mut b, b"\x1b[?1049h");
    assert!(t.mode_snapshot().alt_screen);
    // DECCKM is not part of the alt-screen save/restore in xterm; whatever the
    // engine does, the encoder must agree with the snapshot.
    let snap = t.mode_snapshot();
    let expect: &[u8] = if snap.app_cursor {
        b"\x1bOA"
    } else {
        b"\x1b[A"
    };
    assert_eq!(t.encode_key(&up, none).unwrap(), expect);
    feed(&mut t, &mut b, b"\x1b[?1049l");
    assert!(!t.mode_snapshot().alt_screen);
    let snap = t.mode_snapshot();
    let expect: &[u8] = if snap.app_cursor {
        b"\x1bOA"
    } else {
        b"\x1b[A"
    };
    assert_eq!(t.encode_key(&up, none).unwrap(), expect);
}

/// The kitty keyboard stack swaps with the screen (pre-existing behaviour).
///
/// `US-0105` inverted the second half of this test: `encode_key` used to ignore
/// the flags, and now honours them. The stack mechanics are unchanged, so the
/// probe is the one key the disambiguate flag moves -- `Escape`, `0x1b` in the
/// legacy encoding and `CSI 27 u` once any flag is pushed.
#[test]
fn kitty_flags_swap_with_alt_screen_and_reach_the_bytes() {
    let (mut t, mut b) = term();
    let none = KeyMods::default();
    let escape = KeySpec::Named(NamedKey::Escape);
    assert_eq!(t.encode_key(&escape, none).unwrap(), b"\x1b");

    // every kitty flag combination, pushed with CSI > Ps u
    for flags in 0u8..32 {
        feed(&mut t, &mut b, format!("\x1b[>{flags}u").as_bytes());
        assert_eq!(t.keyboard_flags().bits(), flags, "flags {flags} not stored");
        // Bit 0b1 disambiguates and bit 0b1000 reports every key; either one
        // puts `Escape` in its `CSI u` form, and nothing else does.
        let want: &[u8] = if flags & 0b1001 != 0 {
            b"\x1b[27u"
        } else {
            b"\x1b"
        };
        assert_eq!(
            t.encode_key(&escape, none).unwrap(),
            want,
            "kitty flags {flags}"
        );
    }
    // push / pop through CSI = Ps ; Pm u and CSI < Ps u
    feed(&mut t, &mut b, b"\x1b[=5;1u");
    assert_eq!(t.encode_key(&escape, none).unwrap(), b"\x1b[27u");
    feed(&mut t, &mut b, b"\x1b[>3u");
    let before_alt = t.keyboard_flags().bits();
    feed(&mut t, &mut b, b"\x1b[?1049h");
    let in_alt = t.keyboard_flags().bits();
    feed(&mut t, &mut b, b"\x1b[>7u");
    feed(&mut t, &mut b, b"\x1b[?1049l");
    assert_eq!(
        t.keyboard_flags().bits(),
        before_alt,
        "the keyboard stack did not come back with the primary screen \
         (entered alt with {in_alt})"
    );
    assert_eq!(t.encode_key(&escape, none).unwrap(), b"\x1b[27u");
    // A pop uncovers whatever was pushed before it; the encoding follows the
    // live flags rather than a cached decision either way.
    feed(&mut t, &mut b, b"\x1b[<1u");
    let live = t.keyboard_flags().bits();
    let want: &[u8] = if live & 0b1001 != 0 {
        b"\x1b[27u"
    } else {
        b"\x1b"
    };
    assert_eq!(t.encode_key(&escape, none).unwrap(), want, "live {live}");
    // Popping past the bottom resets every flag, and the legacy byte is back.
    feed(&mut t, &mut b, b"\x1b[<99u");
    assert!(t.keyboard_flags().is_empty());
    assert_eq!(t.encode_key(&escape, none).unwrap(), b"\x1b");
}

/// modifyOtherKeys (`CSI > 4 ; Ps m`) now reaches the bytes too.
///
/// `US-0105` inverted this test as well, keeping its four chords as the table.
/// Level `1` is the chords with no unambiguous legacy encoding, level `2` every
/// modified "other" key.
#[test]
fn modify_other_keys_reaches_the_bytes() {
    let (mut t, mut b) = term();
    let mods = KeyMods {
        ctrl: true,
        ..KeyMods::default()
    };
    // (key, level 0, level 1, level 2)
    let probes: [(KeySpec, &[u8], &[u8], &[u8]); 4] = [
        (
            KeySpec::Character("a".into()),
            b"\x01",
            b"\x01",
            b"\x1b[27;5;97~",
        ),
        (
            KeySpec::Character("2".into()),
            b"\x00",
            b"\x1b[27;5;50~",
            b"\x1b[27;5;50~",
        ),
        (
            KeySpec::Named(NamedKey::Enter),
            b"\x1b[13;5u",
            b"\x1b[27;5;13~",
            b"\x1b[27;5;13~",
        ),
        (
            KeySpec::Named(NamedKey::Tab),
            b"\t",
            b"\x1b[27;5;9~",
            b"\x1b[27;5;9~",
        ),
    ];
    for level in [0u8, 1, 2] {
        feed(&mut t, &mut b, format!("\x1b[>4;{level}m").as_bytes());
        assert_eq!(t.modify_other_keys(), level);
        for (key, at0, at1, at2) in &probes {
            let want: &[u8] = match level {
                0 => at0,
                1 => at1,
                _ => at2,
            };
            assert_eq!(
                t.encode_key(key, mods).unwrap(),
                want,
                "{key:?} at modifyOtherKeys {level}"
            );
        }
    }
}

/// DECKPAM / app_keypad likewise.
#[test]
fn app_keypad_never_reaches_the_bytes() {
    let (mut t, mut b) = term();
    let none = KeyMods::default();
    let probes = [
        KeySpec::Named(NamedKey::ArrowUp),
        KeySpec::Named(NamedKey::Home),
        KeySpec::Named(NamedKey::F1),
        KeySpec::Character("5".into()),
    ];
    let before: Vec<_> = probes.iter().map(|p| t.encode_key(p, none)).collect();
    feed(&mut t, &mut b, b"\x1b=");
    assert!(t.mode_snapshot().app_keypad);
    let after: Vec<_> = probes.iter().map(|p| t.encode_key(p, none)).collect();
    assert_eq!(after, before);
}

/// Hostile input through the convenience method.
#[test]
fn terminal_encode_key_does_not_panic_on_hostile_text() {
    let (t, _b) = term();
    let all_mods = [
        KeyMods::default(),
        KeyMods {
            shift: true,
            ctrl: true,
            alt: true,
        },
        KeyMods {
            ctrl: true,
            ..KeyMods::default()
        },
        KeyMods {
            alt: true,
            ..KeyMods::default()
        },
    ];
    let huge = "\u{1f600}".repeat(50_000);
    for text in [
        "",
        "\u{0}",
        "\u{1f600}",
        "\u{10ffff}",
        "\u{feff}",
        "e\u{301}",
        huge.as_str(),
    ] {
        for m in all_mods {
            let _ = t.encode_key(&KeySpec::Character(text.into()), m);
        }
    }
}
