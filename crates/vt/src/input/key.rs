//! Encode keyboard events → escape sequences for the terminal.
//!
//! Key-to-byte encoding rules:
//! - Defines neutral types (`KeySpec`/`KeyMods`/`NamedKey`) — no dependency on
//!   `keyboard_types` or any UI framework. The embedder maps its own key
//!   events → `KeySpec`.
//! - Returns bytes only, with NO side effects (scroll/selection/shift-tracking
//!   belong to the view/session, not the encoder).
//! - Returns `None` only when the combination has no terminal encoding
//!   (Ctrl + non-ASCII / multi-codepoint text) → the caller ignores it.

use crate::snapshot::ModeSnapshot;

use super::kitty::{self, Encoded};

/// Modifier state when encoding a key (bit-agnostic, uses bool for clarity).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyMods {
    /// Shift is held.
    pub shift: bool,
    /// Control is held.
    pub ctrl: bool,
    /// Alt (Meta / Option) is held.
    pub alt: bool,
}

/// Special key names (not printable characters).
///
/// `#[non_exhaustive]`: new named keys arrive with new keyboard protocols, so
/// an embedder must keep a `_` arm. Construction is unaffected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NamedKey {
    /// Return / Enter.
    Enter,
    /// Backspace.
    Backspace,
    /// Forward delete.
    Delete,
    /// Tab.
    Tab,
    /// Escape.
    Escape,
    /// Cursor up.
    ArrowUp,
    /// Cursor down.
    ArrowDown,
    /// Cursor left.
    ArrowLeft,
    /// Cursor right.
    ArrowRight,
    /// Home.
    Home,
    /// End.
    End,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
    /// Insert.
    Insert,
    /// Function key 1.
    F1,
    /// Function key 2.
    F2,
    /// Function key 3.
    F3,
    /// Function key 4.
    F4,
    /// Function key 5.
    F5,
    /// Function key 6.
    F6,
    /// Function key 7.
    F7,
    /// Function key 8.
    F8,
    /// Function key 9.
    F9,
    /// Function key 10.
    F10,
    /// Function key 11.
    F11,
    /// Function key 12.
    F12,
    /// Function key 13 (xterm: shifted F1).
    F13,
    /// Function key 14 (xterm: shifted F2).
    F14,
    /// Function key 15 (xterm: shifted F3).
    F15,
    /// Function key 16 (xterm: shifted F4).
    F16,
    /// Function key 17 (xterm: shifted F5).
    F17,
    /// Function key 18 (xterm: shifted F6).
    F18,
    /// Function key 19 (xterm: shifted F7).
    F19,
    /// Function key 20 (xterm: shifted F8).
    F20,
    /// Function key 21 (xterm: shifted F9).
    F21,
    /// Function key 22 (xterm: shifted F10).
    F22,
    /// Function key 23 (xterm: shifted F11).
    F23,
    /// Function key 24 (xterm: shifted F12).
    F24,
}

/// Specification of a key event — framework-agnostic.
///
/// `#[non_exhaustive]`: a keyboard protocol that reports something other than a
/// character or a named key would land here. Construction is unaffected.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum KeySpec {
    /// A character (may be multiple codepoints, e.g. a compose key).
    Character(String),
    /// A special key.
    Named(NamedKey),
}

/// Which kind of key event this is.
///
/// A repeat is encoded as a press and a release produces no bytes at all until
/// the program asks for event types with the kitty `REPORT_EVENT_TYPES` flag.
///
/// `#[non_exhaustive]`: a keyboard protocol that reports a fourth kind would
/// land here. Construction is unaffected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum KeyEventKind {
    /// The key went down.
    #[default]
    Press,
    /// The key is being auto-repeated while held.
    Repeat,
    /// The key came up.
    Release,
}

/// Everything the enhanced keyboard protocols can report about one key event.
///
/// `#[non_exhaustive]`: build it with [`KeyEvent::new`] and assign the rest.
/// The three optional fields come from the embedder's platform layer; leave one
/// `None` when the platform does not know it and the encoder omits the
/// corresponding sub-field, which the protocol allows.
///
/// ```
/// use oneterm_vt::input::{KeyEvent, KeyEventKind, KeyMods, KeySpec, NamedKey};
///
/// let mut event = KeyEvent::new(KeySpec::Named(NamedKey::Escape), KeyMods::default());
/// event.kind = KeyEventKind::Release;
/// assert_eq!(event.text, None);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct KeyEvent {
    /// Which key.
    pub key: KeySpec,
    /// Which modifiers were held.
    pub mods: KeyMods,
    /// Press, repeat or release.
    pub kind: KeyEventKind,
    /// The code point this key produces with shift applied, for the kitty
    /// `REPORT_ALTERNATE_KEYS` flag. Sent only when shift is in `mods`.
    pub shifted: Option<char>,
    /// The code point this key carries in the standard PC-101 layout, for the
    /// same flag: what a Cyrillic `C` is when the shortcut is `Ctrl+C`.
    pub base_layout: Option<char>,
    /// The text this event would insert, for `REPORT_ASSOCIATED_TEXT`. A
    /// `Character` key's own payload is used when this is `None`.
    pub text: Option<String>,
}

