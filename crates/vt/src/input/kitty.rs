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

use super::key::{KeyEvent, KeyEventKind, KeyMods, KeySpec, NamedKey, modifier_byte};

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

/// The shape for a key, and whether the key implies shift.
///
/// `F13`–`F24` are xterm's shifted `F1`–`F12` here, which is what the legacy
/// table already sends and what keeps the two paths agreeing. The
/// specification's own private-use codes `57376`–`57387` are the deviation, and
/// guide chapter 6 names it.
fn form(key: &KeySpec) -> Option<(Form, bool)> {
    let form = match key {
        KeySpec::Character(text) => Form::Codepoint(unshifted(text)?),
        // `KeySpec` and `NamedKey` are `#[non_exhaustive]` to the outside, but
        // exhaustive here on purpose: a new variant must be given a code point
        // rather than silently falling into a wildcard that sends nothing.
        KeySpec::Named(named) => return Some(named_form(*named)),
    };
    Some((form, false))
}

/// The shape for a named key.
///
/// `NamedKey` names thirty-eight keys and the specification's private-use table
/// has about a hundred: the keypad (`57399`-`57415`), the lock and system keys
/// (`57358`-`57363`), the media keys (`57428`+) and the modifier keys
/// themselves (`57441`+) have no variant here, so no embedder can deliver one
/// and nothing is dropped on the floor.
fn named_form(named: NamedKey) -> (Form, bool) {
    use NamedKey::*;
    match named {
        Escape => (Form::Codepoint(27), false),
        Enter => (Form::Codepoint(13), false),
        Tab => (Form::Codepoint(9), false),
        Backspace => (Form::Codepoint(127), false),
        ArrowUp => (Form::Letter(b'A'), false),
        ArrowDown => (Form::Letter(b'B'), false),
        ArrowRight => (Form::Letter(b'C'), false),
        ArrowLeft => (Form::Letter(b'D'), false),
        Home => (Form::Letter(b'H'), false),
        End => (Form::Letter(b'F'), false),
        Insert => (Form::Tilde(2), false),
        Delete => (Form::Tilde(3), false),
        PageUp => (Form::Tilde(5), false),
        PageDown => (Form::Tilde(6), false),
        F1 => (Form::Letter(b'P'), false),
        F2 => (Form::Letter(b'Q'), false),
        F3 => (Form::Letter(b'R'), false),
        F4 => (Form::Letter(b'S'), false),
        F5 => (Form::Tilde(15), false),
        F6 => (Form::Tilde(17), false),
        F7 => (Form::Tilde(18), false),
        F8 => (Form::Tilde(19), false),
        F9 => (Form::Tilde(20), false),
        F10 => (Form::Tilde(21), false),
        F11 => (Form::Tilde(23), false),
        F12 => (Form::Tilde(24), false),
        F13 => (Form::Letter(b'P'), true),
        F14 => (Form::Letter(b'Q'), true),
        F15 => (Form::Letter(b'R'), true),
        F16 => (Form::Letter(b'S'), true),
        F17 => (Form::Tilde(15), true),
        F18 => (Form::Tilde(17), true),
        F19 => (Form::Tilde(18), true),
        F20 => (Form::Tilde(19), true),
        F21 => (Form::Tilde(20), true),
        F22 => (Form::Tilde(21), true),
        F23 => (Form::Tilde(23), true),
        F24 => (Form::Tilde(24), true),
    }
}

