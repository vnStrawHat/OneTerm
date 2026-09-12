//! The ground state: the control-byte scan, UTF-8 validation and print runs.
//!
//! Ported from the reference because the parity recordings pin its replacement
//! rules byte for byte, with two deliberate changes (IN-0029 P1, P2): the scan
//! is `memchr3(ESC, LF, CR)` rather than `memchr(ESC)`, so a line feed leaves
//! the per-character loop, and a validated run is handed over whole as
//! [`Dispatch::print_str`] instead of one call per character.

use super::state::State;
use super::{Dispatch, Parser};

/// `U+FFFD REPLACEMENT CHARACTER`, the single-character string form.
const REPLACEMENT: &str = "\u{fffd}";

impl Parser {
    /// Consume bytes from the ground state, returning how many were used.
    pub(super) fn advance_ground<D: Dispatch>(&mut self, dispatch: &mut D, bytes: &[u8]) -> usize {
        let num_bytes = bytes.len();
        let plain = memchr::memchr3(0x1B, 0x0A, 0x0D, bytes).unwrap_or(num_bytes);

        // The run is empty: the first byte is the control byte itself.
        if plain == 0 {
            self.take_ground_control(dispatch, bytes[0]);
            return 1;
        }

        match str::from_utf8(&bytes[..plain]) {
            Ok(text) => {
                print_run(dispatch, text);
                if plain < num_bytes {
                    self.take_ground_control(dispatch, bytes[plain]);
                    return plain + 1;
                }
                plain
            }
            Err(error) => {
                let valid = error.valid_up_to();
                // Safe by construction: `valid_up_to` is a UTF-8 boundary.
                if let Ok(text) = str::from_utf8(&bytes[..valid]) {
                    print_run(dispatch, text);
                }

                match error.error_len() {
                    Some(len) => {
                        // A one-byte error below 0xA0 is how 8-bit C1 reaches
                        // `execute`; it is never an introducer (trap 48).
                        if len == 1 && bytes[valid] <= 0x9F {
                            dispatch.execute(bytes[valid]);
                        } else {
                            dispatch.print_str(REPLACEMENT);
                        }
                        // The rest of the invalid run is skipped, not re-parsed.
                        valid + len
                    }
                    None if plain < num_bytes => {
                        // Truncated by the control byte, so it can never complete.
                        dispatch.print_str(REPLACEMENT);
                        self.take_ground_control(dispatch, bytes[plain]);
                        plain + 1
                    }
                    None => {
                        // Truncated by the end of the chunk: carry it over.
                        let extra = num_bytes - valid;
                        let end = self.partial_utf8_len + extra;
                        self.partial_utf8[self.partial_utf8_len..end]
                            .copy_from_slice(&bytes[valid..valid + extra]);
                        self.partial_utf8_len = end;
                        num_bytes
                    }
                }
            }
        }
    }

    /// Act on the control byte that ended a ground run.
    fn take_ground_control<D: Dispatch>(&mut self, dispatch: &mut D, byte: u8) {
        if byte == 0x1B {
            self.reset_params();
            self.state = State::Escape;
        } else {
            dispatch.execute(byte);
        }
    }

    /// Continue a codepoint carried over from the previous chunk, returning how
    /// many bytes of `bytes` were used.
    pub(super) fn advance_partial_utf8<D: Dispatch>(
        &mut self,
        dispatch: &mut D,
        bytes: &[u8],
    ) -> usize {
        let old_len = self.partial_utf8_len;
        let copied = bytes.len().min(self.partial_utf8.len() - old_len);
        self.partial_utf8[old_len..old_len + copied].copy_from_slice(&bytes[..copied]);
        self.partial_utf8_len += copied;

        match str::from_utf8(&self.partial_utf8[..self.partial_utf8_len]) {
            Ok(text) => {
                let consumed = match text.chars().next() {
                    Some(c) => {
                        print_char(dispatch, c);
                        c.len_utf8().saturating_sub(old_len)
                    }
                    None => copied,
                };
                self.partial_utf8_len = 0;
                consumed
            }
            Err(error) => {
                let valid = error.valid_up_to();
                // Any valid prefix means a shorter codepoint completed and the
                // tail belongs to the next character; print it and hand the
                // tail back.
                //
                // The reference consumes the whole valid prefix here, which
                // silently drops a second character that fitted into the carry
                // buffer — `C5 93 | 40 97` loses the `@`. Consuming only the
                // first character's bytes keeps an arbitrary chunking of one
                // stream producing one action sequence, which the embedder
                // relies on when a sequence straddles two reads.
                if valid > 0 {
                    let mut consumed = valid;
                    if let Ok(text) = str::from_utf8(&self.partial_utf8[..valid])
                        && let Some(c) = text.chars().next()
                    {
                        print_char(dispatch, c);
                        consumed = c.len_utf8();
                    }
                    self.partial_utf8_len = 0;
                    return consumed.saturating_sub(old_len);
                }
                match error.error_len() {
                    Some(invalid_len) => {
                        dispatch.print_str(REPLACEMENT);
                        self.partial_utf8_len = 0;
                        invalid_len.saturating_sub(old_len)
                    }
                    // Still incomplete: everything copied is consumed.
                    None => copied,
                }
            }
        }
    }
}

/// Print `c`, routing C0, `DEL` and C1 to `execute` as the ground state does.
fn print_char<D: Dispatch>(dispatch: &mut D, c: char) {
    if is_ground_control(c) {
        dispatch.execute(c as u8);
    } else {
        dispatch.print(c);
    }
}

/// Hand a validated run to the dispatch, split only at the control characters
/// the ground state executes.
///
/// The scan is over bytes, not characters: every break is either a byte below
/// `0x20`, `DEL`, or the `0xC2` lead of a two-byte C1. Any other byte of a
/// multi-byte character is passed over untouched, so an ASCII run costs one
/// comparison per byte and one `print_str` for the whole run.
fn print_run<D: Dispatch>(dispatch: &mut D, text: &str) {
    let bytes = text.as_bytes();
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte >= 0x20 && byte != 0x7F && byte != 0xC2 {
            index += 1;
            continue;
        }
        if byte == 0xC2 {
            // U+0080..=U+009F are the C1 controls; U+00A0..=U+00BF are printable.
            let Some(&next) = bytes.get(index + 1) else {
                break;
            };
            if next > 0x9F {
                index += 2;
                continue;
            }
            if index > start {
                dispatch.print_str(&text[start..index]);
            }
            dispatch.execute(next);
            index += 2;
            start = index;
            continue;
        }
        if index > start {
            dispatch.print_str(&text[start..index]);
        }
        dispatch.execute(byte);
        index += 1;
        start = index;
    }
    if start < bytes.len() {
        dispatch.print_str(&text[start..]);
    }
}

/// The characters the ground state executes instead of printing: C0, `DEL` and
/// the C1 range.
fn is_ground_control(c: char) -> bool {
    matches!(c, '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}')
}
