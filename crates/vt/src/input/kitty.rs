//! The kitty keyboard protocol encoder, and xterm's `modifyOtherKeys`.
//!
//! The specification is <https://sw.kovidgoyal.net/kitty/keyboard-protocol/>.
//! Every table below is from it, and the two places this encoder deliberately
//! stops short of it are named in the embedder's guide rather than hidden here.
//!
//! The entry point is [`encode`], which answers one of three things: these
//! bytes, no bytes at all, or "not this protocol's business" — the last of
//! which sends the caller to the legacy table in [`super::key`].

use crate::snapshot::ModeSnapshot;
use crate::terminal::KeyboardFlags;

use super::key::{KeyEvent, KeyEventKind, KeySpec, NamedKey, modifier_byte};

/// The longest text field the encoder emits, in scalars. The same ceiling the
/// print path puts on a client-supplied grapheme cluster
/// (`terminal::dispatch::CLUSTER_CARRY_MAX`): past it the text field is dropped
/// and the key code is still sent, so a hostile payload costs a bounded
/// allocation and never a panic.
const TEXT_SCALAR_MAX: usize = 32;

/// What one key event encodes to, before the legacy table is consulted.
pub(crate) enum Encoded {
    /// The event sends nothing at all — a release nobody asked to hear about.
    Silent,
    /// These bytes, from one of the two enhanced protocols.
    Bytes(Vec<u8>),
    /// Neither protocol applies; the legacy encoding answers.
    Legacy,
}

/// Which shape a key keeps once it is reported as an escape code.
///
/// The specification gives functional keys that already have a legacy escape
/// code *that* code with the modifier and event-type fields attached, rather
/// than a private-use code point, so a program that understands only the legacy
/// forms still recognises them.
enum Form {
    /// `CSI <code> u` — the code point form, the only one that carries
    /// alternate keys and associated text.
    Codepoint(u32),
    /// `CSI <number> ; <modifiers> ~`.
    Tilde(u16),
    /// `CSI 1 ; <modifiers> <letter>`, which loses both the `1` and the
    /// modifier field when they are at their defaults.
    Letter(u8),
}

/// The shape for a key.
fn form(key: &KeySpec) -> Option<Form> {
    match key {
        KeySpec::Character(text) => Some(Form::Codepoint(unshifted(text)?)),
        // `KeySpec` and `NamedKey` are `#[non_exhaustive]` to the outside, but
        // exhaustive here on purpose: a new variant must be given a code point
        // rather than silently falling into a wildcard that sends nothing.
        KeySpec::Named(named) => Some(named_form(*named)),
    }
}

/// The shape for a named key, straight from the specification's "Functional key
/// codes" table.
///
/// `NamedKey` names thirty-eight keys and that table has about a hundred: the
/// keypad (`57399`-`57427`), the lock and system keys (`57358`-`57363`), the
/// media keys (`57428`+) and the modifier keys themselves (`57441`+) have no
/// variant here, so no embedder can deliver one and nothing is dropped on the
/// floor.
///
/// `F3` is the row worth knowing about: it has no letter form, because
/// "`CSI R` conflicts with the Cursor Position Report, so it was removed". The
/// `[~ABCDEFHPQS]` set the "Disambiguate escape codes" section permits has no
/// `R` in it either, which is the same rule stated twice.
fn named_form(named: NamedKey) -> Form {
    use NamedKey::*;
    match named {
        Escape => Form::Codepoint(27),
        Enter => Form::Codepoint(13),
        Tab => Form::Codepoint(9),
        Backspace => Form::Codepoint(127),
        ArrowUp => Form::Letter(b'A'),
        ArrowDown => Form::Letter(b'B'),
        ArrowRight => Form::Letter(b'C'),
        ArrowLeft => Form::Letter(b'D'),
        Home => Form::Letter(b'H'),
        End => Form::Letter(b'F'),
        Insert => Form::Tilde(2),
        Delete => Form::Tilde(3),
        PageUp => Form::Tilde(5),
        PageDown => Form::Tilde(6),
        F1 => Form::Letter(b'P'),
        F2 => Form::Letter(b'Q'),
        F3 => Form::Tilde(13),
        F4 => Form::Letter(b'S'),
        F5 => Form::Tilde(15),
        F6 => Form::Tilde(17),
        F7 => Form::Tilde(18),
        F8 => Form::Tilde(19),
        F9 => Form::Tilde(20),
        F10 => Form::Tilde(21),
        F11 => Form::Tilde(23),
        F12 => Form::Tilde(24),
        // The private-use codes the table gives these, rather than xterm's
        // shifted `F1`-`F12` forms the legacy rung still sends. Spelling them
        // as a shifted key would set a modifier bit the user never pressed, so
        // a program matching `shift+F5` would fire on a bare `F17`.
        F13 => Form::Codepoint(57376),
        F14 => Form::Codepoint(57377),
        F15 => Form::Codepoint(57378),
        F16 => Form::Codepoint(57379),
        F17 => Form::Codepoint(57380),
        F18 => Form::Codepoint(57381),
        F19 => Form::Codepoint(57382),
        F20 => Form::Codepoint(57383),
        F21 => Form::Codepoint(57384),
        F22 => Form::Codepoint(57385),
        F23 => Form::Codepoint(57386),
        F24 => Form::Codepoint(57387),
    }
}

