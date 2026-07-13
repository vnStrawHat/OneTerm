//! Output-mode scanning — the flat matcher pipeline for `RowRole::Output`.
//!
//! [`scan_output`] runs matchers in priority order: strings → permission
//! block → keywords → structural regexes → hand-written probes → single-char.

use crate::class::Class;
use crate::profile::ShellProfile;
use crate::rules::RuleSet;

use super::is_word_char;
use super::structural;

/// Run the flat matcher set in priority order (first match wins per span).
pub(super) fn scan_output(
    chars: &[char],
    classes: &mut [u8],
    rules: &RuleSet,
    profile: &ShellProfile,
) {
    let n = chars.len();
    if n == 0 {
        return;
    }

    // 1. Strings (quoted) — a begin/end mini-state for '…' / "…".
    quote_scan(chars, classes);

    // 2. Permission block (ls -l prefix).
    if tag_permission_block(chars, classes) {
        // The whole line is the perms block — done.
        // (ls -l lines: `drwxr-xr-x  2 user group` — only the first 10 chars
        //  are perms; the rest is still output → continue scanning.)
    }

    // 3. Keywords — one Aho-Corasick pass.
    keyword_scan(chars, classes, rules);

    // 4. Structural regexes (IPv6, MAC, DateTime) — skip claimed columns.
    structural::structural_scan(chars, classes, rules);

    // 5. Hand-written probes (IPv4, Path, Number).
    structural::ipv4_probe(chars, classes);
    structural::path_probe(chars, classes, profile);
    structural::number_probe(chars, classes);

    // 5b. Option probe — detect --flag / -x tokens in output (e.g. curl --help).
    option_probe(chars, classes, profile);

    // 6. Single-char classes (last, only on default cells).
    structural::single_char_scan(chars, classes);
}

/// 1. String scanning — tag `'…'` and `"…"` spans as `String`.
fn quote_scan(chars: &[char], classes: &mut [u8]) {
    let n = chars.len();
    let mut i = 0;
    while i < n {
        let q = chars[i];
        if q == '\'' || q == '"' {
            // Find the matching close quote.
            let start = i;
            i += 1;
            while i < n && chars[i] != q {
                i += 1;
            }
            if i < n {
                // Include the closing quote.
                for j in start..=i {
                    classes[j] = Class::String as u8;
                }
                i += 1;
            } else {
                // Unclosed — tag the rest of the line.
                for j in start..n {
                    classes[j] = Class::String as u8;
                }
                break;
            }
        } else {
            i += 1;
        }
    }
}

/// 2. Permission block — tag the 10-char `ls -l` perms prefix, per-char.
///
/// Position 0 is the file-type char (PermType), then each `r`/`w`/`x`/`s`/`t`/`-`
/// gets its own sub-class so the theme can color them individually.
/// Returns `true` if a perms block was tagged.
fn tag_permission_block(chars: &[char], classes: &mut [u8]) -> bool {
    if chars.len() < 10 {
        return false;
    }
    let type_char = chars[0];
    if !matches!(type_char, 'b' | 'c' | 'd' | 'l' | 'p' | 's' | '-') {
        return false;
    }
    // Check chars 1..10 for valid perms chars.
    for j in 1..10 {
        if !matches!(chars[j], 'r' | 'w' | 'x' | 's' | 't' | 'S' | 'T' | '-') {
            return false;
        }
    }
    // Tag position 0 as PermType.
    classes[0] = Class::PermType as u8;
    // Tag each permission char individually.
    for j in 1..10 {
        classes[j] = match chars[j] {
            'r' => Class::PermRead as u8,
            'w' => Class::PermWrite as u8,
            'x' => Class::PermExec as u8,
            's' | 'S' | 't' | 'T' => Class::PermSpecial as u8,
            '-' => Class::PermNone as u8,
            _ => Class::Default as u8, // unreachable (validated above)
        };
    }
    true
}

