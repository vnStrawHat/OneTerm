//! RE-VERIFICATION of US-0105 at 63c99535, written fresh against the
//! specification page rather than against the table the branch adopted.
//!
//! Nothing here reads `verify_us0105_independent.rs`; every expectation is
//! re-derived from <https://sw.kovidgoyal.net/kitty/keyboard-protocol/> and,
//! for `modifyOtherKeys`, from xterm(1).

use oneterm_vt::input::{
    KeyEvent, KeyEventKind, KeyMods, KeySpec, NamedKey, encode_key, encode_key_event,
};
use oneterm_vt::{Config, EventBatch, KeyboardFlags, ModeSnapshot, Size, Terminal, VtEvent};
use std::time::Instant;

const DIS: KeyboardFlags = KeyboardFlags::DISAMBIGUATE_ESC_CODES;
const EVT: KeyboardFlags = KeyboardFlags::REPORT_EVENT_TYPES;
const ALT_K: KeyboardFlags = KeyboardFlags::REPORT_ALTERNATE_KEYS;
const ALL: KeyboardFlags = KeyboardFlags::REPORT_ALL_KEYS_AS_ESC;
const TXT: KeyboardFlags = KeyboardFlags::REPORT_ASSOCIATED_TEXT;

fn snap(flags: KeyboardFlags, level: u8, app_cursor: bool, app_keypad: bool) -> ModeSnapshot {
    let mut m = ModeSnapshot::default();
    m.keyboard_flags = flags;
    m.modify_other_keys = level;
    m.app_cursor = app_cursor;
    m.app_keypad = app_keypad;
    m
}

fn ch(text: &str) -> KeySpec {
    KeySpec::Character(text.into())
}

fn named(key: NamedKey) -> KeySpec {
    KeySpec::Named(key)
}

fn km(shift: bool, ctrl: bool, alt: bool) -> KeyMods {
    KeyMods { shift, ctrl, alt }
}

fn show(bytes: Option<&[u8]>) -> String {
    match bytes {
        None => "<none>".into(),
        Some(b) => b
            .iter()
            .map(|byte| match byte {
                0x1b => "ESC".to_string(),
                0x20..=0x7e => (*byte as char).to_string(),
                other => format!("<{other:#04x}>"),
            })
            .collect::<Vec<_>>()
            .join(""),
    }
}

#[derive(Default)]
struct Board {
    total: usize,
    bad: Vec<String>,
}

impl Board {
    fn check(&mut self, source: &str, got: Option<Vec<u8>>, want: Option<&[u8]>) {
        self.total += 1;
        if got.as_deref() != want {
            self.bad.push(format!(
                "  {source}\n      want {}\n      got  {}",
                show(want),
                show(got.as_deref())
            ));
        }
    }

