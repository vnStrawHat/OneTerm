//! Prompt-line scanning — `RowRole::Prompt` and the OSC-133-less fallback.
//!
//! Tags the prompt-sign glyph and the path before it, then hands the command
//! portion off to [`super::command`]. [`looks_like_prompt`] is the fallback
//! detector used for `RowRole::Output` rows when no OSC 133 marker is present.

use std::sync::LazyLock;

use regex::Regex;

use crate::class::Class;
use crate::profile::{ShellProfile, UNIX_PROMPT_PATTERN};

use super::command::scan_command_mode;
use super::structural;

/// Universal prompt detector — matches both Unix (`$`/`#`/`%`, same rules as
/// the Unix profile: plausible prefix, sign followed by space/EOL — CORR-48)
/// and Windows (`C:\path>`) prompts. Used as a fallback when the profile's own
/// prompt regex doesn't match (e.g. user runs `wsl` inside `cmd.exe` — prompt
/// changes to Unix but the profile stays `Cmd`).
static UNIVERSAL_PROMPT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"^(?:[A-Za-z]:[^\s>]*>[ ]?)|{}",
        UNIX_PROMPT_PATTERN
    ))
    .expect("universal prompt regex is valid")
});

/// Whether an output row should be treated as a prompt line (no OSC 133).
///
/// Tries the profile's own prompt regex first, then a universal fallback that
/// handles cross-shell prompts (e.g. `wsl` inside `cmd.exe`).
pub(super) fn looks_like_prompt(line: &str, profile: &ShellProfile) -> bool {
    profile.prompt_regex().is_match(line) || UNIVERSAL_PROMPT.is_match(line)
}

/// Tag the prompt sign glyph, then switch to `CommandMode` after the sign + space.
pub(super) fn scan_prompt_line(chars: &[char], classes: &mut [u8], profile: &ShellProfile) {
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

    // Tag the path before the prompt sign (e.g. `D:\path` in `D:\path>`).
    // The prompt text typically contains a filesystem path — run the path probe
    // so it gets `Class::Path` instead of staying `Default` (white).
    if sign_end > 0 {
        structural::path_probe(&chars[..sign_end], &mut classes[..sign_end], profile);
    }

    // Skip the space(s) after the sign, then switch to CommandMode.
    let mut start = sign_end + 1;
    while start < n && chars[start] == ' ' {
        start += 1;
    }
    if start < n {
        scan_command_mode(&chars[start..], &mut classes[start..], profile);
    }
}