/// 5b. Option probe — detect `--flag` / `-x` / `/flag` tokens in output mode.
///
/// This runs after structural probes (paths, numbers) so that option-like
/// tokens that are still `Default` get tagged as `Option`. It must run *before*
/// `single_char_scan` which would otherwise tag `-` as `Operator`.
///
/// In output mode (e.g. `curl --help`), option tokens appear in usage lines
/// and option descriptions. We detect:
/// - `--word` (long options: `--help`, `--url URL`)
/// - `-x` (short options: `-x`, `-X`)
/// - `/flag` (Windows cmd-style — only when the profile accepts `/` as an
///   option prefix and the `/` is at a word boundary)
fn option_probe(chars: &[char], classes: &mut [u8], profile: &ShellProfile) {
    let n = chars.len();
    let mut i = 0;
    while i < n {
        if classes[i] != Class::Default as u8 {
            i += 1;
            continue;
        }

        // Check for option prefix: `-` or (for Cmd/Dumb) `/`.
        if !profile.is_option_prefix(chars[i]) {
            i += 1;
            continue;
        }

        // For `/` prefix (Windows cmd), require it to be at a word boundary
        // (preceded by space or start-of-line) to avoid matching paths.
        if chars[i] == '/' && i > 0 && chars[i - 1] != ' ' {
            i += 1;
            continue;
        }

        let start = i;
        // Consume the prefix char.
        i += 1;

        // Long option: `--word`
        if i < n && chars[i] == '-' {
            i += 1;
            // Consume word chars (letters, digits, `-`, `_`, `=`, `.`).
            let had_word = i < n && is_word_char(chars[i]);
            while i < n
                && (is_word_char(chars[i]) || chars[i] == '-' || chars[i] == '=' || chars[i] == '.')
            {
                i += 1;
            }
            if had_word {
                for j in start..i {
                    if classes[j] == Class::Default as u8 {
                        classes[j] = Class::Option as u8;
                    }
                }
            }
            continue;
        }

        // Short option: `-x` (letters, possibly combined like `-vx`).
        if i < n && is_word_char(chars[i]) {
            while i < n && is_word_char(chars[i]) {
                i += 1;
            }
            // Also consume `=value` suffix (e.g. `-x=type`).
            if i < n && chars[i] == '=' {
                i += 1;
                while i < n && (is_word_char(chars[i]) || chars[i] == '.') {
                    i += 1;
                }
            }
            for j in start..i {
                if classes[j] == Class::Default as u8 {
                    classes[j] = Class::Option as u8;
                }
            }
            continue;
        }

        // Not a valid option token — let single_char_scan handle the `-`.
    }
}

/// 3. Keyword scanning — one Aho-Corasick pass with word-boundary checking.
fn keyword_scan(chars: &[char], classes: &mut [u8], rules: &RuleSet) {
    // Build a string from chars for Aho-Corasick (byte-based).
    let s: String = chars.iter().collect();

    // Precompute byte offset → char index mapping.
    let mut byte_to_char: Vec<usize> = Vec::with_capacity(s.len() + 1);
    let mut char_idx = 0;
    for _ in s.char_indices() {
        byte_to_char.push(char_idx);
        char_idx += 1;
    }
    byte_to_char.push(char_idx); // sentinel for end

    for m in rules.keywords.find_iter(&s) {
        let pat = m.pattern();
        let class = rules.class_for_pattern(pat.as_usize()) as u8;

        // Convert byte range → char range.
        let char_start = *byte_to_char.get(m.start()).unwrap_or(&0);
        let char_end = *byte_to_char.get(m.end()).unwrap_or(&chars.len());

        // Word boundary check: the char before and after must not be
        // alphanumeric (prevents "no" matching inside "node").
        let before_ok = char_start == 0 || !is_word_char(chars[char_start - 1]);
        let after_ok = char_end >= chars.len() || !is_word_char(chars[char_end]);

        if !before_ok || !after_ok {
            continue;
        }

        // Only tag cells that are still Default (don't override higher-priority
        // matches like strings).
        for j in char_start..char_end {
            if classes[j] == Class::Default as u8 {
                classes[j] = class;
            }
        }
    }
}
