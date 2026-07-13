//! Structural probes — regex-based matchers (IPv6, MAC, DateTime) and
//! hand-written probes (IPv4, Path, Number, single-char operators/brackets).
//!
//! These run after the higher-priority matchers in [`super::output`] and only
//! tag cells that are still `Default`.

use crate::class::Class;
use crate::profile::{PathSep, ShellProfile};
use crate::rules::RuleSet;

use super::is_word_char;

/// 4. Structural regexes — IPv6, MAC, DateTime. Skip claimed columns.
pub(super) fn structural_scan(chars: &[char], classes: &mut [u8], rules: &RuleSet) {
    let s: String = chars.iter().collect();

    // Byte → char index map.
    let mut byte_to_char: Vec<usize> = Vec::with_capacity(s.len() + 1);
    let mut ci = 0;
    for _ in s.char_indices() {
        byte_to_char.push(ci);
        ci += 1;
    }
    byte_to_char.push(ci);

    let structural: [(&regex::Regex, Class); 3] = [
        (&rules.mac, Class::Mac),
        (&rules.ipv6, Class::Ip),
        (&rules.datetime, Class::DateTime),
    ];

    for (re, cls) in structural {
        for m in re.find_iter(&s) {
            let char_start = *byte_to_char.get(m.start()).unwrap_or(&0);
            let char_end = *byte_to_char.get(m.end()).unwrap_or(&chars.len());

            // IPv6-like strings must not start or end inside an identifier/path token.
            if cls == Class::Ip {
                let prev_is_word = char_start > 0 && is_word_char(chars[char_start - 1]);
                let next_is_word = char_end < chars.len() && is_word_char(chars[char_end]);
                if prev_is_word || next_is_word {
                    continue;
                }
            }

            // Only tag if all cells in the range are Default (skip claimed).
            let all_default = (char_start..char_end)
                .all(|j| classes.get(j).copied().unwrap_or(1) == Class::Default as u8);
            if all_default {
                for j in char_start..char_end {
                    if j < classes.len() {
                        classes[j] = cls as u8;
                    }
                }
            }
        }
    }
}

/// 5a. IPv4 probe — hand-written (faster than regex for 4 dotted octets).
pub(super) fn ipv4_probe(chars: &[char], classes: &mut [u8]) {
    let n = chars.len();
    let mut i = 0;
    while i < n {
        if chars[i].is_ascii_digit() && classes[i] == Class::Default as u8 {
            // Try to match 4 octets: DDD.DDD.DDD.DDD
            if let Some(end) = try_ipv4(chars, i) {
                for j in i..end {
                    classes[j] = Class::Ip as u8;
                }
                i = end;
                continue;
            }
        }
        i += 1;
    }
}

/// Try to match an IPv4 address starting at `start`. Returns the char end index.
fn try_ipv4(chars: &[char], start: usize) -> Option<usize> {
    let n = chars.len();
    let mut pos = start;
    let mut octets = 0;

    while octets < 4 {
        // Read 1-3 digits.
        let digit_start = pos;
        while pos < n && chars[pos].is_ascii_digit() {
            pos += 1;
        }
        let dlen = pos - digit_start;
        if dlen == 0 || dlen > 3 {
            return None;
        }
        // Validate octet value ≤ 255.
        let octet: u32 = chars[digit_start..pos]
            .iter()
            .collect::<String>()
            .parse()
            .ok()?;
        if octet > 255 {
            return None;
        }
        octets += 1;
        if octets < 4 {
            // Expect a dot.
            if pos >= n || chars[pos] != '.' {
                return None;
            }
            pos += 1;
        }
    }

    // Boundary check: next char must not be a digit or dot (avoid matching
    // 1.2.3.4.5 as an IP).
    if pos < n && (chars[pos].is_ascii_digit() || chars[pos] == '.') {
        return None;
    }
    // Preceding char must not be a digit or dot.
    if start > 0 && (chars[start - 1].is_ascii_digit() || chars[start - 1] == '.') {
        return None;
    }

    Some(pos)
}