    fn report(self, name: &str) {
        println!(
            "[{name}] {} cases, {} mismatches",
            self.total,
            self.bad.len()
        );
        if !self.bad.is_empty() {
            panic!(
                "[{name}] {}/{} mismatches:\n{}",
                self.bad.len(),
                self.total,
                self.bad.join("\n")
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 1. The full event-type matrix: 32 flags x key class x press/repeat/release.
//    Rules re-derived from the specification:
//      * a release is reported only when REPORT_EVENT_TYPES is set AND the
//        event may carry an event type at all;
//      * an event may carry one unless it produces text or is Enter/Tab/
//        Backspace, in which case REPORT_ALL_KEYS_AS_ESC must also be set;
//      * a repeat that may not be reported is encoded as a press.
// ---------------------------------------------------------------------------

#[test]
fn event_type_matrix_over_every_flag_set() {
    let mut board = Board::default();
    let classes: &[(&str, KeySpec)] = &[
        ("text key", ch("a")),
        ("functional", named(NamedKey::ArrowUp)),
        ("functional tilde", named(NamedKey::F5)),
        ("Escape", named(NamedKey::Escape)),
        ("Enter", named(NamedKey::Enter)),
        ("Tab", named(NamedKey::Tab)),
        ("Backspace", named(NamedKey::Backspace)),
    ];
    for bits in 0u8..32 {
        let flags = KeyboardFlags::from_bits_truncate(bits);
        let m = snap(flags, 0, false, false);
        for (label, key) in classes {
            let legacy_c0 = matches!(
                key,
                KeySpec::Named(NamedKey::Enter | NamedKey::Tab | NamedKey::Backspace)
            );
            let text_key = matches!(key, KeySpec::Character(_));
            let all = flags.contains(ALL);
            let evt = flags.contains(EVT);
            let reportable = evt && (all || !(legacy_c0 || text_key));

            let press = KeyEvent::new(key.clone(), KeyMods::default());
            let mut repeat = press.clone();
            repeat.kind = KeyEventKind::Repeat;
            let mut release = press.clone();
            release.kind = KeyEventKind::Release;

            let press_bytes = encode_key_event(&press, &m);

            // A release sends bytes only when it is reportable.
            let want_release: Option<Vec<u8>> = if reportable {
                // The press form with the event type appended.
                Some(with_event_type(&press_bytes, 3, key))
            } else {
                None
            };
            board.check(
                &format!("release of {label} under {flags:?}"),
                encode_key_event(&release, &m),
                want_release.as_deref(),
            );

            // A repeat is the press when it is not reportable.
            // A repeat inserts the character, so it carries associated text
            // exactly as the press does; a release inserts nothing, so it must
            // not.
            let want_repeat: Option<Vec<u8>> = if reportable {
                Some(with_event_type_and_text(
                    &press_bytes,
                    2,
                    key,
                    flags,
                    text_key,
                ))
            } else {
                press_bytes.clone()
            };
            board.check(
                &format!("repeat of {label} under {flags:?}"),
                encode_key_event(&repeat, &m),
                want_repeat.as_deref(),
            );
        }
    }
    board.report("event-type-matrix");
}

/// The press bytes with the `:<type>` sub-field attached, built from the
/// specification's shape rules rather than from the encoder.
fn with_event_type_and_text(
    press: &Option<Vec<u8>>,
    kind: u8,
    key: &KeySpec,
    flags: KeyboardFlags,
    text_key: bool,
) -> Vec<u8> {
    let mut out = with_event_type(press, kind, key);
    if text_key && flags.contains(ALL) && flags.contains(TXT) {
        // `CSI 97 ; 1 : 2 ; 97 u`
        out.pop();
        out.extend_from_slice(b";97u");
    }
    out
}

fn with_event_type(press: &Option<Vec<u8>>, kind: u8, key: &KeySpec) -> Vec<u8> {
    let code: &[u8] = match key {
        KeySpec::Character(_) => b"97",
        KeySpec::Named(NamedKey::Escape) => b"27",
        KeySpec::Named(NamedKey::Enter) => b"13",
        KeySpec::Named(NamedKey::Tab) => b"9",
        KeySpec::Named(NamedKey::Backspace) => b"127",
        KeySpec::Named(NamedKey::F5) => b"15",
        _ => b"1",
    };
    let _ = press;
    match key {
        KeySpec::Named(NamedKey::ArrowUp) => format!("\x1b[1;1:{kind}A").into_bytes(),
        KeySpec::Named(NamedKey::F5) => format!("\x1b[15;1:{kind}~").into_bytes(),
        _ => format!(
            "\x1b[{};1:{kind}u",
            String::from_utf8(code.to_vec()).unwrap()
        )
        .into_bytes(),
    }
}

// ---------------------------------------------------------------------------
// 2. Associated text under every modifier combination.
//    "the text associated with key events"; "The associated text must not
//    contain control codes". A chord whose legacy answer is a control byte, or
//    which the platform consumes (alt), has no associated text.
// ---------------------------------------------------------------------------

#[test]
fn associated_text_over_every_modifier_set() {
    let mut board = Board::default();
    let m = snap(ALL | TXT, 0, false, false);
    // (label, payload, mods, expected)
    let rows: &[(&str, &str, KeyMods, &[u8])] = &[
        ("plain a", "a", km(false, false, false), b"\x1b[97;;97u"),
        ("shift+a", "A", km(true, false, false), b"\x1b[97;2;65u"),
        ("ctrl+a", "a", km(false, true, false), b"\x1b[97;5u"),
        ("alt+a", "a", km(false, false, true), b"\x1b[97;3u"),
        ("ctrl+shift+a", "A", km(true, true, false), b"\x1b[97;6u"),
        ("ctrl+alt+a", "a", km(false, true, true), b"\x1b[97;7u"),
        ("shift+alt+a", "A", km(true, false, true), b"\x1b[97;4u"),
        ("ctrl+shift+alt+a", "A", km(true, true, true), b"\x1b[97;8u"),
    ];
    for (label, payload, mods, want) in rows {
        board.check(
            &format!("Text as code points: {label}"),
            encode_key_event(&KeyEvent::new(ch(payload), *mods), &m),
            Some(*want),
        );
    }
    // Embedder-supplied text: the platform is the authority on what a chord
    // produced, so it wins even with ctrl held. Recorded as this engine's
    // documented rule; the specification has no sentence forbidding it.
    let mut supplied = KeyEvent::new(ch("a"), km(false, true, false));
    supplied.text = Some("a".into());
    board.check(
        "Text as code points: embedder-supplied text wins over the ctrl rule",
        encode_key_event(&supplied, &m),
        Some(b"\x1b[97;5;97u"),
    );
    // Shift alone still carries the text: the layout produced it.
    board.report("associated-text-modifiers");
}

// ---------------------------------------------------------------------------
// 3. The specification's detection recipe, push/pop, and the per-screen stack.
// ---------------------------------------------------------------------------

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

#[test]
fn the_detection_recipe_and_the_stack() {
    let mut term = Terminal::new(Size { rows: 24, cols: 80 }, Config::default());
    let mut batch = EventBatch::new();
    let esc = KeyEvent::new(named(NamedKey::Escape), KeyMods::default());
    let now = Instant::now();

    // "first setting the desired progressive enhancements and then querying".
    assert_eq!(replies(&mut term, &mut batch, b"\x1b[?u"), b"\x1b[?0u");
    term.feed(b"\x1b[=5;1u", &mut batch, now);
    assert_eq!(
        replies(&mut term, &mut batch, b"\x1b[?u"),
        b"\x1b[?5u",
        "CSI = 5 ; 1 u must be visible to CSI ? u"
    );
    assert_eq!(
        term.encode_key_event(&esc).as_deref(),
        Some(b"\x1b[27u".as_slice())
    );
    // Mode 2 is a union, mode 3 a difference.
    term.feed(b"\x1b[=2;2u", &mut batch, now);
    assert_eq!(replies(&mut term, &mut batch, b"\x1b[?u"), b"\x1b[?7u");
    term.feed(b"\x1b[=4;3u", &mut batch, now);
    assert_eq!(replies(&mut term, &mut batch, b"\x1b[?u"), b"\x1b[?3u");

    // Push and pop.
    term.feed(b"\x1b[>1u", &mut batch, now);
    assert_eq!(replies(&mut term, &mut batch, b"\x1b[?u"), b"\x1b[?1u");
    term.feed(b"\x1b[<u", &mut batch, now);
    // Expectation corrected: the push above was the *first* stack entry, so
    // this pop empties the stack, and the specification's own next sentence --
    // quoted below and by the original row -- is "If a pop request is received
    // that empties the stack, **all flags are reset**." It resets to `0`, not
    // to the live value `CSI = 4 ; 3 u` had set before the push, which the
    // stack never held. A pop restores what the push covered only while
    // something is left underneath, which the next block checks.
    assert_eq!(
        replies(&mut term, &mut batch, b"\x1b[?u"),
        b"\x1b[?0u",
        "a pop that empties the stack resets every flag"
    );
    // ...and with two entries, the pop really does uncover the older one.
    term.feed(b"\x1b[>3u\x1b[>1u", &mut batch, now);
    assert_eq!(replies(&mut term, &mut batch, b"\x1b[?u"), b"\x1b[?1u");
    term.feed(b"\x1b[<u", &mut batch, now);
    assert_eq!(
        replies(&mut term, &mut batch, b"\x1b[?u"),
        b"\x1b[?3u",
        "a pop restores what the push covered"
    );
    term.feed(b"\x1b[<u", &mut batch, now);
    // "If a pop request is received that empties the stack, all flags are reset."
    term.feed(b"\x1b[<99u", &mut batch, now);
    assert_eq!(replies(&mut term, &mut batch, b"\x1b[?u"), b"\x1b[?0u");
    assert_eq!(
        term.encode_key_event(&esc).as_deref(),
        Some(b"\x1b".as_slice())
    );

    // "Terminals must maintain separate stacks for the main and alternate
    // screens."
    term.feed(b"\x1b[>1u", &mut batch, now);
    term.feed(b"\x1b[?1049h", &mut batch, now);
    assert_eq!(
        replies(&mut term, &mut batch, b"\x1b[?u"),
        b"\x1b[?0u",
        "the alternate screen has its own stack"
    );
    term.feed(b"\x1b[>9u", &mut batch, now);
    assert_eq!(replies(&mut term, &mut batch, b"\x1b[?u"), b"\x1b[?9u");
    term.feed(b"\x1b[?1049l", &mut batch, now);
    assert_eq!(
        replies(&mut term, &mut batch, b"\x1b[?u"),
        b"\x1b[?1u",
        "the main screen kept its own"
    );
}

// ---------------------------------------------------------------------------
// 4. modifyOtherKeys, per xterm(1).
// ---------------------------------------------------------------------------

#[test]
fn modify_other_keys_matrix() {
    let mut board = Board::default();
    let ctrl = km(false, true, false);
    let shift = km(true, false, false);
    // Level 0 is the legacy table for everything.
    for (label, key, mods) in [
        ("ctrl+a", ch("a"), ctrl),
        ("ctrl+2", ch("2"), ctrl),
        ("shift+a", ch("A"), shift),
    ] {
        board.check(
            &format!("modifyOtherKeys 0: {label}"),
            encode_key_event(
                &KeyEvent::new(key.clone(), mods),
                &snap(KeyboardFlags::empty(), 0, false, false),
            ),
            encode_key(&key, mods, &snap(KeyboardFlags::empty(), 0, false, false)).as_deref(),
        );
    }
    // Level 1: "except for those with well-known behavior, e.g., Tab,
    // Backarrow and some special control character cases which are built into
    // the X11 library, e.g., Control-Space to make a NUL, or Control-3 to make
    // an Escape character."  A chord whose legacy answer is already a control
    // byte stays legacy; one that is not gets the sequence.
    let l1: &[(&str, KeySpec, KeyMods, &[u8])] = &[
        ("ctrl+a stays 0x01", ch("a"), ctrl, b"\x01"),
        ("ctrl+space stays NUL", ch(" "), ctrl, b"\x00"),
        ("ctrl+3 stays ESC", ch("3"), ctrl, b"\x1b"),
        ("ctrl+Tab stays 0x09", named(NamedKey::Tab), ctrl, b"\x09"),
        (
            "ctrl+Backspace stays 0x08",
            named(NamedKey::Backspace),
            ctrl,
            b"\x08",
        ),
        ("ctrl+; is escaped", ch(";"), ctrl, b"\x1b[27;5;59~"),
        ("ctrl+0 is escaped", ch("0"), ctrl, b"\x1b[27;5;48~"),
        ("ctrl+1 is escaped", ch("1"), ctrl, b"\x1b[27;5;49~"),
        ("ctrl+9 is escaped", ch("9"), ctrl, b"\x1b[27;5;57~"),
        // shift alone never fires: the layout consumed it to make the letter.
        ("shift+a is the letter", ch("A"), shift, b"A"),
    ];
    for (label, key, mods, want) in l1 {
        board.check(
            &format!("modifyOtherKeys 1: {label}"),
            encode_key_event(
                &KeyEvent::new(key.clone(), *mods),
                &snap(KeyboardFlags::empty(), 1, false, false),
            ),
            Some(*want),
        );
    }
    // Level 2: "Enables this feature for keys including the exceptions listed."
    let l2: &[(&str, KeySpec, KeyMods, &[u8])] = &[
        ("ctrl+a", ch("a"), ctrl, b"\x1b[27;5;97~"),
        ("ctrl+space", ch(" "), ctrl, b"\x1b[27;5;32~"),
        ("ctrl+3", ch("3"), ctrl, b"\x1b[27;5;51~"),
        ("ctrl+Tab", named(NamedKey::Tab), ctrl, b"\x1b[27;5;9~"),
        ("ctrl+Enter", named(NamedKey::Enter), ctrl, b"\x1b[27;5;13~"),
        (
            "ctrl+Backspace",
            named(NamedKey::Backspace),
            ctrl,
            b"\x1b[27;5;127~",
        ),
        (
            "ctrl+Escape",
            named(NamedKey::Escape),
            ctrl,
            b"\x1b[27;5;27~",
        ),
        // A capital letter must stay a capital letter.
        ("shift+a", ch("A"), shift, b"A"),
        // A functional key already has an unambiguous encoding.
        ("ctrl+Up", named(NamedKey::ArrowUp), ctrl, b"\x1b[1;5A"),
        ("ctrl+F5", named(NamedKey::F5), ctrl, b"\x1b[15~"),
    ];
    for (label, key, mods, want) in l2 {
        board.check(
            &format!("modifyOtherKeys 2: {label}"),
            encode_key_event(
                &KeyEvent::new(key.clone(), *mods),
                &snap(KeyboardFlags::empty(), 2, false, false),
            ),
            Some(*want),
        );
    }
    // The kitty flags supersede modifyOtherKeys at every level.
    for level in 0u8..3 {
        board.check(
            &format!("kitty wins over modifyOtherKeys {level}"),
            encode_key_event(
                &KeyEvent::new(ch("a"), ctrl),
                &snap(DIS, level, false, false),
            ),
            Some(b"\x1b[97;5u"),
        );
    }
    board.report("modify-other-keys");
}

/// What `alt` alone does at level 1 is the one row xterm(1) is quoted both ways
/// on, so it is measured and printed rather than asserted.
#[test]
fn print_alt_at_modify_other_keys_level_one() {
    let alt = km(false, false, true);
    for level in 0u8..3 {
        let got = encode_key_event(
            &KeyEvent::new(ch("a"), alt),
            &snap(KeyboardFlags::empty(), level, false, false),
        );
        println!(
            "[alt-level] modifyOtherKeys {level}: alt+a -> {}",
            show(got.as_deref())
        );
    }
}

// ---------------------------------------------------------------------------
// 5. The PC-101 shift relation, over every printable ASCII key.
// ---------------------------------------------------------------------------

#[test]
fn unshifted_key_code_for_every_printable_ascii_key() {
    let mut board = Board::default();
    let m = snap(DIS, 0, false, false);
    // The US keyboard's shift relation, written out here independently.
    let pairs: &[(char, char)] = &[
        ('~', '`'),
        ('!', '1'),
        ('@', '2'),
        ('#', '3'),
        ('$', '4'),
        ('%', '5'),
        ('^', '6'),
        ('&', '7'),
        ('*', '8'),
        ('(', '9'),
        (')', '0'),
        ('_', '-'),
        ('+', '='),
        ('{', '['),
        ('}', ']'),
        ('|', '\\'),
        (':', ';'),
        ('"', '\''),
        ('<', ','),
        ('>', '.'),
        ('?', '/'),
    ];
    for (shifted, unshifted) in pairs {
        let want = format!("\x1b[{};6u", *unshifted as u32);
        board.check(
            &format!("Key codes: ctrl+shift+{unshifted} reports {unshifted}"),
            encode_key_event(
                &KeyEvent::new(ch(&shifted.to_string()), km(true, true, false)),
                &m,
            ),
            Some(want.as_bytes()),
        );
    }
    for letter in 'a'..='z' {
        let upper = letter.to_ascii_uppercase();
        let want = format!("\x1b[{};6u", letter as u32);
        board.check(
            &format!("Key codes: ctrl+shift+{letter} reports {letter}"),
            encode_key_event(
                &KeyEvent::new(ch(&upper.to_string()), km(true, true, false)),
                &m,
            ),
            Some(want.as_bytes()),
        );
    }
    // Un-shifted keys report themselves.
    for key in "`1234567890-=[]\\;',./".chars() {
        let want = format!("\x1b[{};5u", key as u32);
        board.check(
            &format!("Key codes: ctrl+{key} reports itself"),
            encode_key_event(
                &KeyEvent::new(ch(&key.to_string()), km(false, true, false)),
                &m,
            ),
            Some(want.as_bytes()),
        );
    }
    board.report("pc101-shift-table");
}

// ---------------------------------------------------------------------------
// 6. F1-F24 in both rungs, and the Cursor Position Report guard.
// ---------------------------------------------------------------------------

#[test]
fn function_keys_in_both_rungs() {
    let mut board = Board::default();
    let kitty = snap(ALL, 0, false, false);
    let legacy = snap(KeyboardFlags::empty(), 0, false, false);
    let table: &[(NamedKey, &[u8])] = &[
        (NamedKey::F1, b"\x1b[P"),
        (NamedKey::F2, b"\x1b[Q"),
        (NamedKey::F3, b"\x1b[13~"),
        (NamedKey::F4, b"\x1b[S"),
        (NamedKey::F5, b"\x1b[15~"),
        (NamedKey::F6, b"\x1b[17~"),
        (NamedKey::F7, b"\x1b[18~"),
        (NamedKey::F8, b"\x1b[19~"),
        (NamedKey::F9, b"\x1b[20~"),
        (NamedKey::F10, b"\x1b[21~"),
        (NamedKey::F11, b"\x1b[23~"),
        (NamedKey::F12, b"\x1b[24~"),
        (NamedKey::F13, b"\x1b[57376u"),
        (NamedKey::F14, b"\x1b[57377u"),
        (NamedKey::F15, b"\x1b[57378u"),
        (NamedKey::F16, b"\x1b[57379u"),
        (NamedKey::F17, b"\x1b[57380u"),
        (NamedKey::F18, b"\x1b[57381u"),
        (NamedKey::F19, b"\x1b[57382u"),
        (NamedKey::F20, b"\x1b[57383u"),
        (NamedKey::F21, b"\x1b[57384u"),
        (NamedKey::F22, b"\x1b[57385u"),
        (NamedKey::F23, b"\x1b[57386u"),
        (NamedKey::F24, b"\x1b[57387u"),
    ];
    for (key, want) in table {
        board.check(
            &format!("Functional key codes: {key:?}"),
            encode_key_event(&KeyEvent::new(named(*key), KeyMods::default()), &kitty),
            Some(*want),
        );
        // No phantom shift: the modifier field must be absent with no modifier.
        let with_shift =
            encode_key_event(&KeyEvent::new(named(*key), km(true, false, false)), &kitty);
        assert_ne!(
            with_shift.as_deref(),
            Some(*want),
            "{key:?} must distinguish shift from no shift"
        );
    }
    // The legacy rung's F15 answer is measured and printed: `CSI 1 ; 2 R` is
    // byte-identical to a Cursor Position Report for row 1, column 2.
    let f15_legacy = encode_key(&named(NamedKey::F15), KeyMods::default(), &legacy);
    println!(
        "[legacy-f15] F15 on the legacy rung -> {}",
        show(f15_legacy.as_deref())
    );
    board.report("function-keys");
}

/// No key event, under any flag set, modifier set or event kind, may end in the
/// final byte `R`: that is the Cursor Position Report.
#[test]
fn no_kitty_form_collides_with_the_cursor_position_report() {
    let mut collisions = Vec::new();
    let keys: Vec<KeySpec> = (1..=24)
        .map(|n| match n {
            1 => NamedKey::F1,
            2 => NamedKey::F2,
            3 => NamedKey::F3,
            4 => NamedKey::F4,
            5 => NamedKey::F5,
            6 => NamedKey::F6,
            7 => NamedKey::F7,
            8 => NamedKey::F8,
            9 => NamedKey::F9,
            10 => NamedKey::F10,
            11 => NamedKey::F11,
            12 => NamedKey::F12,
            13 => NamedKey::F13,
            14 => NamedKey::F14,
            15 => NamedKey::F15,
            16 => NamedKey::F16,
            17 => NamedKey::F17,
            18 => NamedKey::F18,
            19 => NamedKey::F19,
            20 => NamedKey::F20,
            21 => NamedKey::F21,
            22 => NamedKey::F22,
            23 => NamedKey::F23,
            _ => NamedKey::F24,
        })
        .map(named)
        .chain(
            [
                NamedKey::Escape,
                NamedKey::Enter,
                NamedKey::Tab,
                NamedKey::Backspace,
                NamedKey::ArrowUp,
                NamedKey::ArrowDown,
                NamedKey::ArrowLeft,
                NamedKey::ArrowRight,
                NamedKey::Home,
                NamedKey::End,
                NamedKey::Insert,
                NamedKey::Delete,
                NamedKey::PageUp,
                NamedKey::PageDown,
            ]
            .into_iter()
            .map(named),
        )
        .chain(["a", "A", "1", "!", " "].into_iter().map(ch))
        .collect();
    let mut count = 0usize;
    for bits in 1u8..32 {
        let flags = KeyboardFlags::from_bits_truncate(bits);
        for level in 0u8..3 {
            let m = snap(flags, level, false, false);
            for key in &keys {
                for bitmods in 0u8..8 {
                    let mods = km(bitmods & 1 != 0, bitmods & 2 != 0, bitmods & 4 != 0);
                    for kind in [
                        KeyEventKind::Press,
                        KeyEventKind::Repeat,
                        KeyEventKind::Release,
                    ] {
                        let mut event = KeyEvent::new(key.clone(), mods);
                        event.kind = kind;
                        count += 1;
                        let Some(bytes) = encode_key_event(&event, &m) else {
                            continue;
                        };
                        if bytes.starts_with(b"\x1b[") && bytes.last() == Some(&b'R') {
                            collisions.push(format!("{event:?} under {flags:?}/{level}"));
                        }
                    }
                }
            }
        }
    }
    println!("[cpr-guard] {count} cases with at least one flag set");
    assert!(
        collisions.is_empty(),
        "{} CPR collisions: {}",
        collisions.len(),
        collisions.join("; ")
    );
}

// ---------------------------------------------------------------------------
// 7. DECCKM and DECKPAM.
// ---------------------------------------------------------------------------

#[test]
fn cursor_key_mode_and_keypad_mode() {
    let mut board = Board::default();
    // Legacy rung: cursor key mode applies to an unmodified arrow only.
    board.check(
        "DECCKM legacy: Up is SS3 A",
        encode_key(
            &named(NamedKey::ArrowUp),
            KeyMods::default(),
            &snap(KeyboardFlags::empty(), 0, true, false),
        ),
        Some(b"\x1bOA"),
    );
    board.check(
        "DECCKM legacy: ctrl+Up is the modified form",
        encode_key(
            &named(NamedKey::ArrowUp),
            km(false, true, false),
            &snap(KeyboardFlags::empty(), 0, true, false),
        ),
        Some(b"\x1b[1;5A"),
    );
    // Every kitty flag set that puts the arrow on rung 2 ignores DECCKM.
    for bits in 1u8..32 {
        let flags = KeyboardFlags::from_bits_truncate(bits);
        if !(flags.contains(DIS) || flags.contains(ALL)) {
            continue;
        }
        board.check(
            &format!("DECCKM is not read on the kitty rung under {flags:?}"),
            encode_key_event(
                &KeyEvent::new(named(NamedKey::ArrowUp), KeyMods::default()),
                &snap(flags, 0, true, false),
            ),
            Some(b"\x1b[A"),
        );
    }
    // DECKPAM: the crate cannot name a keypad key, so app_keypad must change
    // nothing the encoder returns.
    for bits in 0u8..32 {
        let flags = KeyboardFlags::from_bits_truncate(bits);
        for key in [named(NamedKey::ArrowUp), ch("1"), named(NamedKey::Enter)] {
            let off = encode_key_event(
                &KeyEvent::new(key.clone(), KeyMods::default()),
                &snap(flags, 0, false, false),
            );
            let on = encode_key_event(
                &KeyEvent::new(key.clone(), KeyMods::default()),
                &snap(flags, 0, false, true),
            );
            board.check(
                &format!("DECKPAM changes nothing for {key:?} under {flags:?}"),
                on,
                off.as_deref(),
            );
        }
    }
    board.report("deckm-deckpam");
}

// ---------------------------------------------------------------------------
// 8. Alternate keys, and hostile text.
// ---------------------------------------------------------------------------

#[test]
fn alternate_key_sub_fields() {
    let mut board = Board::default();
    let m = snap(ALL | ALT_K, 0, false, false);
    let mut shifted = KeyEvent::new(ch("a"), km(true, false, false));
    shifted.shifted = Some('A');
    board.check(
        "Key codes: one alternate is the shifted key",
        encode_key_event(&shifted, &m),
        Some(b"\x1b[97:65;2u"),
    );
    let mut no_shift = KeyEvent::new(ch("a"), km(false, true, false));
    no_shift.shifted = Some('A');
    board.check(
        "Key codes: the shifted key needs the shift modifier",
        encode_key_event(&no_shift, &m),
        Some(b"\x1b[97;5u"),
    );
    let mut base = KeyEvent::new(ch("\u{441}"), km(false, true, false));
    base.base_layout = Some('c');
    board.check(
        "Key codes: an empty sub-field for a missing shifted key",
        encode_key_event(&base, &m),
        Some("\x1b[1089::99;5u".as_bytes()),
    );
    let mut both = KeyEvent::new(ch("a"), km(true, false, false));
    both.shifted = Some('A');
    both.base_layout = Some('a');
    board.check(
        "Key codes: shifted then base layout",
        encode_key_event(&both, &m),
        Some(b"\x1b[97:65:97;2u"),
    );
    // "only if a key event was already going to be represented as an escape
    // code ... will this enhancement affect it"
    let mut shifted_payload = KeyEvent::new(ch("A"), km(true, false, false));
    shifted_payload.shifted = Some('A');
    board.check(
        "Report alternate keys is inert on its own",
        encode_key_event(&shifted_payload, &snap(ALT_K, 0, false, false)),
        Some(b"A"),
    );
    board.report("alternate-keys");
}

#[test]
fn hostile_text_is_bounded() {
    let payloads = [
        String::new(),
        "\u{0}".into(),
        "\u{1f600}".into(),
        "\u{10ffff}".into(),
        "e\u{301}".into(),
        "a".repeat(10_000),
        "\u{1f600}".repeat(10_000),
        "\u{feff}".into(),
    ];
    for bits in 0u8..32 {
        let flags = KeyboardFlags::from_bits_truncate(bits);
        for level in 0u8..3 {
            let m = snap(flags, level, false, false);
            for payload in &payloads {
                for kind in [
                    KeyEventKind::Press,
                    KeyEventKind::Repeat,
                    KeyEventKind::Release,
                ] {
                    for bitmods in 0u8..8 {
                        let mods = km(bitmods & 1 != 0, bitmods & 2 != 0, bitmods & 4 != 0);
                        let mut event = KeyEvent::new(ch(payload), mods);
                        event.kind = kind;
                        event.text = Some(payload.clone());
                        if let Some(bytes) = encode_key_event(&event, &m)
                            && bytes.starts_with(b"\x1b[")
                        {
                            assert!(
                                bytes.len() < 1024,
                                "unbounded sequence: {} bytes under {flags:?}",
                                bytes.len()
                            );
                        }
                    }
                }
            }
        }
    }
}
