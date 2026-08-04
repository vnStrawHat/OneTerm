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
    let chars: Vec<char> = line.chars().collect();
    let n = chars.len();
    let mut classes = vec![Class::Default as u8; n];

    match role {
        RowRole::Prompt => prompt::scan_prompt_line(&chars, &mut classes, profile),
        RowRole::Command => command::scan_command_mode(&chars, &mut classes, profile),
        RowRole::Output => {
            // Fallback: if the line looks like a prompt, treat it as one.
            if prompt::looks_like_prompt(line, profile) {
                prompt::scan_prompt_line(&chars, &mut classes, profile);
            } else {
                output::scan_output(&chars, &mut classes, rules, profile);
            }
        }
    }

    classes
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
/// char range. Shared by the [`output`] and [`structural`] submodules.
pub(super) fn byte_to_char_map(s: &str) -> Vec<usize> {
    let mut map: Vec<usize> = Vec::with_capacity(s.len() + 1);
    let mut char_index = 0;
    for _ in s.char_indices() {
        map.push(char_index);
        char_index += 1;
    }
    map.push(char_index); // sentinel for end
    map
}
