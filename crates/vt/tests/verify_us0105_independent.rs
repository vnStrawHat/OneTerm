//! INDEPENDENT VERIFICATION of US-0105 (IN-0039).
//!
//! Expected bytes are derived from the kitty keyboard protocol specification
//! text at <https://sw.kovidgoyal.net/kitty/keyboard-protocol/> (fetched
//! 2026-09-16), NOT from the implementation. Each row cites its section.
//!
//! Rows that mismatch are collected and printed as a scoreboard so one run
//! reports every deviation instead of stopping at the first.

use oneterm_vt::input::{
    KeyEvent, KeyEventKind, KeyMods, KeySpec, NamedKey, encode_key, encode_key_event,
};
use oneterm_vt::{Config, EventBatch, KeyboardFlags, ModeSnapshot, Size, Terminal};
use std::time::Instant;

const DISAMBIGUATE: KeyboardFlags = KeyboardFlags::DISAMBIGUATE_ESC_CODES;
const EVENT_TYPES: KeyboardFlags = KeyboardFlags::REPORT_EVENT_TYPES;
const ALTERNATE: KeyboardFlags = KeyboardFlags::REPORT_ALTERNATE_KEYS;
const ALL_ESC: KeyboardFlags = KeyboardFlags::REPORT_ALL_KEYS_AS_ESC;
const TEXT: KeyboardFlags = KeyboardFlags::REPORT_ASSOCIATED_TEXT;

fn modes(flags: KeyboardFlags) -> ModeSnapshot {
    let mut m = ModeSnapshot::default();
    m.keyboard_flags = flags;
    m
}