/// The un-shifted code point of a text key.
///
/// The specification is explicit: `ctrl+shift+a` is `CSI 97 ; 6 u` and must not
/// be `CSI 65 ; 6 u`. The embedder hands over the text the key produced, so the
/// best this can do without a platform key map is to lower-case it; shifted
/// punctuation (`$` for `4`) cannot be un-shifted this way and is sent as
/// itself. Guide chapter 6 names the ceiling.
fn unshifted(text: &str) -> Option<u32> {
    let first = text.chars().next()?;
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

    // Rung 1: nobody asked to hear about releases, so there are no bytes.
    if event.kind == KeyEventKind::Release && (!event_types || (legacy_c0 && !all_esc)) {
        return Encoded::Silent;
    }

    // Rung 2: the kitty flags supersede `modifyOtherKeys` whenever they apply.
    if kitty_applies(event, flags, legacy_c0) {
        return match csi_u(event, flags) {
            Some(bytes) => Encoded::Bytes(bytes),
            // A key with no code point at all (an empty `Character`): the
            // enhanced protocols cannot name it, and neither can the legacy
            // one meaningfully, so it sends nothing.
            None => Encoded::Silent,
        };
    }

    // Rung 3: xterm's older answer to the same problem.
    match modify_other_keys(event, modes.modify_other_keys) {
        Some(bytes) => Encoded::Bytes(bytes),
        None => Encoded::Legacy,
    }
}

/// Whether this event is reported as a kitty escape code.
fn kitty_applies(event: &KeyEvent, flags: KeyboardFlags, legacy_c0: bool) -> bool {
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
    // A repeat or a release has no legacy spelling, so the only way to report
    // one at all is the escape code that carries an event type.
    if event.kind != KeyEventKind::Press && flags.contains(KeyboardFlags::REPORT_EVENT_TYPES) {
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
    let (form, implies_shift) = form(&event.key)?;
    let mods = KeyMods {
        shift: event.mods.shift || implies_shift,
        ..event.mods
    };
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
/// rather than guessing. The text the embedder supplies wins; a `Character`
/// key's own payload is the fallback, because that *is* the text it produced.
/// A payload carrying a control code, or longer than [`TEXT_SCALAR_MAX`], drops
/// the field: the specification forbids the first and nothing needs the second.
fn text_field(event: &KeyEvent, flags: KeyboardFlags) -> Option<String> {
    if !flags.contains(KeyboardFlags::REPORT_ASSOCIATED_TEXT)
        || !flags.contains(KeyboardFlags::REPORT_ALL_KEYS_AS_ESC)
    {
        return None;
    }
    let text = match (event.text.as_deref(), &event.key) {
        (Some(text), _) => text,
        (None, KeySpec::Character(payload)) => payload.as_str(),
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
/// Level `1` covers the chords with no unambiguous legacy encoding; level `2`
/// covers every modified key that is not already an unambiguous functional key.
/// A functional key (an arrow, `F5`, `Home`) already has one, so neither level
/// touches it.
fn modify_other_keys(event: &KeyEvent, level: u8) -> Option<Vec<u8>> {
    let mods = event.mods;
    if level == 0 || !(mods.shift || mods.ctrl || mods.alt) {
        return None;
    }
    let code = match &event.key {
        KeySpec::Character(text) => unshifted(text)?,
        KeySpec::Named(NamedKey::Enter) => 13,
        KeySpec::Named(NamedKey::Tab) => 9,
        KeySpec::Named(NamedKey::Backspace) => 127,
        _ => return None,
    };
    if level == 1 && !ambiguous_in_legacy(&event.key, mods) {
        return None;
    }
    Some(format!("\x1b[27;{};{code}~", modifier_byte(mods)).into_bytes())
}

/// The chords xterm calls "other" at level `1`: those whose legacy bytes are
/// already spoken for by a different key.
fn ambiguous_in_legacy(key: &KeySpec, mods: KeyMods) -> bool {
    match key {
        // `Ctrl+a` is `0x01` and unambiguous; `Ctrl+3` is `ESC`, which is not.
        KeySpec::Character(text) => {
            mods.ctrl
                && !text
                    .chars()
                    .next()
                    .is_some_and(|first| first.is_ascii_alphabetic())
        }
        // `Ctrl+Enter` is `\r`, `Shift+Enter` is `\r`, `Ctrl+Tab` is `\t`.
        KeySpec::Named(NamedKey::Enter) => mods.ctrl || mods.shift,
        KeySpec::Named(NamedKey::Tab) => mods.ctrl,
        _ => false,
    }
}

#[cfg(test)]
#[path = "kitty_tests.rs"]
mod tests;