/// The two halves of the US keyboard's shift relation, in the same order.
///
/// The specification's legacy section states the relation it wants undone --
/// "output the shifted key, for example, `A` for `a` and `$` for `4`" -- and
/// gives no way to ask the platform. This is the PC-101 answer, which is also
/// the layout the base-layout sub-field is defined against.
const SHIFTED_ASCII: &str = "~!@#$%^&*()_+{}|:\"<>?";
const UNSHIFTED_ASCII: &str = "`1234567890-=[]\\;',./";

/// The un-shifted code point of a text key.
///
/// The specification is explicit: "the codepoint used is always the lower-case
/// (or more technically, un-shifted) version of the key ... If the user
/// presses, for example, `ctrl+shift+a` the escape code would be
/// `CSI 97;modifiers u`. It must not be `CSI 65; modifiers u`."
///
/// The embedder hands over the text the key produced, so this undoes the shift
/// relation itself: lower-case for letters, and the PC-101 table for
/// punctuation, so `ctrl+shift+1` reports `49` rather than `33`. A layout that
/// pairs them differently is the ceiling, and guide chapter 6 names it.
fn unshifted(text: &str) -> Option<u32> {
    let first = text.chars().next()?;
    if let Some(at) = SHIFTED_ASCII.find(first) {
        // Both tables are ASCII, so a byte offset is a character offset.
        return UNSHIFTED_ASCII.as_bytes().get(at).map(|byte| *byte as u32);
    }
    Some(first.to_lowercase().next().unwrap_or(first) as u32)
}

/// Encode one key event against the terminal's live keyboard state.
pub(crate) fn encode(event: &KeyEvent, modes: &ModeSnapshot) -> Encoded {
    let flags = modes.keyboard_flags;
    let all_esc = flags.contains(KeyboardFlags::REPORT_ALL_KEYS_AS_ESC);
    let event_types = flags.contains(KeyboardFlags::REPORT_EVENT_TYPES);
    // `Enter`, `Tab` and `Backspace` keep their legacy bytes under every flag
    // but `REPORT_ALL_KEYS_AS_ESC`, so that a user can still type `reset` after
    // a program crashes with the flags set. The specification calls this out
    // twice, once for the disambiguate flag and once for release events.
    let legacy_c0 = matches!(
        event.key,
        KeySpec::Named(NamedKey::Enter | NamedKey::Tab | NamedKey::Backspace)
    );

    // Rung 1: nobody asked to hear about this release, so there are no bytes.
    //
    // `REPORT_EVENT_TYPES` alone is not enough for a key that produces text.
    // The specification, under "Event types": "Key events that result in text
    // are reported as plain UTF-8 text, so events are not supported for them,
    // unless the application requests key report mode" -- key report mode being
    // `REPORT_ALL_KEYS_AS_ESC`. Same sentence for `Enter`, `Tab` and
    // `Backspace`, which "will not have release events unless Report all keys
    // as escape codes is also set".
    let reportable = event_types && (all_esc || !(legacy_c0 || generates_text(event)));
    if event.kind == KeyEventKind::Release && !reportable {
        return Encoded::Silent;
    }

    // Rung 2: the kitty flags supersede `modifyOtherKeys` whenever they apply.
    if kitty_applies(event, flags, legacy_c0, reportable) {
        return match csi_u(event, flags) {
            Some(bytes) => Encoded::Bytes(bytes),
            // A key with no code point at all (an empty `Character`): the
            // enhanced protocols cannot name it, and neither can the legacy
            // one meaningfully, so it sends nothing.
            None => Encoded::Silent,
        };
    }

    // Rung 3: xterm's older answer to the same problem.
    match modify_other_keys(event, modes.modify_other_keys, modes) {
        Some(bytes) => Encoded::Bytes(bytes),
        None => Encoded::Legacy,
    }
}

