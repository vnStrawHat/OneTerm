//! OSC 7501 — the Program Status Protocol, revision 0.3.
//!
//! A program reports what it is doing (`idle`, `working`, `done`, `blocked`,
//! `error`, or `clear`) and the terminal keeps one record per `id`. OneTerm
//! reduces the records to one summary state per session, which the tab title
//! shows (`docs/spec-intakes/IN-0049-osc-7501/`, `DEC-0022`).
//!
//! The bytes are untrusted, and over SSH they come from another machine. A
//! report that breaks any rule is dropped whole; nothing in one is ever acted
//! on, and the free text (`title`, `msg`) is validated and then thrown away —
//! the summary needs only the state.
//!
//! Pure functions and one table: no gpui, no I/O. [`crate::backend`]'s router
//! owns the table per session and answers the probe.

use std::collections::BTreeMap;

use base64::Engine as _;
use base64::alphabet;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use oneterm_vt::StringTerm;

/// The OSC number.
pub(crate) const OSC: u32 = 7501;
/// The longest whole sequence the spec allows.
const MAX_SEQUENCE: usize = 4096;
/// The longest body that fits in [`MAX_SEQUENCE`] after `ESC ] 7501 ;` and `ESC \`.
const MAX_BODY: usize = MAX_SEQUENCE - b"\x1b]7501;".len() - 2;
/// Records kept per session: the spec's minimum.
pub(crate) const MAX_RECORDS: usize = 64;

/// Standard base64, padding optional (rev 0.3).
const BASE64: GeneralPurpose = GeneralPurpose::new(
    &alphabet::STANDARD,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

/// What a program reports. Declaration order is the summary's priority:
/// a terminal with one `Blocked` record shows `Blocked`, whatever else it has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProgramState {
    /// Running, nothing to report.
    Idle,
    /// Finished successfully.
    Done,
    /// Busy.
    Working,
    /// Finished with a failure.
    Error,
    /// Waiting on the user.
    Blocked,
}

/// A forwarded OSC 7501 that passed every rule.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Osc7501 {
    /// `OSC 7501 ; ?` — the support probe.
    Probe,
    /// A report for record `id` (`""` is the root). `state: None` is `clear`.
    Report {
        id: String,
        state: Option<ProgramState>,
    },
}

/// Parse a forwarded OSC 7501. `params[0]` is the number as written.
pub(crate) fn parse(params: &[&[u8]]) -> Option<Osc7501> {
    // `;` is outside the value charset, so a conforming body is one parameter.
    let [_, body] = params else {
        return None;
    };
    if *body == b"?" {
        return Some(Osc7501::Probe);
    }
    if body.len() > MAX_BODY {
        return None;
    }
    // Malformed pairs are skipped, unknown keys ignored, the last repeat wins.
    let mut fields: BTreeMap<&[u8], &[u8]> = BTreeMap::new();
    for pair in body.split(|&b| b == b':') {
        let Some(eq) = pair.iter().position(|&b| b == b'=') else {
            continue;
        };
        let (key, value) = (&pair[..eq], &pair[eq + 1..]);
        if is_key(key) && value.iter().all(|&b| is_value_byte(b)) {
            fields.insert(key, value);
        }
    }
    let state = match *fields.get(b"state".as_slice())? {
        b"idle" => Some(ProgramState::Idle),
        b"working" => Some(ProgramState::Working),
        b"done" => Some(ProgramState::Done),
        b"blocked" => Some(ProgramState::Blocked),
        b"error" => Some(ProgramState::Error),
        b"clear" => None,
        _ => return None,
    };
    // A malformed id drops the report; it does not fall back to the root.
    let id = match fields.get(b"id".as_slice()) {
        Some(id) => valid_id(id)?.to_owned(),
        None => String::new(),
    };
    for (key, max_decoded) in [(b"title".as_slice(), 192), (b"msg".as_slice(), 2048)] {
        if let Some(value) = fields.get(key)
            && !is_safe_text(value, max_decoded)
        {
            return None;
        }
    }
    Some(Osc7501::Report { id, state })
}

/// The only bytes OneTerm ever sends for OSC 7501: the probe echoed back,
/// terminated the way the probe was.
pub(crate) fn probe_reply(terminator: StringTerm) -> Vec<u8> {
    let mut reply = b"\x1b]7501;?".to_vec();
    reply.extend_from_slice(match terminator {
        StringTerm::Bel => b"\x07",
        StringTerm::St => b"\x1b\\",
    });
    reply
}

fn is_key(key: &[u8]) -> bool {
    (1..=16).contains(&key.len()) && key.iter().all(u8::is_ascii_lowercase)
}

fn is_value_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"_.,+/=-".contains(&b)
}

fn is_id_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"_.+-".contains(&b)
}

/// `/`-separated segments of `[A-Za-z0-9_.+-]{1,32}`, at most 8, at most 128 bytes.
fn valid_id(id: &[u8]) -> Option<&str> {
    if id.len() > 128 {
        return None;
    }
    let mut segments = 0;
    for segment in id.split(|&b| b == b'/') {
        segments += 1;
        if segments > 8
            || !(1..=32).contains(&segment.len())
            || !segment.iter().all(|&b| is_id_byte(b))
        {
            return None;
        }
    }
    // Every byte was checked to be ASCII above.
    std::str::from_utf8(id).ok()
}

/// Base64 of UTF-8, at most `max_decoded` bytes, with no control character
/// (C0, DEL or C1 — the spec says refuse, not strip).
fn is_safe_text(value: &[u8], max_decoded: usize) -> bool {
    if value.len() > max_decoded.div_ceil(3) * 4 {
        return false;
    }
    let Ok(bytes) = BASE64.decode(value) else {
        return false;
    };
    bytes.len() <= max_decoded
        && std::str::from_utf8(&bytes).is_ok_and(|text| !text.chars().any(char::is_control))
}

/// One session's records, and the summary the UI shows.
#[derive(Debug, Default)]
pub(crate) struct RecordTable {
    /// id -> (state, last update tick).
    records: BTreeMap<String, (ProgramState, u64)>,
    tick: u64,
}

impl RecordTable {
    /// Apply one report: replace the record, or clear it and its subtree
    /// (everything, for the root).
    pub(crate) fn apply(&mut self, id: String, state: Option<ProgramState>) {
        let Some(state) = state else {
            if id.is_empty() {
                self.records.clear();
            } else {
                let subtree = format!("{id}/");
                self.records
                    .retain(|key, _| *key != id && !key.starts_with(&subtree));
            }
            return;
        };
        self.tick += 1;
        if self.records.len() >= MAX_RECORDS && !self.records.contains_key(&id) {
            let oldest = self
                .records
                .iter()
                .min_by_key(|(_, (_, tick))| *tick)
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                self.records.remove(&oldest);
            }
        }
        self.records.insert(id, (state, self.tick));
    }

    /// A new shell prompt began (OSC 133 `A`): `working`, `blocked` and `idle`
    /// records go, `done` and `error` stay.
    pub(crate) fn prompt_started(&mut self) {
        self.records
            .retain(|_, (state, _)| matches!(state, ProgramState::Done | ProgramState::Error));
    }

    /// The highest-priority state across every record; `None` when empty.
    pub(crate) fn summary(&self) -> Option<ProgramState> {
        self.records.values().map(|(state, _)| *state).max()
    }
}

#[cfg(test)]
#[path = "program_status_tests.rs"]
mod tests;