impl KeyEvent {
    /// A plain press of `key` with `mods` held, and nothing the platform had to
    /// look up.
    pub fn new(key: KeySpec, mods: KeyMods) -> KeyEvent {
        KeyEvent {
            key,
            mods,
            kind: KeyEventKind::Press,
            shifted: None,
            base_layout: None,
            text: None,
        }
    }
}

/// Byte CSI u / xterm modifier: `1 + shift + alt*2 + ctrl*4`.
pub(super) fn modifier_byte(mods: KeyMods) -> u8 {
    1 + mods.shift as u8 + (mods.alt as u8) * 2 + (mods.ctrl as u8) * 4
}

/// Bytes for Ctrl + a printable ASCII character, following the xterm table.
///
/// Letters map to their C0 control (`byte & 0x1f`, case-insensitive). The
/// digits and punctuation that share a control byte in xterm are listed
/// explicitly; every other ASCII character (`0`, `1`, `9`, `,`, `.`, …) is
/// sent unchanged, as xterm does. Non-ASCII or multi-codepoint text has no
/// Ctrl encoding → `None`.
fn ctrl_bytes(text: &str) -> Option<Vec<u8>> {
    let mut chars = text.chars();
    let ch = chars.next()?;
    if chars.next().is_some() || !ch.is_ascii() {
        return None;
    }
    let byte = match ch {
        'a'..='z' | 'A'..='Z' => (ch as u8) & 0x1f,
        ' ' | '2' | '@' => 0x00,
        '3' | '[' => 0x1b,
        '4' | '\\' => 0x1c,
        '5' | ']' => 0x1d,
        // `~` is in the specification's table beside `^`; it was missing here
        // until `US-0105` measured the table row by row.
        '6' | '^' | '~' => 0x1e,
        '7' | '_' | '/' => 0x1f,
        '8' | '?' => 0x7f,
        other => other as u8,
    };
    Some(vec![byte])
}

/// Encode a plain key press → the bytes the program expects.
///
/// [`encode_key_event`] for a [`KeyEvent::new`] press, which is what an
/// embedder that has no release, repeat or alternate-key information to give
/// wants. Everything that entry point documents applies here, including the two
/// enhanced protocols: **the bytes this returns change once the program has
/// pushed kitty keyboard flags or set a `modifyOtherKeys` level**, because that
/// is what the program asked for. With both at their defaults the answer is the
/// legacy encoding described below, byte for byte.
///
/// Returns `None` when the combination has no terminal encoding — Ctrl plus a
/// non-ASCII or multi-codepoint `Character` (there is no control byte for
/// `Ctrl+é`); the caller drops the event.
pub fn encode_key(key: &KeySpec, mods: KeyMods, modes: &ModeSnapshot) -> Option<Vec<u8>> {
    encode_key_event(&KeyEvent::new(key.clone(), mods), modes)
}

