//! Prompt-line scanning — `RowRole::Prompt` and the OSC-133-less fallback.
//!
//! Tags the prompt-sign glyph and the path before it, then hands the command
//! portion off to [`super::command`]. [`looks_like_prompt`] is the fallback
//! detector used for `RowRole::Output` rows when no OSC 133 marker is present.

use std::sync::LazyLock;

use regex::Regex;

use crate::class::Class;
use crate::profile::{ShellProfile, UNIX_PROMPT_PATTERN, cmd_prompt_pattern};

use super::command::scan_command_mode;
use super::structural;

/// Universal prompt detector — matches both Unix (`$`/`#`/`%`, same rules as
/// the Unix profile: plausible prefix, sign followed by space/EOL — CORR-48)
/// and Windows (`C:\path>`, spaces in the path allowed) prompts. Used as a
/// fallback when the profile's own prompt regex doesn't match (e.g. user runs
/// `wsl` inside `cmd.exe` — prompt changes to Unix but the profile stays `Cmd`).
static UNIVERSAL_PROMPT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!("{}|{}", cmd_prompt_pattern(), UNIX_PROMPT_PATTERN))
        .expect("universal prompt regex is valid")
});

/// The char index of the prompt sign, when `line` is a prompt line (no OSC 133).
///
/// Tries the profile's own prompt regex first, then a universal fallback that
/// handles cross-shell prompts (e.g. `wsl` inside `cmd.exe`). The sign is the
/// last prompt glyph **inside the match**, which is what lets a Windows path
/// contain spaces and a later `>` redirection stay out of the prompt region
/// (`BUG-0071`): the match, not a left-to-right glyph hunt, decides where the
/// prompt ends.
pub(super) fn prompt_sign(line: &str, profile: &ShellProfile) -> Option<usize> {
    let end = profile
        .prompt_regex()
        .find(line)
        .or_else(|| UNIVERSAL_PROMPT.find(line))?
        .end();
    // The match may include one trailing space after the sign.
    line[..end]
        .char_indices()
        .rev()
        .position(|(_, c)| profile.is_prompt_sign(c))
        .map(|back| line[..end].chars().count() - 1 - back)
}

/// Where the path starts in the prompt region `head`, when that region is a
/// Windows prompt path: a drive root (`C:\`), a UNC root (`\\`), or either of
/// those after PowerShell's `PS ` prefix. `None` for every other prompt shape
/// (Unix, `PS>`, a bare `>`), which keeps the generic probe.
fn windows_prompt_path(head: &[char]) -> Option<usize> {
    let start = if head.starts_with(&['P', 'S', ' ']) {
        3
    } else {
        0
    };
    let rest = head.get(start..)?;
    let drive = matches!(rest, [c, ':', sep, ..] if c.is_ascii_alphabetic() && (*sep == '\\' || *sep == '/'));
    let unc = matches!(rest, ['\\', '\\', ..]);
    (drive || unc).then_some(start)
}

/// Tag the prompt sign glyph, then switch to `CommandMode` after the sign + space.
///
/// `sign` is the char index of the prompt sign when the detector found one. It
/// is `None` only on the OSC 133 path (`RowRole::Prompt`), where no regex ran;
/// the first prompt glyph is used then.
pub(super) fn scan_prompt_line(
    chars: &[char],
    classes: &mut [u8],
    profile: &ShellProfile,
    sign: Option<usize>,
) {
    let n = chars.len();
    let sign_end = match sign.filter(|&i| i < n) {
        Some(i) => i,
        None => {
            // No detector ran (OSC 133 says this row is a prompt): the sign is
            // the first recognized prompt glyph before the first space.
            let mut found = None;
            for (i, &c) in chars.iter().enumerate() {
                if profile.is_prompt_sign(c) {
                    found = Some(i);
                    break;
                }
                if c == ' ' && i > 0 {
                    break;
                }
            }
            match found {
                Some(i) => i,
                None => return, // No sign found — leave as default.
            }
        }
    };
    // Tag only the sign glyph itself as PromptSign.
    classes[sign_end] = Class::PromptSign as u8;

    // Tag the path before the prompt sign (e.g. `D:\path` in `D:\path>`).
    // The prompt text typically contains a filesystem path — run the path probe
    // so it gets `Class::Path` instead of staying `Default` (white).
    if sign_end > 0 {
        let head = &chars[..sign_end];
        let head_classes = &mut classes[..sign_end];
        match windows_prompt_path(head) {
            // The detector already proved this whole region is a Windows
            // prompt path, so colour it whole. The generic probe stops at a
            // space and needs two separators for a relative segment, which
            // left `C:\Users\John Doe\ws` tagged only as far as `John`
            // (`BUG-0071` F2).
            Some(start) => head_classes[start..].fill(Class::Path as u8),
            None => structural::path_probe(head, head_classes, profile),
        }
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
