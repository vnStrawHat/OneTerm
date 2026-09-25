//! Single-pass line scanner — classifies tokens in one line of terminal text.
//!
//! Produces a `Vec<u8>` of [`Class`] per *char index* (one byte per char in
//! the input string). The `ui` layer flattens this to per-column classes (see
//! §Q4 of the design doc).
//!
//! The scanner has three states driven by the row role:
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

pub use prompt::tint_prompt_sign;

/// Scan one line of terminal text → `Vec<u8>` of `Class` (one per char).
///
/// `line` is the display text of one **logical** line (the wrap-connected rows
/// joined, spacers skipped). The output length equals `line.chars().count()`.
///
/// `role` is what the shell's OSC 133 marks say about the line, and `None` means
/// the shell said nothing — only then does the prompt regex run (§4.2).
pub fn scan_line(
    line: &str,
    rules: &RuleSet,
    profile: &ShellProfile,
    role: Option<RowRole>,
    input_at: Option<usize>,
) -> Vec<u8> {
    let mut classes = Vec::new();
    let mut scratch = ScanScratch::default();
    scan_line_into(
        line,
        rules,
        profile,
        role,
        input_at,
        &mut scratch,
        &mut classes,
    );
    classes
}

/// The scanner's per-line working buffers, owned by the caller and reused
/// across lines so a steady-state scan allocates nothing (`US-0144`).
#[derive(Default)]
pub struct ScanScratch {
    /// The line's chars: the matchers index the class buffer per char.
    chars: Vec<char>,
    /// Byte offset → char index, for the byte-based matchers. Left empty for
    /// an ASCII line, where the two are equal.
    byte_to_char: Vec<usize>,
}

/// [`scan_line`] into caller-owned buffers: `out` is cleared and refilled, and
/// `scratch` holds the working buffers, so a per-frame scan reuses its
/// allocations (PERF-23, `US-0144`).
///
/// `input_at` is the char index where the typed command begins (`OSC 133;B`) and
/// is honoured only for [`RowRole::Prompt`]: it bounds the prompt region, so the
/// sign is found exactly instead of guessed by a glyph hunt that a prompt
/// containing a space (`PS C:\src>`, `[user@host ~]$`) defeats.
#[cfg_attr(feature = "hotpath-profiling", hotpath::measure)]
pub fn scan_line_into(
    line: &str,
    rules: &RuleSet,
    profile: &ShellProfile,
    role: Option<RowRole>,
    input_at: Option<usize>,
    scratch: &mut ScanScratch,
    out: &mut Vec<u8>,
) {
    scratch.chars.clear();
    scratch.chars.extend(line.chars());
    let chars = scratch.chars.as_slice();
    let n = chars.len();
    out.clear();
    out.resize(n, Class::Default as u8);
    let classes = out.as_mut_slice();

    match role {
        Some(RowRole::Prompt) => {
            let sign = prompt::marked_sign(chars, profile, input_at);
            prompt::scan_prompt_line(chars, classes, profile, sign);
        }
        Some(RowRole::Command) => command::scan_command_mode(chars, classes, profile),
        // Marked as output: the shell was explicit, so the prompt regex — the
        // only reason a marked row could still be misread — never runs.
        Some(RowRole::Output) => {
            let text = LineText::new(line, &mut scratch.byte_to_char);
            output::scan_output(&text, chars, classes, rules, profile);
        }
        None => {
            // No mark: if the line looks like a prompt, treat it as one.
            if let Some(sign) = prompt::prompt_sign(line, profile) {
                prompt::scan_prompt_line(chars, classes, profile, Some(sign));
            } else {
                let text = LineText::new(line, &mut scratch.byte_to_char);
                output::scan_output(&text, chars, classes, rules, profile);
            }
        }
    }
}

/// One line as the byte-based matchers (keyword automaton, structural regexes)
/// see it: the original `&str` plus its byte offset → char index map, built
/// once per line and shared by every matcher.
pub(super) struct LineText<'a> {
    pub(super) text: &'a str,
    /// Empty for an ASCII line: byte offset == char index.
    byte_to_char: &'a [usize],
}

impl<'a> LineText<'a> {
    /// Build the map for `text` into `map` (cleared first), unless `text` is
    /// ASCII, where the map would be the identity.
    fn new(text: &'a str, map: &'a mut Vec<usize>) -> Self {
        map.clear();
        if !text.is_ascii() {
            fill_byte_to_char_map(text, map);
        }
        Self {
            text,
            byte_to_char: map,
        }
    }

    /// Char index for byte offset `byte` (`byte == len` → char count).
    pub(super) fn char_index(&self, byte: usize) -> usize {
        if self.byte_to_char.is_empty() {
            // ASCII: past the end clamps to the char count, as the sentinel does.
            return byte.min(self.text.len());
        }
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

/// Fill `map` with each byte offset of `s` mapped back to its char index, plus
/// a trailing sentinel equal to the char count.
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
fn fill_byte_to_char_map(s: &str, map: &mut Vec<usize>) {
    let mut count = 0;
    for (char_index, c) in s.chars().enumerate() {
        map.resize(map.len() + c.len_utf8(), char_index);
        count = char_index + 1;
    }
    map.push(count); // sentinel for end
}
