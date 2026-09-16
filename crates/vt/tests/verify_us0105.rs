//! `US-0105`: the key encoder honours the kitty keyboard flags it already
//! answers.
//!
//! Three claims, from outside the crate, the way an embedder sees them:
//!
//! 1. a program that negotiates the protocol and is told "yes" gets the bytes
//!    that protocol defines -- the gap an outside evaluation found;
//! 2. no flag combination, key, modifier set or event kind can make the encoder
//!    panic, and nothing it returns is malformed;
//! 3. the hostile set from `US-0099` still goes through the richer entry point
//!    without panicking.

use std::time::Instant;

use oneterm_vt::input::{KeyEvent, KeyEventKind, KeyMods, KeySpec, NamedKey, encode_key_event};
use oneterm_vt::{Config, EventBatch, KeyboardFlags, ModeSnapshot, Size, Terminal, VtEvent};

fn term() -> (Terminal, EventBatch) {
    (
        Terminal::new(Size { rows: 24, cols: 80 }, Config::default()),
        EventBatch::new(),
    )
}

fn replies(term: &mut Terminal, batch: &mut EventBatch, bytes: &[u8]) -> Vec<u8> {
    batch.clear();
    term.feed(bytes, batch, Instant::now());
    let mut out = Vec::new();
    for event in batch.iter() {
        if let VtEvent::Reply(span) = event {
            out.extend_from_slice(batch.bytes(*span));
        }
    }
    out
}

/// The claim the evaluation's gap 2 would have failed: `CSI ? u` says the flag
/// is set **and** the encoder acts on it.
#[test]
fn a_negotiated_protocol_is_actually_spoken() {
    let (mut t, mut b) = term();
    let escape = KeyEvent::new(KeySpec::Named(NamedKey::Escape), KeyMods::default());
    assert_eq!(
        t.encode_key_event(&escape).as_deref(),
        Some(b"\x1b".as_slice())
    );

    t.feed(b"\x1b[>1u", &mut b, Instant::now());
    assert_eq!(replies(&mut t, &mut b, b"\x1b[?u"), b"\x1b[?1u");
    assert_eq!(
        t.encode_key_event(&escape).as_deref(),
        Some(b"\x1b[27u".as_slice()),
        "the terminal answered CSI ? 1 u and then sent a legacy Escape"
    );

    // ...and popping the flag takes the claim back with it.
    t.feed(b"\x1b[<1u", &mut b, Instant::now());
    assert_eq!(replies(&mut t, &mut b, b"\x1b[?u"), b"\x1b[?0u");
    assert_eq!(
        t.encode_key_event(&escape).as_deref(),
        Some(b"\x1b".as_slice())
    );
}

/// `Terminal::encode_key` and the free function agree, and both see the live
/// flags rather than a stale snapshot.
#[test]
fn the_two_entry_points_agree_under_flags() {
    let (mut t, mut b) = term();
    t.feed(b"\x1b[>9u", &mut b, Instant::now());
    let modes = t.mode_snapshot();
    assert_eq!(modes.keyboard_flags.bits(), 9);
    for key in named_keys().into_iter().chain([
        KeySpec::Character("a".into()),
        KeySpec::Character("\u{1f600}".into()),
    ]) {
        for mods in mod_sets() {
            let event = KeyEvent::new(key.clone(), mods);
            assert_eq!(t.encode_key_event(&event), encode_key_event(&event, &modes));
            assert_eq!(t.encode_key(&key, mods), encode_key_event(&event, &modes));
        }
    }
}

fn named_keys() -> Vec<KeySpec> {
    use NamedKey::*;
    [
        Enter, Backspace, Delete, Tab, Escape, ArrowUp, ArrowDown, ArrowLeft, ArrowRight, Home,
        End, PageUp, PageDown, Insert, F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12, F13, F14,
        F15, F16, F17, F18, F19, F20, F21, F22, F23, F24,
    ]
    .into_iter()
    .map(KeySpec::Named)
    .collect()
}

fn mod_sets() -> Vec<KeyMods> {
    (0u8..8)
        .map(|bits| KeyMods {
            shift: bits & 1 != 0,
            ctrl: bits & 2 != 0,
            alt: bits & 4 != 0,
        })
        .collect()
}

fn key_specs() -> Vec<KeySpec> {
    let mut v = named_keys();
    for text in [
        "a",
        "A",
        "z",
        "Z",
        "0",
        "1",
        "9",
        " ",
        "2",
        "@",
        "3",
        "[",
        "4",
        "\\",
        "5",
        "]",
        "6",
        "^",
        "7",
        "_",
        "/",
        "8",
        "?",
        ",",
        ".",
        "~",
        "\x7f",
        "\u{0}",
        "",
        "e\u{301}",
        "\u{e9}",
        "\u{1f600}",
        "\u{10ffff}",
        "ab",
        "\u{4e2d}\u{6587}",
        "\r\n",
        "\t",
    ] {
        v.push(KeySpec::Character(text.into()));
    }
    v
}