/// 5b. Path probe — hand-written scan for filesystem paths.
///
/// A "path anchor" is a `/`, `\`, `~`, or drive letter (`C:\`). When we find
/// one, we extend **backward** to include a preceding path segment (so
/// `src/views/...` highlights `src` too) and **forward** through path chars.
///
/// Tagging rules:
/// - **Absolute** (starts with `/`, `\`, `~`, or drive letter): tag if the
///   preceding char is not a word char (avoids `foolbar/etc`).
/// - **Relative** (starts with a word char): tag only if there are **2+**
///   separators (clearly a multi-segment path like `src/views/terminal/mod.rs`)
///   **or** the last segment has a file extension (like `src/main.rs`).
///   This prevents `link/ether`, `bytes/sec`, and `3/5` from being tagged.
pub(super) fn path_probe(chars: &[char], classes: &mut [u8], profile: &ShellProfile) {
    let n = chars.len();
    let sep = profile.path_sep();
    let mut i = 0;
    while i < n {
        // Only start at a path anchor that's still Default.
        let is_path_anchor = match sep {
            PathSep::Unix => {
                chars[i] == '/' || (chars[i] == '~' && i + 1 < n && chars[i + 1] == '/')
            }
            PathSep::Windows => {
                chars[i] == '/'
                    || chars[i] == '\\'
                    || chars[i] == '~'
                    || (i + 2 < n
                        && chars[i].is_ascii_alphabetic()
                        && chars[i + 1] == ':'
                        && (chars[i + 2] == '\\' || chars[i + 2] == '/'))
            }
        };

        if !is_path_anchor || classes[i] != Class::Default as u8 {
            i += 1;
            continue;
        }

        // Extend backward to include a preceding path segment (word chars,
        // dots, dashes, underscores — but NOT separators, to avoid merging
        // with an already-tagged path).
        let mut start = i;
        if chars[i] == '/' || chars[i] == '\\' {
            let mut b = i;
            while b > 0
                && is_path_segment_char(chars[b - 1])
                && classes[b - 1] == Class::Default as u8
            {
                b -= 1;
            }
            start = b;
        }

        // Extend forward: consume path chars (alphanumerics, separators,
        // dots, dashes, underscores, ~, and the drive-letter colon).
        let mut end = i;
        while end < n
            && (chars[end].is_alphanumeric()
                || chars[end] == '/'
                || chars[end] == '\\'
                || chars[end] == '.'
                || chars[end] == '-'
                || chars[end] == '_'
                || chars[end] == '~'
                || chars[end] == ':')
        {
            // For the drive-letter colon (C:), only allow it at position 1.
            if chars[end] == ':' && end != start + 1 {
                break;
            }
            end += 1;
        }

        // Analyze the candidate span.
        let span = &chars[start..end];
        let sep_count = span.iter().filter(|&&c| c == '/' || c == '\\').count();
        if sep_count == 0 || span.len() < 2 {
            i = end;
            continue;
        }

        let is_absolute = span[0] == '/'
            || span[0] == '\\'
            || span[0] == '~'
            || (span.len() > 2 && span[0].is_ascii_alphabetic() && span[1] == ':');

        let should_tag = if is_absolute {
            // Absolute path: preceding char must not be a word char.
            start == 0 || !is_word_char(chars[start - 1])
        } else {
            // Relative path: need 2+ separators, or 1 separator with a file
            // extension in the last segment.
            sep_count >= 2 || path_has_extension(span)
        };

        if should_tag {
            for j in start..end {
                if classes[j] == Class::Default as u8 {
                    classes[j] = Class::Path as u8;
                }
            }
        }
        i = end;
    }
}

/// Whether a char can be part of a path *segment* (not a separator).
fn is_path_segment_char(c: char) -> bool {
    c.is_alphanumeric() || c == '.' || c == '-' || c == '_'
}

/// Whether the last segment of a path span (after the last `/` or `\`)
/// contains a `.` that looks like a file extension.
fn path_has_extension(span: &[char]) -> bool {
    let last_sep = span.iter().rposition(|&c| c == '/' || c == '\\');
    let last_seg = &span[last_sep.map_or(0, |p| p + 1)..];
    // A `.` at position 0 means `.` or `..` (directory), not an extension.
    last_seg
        .iter()
        .position(|&c| c == '.')
        .map_or(false, |p| p > 0)
}