/// Whether this event is reported as a kitty escape code.
///
/// `reportable` is rung 1's answer to "may this event carry an event type at
/// all": it is what keeps a held letter key typing that letter under
/// `REPORT_EVENT_TYPES` alone.
fn kitty_applies(
    event: &KeyEvent,
    flags: KeyboardFlags,
    legacy_c0: bool,
    reportable: bool,
) -> bool {
    if flags.is_empty() {
        return false;
    }
    // "all keys are reported as escape codes, including Enter, Tab, Backspace",
    // and that flag "implies all keys are automatically disambiguated as well".
    if flags.contains(KeyboardFlags::REPORT_ALL_KEYS_AS_ESC) {
        return true;
    }
    if legacy_c0 {
        return false;
    }
    // A repeat that may be reported has no legacy spelling, so the only way to
    // say "repeat" at all is the escape code that carries an event type. A
    // repeat that may *not* be reported falls through to the legacy rung, which
    // is the specification's "key repeat events are treated as key press
    // events".
    if event.kind != KeyEventKind::Press && reportable {
        return true;
    }
    // "all key events that do not generate text are represented in one of the
    // following two forms". `REPORT_ALTERNATE_KEYS` and `REPORT_ASSOCIATED_TEXT`
    // are pure enhancements of a form some other flag already chose, so neither
    // reaches this far on its own.
    flags.contains(KeyboardFlags::DISAMBIGUATE_ESC_CODES) && !generates_text(event)
}

/// Whether the event would have produced text in the legacy encoding.
fn generates_text(event: &KeyEvent) -> bool {
    matches!(event.key, KeySpec::Character(_)) && !(event.mods.ctrl || event.mods.alt)
}

/// The `CSI u`, `CSI ~` and `CSI <letter>` forms, with every field that would
/// carry its default value dropped.
fn csi_u(event: &KeyEvent, flags: KeyboardFlags) -> Option<Vec<u8>> {
    let form = form(&event.key)?;
    let mods = event.mods;
    let modifier = modifier_byte(mods);
    let kind = if flags.contains(KeyboardFlags::REPORT_EVENT_TYPES) {
        match event.kind {
            KeyEventKind::Repeat => 2,
            KeyEventKind::Release => 3,
            _ => 1,
        }
    } else {
        // Without event-type reporting a repeat is indistinguishable from a
        // press, which is exactly what the legacy encoding does.
        1
    };

    // The modifiers field, empty when both it and the event type are default.
    let field2 = match (modifier, kind) {
        (1, 1) => String::new(),
        (m, 1) => m.to_string(),
        (m, k) => format!("{m}:{k}"),
    };

    let seq = match form {
        Form::Tilde(number) if field2.is_empty() => format!("\x1b[{number}~"),
        Form::Tilde(number) => format!("\x1b[{number};{field2}~"),
        Form::Letter(final_byte) if field2.is_empty() => {
            format!("\x1b[{}", final_byte as char)
        }
        Form::Letter(final_byte) => format!("\x1b[1;{field2}{}", final_byte as char),
        Form::Codepoint(code) => {
            let mut fields = vec![
                format!("{code}{}", alternates(event, flags, mods.shift)),
                field2,
                text_field(event, flags).unwrap_or_default(),
            ];
            while fields.last().is_some_and(String::is_empty) {
                fields.pop();
            }
            format!("\x1b[{}u", fields.join(";"))
        }
    };
    Some(seq.into_bytes())
}

/// The `:shifted[:base-layout]` sub-fields.
///
/// "The shifted key must be present only if shift is also present in the
/// modifiers", and a base layout key without a shifted key needs the empty
/// sub-field rather than a shorter sequence.
fn alternates(event: &KeyEvent, flags: KeyboardFlags, shift: bool) -> String {
    if !flags.contains(KeyboardFlags::REPORT_ALTERNATE_KEYS) {
        return String::new();
    }
    let shifted = if shift { event.shifted } else { None };
    match (shifted, event.base_layout) {
        (None, None) => String::new(),
        (Some(s), None) => format!(":{}", s as u32),
        (Some(s), Some(base)) => format!(":{}:{}", s as u32, base as u32),
        (None, Some(base)) => format!("::{}", base as u32),
    }
}

