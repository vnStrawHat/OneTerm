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

mod detect;
#[cfg(test)]
mod tests;

use detect::{is_secret_flag, is_secret_key, looks_like_secret, strip_url_userinfo};

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
                if is_secret_flag(name) {
                    out.push(name.to_string());
                    changed = true;
                } else {
                    out.push(tok.to_string());
                }
            } else if is_secret_flag(tok) {
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