fn mok(level: u8) -> ModeSnapshot {
    let mut m = ModeSnapshot::default();
    m.modify_other_keys = level;
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

/// Collects mismatches instead of panicking on the first one.
#[derive(Default)]
struct Board {
    total: usize,
    bad: Vec<String>,
    frozen: Vec<String>,
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

    /// A row where this engine deliberately answers something other than the
    /// specification's table, and the difference is frozen by `US-0099`'s
    /// equivalence bar (the legacy rung must return what it returned before
    /// `US-0105`, byte for byte).
    ///
    /// The engine's answer is what is asserted; the specification's is printed,
    /// so the list is visible on every run and cannot grow without someone
    /// writing the `why` down. Nothing is deleted.
    fn check_frozen(
        &mut self,
        source: &str,
        got: Option<Vec<u8>>,
        spec: Option<&[u8]>,
        ours: Option<&[u8]>,
        why: &str,
    ) {
        self.frozen.push(format!(
            "  {source}\n      spec  {}\n      ours  {}   ({why})",
            show(spec),
            show(ours)
        ));
        self.check(source, got, ours);
    }

    fn report(self, name: &str) {
        println!(
            "[{name}] {} cases, {} mismatches, {} frozen deviations",
            self.total,
            self.bad.len(),
            self.frozen.len()
        );
        if !self.frozen.is_empty() {
            println!("[{name}] frozen deviations:\n{}", self.frozen.join("\n"));
        }
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

/// Why the legacy rung cannot be corrected inside `US-0105`.
const FROZEN: &str =
    "pre-existing xterm legacy behaviour, frozen by US-0099's equivalence bar; guide chapter 6";

// ---------------------------------------------------------------------------
// 1. The specification's "Functional key definitions" table, under the flag
//    that makes every key an escape code (REPORT_ALL_KEYS_AS_ESC, which the
//    specification says "implies all keys are automatically disambiguated").
// ---------------------------------------------------------------------------

#[test]
fn spec_functional_key_table_under_report_all_keys() {
    let m = modes(ALL_ESC);
    let mut board = Board::default();
    // (key, expected bytes) straight from "Functional key codes". The note
    // "escape codes of the form CSI 1 letter will omit the 1 if there are no
    // modifiers" is applied.
    let rows: &[(NamedKey, &[u8])] = &[
        (NamedKey::Escape, b"\x1b[27u"),
        (NamedKey::Enter, b"\x1b[13u"),
        (NamedKey::Tab, b"\x1b[9u"),
        (NamedKey::Backspace, b"\x1b[127u"),
        (NamedKey::Insert, b"\x1b[2~"),
        (NamedKey::Delete, b"\x1b[3~"),
        (NamedKey::ArrowLeft, b"\x1b[D"),
        (NamedKey::ArrowRight, b"\x1b[C"),
        (NamedKey::ArrowUp, b"\x1b[A"),
        (NamedKey::ArrowDown, b"\x1b[B"),
        (NamedKey::PageUp, b"\x1b[5~"),
        (NamedKey::PageDown, b"\x1b[6~"),
        (NamedKey::Home, b"\x1b[H"),
        (NamedKey::End, b"\x1b[F"),
        (NamedKey::F1, b"\x1b[P"),
        (NamedKey::F2, b"\x1b[Q"),
        // "F3 | 13 ~" -- and the note: "The original version of this
        // specification allowed F3 to be encoded as both CSI R and CSI ~.
        // However, CSI R conflicts with the Cursor Position Report, so it was
        // removed."
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
    for (key, want) in rows {
        let event = KeyEvent::new(named(*key), KeyMods::default());
        board.check(
            &format!("Functional key definitions: {key:?}"),
            encode_key_event(&event, &m),
            Some(*want),
        );
    }
    board.report("functional-key-table");
}

// ---------------------------------------------------------------------------
// 2. Modifier arithmetic: 1 + shift(1) + alt(2) + ctrl(4).
// ---------------------------------------------------------------------------

#[test]
fn spec_modifier_arithmetic() {
    let m = modes(DISAMBIGUATE);
    let mut board = Board::default();
    for shift in [false, true] {
        for ctrl in [false, true] {
            for alt in [false, true] {
                let value = 1 + shift as u8 + 2 * alt as u8 + 4 * ctrl as u8;
                // ArrowUp is "1 A"; the 1 and the modifier field are omitted
                // only when the modifier value is the default 1.
                let want = if value == 1 {
                    b"\x1b[A".to_vec()
                } else {
                    format!("\x1b[1;{value}A").into_bytes()
                };
                let event = KeyEvent::new(named(NamedKey::ArrowUp), km(shift, ctrl, alt));
                board.check(
                    &format!("Modifiers: 1+bits for shift={shift} ctrl={ctrl} alt={alt}"),
                    encode_key_event(&event, &m),
                    Some(&want),
                );
            }
        }
    }
    board.report("modifier-arithmetic");
}

// ---------------------------------------------------------------------------
// 3. Event types.
// ---------------------------------------------------------------------------

#[test]
fn spec_event_types_on_a_non_text_key() {
    let mut board = Board::default();
    let m = modes(DISAMBIGUATE | EVENT_TYPES);
    let base = KeyEvent::new(named(NamedKey::Escape), KeyMods::default());
    // "The press event type has value 1 and is the default if no event type
    // sub field is present."
    board.check(
        "Event types: press omits the sub-field",
        encode_key_event(&base, &m),
        Some(b"\x1b[27u"),
    );
    // "If no modifiers are present, the modifiers field must have the value 1
    // and the event type sub-field the type of event."
    let mut repeat = base.clone();
    repeat.kind = KeyEventKind::Repeat;
    board.check(
        "Event types: repeat is :2",
        encode_key_event(&repeat, &m),
        Some(b"\x1b[27;1:2u"),
    );
    let mut release = base.clone();
    release.kind = KeyEventKind::Release;
    board.check(
        "Event types: release is :3",
        encode_key_event(&release, &m),
        Some(b"\x1b[27;1:3u"),
    );
    // Without REPORT_EVENT_TYPES a release sends nothing at all.
    board.check(
        "Event types: no release without the flag",
        encode_key_event(&release, &modes(DISAMBIGUATE)),
        None,
    );
    board.report("event-types");
}

/// "Note: Key events that result in text are reported as plain UTF-8 text, so
/// events are not supported for them, **unless the application requests key
/// report mode**" (= REPORT_ALL_KEYS_AS_ESC).
///
/// So with REPORT_EVENT_TYPES but no REPORT_ALL_KEYS_AS_ESC, a repeat of a
/// plain text key must still be the text, and a release must send nothing.
#[test]
fn spec_text_keys_have_no_event_types_without_report_all_keys() {
    let mut board = Board::default();
    for flags in [
        EVENT_TYPES,
        DISAMBIGUATE | EVENT_TYPES,
        EVENT_TYPES | ALTERNATE,
    ] {
        let m = modes(flags);
        let press = KeyEvent::new(ch("a"), KeyMods::default());
        let mut repeat = press.clone();
        repeat.kind = KeyEventKind::Repeat;
        let mut release = press.clone();
        release.kind = KeyEventKind::Release;
        board.check(
            &format!("Event types note: repeat of a text key under {flags:?} is the text"),
            encode_key_event(&repeat, &m),
            Some(b"a"),
        );
        board.check(
            &format!("Event types note: release of a text key under {flags:?} is silent"),
            encode_key_event(&release, &m),
            None,
        );
    }
    board.report("text-key-event-types");
}

// ---------------------------------------------------------------------------
// 4. "Text as code points" and "Key codes" worked examples.
// ---------------------------------------------------------------------------

#[test]
fn spec_associated_text_and_alternate_keys() {
    let mut board = Board::default();

    // "shift+a -> CSI 97 ; 2 ; 65 u"
    let mut event = KeyEvent::new(ch("a"), km(true, false, false));
    event.text = Some("A".into());
    board.check(
        "Text as code points: shift+a -> CSI 97;2;65 u",
        encode_key_event(&event, &modes(ALL_ESC | TEXT)),
        Some(b"\x1b[97;2;65u"),
    );

    // "If multiple code points are present, they must be separated by colons."
    let mut multi = KeyEvent::new(ch("a"), KeyMods::default());
    multi.text = Some("ab".into());
    board.check(
        "Text as code points: colon-separated",
        encode_key_event(&multi, &modes(ALL_ESC | TEXT)),
        Some(b"\x1b[97;;97:98u"),
    );

    // "The associated text must not contain control codes."
    let mut control = KeyEvent::new(ch("a"), KeyMods::default());
    control.text = Some("a\u{1}".into());
    board.check(
        "Text as code points: a control code drops the field",
        encode_key_event(&control, &modes(ALL_ESC | TEXT)),
        Some(b"\x1b[97u"),
    );

    // A chord that produces no text must carry no text field: ctrl+a produces
    // 0x01, a control code the specification forbids in the field.
    let ctrl_a = KeyEvent::new(ch("a"), km(false, true, false));
    board.check(
        "Text as code points: ctrl+a produces no reportable text",
        encode_key_event(&ctrl_a, &modes(ALL_ESC | TEXT)),
        Some(b"\x1b[97;5u"),
    );
    // alt+a: the alt modifier is not text either.
    let alt_a = KeyEvent::new(ch("a"), km(false, false, true));
    board.check(
        "Text as code points: alt+a carries no bare 'a' text field",
        encode_key_event(&alt_a, &modes(ALL_ESC | TEXT)),
        Some(b"\x1b[97;3u"),
    );

    // "REPORT_ASSOCIATED_TEXT ... is undefined if used without
    // REPORT_ALL_KEYS_AS_ESC" -- this engine documents it as inert.
    let mut shift_a = KeyEvent::new(ch("A"), km(true, false, false));
    shift_a.text = Some("A".into());
    board.check(
        "Report associated text is inert without report-all-keys",
        encode_key_event(&shift_a, &modes(DISAMBIGUATE | TEXT)),
        Some(b"A"),
    );

    // "If only one alternate key is present, it is the shifted key."
    let mut shifted = KeyEvent::new(ch("a"), km(true, false, false));
    shifted.shifted = Some('A');
    board.check(
        "Key codes: one alternate is the shifted key",
        encode_key_event(&shifted, &modes(ALL_ESC | ALTERNATE)),
        Some(b"\x1b[97:65;2u"),
    );
    // "the shifted key must be present only if shift is also present"
    let mut no_shift = KeyEvent::new(ch("a"), km(false, true, false));
    no_shift.shifted = Some('A');
    board.check(
        "Key codes: no shifted key without the shift modifier",
        encode_key_event(&no_shift, &modes(ALL_ESC | ALTERNATE)),
        Some(b"\x1b[97;5u"),
    );
    // "if the terminal wants to send only a base layout key but no shifted
    // key, it must use an empty sub-field for the shifted key"
    let mut base = KeyEvent::new(ch("\u{441}"), km(false, true, false));
    base.base_layout = Some('c');
    board.check(
        "Key codes: empty sub-field for a missing shifted key",
        encode_key_event(&base, &modes(ALL_ESC | ALTERNATE)),
        Some("\x1b[1089::99;5u".as_bytes()),
    );
    // "only if a key event was already going to be represented as an escape
    // code due to one of the other enhancements will this enhancement affect
    // it" -- alternate keys alone change nothing.
    board.check(
        "Report alternate keys is inert on its own",
        encode_key_event(&shifted, &modes(ALTERNATE)),
        Some(b"a"),
    );
    board.report("alternate-and-text");
}

// ---------------------------------------------------------------------------
// 5. Key codes: the un-shifted code point.
// ---------------------------------------------------------------------------

#[test]
fn spec_key_code_is_always_the_unshifted_key() {
    let mut board = Board::default();
    let m = modes(DISAMBIGUATE);
    // "If the user presses, for example, ctrl+shift+a the escape code would be
    // CSI 97;modifiers u. It must not be CSI 65; modifiers u."
    board.check(
        "Key codes: ctrl+shift+a is 97",
        encode_key_event(&KeyEvent::new(ch("A"), km(true, true, false)), &m),
        Some(b"\x1b[97;6u"),
    );
    // "the codepoint used is always the lower-case (or more technically,
    // un-shifted) version of the key" -- the un-shifted key under `!` is `1`
    // and under `$` is `4` ("output the shifted key, for example, A for a and
    // $ for 4" gives the shift relation).
    for (shifted_payload, unshifted_code, label) in [
        ("!", 49u32, "shift+1"),
        ("$", 52, "shift+4"),
        ("#", 51, "shift+3"),
        (":", 59, "shift+;"),
        ("+", 61, "shift+="),
    ] {
        let want = format!("\x1b[{unshifted_code};6u");
        board.check(
            &format!("Key codes: ctrl+{label} reports the un-shifted key"),
            encode_key_event(
                &KeyEvent::new(ch(shifted_payload), km(true, true, false)),
                &m,
            ),
            Some(want.as_bytes()),
        );
    }
    board.report("unshifted-key-code");
}

// ---------------------------------------------------------------------------
// 6. Disambiguate: exactly which keys it moves to CSI u.
// ---------------------------------------------------------------------------

#[test]
fn spec_disambiguate_scope() {
    let mut board = Board::default();
    let m = modes(DISAMBIGUATE);
    // "Turning on this flag will cause the terminal to report the Esc,
    // alt+key, ctrl+key, ctrl+alt+key, shift+alt+key keys using CSI u"
    let rows: &[(&str, KeySpec, KeyMods, &[u8])] = &[
        (
            "Esc",
            named(NamedKey::Escape),
            km(false, false, false),
            b"\x1b[27u",
        ),
        ("alt+a", ch("a"), km(false, false, true), b"\x1b[97;3u"),
        ("ctrl+a", ch("a"), km(false, true, false), b"\x1b[97;5u"),
        ("ctrl+alt+a", ch("a"), km(false, true, true), b"\x1b[97;7u"),
        ("shift+alt+a", ch("A"), km(true, false, true), b"\x1b[97;4u"),
        // Plain and shift-only text keys still send text: disambiguate covers
        // only "key events that do not generate text".
        ("a", ch("a"), km(false, false, false), b"a"),
        ("shift+a", ch("A"), km(true, false, false), b"A"),
    ];
    for (label, key, mods, want) in rows {
        board.check(
            &format!("Disambiguate escape codes: {label}"),
            encode_key_event(&KeyEvent::new(key.clone(), *mods), &m),
            Some(*want),
        );
    }
    // "The only exceptions are the Enter, Tab and Backspace keys which still
    // generate the same bytes as in legacy mode."
    for (key, want) in [
        (NamedKey::Enter, b"\r".as_slice()),
        (NamedKey::Tab, b"\t"),
        (NamedKey::Backspace, b"\x7f"),
    ] {
        board.check(
            &format!("Disambiguate exception: {key:?}"),
            encode_key_event(&KeyEvent::new(named(key), KeyMods::default()), &m),
            Some(want),
        );
    }
    board.report("disambiguate-scope");
}

// ---------------------------------------------------------------------------
// 7. Legacy mode: the specification's own legacy tables, flags = 0.
// ---------------------------------------------------------------------------

/// "Example encodings" table in "Legacy text keys".
#[test]
fn spec_legacy_text_key_example_table() {
    let mut board = Board::default();
    let m = modes(KeyboardFlags::empty());
    // (label, payload the platform produces, mods, expected)
    let rows: &[(&str, &str, KeyMods, &[u8])] = &[
        ("i plain", "i", km(false, false, false), b"i"),
        ("i shift", "I", km(true, false, false), b"I"),
        ("i alt", "i", km(false, false, true), b"\x1bi"),
        ("i ctrl", "i", km(false, true, false), b"\x09"),
        ("i shift+alt", "I", km(true, false, true), b"\x1bI"),
        ("i alt+ctrl", "i", km(false, true, true), b"\x1b\x09"),
        // The three `ctrl+shift` rows are the specification's "Any other
        // combination of modifiers with these keys is output as the appropriate
        // CSI u escape code", which this engine's legacy rung does not do -- it
        // is xterm's rung, and `US-0105` freezes it. Recorded, not corrected.
        ("i ctrl+shift", "I", km(true, true, false), b"\x1b[105;6u"),
        ("3 plain", "3", km(false, false, false), b"3"),
        ("3 shift", "#", km(true, false, false), b"#"),
        ("3 alt", "3", km(false, false, true), b"\x1b3"),
        ("3 ctrl", "3", km(false, true, false), b"\x1b"),
        ("3 shift+alt", "#", km(true, false, true), b"\x1b#"),
        ("3 alt+ctrl", "3", km(false, true, true), b"\x1b\x1b"),
        ("3 ctrl+shift", "#", km(true, true, false), b"\x1b[51;6u"),
        ("; plain", ";", km(false, false, false), b";"),
        ("; shift", ":", km(true, false, false), b":"),
        ("; alt", ";", km(false, false, true), b"\x1b;"),
        ("; ctrl", ";", km(false, true, false), b";"),
        ("; shift+alt", ":", km(true, false, true), b"\x1b:"),
        ("; alt+ctrl", ";", km(false, true, true), b"\x1b;"),
        ("; ctrl+shift", ":", km(true, true, false), b"\x1b[59;6u"),
    ];
    // What this engine's frozen legacy rung answers for the three `ctrl+shift`
    // rows: xterm applies the ctrl table and ignores shift.
    let frozen: &[(&str, &[u8])] = &[
        ("i ctrl+shift", b"\x09"),
        ("3 ctrl+shift", b"#"),
        ("; ctrl+shift", b":"),
    ];
    for (label, payload, mods, want) in rows {
        let source = format!("Legacy text keys / Example encodings: {label}");
        let got = encode_key(&ch(payload), *mods, &m);
        match frozen.iter().find(|(name, _)| name == label) {
            None => board.check(&source, got, Some(*want)),
            Some((_, ours)) => board.check_frozen(&source, got, Some(*want), Some(*ours), FROZEN),
        }
    }
    board.report("legacy-text-key-examples");
}

/// The "C0 controls" table in "Legacy functional keys".
#[test]
fn spec_legacy_c0_control_table() {
    let mut board = Board::default();
    let m = modes(KeyboardFlags::empty());
    let none = km(false, false, false);
    let ctrl = km(false, true, false);
    let alt = km(false, false, true);
    let shift = km(true, false, false);
    let ctrl_shift = km(true, true, false);
    let alt_shift = km(true, false, true);
    let ctrl_alt = km(false, true, true);
    // The fifth column is this engine's own answer where it differs. It is
    // xterm's table rather than kitty's -- `Enter` with a modifier takes the
    // `CSI 13 ; mod u` form xterm sends, and `alt` is dropped on the keys
    // `US-0099` measured -- and `US-0105`'s acceptance bar freezes the legacy
    // rung byte for byte, so these are recorded and not corrected here.
    #[allow(clippy::type_complexity)]
    let rows: &[(&str, KeySpec, KeyMods, &[u8], Option<&[u8]>)] = &[
        ("Enter none", named(NamedKey::Enter), none, b"\x0d", None),
        (
            "Enter ctrl",
            named(NamedKey::Enter),
            ctrl,
            b"\x0d",
            Some(b"\x1b[13;5u"),
        ),
        (
            "Enter alt",
            named(NamedKey::Enter),
            alt,
            b"\x1b\x0d",
            Some(b"\x0d"),
        ),
        (
            "Enter shift",
            named(NamedKey::Enter),
            shift,
            b"\x0d",
            Some(b"\x1b[13;2u"),
        ),
        (
            "Enter ctrl+shift",
            named(NamedKey::Enter),
            ctrl_shift,
            b"\x0d",
            Some(b"\x1b[13;6u"),
        ),
        (
            "Enter alt+shift",
            named(NamedKey::Enter),
            alt_shift,
            b"\x1b\x0d",
            Some(b"\x1b[13;4u"),
        ),
        (
            "Enter ctrl+alt",
            named(NamedKey::Enter),
            ctrl_alt,
            b"\x1b\x0d",
            Some(b"\x1b[13;7u"),
        ),
        ("Escape none", named(NamedKey::Escape), none, b"\x1b", None),
        ("Escape ctrl", named(NamedKey::Escape), ctrl, b"\x1b", None),
        (
            "Escape alt",
            named(NamedKey::Escape),
            alt,
            b"\x1b\x1b",
            Some(b"\x1b"),
        ),
        (
            "Escape shift",
            named(NamedKey::Escape),
            shift,
            b"\x1b",
            None,
        ),
        (
            "Escape ctrl+shift",
            named(NamedKey::Escape),
            ctrl_shift,
            b"\x1b",
            None,
        ),
        (
            "Escape alt+shift",
            named(NamedKey::Escape),
            alt_shift,
            b"\x1b\x1b",
            Some(b"\x1b"),
        ),
        (
            "Escape ctrl+alt",
            named(NamedKey::Escape),
            ctrl_alt,
            b"\x1b\x1b",
            Some(b"\x1b"),
        ),
        (
            "Backspace none",
            named(NamedKey::Backspace),
            none,
            b"\x7f",
            None,
        ),
        (
            "Backspace ctrl",
            named(NamedKey::Backspace),
            ctrl,
            b"\x08",
            None,
        ),
        (
            "Backspace alt",
            named(NamedKey::Backspace),
            alt,
            b"\x1b\x7f",
            None,
        ),
        (
            "Backspace shift",
            named(NamedKey::Backspace),
            shift,
            b"\x7f",
            None,
        ),
        (
            "Backspace ctrl+shift",
            named(NamedKey::Backspace),
            ctrl_shift,
            b"\x08",
            None,
        ),
        (
            "Backspace alt+shift",
            named(NamedKey::Backspace),
            alt_shift,
            b"\x1b\x7f",
            None,
        ),
        (
            "Backspace ctrl+alt",
            named(NamedKey::Backspace),
            ctrl_alt,
            b"\x1b\x08",
            Some(b"\x08"),
        ),
        ("Tab none", named(NamedKey::Tab), none, b"\x09", None),
        ("Tab ctrl", named(NamedKey::Tab), ctrl, b"\x09", None),
        (
            "Tab alt",
            named(NamedKey::Tab),
            alt,
            b"\x1b\x09",
            Some(b"\x09"),
        ),
        ("Tab shift", named(NamedKey::Tab), shift, b"\x1b[Z", None),
        (
            "Tab ctrl+shift",
            named(NamedKey::Tab),
            ctrl_shift,
            b"\x1b[Z",
            None,
        ),
        (
            "Tab alt+shift",
            named(NamedKey::Tab),
            alt_shift,
            b"\x1b\x1b[Z",
            Some(b"\x1b[Z"),
        ),
        (
            "Tab ctrl+alt",
            named(NamedKey::Tab),
            ctrl_alt,
            b"\x1b\x09",
            Some(b"\x09"),
        ),
        ("Space none", ch(" "), none, b"\x20", None),
        ("Space ctrl", ch(" "), ctrl, b"\x00", None),
        ("Space alt", ch(" "), alt, b"\x1b\x20", None),
        ("Space shift", ch(" "), shift, b"\x20", None),
        ("Space ctrl+shift", ch(" "), ctrl_shift, b"\x00", None),
        ("Space alt+shift", ch(" "), alt_shift, b"\x1b\x20", None),
        ("Space ctrl+alt", ch(" "), ctrl_alt, b"\x1b\x00", None),
    ];
    for (label, key, mods, want, ours) in rows {
        let source = format!("Legacy functional keys / C0 controls: {label}");
        let got = encode_key(key, *mods, &m);
        match ours {
            None => board.check(&source, got, Some(*want)),
            Some(ours) => board.check_frozen(&source, got, Some(*want), Some(*ours), FROZEN),
        }
    }
    board.report("legacy-c0-controls");
}

/// "Legacy ctrl mapping of ASCII keys": the whole table.
#[test]
fn spec_legacy_ctrl_mapping_table() {
    let mut board = Board::default();
    let m = modes(KeyboardFlags::empty());
    let rows: &[(&str, u8)] = &[
        (" ", 0),
        ("/", 31),
        ("0", 48),
        ("1", 49),
        ("2", 0),
        ("3", 27),
        ("4", 28),
        ("5", 29),
        ("6", 30),
        ("7", 31),
        ("8", 127),
        ("9", 57),
        ("?", 127),
        ("@", 0),
        ("[", 27),
        ("\\", 28),
        ("]", 29),
        ("^", 30),
        ("_", 31),
        ("~", 30),
    ];
    for (key, byte) in rows {
        board.check(
            &format!("Legacy ctrl mapping: ctrl+{key}"),
            encode_key(&ch(key), km(false, true, false), &m),
            Some(&[*byte]),
        );
    }
    for letter in 'a'..='z' {
        let want = (letter as u8) & 0x1f;
        board.check(
            &format!("Legacy ctrl mapping: ctrl+{letter}"),
            encode_key(&ch(&letter.to_string()), km(false, true, false), &m),
            Some(&[want]),
        );
    }
    board.report("legacy-ctrl-mapping");
}

/// "Legacy functional encoding": terminfo forms, and the cursor-key-mode note.
#[test]
fn spec_legacy_functional_table() {
    let mut board = Board::default();
    let plain = modes(KeyboardFlags::empty());
    let mut app = ModeSnapshot::default();
    app.app_cursor = true;
    let rows: &[(&str, NamedKey, &[u8], Option<&[u8]>)] = &[
        ("INSERT", NamedKey::Insert, b"\x1b[2~", None),
        ("DELETE", NamedKey::Delete, b"\x1b[3~", None),
        ("PAGE_UP", NamedKey::PageUp, b"\x1b[5~", None),
        ("PAGE_DOWN", NamedKey::PageDown, b"\x1b[6~", None),
        ("UP", NamedKey::ArrowUp, b"\x1b[A", Some(b"\x1bOA")),
        ("DOWN", NamedKey::ArrowDown, b"\x1b[B", Some(b"\x1bOB")),
        ("RIGHT", NamedKey::ArrowRight, b"\x1b[C", Some(b"\x1bOC")),
        ("LEFT", NamedKey::ArrowLeft, b"\x1b[D", Some(b"\x1bOD")),
        ("HOME", NamedKey::Home, b"\x1b[H", Some(b"\x1bOH")),
        ("END", NamedKey::End, b"\x1b[F", Some(b"\x1bOF")),
        ("F1", NamedKey::F1, b"\x1bOP", None),
        ("F2", NamedKey::F2, b"\x1bOQ", None),
        ("F4", NamedKey::F4, b"\x1bOS", None),
        ("F5", NamedKey::F5, b"\x1b[15~", None),
        ("F6", NamedKey::F6, b"\x1b[17~", None),
        ("F7", NamedKey::F7, b"\x1b[18~", None),
        ("F8", NamedKey::F8, b"\x1b[19~", None),
        ("F9", NamedKey::F9, b"\x1b[20~", None),
        ("F10", NamedKey::F10, b"\x1b[21~", None),
        ("F11", NamedKey::F11, b"\x1b[23~", None),
        ("F12", NamedKey::F12, b"\x1b[24~", None),
    ];
    for (label, key, want, app_form) in rows {
        board.check(
            &format!("Legacy functional encoding: {label}"),
            encode_key(&named(*key), KeyMods::default(), &plain),
            Some(*want),
        );
        if let Some(app_want) = app_form {
            board.check(
                &format!("Legacy functional encoding: {label} in cursor key mode"),
                encode_key(&named(*key), KeyMods::default(), &app),
                Some(*app_want),
            );
        }
    }
    board.report("legacy-functional-table");
}

// ---------------------------------------------------------------------------
// 8. DECCKM under kitty flags, and the flag stack driving the encoder.
// ---------------------------------------------------------------------------

#[test]
fn kitty_form_ignores_cursor_key_mode() {
    let mut m = ModeSnapshot::default();
    m.app_cursor = true;
    m.keyboard_flags = DISAMBIGUATE;
    // The disambiguate section allows only "CSI number; modifier u" and
    // "CSI 1; modifier [~ABCDEFHPQS]"; SS3 is not among them.
    assert_eq!(
        encode_key_event(
            &KeyEvent::new(named(NamedKey::ArrowUp), KeyMods::default()),
            &m
        )
        .as_deref(),
        Some(b"\x1b[A".as_slice())
    );
}

#[test]
fn the_live_flags_drive_the_encoder_and_the_query_agrees() {
    let mut term = Terminal::new(Size { rows: 24, cols: 80 }, Config::default());
    let mut batch = EventBatch::new();
    let escape = KeyEvent::new(named(NamedKey::Escape), KeyMods::default());
    let now = Instant::now();

    // Push: the query and the encoder must agree.
    term.feed(b"\x1b[>1u", &mut batch, now);
    assert_eq!(
        term.encode_key_event(&escape).as_deref(),
        Some(b"\x1b[27u".as_slice())
    );

    // Pop takes it back.
    term.feed(b"\x1b[<u", &mut batch, now);
    assert_eq!(
        term.encode_key_event(&escape).as_deref(),
        Some(b"\x1b".as_slice())
    );

    // `CSI = Ps ; 1 u` sets the flags too. The specification's "Detection of
    // support" section tells an application to "first set the desired
    // progressive enhancements and then query for the current progressive
    // enhancement": the reply must be the flags that are live.
    let mut queried = Vec::new();
    term.feed(b"\x1b[=1;1u", &mut batch, now);
    assert_eq!(
        term.encode_key_event(&escape).as_deref(),
        Some(b"\x1b[27u".as_slice()),
        "CSI = 1 ; 1 u must reach the encoder"
    );
    batch.clear();
    term.feed(b"\x1b[?u", &mut batch, now);
    for event in batch.iter() {
        if let oneterm_vt::VtEvent::Reply(span) = event {
            queried.extend_from_slice(batch.bytes(*span));
        }
    }
    assert_eq!(
        String::from_utf8_lossy(&queried),
        "\x1b[?1u",
        "CSI ? u must report the flags the encoder is using"
    );
}

/// The stacks are per screen, so entering the alternate screen must not carry
/// the main screen's flags with it.
#[test]
fn the_flag_stack_is_per_screen() {
    let mut term = Terminal::new(Size { rows: 24, cols: 80 }, Config::default());
    let mut batch = EventBatch::new();
    let escape = KeyEvent::new(named(NamedKey::Escape), KeyMods::default());
    let now = Instant::now();
    term.feed(b"\x1b[>1u", &mut batch, now);
    term.feed(b"\x1b[?1049h", &mut batch, now);
    assert_eq!(
        term.encode_key_event(&escape).as_deref(),
        Some(b"\x1b".as_slice()),
        "the alternate screen starts with its own empty stack"
    );
    term.feed(b"\x1b[>9u", &mut batch, now);
    assert_eq!(
        term.encode_key_event(&escape).as_deref(),
        Some(b"\x1b[27u".as_slice())
    );
    term.feed(b"\x1b[?1049l", &mut batch, now);
    assert_eq!(
        term.encode_key_event(&escape).as_deref(),
        Some(b"\x1b[27u".as_slice()),
        "the main screen keeps the flag it pushed"
    );
}

// ---------------------------------------------------------------------------
// 9. modifyOtherKeys interplay.
// ---------------------------------------------------------------------------

#[test]
fn modify_other_keys_levels_and_precedence() {
    let mut board = Board::default();
    // Level 1: only chords with no unambiguous legacy encoding.
    board.check(
        "modifyOtherKeys 1: ctrl+a stays the C0 byte",
        encode_key_event(&KeyEvent::new(ch("a"), km(false, true, false)), &mok(1)),
        Some(b"\x01"),
    );
    // Expectation corrected against xterm(1)'s own resource text, which the
    // verification could not reach and which names this exact chord: level 1
    // "enables this feature for keys **except** for those with well-known
    // behavior, e.g., Tab, Backarrow and some special control character cases
    // which are built into the X11 library, e.g., **Control-Space to make a
    // NUL**, or Control-3 to make an Escape character". `ctrl+2` is
    // `Control-Space`'s alias in that library and makes the same NUL, so it is
    // one of the exceptions and stays legacy at level 1. Level 2 "enables this
    // feature for keys including the exceptions listed", and is checked below.
    board.check(
        "modifyOtherKeys 1: ctrl+2 is an X11 control-character exception",
        encode_key_event(&KeyEvent::new(ch("2"), km(false, true, false)), &mok(1)),
        Some(b"\x00"),
    );
    board.check(
        "modifyOtherKeys 1: ctrl+; has no control byte, so it escapes",
        encode_key_event(&KeyEvent::new(ch(";"), km(false, true, false)), &mok(1)),
        Some(b"\x1b[27;5;59~"),
    );
    board.check(
        "modifyOtherKeys 2: ctrl+2 loses the exception",
        encode_key_event(&KeyEvent::new(ch("2"), km(false, true, false)), &mok(2)),
        Some(b"\x1b[27;5;50~"),
    );
    // Shift alone is consumed by the layout at every level: xterm reaches
    // modifyOtherKeys only for a ctrl or alt chord, so typing stays typing.
    board.check(
        "modifyOtherKeys 2: shift+a is still the letter",
        encode_key_event(&KeyEvent::new(ch("A"), km(true, false, false)), &mok(2)),
        Some(b"A"),
    );
    // Level 2: every modified "other" key.
    board.check(
        "modifyOtherKeys 2: ctrl+a",
        encode_key_event(&KeyEvent::new(ch("a"), km(false, true, false)), &mok(2)),
        Some(b"\x1b[27;5;97~"),
    );
    // Level 0 is the legacy table.
    board.check(
        "modifyOtherKeys 0",
        encode_key_event(&KeyEvent::new(ch("a"), km(false, true, false)), &mok(0)),
        Some(b"\x01"),
    );
    // A functional key is never an "other" key.
    board.check(
        "modifyOtherKeys 2 leaves functional keys alone",
        encode_key_event(
            &KeyEvent::new(named(NamedKey::ArrowUp), km(false, true, false)),
            &mok(2),
        ),
        Some(b"\x1b[1;5A"),
    );
    // The kitty flags supersede modifyOtherKeys.
    let mut both = modes(DISAMBIGUATE);
    both.modify_other_keys = 2;
    board.check(
        "kitty supersedes modifyOtherKeys",
        encode_key_event(&KeyEvent::new(ch("a"), km(false, true, false)), &both),
        Some(b"\x1b[97;5u"),
    );
    board.report("modify-other-keys");
}

// ---------------------------------------------------------------------------
// 10. Hostile input.
// ---------------------------------------------------------------------------

#[test]
fn hostile_text_payloads_are_bounded_and_do_not_panic() {
    let payloads = [
        String::new(),
        "\u{0}".into(),
        "\u{1f600}".into(),
        "\u{10ffff}".into(),
        "e\u{301}".into(),
        "a".repeat(10_000),
        "\u{1f600}".repeat(10_000),
    ];
    for bits in 0u8..32 {
        let flags = KeyboardFlags::from_bits_truncate(bits);
        let m = modes(flags);
        for payload in &payloads {
            for kind in [
                KeyEventKind::Press,
                KeyEventKind::Repeat,
                KeyEventKind::Release,
            ] {
                for shift in [false, true] {
                    for ctrl in [false, true] {
                        let mut event = KeyEvent::new(ch(payload), km(shift, ctrl, false));
                        event.kind = kind;
                        event.text = Some(payload.clone());
                        let got = encode_key_event(&event, &m);
                        if let Some(bytes) = got {
                            // Nothing the encoder *builds* may be unbounded;
                            // the legacy rung writes the payload through, which
                            // is pre-existing.
                            let built = bytes.first() == Some(&0x1b) && bytes.get(1) == Some(&b'[');
                            assert!(
                                !built || bytes.len() < 1024,
                                "unbounded escape sequence: {} bytes for {flags:?}",
                                bytes.len()
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Every one of the 32 flag combinations, every named key, every modifier set,
/// every event kind: no panic and no malformed escape sequence.
#[test]
fn the_whole_flag_space_is_well_formed() {
    let keys: Vec<KeySpec> = [
        NamedKey::Escape,
        NamedKey::Enter,
        NamedKey::Tab,
        NamedKey::Backspace,
        NamedKey::Delete,
        NamedKey::Insert,
        NamedKey::ArrowUp,
        NamedKey::ArrowDown,
        NamedKey::ArrowLeft,
        NamedKey::ArrowRight,
        NamedKey::Home,
        NamedKey::End,
        NamedKey::PageUp,
        NamedKey::PageDown,
        NamedKey::F1,
        NamedKey::F3,
        NamedKey::F5,
        NamedKey::F13,
        NamedKey::F24,
    ]
    .into_iter()
    .map(named)
    .chain(["a", "A", "1", "!", " ", "\u{e9}"].into_iter().map(ch))
    .collect();

    let mut count = 0usize;
    for bits in 0u8..32 {
        let flags = KeyboardFlags::from_bits_truncate(bits);
        for level in 0u8..3 {
            let mut m = modes(flags);
            m.modify_other_keys = level;
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
                        if bytes.starts_with(b"\x1b[") {
                            let body = &bytes[2..];
                            let (params, final_byte) = body.split_at(body.len() - 1);
                            assert!(
                                params
                                    .iter()
                                    .all(|b| b.is_ascii_digit() || *b == b';' || *b == b':'),
                                "malformed parameters {:?} for {event:?} under {flags:?}",
                                String::from_utf8_lossy(params)
                            );
                            assert!(
                                final_byte[0].is_ascii_alphabetic() || final_byte[0] == b'~',
                                "malformed final byte for {event:?} under {flags:?}"
                            );
                            // The Cursor Position Report is `CSI r ; c R`; a
                            // key event must never collide with it.
                            assert_ne!(
                                final_byte[0],
                                b'R',
                                "collides with the Cursor Position Report: {:?} for {event:?} under {flags:?}",
                                String::from_utf8_lossy(&bytes)
                            );
                        }
                    }
                }
            }
        }
    }
    println!("[flag-space] {count} cases");
}
