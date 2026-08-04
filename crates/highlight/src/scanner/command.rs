//! Command-mode scanning — `RowRole::Command` and the command portion of a
//! prompt line.
//!
//! The first non-space token is the `Command`; option tokens (`--flag`, `-x`,
//! `/flag`) are `Option`; `;`/`|`/`&&`/`||` reset to expect a new `Command`.
//! Arguments are left as `Default` (structural matchers do not run here).

use crate::class::Class;
use crate::profile::ShellProfile;

/// Scan the entire line in `CommandMode` (used when `RowRole::Command`).
pub(super) fn scan_command_mode(chars: &[char], classes: &mut [u8], profile: &ShellProfile) {
    scan_command_mode_from(chars, classes, profile);
}

/// Core command-mode scan: first non-space token = `Command`, options = `Option`,
/// `;`/`|`/`&&`/`||` reset to expect a new `Command`.
pub(super) fn scan_command_mode_from(chars: &[char], classes: &mut [u8], profile: &ShellProfile) {
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