/// 5c. Number probe — hand-written scan for numeric literals.
///
/// Matches: integers (`42`), decimals (`1.5`), hex (`0x1F`), exponents (`1.5e3`).
pub(super) fn number_probe(chars: &[char], classes: &mut [u8]) {
    let n = chars.len();
    let mut i = 0;
    while i < n {
        if classes[i] != Class::Default as u8 {
            i += 1;
            continue;
        }
        // Hex: 0x...
        if chars[i] == '0' && i + 1 < n && (chars[i + 1] == 'x' || chars[i + 1] == 'X') {
            let start = i;
            i += 2;
            while i < n && chars[i].is_ascii_hexdigit() {
                i += 1;
            }
            if i > start + 2 {
                // Boundary check.
                if i >= n || !is_word_char(chars[i]) {
                    for j in start..i {
                        classes[j] = Class::Number as u8;
                    }
                }
            }
            continue;
        }
        // Decimal/integer: digits[.digits][e[+-]digits], or a leading `-`
        // followed by digits when it forms a standalone negative number token.
        let is_negative_number = chars[i] == '-'
            && i + 1 < n
            && chars[i + 1].is_ascii_digit()
            && (i == 0 || is_number_boundary(chars[i - 1]));
        if chars[i].is_ascii_digit()
            || (chars[i] == '.' && i + 1 < n && chars[i + 1].is_ascii_digit())
            || is_negative_number
        {
            let start = i;
            if is_negative_number {
                i += 1; // skip the leading '-'
            }
            // Integer part.
            while i < n && chars[i].is_ascii_digit() {
                i += 1;
            }
            // Fractional part.
            if i < n && chars[i] == '.' {
                i += 1;
                while i < n && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            // Exponent.
            if i < n && (chars[i] == 'e' || chars[i] == 'E') {
                let exp_start = i;
                i += 1;
                if i < n && (chars[i] == '+' || chars[i] == '-') {
                    i += 1;
                }
                let exp_digits = i;
                while i < n && chars[i].is_ascii_digit() {
                    i += 1;
                }
                // If no digits after e/E, revert (not a number).
                if i == exp_digits {
                    i = exp_start;
                }
            }
            // Consume trailing percent sign (e.g. `89%`).
            if i < n && chars[i] == '%' {
                i += 1;
            }
            // Boundary check: the number token must be surrounded by whitespace
            // or common separators, not embedded in an identifier or filename
            // like `math-2` or `file_v2`.
            let before_ok = start == 0 || is_number_boundary(chars[start - 1]);
            let after_ok = i >= n || is_number_boundary(chars[i]);
            if before_ok && after_ok && i > start {
                for j in start..i {
                    classes[j] = Class::Number as u8;
                }
            }
            continue;
        }
        i += 1;
    }
}

/// 6. Single-char classes — tag operators and brackets on remaining default cells.
pub(super) fn single_char_scan(chars: &[char], classes: &mut [u8]) {
    for (i, &c) in chars.iter().enumerate() {
        if classes[i] != Class::Default as u8 {
            continue;
        }
        if is_operator(c) {
            classes[i] = Class::Operator as u8;
        } else if is_bracket(c) {
            classes[i] = Class::Bracket as u8;
        }
    }
}

/// Whether a char is an operator.
fn is_operator(c: char) -> bool {
    matches!(
        c,
        '=' | ';' | '|' | '?' | '*' | '$' | '<' | '>' | '&' | '+' | '-' | ':' | '!' | '~' | '^'
    )
}

/// Whether a char is a bracket.
fn is_bracket(c: char) -> bool {
    matches!(c, '(' | ')' | '[' | ']' | '{' | '}')
}

/// Whether a char can border a numeric literal without being part of it.
fn is_number_boundary(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' | '|' | '<' | '>' | '"' | '\''
                | '+' | '*' | '\\' | '=' | '!' | '?' | '@' | '#' | '$' | '%' | '^' | '&'
        )
}