/// Encode one key event, honouring every keyboard protocol the snapshot
/// reports.
///
/// Exactly one encoding is chosen, and the first rung that applies wins:
///
/// 1. a release event, with no `REPORT_EVENT_TYPES` to ask for it → `None`;
/// 2. any kitty keyboard flag that applies to this key → the kitty `CSI u`
///    form, or the legacy functional form with the kitty modifier and
///    event-type fields attached;
/// 3. a non-zero `modifyOtherKeys` level and a modified "other" key →
///    `CSI 27 ; <modifier> ; <code point> ~`;
/// 4. otherwise the legacy table, which reads only
///    [`ModeSnapshot::app_cursor`].
///
/// Rung 2 before rung 3 is the protocols' own rule: a program that has pushed
/// kitty flags *and* set `modifyOtherKeys` gets kitty. Once rung 2 applies,
/// `app_cursor` is not read at all — an unmodified arrow under
/// `DISAMBIGUATE_ESC_CODES` is `CSI A` and never `ESC O A`, even in application
/// cursor key mode, because the kitty form is not the cursor-key form.
///
/// `None` means the event sends nothing: a release nobody asked to hear about,
/// a key with no code point, or a chord with no encoding at all. The embedder
/// drops it, exactly as it already must for `Ctrl` plus a non-ASCII character.
///
/// ```
/// use oneterm_vt::input::{KeyEvent, KeyMods, KeySpec, NamedKey, encode_key_event};
/// use oneterm_vt::{Config, EventBatch, Size, Terminal};
/// use std::time::Instant;
///
/// let mut term = Terminal::new(Size { rows: 24, cols: 80 }, Config::default());
/// let mut batch = EventBatch::new();
/// let escape = KeyEvent::new(KeySpec::Named(NamedKey::Escape), KeyMods::default());
///
/// // Nothing negotiated: the legacy byte.
/// assert_eq!(term.encode_key_event(&escape).as_deref(), Some(b"\x1b".as_slice()));
///
/// // The program pushes `DISAMBIGUATE_ESC_CODES`, and gets what it asked for.
/// term.feed(b"\x1b[>1u", &mut batch, Instant::now());
/// let modes = term.mode_snapshot();
/// assert_eq!(
///     encode_key_event(&escape, &modes).as_deref(),
///     Some(b"\x1b[27u".as_slice())
/// );
/// ```
pub fn encode_key_event(event: &KeyEvent, modes: &ModeSnapshot) -> Option<Vec<u8>> {
    match kitty::encode(event, modes) {
        Encoded::Silent => None,
        Encoded::Bytes(bytes) => Some(bytes),
        Encoded::Legacy => encode_legacy(&event.key, event.mods, modes),
    }
}

