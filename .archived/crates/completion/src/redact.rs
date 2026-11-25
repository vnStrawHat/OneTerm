//! Secret-value redaction for the `memory` (history) source.
//!
//! Redaction runs **before** anything enters the in-RAM history ring
//! (docs/auto-completion/08 §2): it keeps the command name and every option flag
//! name but drops secret **values** (after `--password`, secret `KEY=VALUE`
//! assignments, and standalone credential-shaped tokens). A cheap
//! [`contains_secret`] guard also runs at suggestion time (defense in depth,
//! docs 08 §5).
//!
//! Note: the architecture keeps this crate alacritty-free (depends only on
//! `core`), so the control-char/length hygiene is implemented locally rather
//! than pulling in `oneterm_terminal::security_policy`.

#[path = "redact_detect.rs"]
mod detect;
#[cfg(test)]
#[path = "redact_tests.rs"]
mod tests;

use detect::{
    command_name, is_command_secret_long_flag, is_secret_flag, is_secret_key, looks_like_secret,
    short_secret_flags, strip_url_userinfo,
};

/// Cap on a stored history line (defensive length bound).
const MAX_LINE_LEN: usize = 4096;

/// Redact a raw command line into a form safe to store and suggest.
pub fn redact(line: &str) -> String {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let (kept, _changed) = scrub_tokens(&tokens);
    let joined = kept.join(" ");
    hygiene(&joined)
}

/// Suggestion-time guard: does this candidate contain anything that looks like a
/// secret? Used to drop history-derived suggestions that slipped past capture.
pub fn contains_secret(line: &str) -> bool {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let (_kept, changed) = scrub_tokens(&tokens);
    changed
}

/// Core token scrubber. Returns `(kept_tokens, changed)` where `changed` is true
/// if anything was dropped or rewritten (a secret was present).
fn scrub_tokens(tokens: &[&str]) -> (Vec<String>, bool) {
    let mut out: Vec<String> = Vec::with_capacity(tokens.len());
    let mut changed = false;
    let mut drop_next_value = false;
    let (command, subcommand) = command_and_subcommand(tokens);
    let short_secret = short_secret_flags(&command, subcommand.as_deref());
    let secret_flag =
        |name: &str| is_secret_flag(name) || is_command_secret_long_flag(&command, name);

    for &tok in tokens {
        if drop_next_value {
            // This token is the secret value following a secret flag.
            drop_next_value = false;
            changed = true;
            continue;
        }

        if is_flag(tok) {
            if let Some((name, _value)) = split_inline_flag(tok) {
                // `--flag=value` / `/FLAG:value` / `-p=value`
                if secret_flag(name) || is_short_secret_flag(name, short_secret) {
                    out.push(name.to_string());
                    changed = true;
                } else {
                    out.push(tok.to_string());
                }
            } else if let Some(flag) = attached_short_secret(tok, short_secret) {
                // `-pSECRET` / `-uuser:pass` — keep the flag, drop the value.
                out.push(flag);
                changed = true;
            } else if secret_flag(tok) || is_short_secret_flag(tok, short_secret) {
                out.push(tok.to_string());
                drop_next_value = true;
            } else {
                out.push(tok.to_string());
            }
            continue;
        }

        // KEY=VALUE assignment.
        if let Some((key, _val)) = split_assignment(tok) {
            if is_secret_key(key) {
                changed = true;
                continue; // drop whole assignment
            }
            // Non-secret key but secret-shaped value → drop whole assignment.
            if let Some((_k, v)) = tok.split_once('=') {
                if looks_like_secret(v) {
                    changed = true;
                    continue;
                }
            }
            out.push(tok.to_string());
            continue;
        }

        // URL with embedded `user:pass@` → strip only the userinfo.
        if let Some(stripped) = strip_url_userinfo(tok) {
            changed = true;
            out.push(stripped);
            continue;
        }

        // Standalone credential-shaped token → drop.
        if looks_like_secret(tok) {
            changed = true;
            continue;
        }

        out.push(tok.to_string());
    }

    (out, changed)
}

/// Whether a token starts with an option trigger.
fn is_flag(tok: &str) -> bool {
    tok.starts_with('-') || tok.starts_with('/')
}

/// The normalized command name and its first subcommand, skipping leading
/// `KEY=VALUE` assignments and privilege wrappers (`sudo mysql -p…`).
fn command_and_subcommand(tokens: &[&str]) -> (String, Option<String>) {
    let mut rest = tokens
        .iter()
        .copied()
        .skip_while(|tok| split_assignment(tok).is_some() || *tok == "sudo" || *tok == "doas");
    let command = rest.next().map(command_name).unwrap_or_default();
    let subcommand = rest
        .find(|tok| !is_flag(tok))
        .map(|tok| tok.to_ascii_lowercase());
    (command, subcommand)
}

/// A single-dash short flag (`-p`) whose letter is a per-command secret flag.
fn is_short_secret_flag(tok: &str, short_secret: &[char]) -> bool {
    let mut chars = tok.strip_prefix('-').unwrap_or_default().chars();
    matches!((chars.next(), chars.next()), (Some(letter), None) if short_secret.contains(&letter))
}

/// The attached form `-pSECRET` / `-uuser:pass`: returns the bare flag
/// (`-p`) when the token is a per-command short secret flag with a value glued
/// on. Long options (`--…`) never take the attached form.
fn attached_short_secret(tok: &str, short_secret: &[char]) -> Option<String> {
    let body = tok.strip_prefix('-')?;
    if body.starts_with('-') {
        return None;
    }
    let mut chars = body.chars();
    let letter = chars.next()?;
    if chars.next().is_none() || !short_secret.contains(&letter) {
        return None;
    }
    Some(format!("-{letter}"))
}

/// Split an inline-value flag into `(flag_name, value)` for `--flag=value`,
/// `/FLAG:value`. Returns `None` if there is no inline value.
fn split_inline_flag(tok: &str) -> Option<(&str, &str)> {
    if let Some(idx) = tok.find('=') {
        return Some((&tok[..idx], &tok[idx + 1..]));
    }
    // Windows `/FLAG:value`. Only treat ':' as a separator for `/`-flags so we
    // never split a Unix `-o` from a path like `-o/tmp` (rare) incorrectly.
    if tok.starts_with('/') {
        if let Some(idx) = tok.find(':') {
            return Some((&tok[..idx], &tok[idx + 1..]));
        }
    }
    None
}

/// Split `KEY=VALUE` where KEY is an env-style identifier. Returns `None` if the
/// token is not a plain assignment.
fn split_assignment(tok: &str) -> Option<(&str, &str)> {
    let (key, val) = tok.split_once('=')?;
    if key.is_empty()
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return None;
    }
    Some((key, val))
}

/// Control-char strip + length truncation (local hygiene layer).
fn hygiene(s: &str) -> String {
    let mut out: String = s
        .chars()
        .filter(|c| !c.is_control() || *c == '\t')
        .map(|c| if c == '\t' { ' ' } else { c })
        .collect();
    if out.len() > MAX_LINE_LEN {
        // Truncate on a char boundary.
        let mut end = MAX_LINE_LEN;
        while !out.is_char_boundary(end) {
            end -= 1;
        }
        out.truncate(end);
    }
    out.trim().to_string()
}
