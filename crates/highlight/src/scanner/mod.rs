//! Single-pass line scanner — classifies tokens in one line of terminal text.
//!
//! Produces a `Vec<u8>` of [`Class`] per *char index* (one byte per char in
//! the input string). The `ui` layer flattens this to per-column classes (see
//! §Q4 of the design doc).
//!
//! The scanner has two states driven by the row role:
//! - `PromptLine` → tag the prompt sign, then switch to `CommandMode`.
//! - `CommandMode` → first token = `Command`, options = `Option`, `;`/`|`/`&&`/`||` reset.
//! - `OutputMode` → run the flat matcher set in priority order.
//!
//! See §4.1 and the appendix pseudocode.

mod command;
mod output;
mod prompt;
#[cfg(test)]
mod scanner_tests;
mod structural;

use crate::class::Class;
use crate::profile::ShellProfile;
use crate::role::RowRole;
use crate::rules::RuleSet;

/// Scan one line of terminal text → `Vec<u8>` of `Class` (one per char).
///
/// `line` is the display text of one row (cells joined, spacers skipped). The
/// output length equals `line.chars().count()`.
pub fn scan_line(line: &str, rules: &RuleSet, profile: &ShellProfile, role: RowRole) -> Vec<u8> {
    let mut classes = Vec::new();
    scan_line_into(line, rules, profile, role, &mut classes);
    classes
}

/// [`scan_line`] into a caller-owned buffer: `out` is cleared and refilled, so
/// a per-frame scan reuses one allocation per row (PERF-23).
pub fn scan_line_into(
    line: &str,
    rules: &RuleSet,
    profile: &ShellProfile,
    role: RowRole,
    out: &mut Vec<u8>,
) {
    let chars: Vec<char> = line.chars().collect();
    let n = chars.len();
    out.clear();
    out.resize(n, Class::Default as u8);
    let classes = out.as_mut_slice();

    match role {
        RowRole::Prompt => prompt::scan_prompt_line(&chars, classes, profile, None),
        RowRole::Command => command::scan_command_mode(&chars, classes, profile),
        RowRole::Output => {
            // Fallback: if the line looks like a prompt, treat it as one.
            if let Some(sign) = prompt::prompt_sign(line, profile) {
                prompt::scan_prompt_line(&chars, classes, profile, Some(sign));
            } else {
                // The byte-based matchers (keyword automaton, structural
                // regexes) share one text + byte→char map built here once.
                let text = LineText {
                    text: line,
                    byte_to_char: byte_to_char_map(line),
                };
                output::scan_output(&text, &chars, classes, rules, profile);
            }
        }
    }
}

/// One line as the byte-based matchers see it: the original `&str` plus its
/// byte offset → char index map (built once per line, shared by every matcher).
pub(super) struct LineText<'a> {
    pub(super) text: &'a str,
    byte_to_char: Vec<usize>,
}

impl LineText<'_> {
    /// Char index for byte offset `byte` (`byte == len` → char count).
    pub(super) fn char_index(&self, byte: usize) -> usize {
        self.byte_to_char
            .get(byte)
            .copied()
            // Past the end: the sentinel, which is the char count.
            .unwrap_or_else(|| self.byte_to_char.last().copied().unwrap_or(0))
    }
}

/// Whether a char is a "word" char (alphanumeric or underscore).
/// Shared by the [`output`], [`structural`], and [`prompt`] submodules.
pub(super) fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Map each byte offset of `s` back to its char index, with a trailing
/// sentinel equal to the char count.
///
/// The keyword automaton and structural regexes match on bytes, but the class
/// buffer is indexed per char. This lets a byte match range be converted to a
/// char range (see [`LineText`]).
///
/// One entry **per byte**, not per char: a multi-byte char occupies as many
/// entries as it has bytes, all naming that char. Pushing once per char made
/// the map the identity, so every keyword and structural class on a line with
/// any non-ASCII char was written shifted right by the extra UTF-8 bytes before
/// it — `日本語 error here` painted `here` as `Error` (`BUG-0071` F3).
fn byte_to_char_map(s: &str) -> Vec<usize> {
    let mut map: Vec<usize> = Vec::with_capacity(s.len() + 1);
    for (char_index, c) in s.chars().enumerate() {
        map.resize(map.len() + c.len_utf8(), char_index);
    }
    map.push(s.chars().count()); // sentinel for end
    map
}