/// The xterm legacy encoding, which is what rung 4 answers.
///
/// The only field read from `modes` is [`ModeSnapshot::app_cursor`], the
/// terminal's DECCKM state: when the program has enabled Application Cursor
/// Keys (e.g. vim/less/man send `CSI ?1h`), the plain cursor keys (arrows,
/// Home, End) must use the `ESC O{ch}` form instead of `ESC [{ch}` so the
/// program recognizes them. [`crate::Terminal::encode_key`] passes the
/// terminal's own snapshot, so an embedder that has one needs nothing else.
///
/// Conventions:
/// - `Character` + `ctrl` → xterm control table (see `ctrl_bytes`).
/// - `Character` + `alt` → `ESC` prefix; combined with `ctrl` the prefix is
///   applied to the control byte (`Ctrl+Alt+a` → `ESC 0x01`).
/// - `Enter` shift/ctrl → CSI u; plain → `\r`.
/// - `Backspace`: ctrl → `0x08`, alt → `ESC DEL`, plain → `0x7f`.
/// - Arrow + (shift|ctrl) → `CSI 1;{mod}{ch}`; plain → `ESC O{ch}` when
///   `app_cursor`, else `ESC [{ch}`.
/// - `Home`/`End` + (shift|ctrl) → `CSI 1;{mod}H/F`; plain → `ESC OH/F`
///   when `app_cursor`, else `ESC [H/F`.
pub(super) fn encode_legacy(key: &KeySpec, mods: KeyMods, modes: &ModeSnapshot) -> Option<Vec<u8>> {
    let shift = mods.shift;
    let ctrl = mods.ctrl;
    let alt = mods.alt;
    let app_cursor = modes.app_cursor;

    let seq: Vec<u8> = match key {
        KeySpec::Character(ch) => {
            let mut bytes = if ctrl {
                ctrl_bytes(ch)?
            } else {
                ch.as_bytes().to_vec()
            };
            if alt {
                // Alt (Meta) prefixes ESC — also for Ctrl+Alt chords.
                bytes.insert(0, 0x1b);
            }
            bytes
        }

        KeySpec::Named(NamedKey::Enter) if shift || ctrl => {
            format!("\x1b[13;{}u", modifier_byte(mods)).into_bytes()
        }
        KeySpec::Named(NamedKey::Enter) => b"\r".to_vec(),

        KeySpec::Named(NamedKey::Backspace) if ctrl => vec![0x08],
        KeySpec::Named(NamedKey::Backspace) if alt => vec![0x1b, 0x7f],
        KeySpec::Named(NamedKey::Backspace) => vec![0x7f],

        KeySpec::Named(NamedKey::Delete) if alt || ctrl || shift => {
            format!("\x1b[3;{}~", modifier_byte(mods)).into_bytes()
        }
        KeySpec::Named(NamedKey::Delete) => b"\x1b[3~".to_vec(),

        KeySpec::Named(NamedKey::Tab) if shift => b"\x1b[Z".to_vec(),
        KeySpec::Named(NamedKey::Tab) => b"\t".to_vec(),

        KeySpec::Named(NamedKey::Escape) => vec![0x1b],

        KeySpec::Named(
            dir @ (NamedKey::ArrowUp
            | NamedKey::ArrowDown
            | NamedKey::ArrowLeft
            | NamedKey::ArrowRight),
        ) => {
            let ch = match dir {
                NamedKey::ArrowUp => 'A',
                NamedKey::ArrowDown => 'B',
                NamedKey::ArrowRight => 'C',
                NamedKey::ArrowLeft => 'D',
                _ => unreachable!(),
            };
            if shift || ctrl || alt {
                format!("\x1b[1;{}{ch}", modifier_byte(mods)).into_bytes()
            } else if app_cursor {
                vec![0x1b, b'O', ch as u8]
            } else {
                vec![0x1b, b'[', ch as u8]
            }
        }

        KeySpec::Named(NamedKey::Home) if shift || ctrl || alt => {
            format!("\x1b[1;{}H", modifier_byte(mods)).into_bytes()
        }
        KeySpec::Named(NamedKey::Home) if app_cursor => b"\x1bOH".to_vec(),
        KeySpec::Named(NamedKey::Home) => b"\x1b[H".to_vec(),

        KeySpec::Named(NamedKey::End) if shift || ctrl || alt => {
            format!("\x1b[1;{}F", modifier_byte(mods)).into_bytes()
        }
        KeySpec::Named(NamedKey::End) if app_cursor => b"\x1bOF".to_vec(),
        KeySpec::Named(NamedKey::End) => b"\x1b[F".to_vec(),

        KeySpec::Named(NamedKey::PageUp) if shift || ctrl || alt => {
            format!("\x1b[5;{}~", modifier_byte(mods)).into_bytes()
        }
        KeySpec::Named(NamedKey::PageUp) => b"\x1b[5~".to_vec(),
        KeySpec::Named(NamedKey::PageDown) if shift || ctrl || alt => {
            format!("\x1b[6;{}~", modifier_byte(mods)).into_bytes()
        }
        KeySpec::Named(NamedKey::PageDown) => b"\x1b[6~".to_vec(),
        KeySpec::Named(NamedKey::Insert) => b"\x1b[2~".to_vec(),
        // F1–F4: xterm SS3 sequences (ESC O P/Q/R/S).
        KeySpec::Named(NamedKey::F1) => b"\x1bOP".to_vec(),
        KeySpec::Named(NamedKey::F2) => b"\x1bOQ".to_vec(),
        KeySpec::Named(NamedKey::F3) => b"\x1bOR".to_vec(),
        KeySpec::Named(NamedKey::F4) => b"\x1bOS".to_vec(),
        // F5–F12: CSI ~ with parameter codes 15–24.
        KeySpec::Named(NamedKey::F5) => b"\x1b[15~".to_vec(),
        KeySpec::Named(NamedKey::F6) => b"\x1b[17~".to_vec(),
        KeySpec::Named(NamedKey::F7) => b"\x1b[18~".to_vec(),
        KeySpec::Named(NamedKey::F8) => b"\x1b[19~".to_vec(),
        KeySpec::Named(NamedKey::F9) => b"\x1b[20~".to_vec(),
        KeySpec::Named(NamedKey::F10) => b"\x1b[21~".to_vec(),
        KeySpec::Named(NamedKey::F11) => b"\x1b[23~".to_vec(),
        KeySpec::Named(NamedKey::F12) => b"\x1b[24~".to_vec(),
        // F13–F24: shifted F1–F12 (CSI ~ with modifier 2 = shift).
        // These are rarely used but xterm maps them to 2P/2Q/.../~ forms.
        KeySpec::Named(NamedKey::F13) => b"\x1b[1;2P".to_vec(),
        KeySpec::Named(NamedKey::F14) => b"\x1b[1;2Q".to_vec(),
        KeySpec::Named(NamedKey::F15) => b"\x1b[1;2R".to_vec(),
        KeySpec::Named(NamedKey::F16) => b"\x1b[1;2S".to_vec(),
        KeySpec::Named(NamedKey::F17) => b"\x1b[15;2~".to_vec(),
        KeySpec::Named(NamedKey::F18) => b"\x1b[17;2~".to_vec(),
        KeySpec::Named(NamedKey::F19) => b"\x1b[18;2~".to_vec(),
        KeySpec::Named(NamedKey::F20) => b"\x1b[19;2~".to_vec(),
        KeySpec::Named(NamedKey::F21) => b"\x1b[20;2~".to_vec(),
        KeySpec::Named(NamedKey::F22) => b"\x1b[21;2~".to_vec(),
        KeySpec::Named(NamedKey::F23) => b"\x1b[23;2~".to_vec(),
        KeySpec::Named(NamedKey::F24) => b"\x1b[24;2~".to_vec(),
    };

    Some(seq)
}

#[cfg(test)]
#[path = "key_tests.rs"]
mod tests;
