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

mod output;
mod structural;
#[cfg(test)]
mod tests;

use crate::class::Class;
use crate::profile::ShellProfile;
use crate::role::RowRole;
use crate::rules::RuleSet;

use regex::Regex;
use std::sync::LazyLock;

/// Universal prompt detector — matches both Unix (`$`/`#`) and Windows (`>`)
/// prompts. Used as a fallback when the profile's own prompt regex doesn't
/// match (e.g. user runs `wsl` inside `cmd.exe` — prompt changes to Unix but
/// the profile stays `Cmd`).
static UNIVERSAL_PROMPT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:[A-Za-z]:[^\s>]*>[ ]|[^\s]*[\$#%][ ])").unwrap());

/// Scan one line of terminal text → `Vec<u8>` of `Class` (one per char).
///
/// `line` is the display text of one row (cells joined, spacers skipped). The
/// output length equals `line.chars().count()`.
pub fn scan_line(line: &str, rules: &RuleSet, profile: &ShellProfile, role: RowRole) -> Vec<u8> {
    let chars: Vec<char> = line.chars().collect();
    let n = chars.len();
    let mut classes = vec![Class::Default as u8; n];

    match role {
        RowRole::Prompt => scan_prompt_line(&chars, &mut classes, profile),
        RowRole::Command => scan_command_mode(&chars, &mut classes, profile),
        RowRole::Output => {
            // Fallback: if the prompt regex matches, treat as a prompt line.
            // Try the profile's own regex first, then a universal fallback that
            // handles cross-shell prompts (e.g. `wsl` inside `cmd.exe`).
            if profile.prompt_regex().is_match(line) || UNIVERSAL_PROMPT.is_match(line) {
                scan_prompt_line(&chars, &mut classes, profile);
            } else {
                output::scan_output(&chars, &mut classes, rules, profile);
            }
        }
    }

    classes
}

// ── PromptLine state ───────────────────────────────────────────────────────

/// Tag the prompt sign glyph, then switch to `CommandMode` after the sign + space.
fn scan_prompt_line(chars: &[char], classes: &mut [u8], profile: &ShellProfile) {
    let n = chars.len();
    // Find the prompt sign: scan from the start, the sign is the first
    // recognized prompt glyph (or the last non-space char before the command).
    let mut sign_end = None;
    for (i, &c) in chars.iter().enumerate() {
        if profile.is_prompt_sign(c) {
            // Tag only the sign glyph itself as PromptSign.
            classes[i] = Class::PromptSign as u8;
            sign_end = Some(i);
            break;
        }
        // If we hit a space before finding a sign, stop (not a prompt line).
        if c == ' ' && i > 0 {
            break;
        }
    }

    let sign_end = match sign_end {
        Some(e) => e,
        None => return, // No sign found — leave as default.
    };

    // Skip the space(s) after the sign, then switch to CommandMode.
    let mut start = sign_end + 1;
    while start < n && chars[start] == ' ' {
        start += 1;
    }
    if start < n {
        scan_command_mode_from(&chars[start..], &mut classes[start..], profile);
    }
}

// ── CommandMode state ──────────────────────────────────────────────────────

/// Scan the entire line in `CommandMode` (used when `RowRole::Command`).
fn scan_command_mode(chars: &[char], classes: &mut [u8], profile: &ShellProfile) {
    scan_command_mode_from(chars, classes, profile);
}

/// Core command-mode scan: first non-space token = `Command`, options = `Option`,
/// `;`/`|`/`&&`/`||` reset to expect a new `Command`.
fn scan_command_mode_from(chars: &[char], classes: &mut [u8], profile: &ShellProfile) {
    let n = chars.len();
    let mut i = 0;
    let mut expect_command = true;

    while i < n {
        // Skip whitespace.
        if chars[i] == ' ' {
            i += 1;
            continue;
        }

        // Check for command separators: ; | &&
        if chars[i] == ';' || chars[i] == '|' {
            classes[i] = Class::Operator as u8;
            // || is two chars.
            if chars[i] == '|' && i + 1 < n && chars[i + 1] == '|' {
                classes[i + 1] = Class::Operator as u8;
                i += 2;
            } else {
                i += 1;
            }
            expect_command = true;
            continue;
        }
        // && separator.
        if chars[i] == '&' && i + 1 < n && chars[i + 1] == '&' {
            classes[i] = Class::Operator as u8;
            classes[i + 1] = Class::Operator as u8;
            i += 2;
            expect_command = true;
            continue;
        }

        // Find the end of the current token.
        let tok_start = i;
        while i < n && chars[i] != ' ' && chars[i] != ';' && chars[i] != '|' {
            i += 1;
        }
        let tok_end = i; // exclusive

        if expect_command {
            // First token after a separator (or start) = Command.
            for j in tok_start..tok_end {
                classes[j] = Class::Command as u8;
            }
            expect_command = false;
        } else if is_option_token(&chars[tok_start..tok_end], profile) {
            // Option token (--flag, -x, /flag).
            for j in tok_start..tok_end {
                classes[j] = Class::Option as u8;
            }
        } else {
            // Argument — leave as Default (the structural matchers below
            // will pick up paths, numbers, etc. if we run them).
            // For command mode we keep it simple: arguments stay Default.
        }
    }
}

/// Whether a token is an option (starts with an option prefix char).
fn is_option_token(token: &[char], profile: &ShellProfile) -> bool {
    !token.is_empty() && profile.is_option_prefix(token[0])
}

/// Whether a char is a "word" char (alphanumeric or underscore).
/// Shared by [`output`] and [`structural`] submodules.
pub(super) fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}