/// The trailing `;text-as-codepoints` field, colon-separated.
///
/// The specification calls associated text undefined without
/// `REPORT_ALL_KEYS_AS_ESC`, so this engine treats the flag as inert there
/// rather than guessing. A **release** never carries text either: nothing was
/// inserted by letting a key go.
///
/// The text the embedder supplies wins. The fallback is a `Character` key's own
/// payload, but **only when the modifiers would have let that payload reach the
/// program**: `ctrl+a` produces `0x01`, which the specification forbids in this
/// field ("The associated text must not contain control codes"), and `alt+a`
/// produces no text at all on most platforms. Reporting the bare payload for
/// either would tell the program that `Ctrl+A` inserted an `a`.
///
/// A payload carrying a control code, or longer than [`TEXT_SCALAR_MAX`], drops
/// the field: the specification forbids the first and nothing needs the second.
fn text_field(event: &KeyEvent, flags: KeyboardFlags) -> Option<String> {
    if !flags.contains(KeyboardFlags::REPORT_ASSOCIATED_TEXT)
        || !flags.contains(KeyboardFlags::REPORT_ALL_KEYS_AS_ESC)
    {
        return None;
    }
    // A release inserts nothing, so it has no associated text. A repeat does.
    if event.kind == KeyEventKind::Release {
        return None;
    }
    let text = match (event.text.as_deref(), &event.key) {
        (Some(text), _) => text,
        (None, KeySpec::Character(payload)) if !(event.mods.ctrl || event.mods.alt) => {
            payload.as_str()
        }
        (None, _) => return None,
    };
    if text.is_empty() || text.chars().count() > TEXT_SCALAR_MAX {
        return None;
    }
    let mut out = String::new();
    for scalar in text.chars() {
        // "no C0/C1 control codes allowed": one bad scalar drops the whole
        // field rather than sending something the receiver must reject.
        if scalar.is_control() {
            return None;
        }
        if !out.is_empty() {
            out.push(':');
        }
        out.push_str(&(scalar as u32).to_string());
    }
    Some(out)
}

/// xterm's `CSI 27 ; <modifier> ; <code point> ~`, reached only when no kitty
/// flag applied.
///
/// The level decides which chords it covers. xterm's own resource
/// documentation:
///
/// > **1** -- Enables this feature for keys **except** for those with
/// > well-known behavior, e.g., Tab, Backarrow and some special control
/// > character cases which are built into the X11 library, e.g., Control-Space
/// > to make a NUL, or Control-3 to make an Escape character.
/// >
/// > **2** -- Enables this feature for keys including the exceptions listed.
///
/// So level `1` keeps every chord whose legacy encoding already *is* a control
/// byte -- `Ctrl+A` stays `0x01`, `Ctrl+Space` stays `0x00`, `Ctrl+3` stays
/// `ESC`, `Tab` stays `0x09` -- and escapes the rest, which is what makes
/// `Ctrl+;` reachable without making `Ctrl+I` stop being `Tab`. Level `2`
/// escapes them all, which is the setting that separates `Ctrl+I` from `Tab`.
///
/// Neither level touches a key that already has an unambiguous functional
/// encoding (an arrow, `F5`, `Home`), and neither fires on shift alone: the
/// layout has already consumed shift to produce the character, so `Shift+A`
/// is the letter `A` at every level.
fn modify_other_keys(event: &KeyEvent, level: u8, modes: &ModeSnapshot) -> Option<Vec<u8>> {
    let mods = event.mods;
    if level == 0 || !(mods.ctrl || mods.alt) {
        return None;
    }
    let code = match &event.key {
        KeySpec::Character(text) => unshifted(text)?,
        KeySpec::Named(NamedKey::Enter) => 13,
        KeySpec::Named(NamedKey::Tab) => 9,
        KeySpec::Named(NamedKey::Backspace) => 127,
        KeySpec::Named(NamedKey::Escape) => 27,
        _ => return None,
    };
    if level == 1 && produces_control_byte(event, modes) {
        return None;
    }
    Some(format!("\x1b[27;{};{code}~", modifier_byte(mods)).into_bytes())
}

/// Whether the legacy encoding of this chord is already a control byte, which
/// is xterm's level-`1` exception in one predicate rather than a list that
/// drifts away from the table it describes.
fn produces_control_byte(event: &KeyEvent, modes: &ModeSnapshot) -> bool {
    match super::key::encode_legacy(&event.key, event.mods, modes) {
        Some(bytes) => match bytes.as_slice() {
            [byte] => byte.is_ascii_control() || *byte == 0x7f,
            // `Alt` prefixes ESC onto the control byte and does not change
            // whether there is one.
            [0x1b, byte] => byte.is_ascii_control() || *byte == 0x7f,
            _ => false,
        },
        None => false,
    }
}

#[cfg(test)]
#[path = "kitty_tests.rs"]
mod tests;