/// Every non-`None` answer is a well-formed escape sequence or a legacy byte
/// string. A parser written here rather than in the crate, so it cannot agree
/// with the encoder by construction.
fn well_formed(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return true;
    }
    // `ESC [` on its own is legacy `Alt+[` -- the very ambiguity the
    // disambiguate flag exists to remove, and still the right answer when no
    // flag is set.
    if bytes == b"\x1b[" {
        return true;
    }
    // Alt prefixes ESC onto a legacy byte string; strip at most one.
    let body = match bytes {
        [0x1b, b'[', ..] | [0x1b, b'O', ..] => bytes,
        [0x1b, rest @ ..] if !rest.is_empty() => rest,
        _ => bytes,
    };
    let Some(params) = body.strip_prefix(b"\x1b[") else {
        // A legacy byte string: SS3 or raw bytes, never a bare CSI.
        return !body.starts_with(b"\x1b[");
    };
    let Some((&last, params)) = params.split_last() else {
        return false;
    };
    if !matches!(last, b'u' | b'~' | b'A'..=b'Z') {
        return false;
    }
    // Parameters: digits, `;` and `:` only, and no empty sub-field at the end.
    params
        .iter()
        .all(|byte| byte.is_ascii_digit() || matches!(byte, b';' | b':'))
        && !params.ends_with(b";")
        && !params.ends_with(b":")
}

/// All 32 flag values x every key x every modifier set x all three event kinds:
/// no panic, and nothing malformed.
#[test]
fn the_whole_flag_space_is_safe() {
    let specs = key_specs();
    let kinds = [
        KeyEventKind::Press,
        KeyEventKind::Repeat,
        KeyEventKind::Release,
    ];
    let mut cases = 0u64;
    let mut emitted = 0u64;
    for bits in 0u8..32 {
        let mut modes = ModeSnapshot::default();
        modes.keyboard_flags = KeyboardFlags::from_bits_truncate(bits);
        for app_cursor in [false, true] {
            modes.app_cursor = app_cursor;
            for spec in &specs {
                for mods in mod_sets() {
                    for kind in kinds {
                        let mut event = KeyEvent::new(spec.clone(), mods);
                        event.kind = kind;
                        event.shifted = Some('A');
                        event.base_layout = Some('a');
                        event.text = Some("a".into());
                        cases += 1;
                        if let Some(bytes) = encode_key_event(&event, &modes) {
                            emitted += 1;
                            assert!(
                                well_formed(&bytes),
                                "malformed {bytes:?} for {spec:?} {mods:?} {kind:?} flags {bits}"
                            );
                        }
                    }
                }
            }
        }
    }
    println!("flag cross-product: {cases} cases, {emitted} produced bytes");
    assert_eq!(cases, 32 * 2 * specs.len() as u64 * 8 * 3);
}

/// `US-0099`'s hostile set, through the new entry point.
#[test]
fn hostile_input_does_not_panic() {
    let huge = "x".repeat(100_000);
    let payloads = [
        "\u{1f600}",
        "\u{10ffff}",
        "\u{feff}",
        "e\u{301}",
        "\u{0}",
        "",
        huge.as_str(),
    ];
    for bits in 0u8..32 {
        let mut modes = ModeSnapshot::default();
        modes.keyboard_flags = KeyboardFlags::from_bits_truncate(bits);
        modes.modify_other_keys = bits % 3;
        for text in payloads {
            for mods in mod_sets() {
                let mut event = KeyEvent::new(KeySpec::Character(text.into()), mods);
                // A text field carrying a C0 byte must drop the field, never
                // emit a code point the receiver has to reject.
                event.text = Some(format!("a\u{7}{text}"));
                let bytes = encode_key_event(&event, &modes).unwrap_or_default();
                assert!(well_formed(&bytes));
                // An escape sequence this encoder *builds* is bounded. A
                // `Character` payload the embedder hands over on the legacy
                // rung is written through unchanged, as it always was.
                if bytes.starts_with(b"\x1b[") {
                    assert!(
                        bytes.len() < 1024,
                        "{} bytes of escape sequence for a hostile payload",
                        bytes.len()
                    );
                }
            }
        }
    }
}

/// A release with nothing negotiated sends nothing, which is the contract an
/// embedder delivering key-up events has to tolerate.
#[test]
fn a_release_is_silent_until_it_is_asked_for() {
    let (mut t, mut b) = term();
    let mut event = KeyEvent::new(KeySpec::Character("a".into()), KeyMods::default());
    event.kind = KeyEventKind::Release;
    assert_eq!(t.encode_key_event(&event), None);

    // `REPORT_EVENT_TYPES` alone is not enough for a key that produces text:
    // "events are not supported for them, unless the application requests key
    // report mode". A held letter must keep typing the letter.
    t.feed(b"\x1b[>2u", &mut b, Instant::now());
    assert_eq!(t.encode_key_event(&event), None);
    let mut repeat = event.clone();
    repeat.kind = KeyEventKind::Repeat;
    assert_eq!(
        t.encode_key_event(&repeat).as_deref(),
        Some(b"a".as_slice())
    );

    // A key that produces no text gets its event types from the flag alone.
    let mut up = KeyEvent::new(KeySpec::Named(NamedKey::ArrowUp), KeyMods::default());
    up.kind = KeyEventKind::Release;
    assert_eq!(
        t.encode_key_event(&up).as_deref(),
        Some(b"\x1b[1;1:3A".as_slice())
    );

    // Key report mode turns the letter into escape codes, releases included.
    t.feed(b"\x1b[>10u", &mut b, Instant::now());
    assert_eq!(
        t.encode_key_event(&event).as_deref(),
        Some(b"\x1b[97;1:3u".as_slice())
    );
}
