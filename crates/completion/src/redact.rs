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

/// Cap on a stored history line (defensive length bound).
const MAX_LINE_LEN: usize = 4096;

/// Flag names (normalized) whose following/inline value is a secret.
const SECRET_FLAG_VOCAB: &[&str] = &[
    "password",
    "passwd",
    "pwd",
    "pass",
    "p",
    "secret",
    "token",
    "api-key",
    "apikey",
    "access-key",
    "secret-key",
    "auth",
    "authorization",
    "bearer",
    "credential",
    "private-key",
    "client-secret",
    "session-token",
    "otp",
    "passphrase",
];

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

/// Normalize a flag name for vocabulary comparison: strip leading triggers,
/// lowercase, and map `_` → `-`.
fn normalize_flag(name: &str) -> String {
    name.trim_start_matches(['-', '/'])
        .to_ascii_lowercase()
        .replace('_', "-")
}

fn is_secret_flag(name: &str) -> bool {
    let n = normalize_flag(name);
    SECRET_FLAG_VOCAB.contains(&n.as_str())
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

fn is_secret_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    let norm = k.replace('_', "-");
    if SECRET_FLAG_VOCAB.contains(&norm.as_str()) {
        return true;
    }
    k.ends_with("_token")
        || k.ends_with("_key")
        || k.ends_with("_secret")
        || k.ends_with("_password")
        || k.contains("secret")
        || k.contains("password")
        || k.contains("token")
}

/// Strip `scheme://user:pass@host/...` down to `scheme://host/...`.
fn strip_url_userinfo(tok: &str) -> Option<String> {
    let scheme_end = tok.find("://")?;
    let after = &tok[scheme_end + 3..];
    let at = after.find('@')?;
    let userinfo = &after[..at];
    // Only strip if the userinfo actually carries credentials (`user:pass`).
    if !userinfo.contains(':') {
        return None;
    }
    let rest = &after[at + 1..];
    Some(format!("{}://{}", &tok[..scheme_end], rest))
}

/// Heuristic: does a standalone token look like a credential?
fn looks_like_secret(tok: &str) -> bool {
    let t = tok.trim_matches(|c| c == '"' || c == '\'');
    if t.is_empty() {
        return false;
    }

    // Known credential prefixes / shapes.
    if t.starts_with("AKIA") && t.len() >= 16 && t[4..].chars().all(|c| c.is_ascii_alphanumeric()) {
        return true;
    }
    for p in [
        "ghp_",
        "gho_",
        "ghs_",
        "ghu_",
        "ghr_",
        "github_pat_",
        "sk-",
        "xox",
    ] {
        if t.starts_with(p) && t.len() >= p.len() + 8 {
            return true;
        }
    }
    if t.contains("-----BEGIN") {
        return true;
    }
    // Authorization header value containing a Bearer token.
    if t.to_ascii_lowercase().contains("bearer ")
        || t.to_ascii_lowercase().starts_with("authorization")
    {
        return true;
    }
    // JWT: three dot-separated base64url segments, first starting with `eyJ`.
    if is_jwt(t) {
        return true;
    }
    // High-entropy blob.
    if is_high_entropy(t) {
        return true;
    }
    false
}

fn is_jwt(t: &str) -> bool {
    let parts: Vec<&str> = t.split('.').collect();
    if parts.len() != 3 {
        return false;
    }
    if !parts[0].starts_with("eyJ") {
        return false;
    }
    parts
        .iter()
        .all(|p| p.len() >= 4 && p.chars().all(is_base64url_char))
}

fn is_base64url_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '+' || c == '/' || c == '='
}

/// Long, high-Shannon-entropy tokens that are mostly base64/hex are treated as
/// probable secrets. Guards against flagging ordinary paths / words.
fn is_high_entropy(t: &str) -> bool {
    if t.len() < 20 {
        return false;
    }
    // Reject obvious non-secrets: paths, URLs, and tokens with separators that
    // read like structured data.
    if t.contains('/') || t.contains('\\') || t.contains("://") {
        return false;
    }
    let alnum = t
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '+' || *c == '/' || *c == '_' || *c == '-')
        .count();
    if alnum * 100 / t.len() < 90 {
        return false;
    }
    // Must contain a mix of letters and digits (words alone are not secrets).
    let has_digit = t.chars().any(|c| c.is_ascii_digit());
    let has_alpha = t.chars().any(|c| c.is_ascii_alphabetic());
    if !(has_digit && has_alpha) {
        return false;
    }
    shannon_entropy(t) >= 3.5
}

fn shannon_entropy(s: &str) -> f64 {
    let len = s.chars().count() as f64;
    if len == 0.0 {
        return 0.0;
    }
    let mut counts = std::collections::HashMap::new();
    for c in s.chars() {
        *counts.entry(c).or_insert(0u32) += 1;
    }
    let mut entropy = 0.0;
    for &count in counts.values() {
        let p = count as f64 / len;
        entropy -= p * p.log2();
    }
    entropy
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_value_after_secret_flag() {
        assert_eq!(redact("az login --password S3cr3t!"), "az login --password");
    }

    #[test]
    fn drops_inline_secret_value_forms() {
        assert_eq!(redact("login --password=S3cr3t!"), "login --password");
        assert_eq!(redact("login /PASSWORD:S3cr3t!"), "login /PASSWORD");
        assert_eq!(redact("login -p S3cr3t!"), "login -p");
    }

    #[test]
    fn drops_secret_env_assignment() {
        assert_eq!(
            redact("AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI aws s3 ls"),
            "aws s3 ls"
        );
    }

    #[test]
    fn keeps_ordinary_assignment() {
        assert_eq!(
            redact("RUST_LOG=debug cargo test"),
            "RUST_LOG=debug cargo test"
        );
    }

    #[test]
    fn drops_standalone_credential_tokens() {
        assert!(!redact("echo ghp_0123456789abcdefghij").contains("ghp_"));
        assert!(!redact("echo sk-0123456789abcdef0123").contains("sk-"));
        assert!(!redact("echo AKIAIOSFODNN7EXAMPLE").contains("AKIA"));
        let jwt = "eyJhbGciOi.eyJzdWIiOi.SflKxwRJSM";
        assert!(!redact(&format!("echo {jwt}")).contains("eyJ"));
    }

    #[test]
    fn strips_url_userinfo_but_keeps_host() {
        assert_eq!(
            redact("psql postgres://user:pass@db.example.com/app"),
            "psql postgres://db.example.com/app"
        );
    }

    #[test]
    fn does_not_over_redact_normal_command() {
        assert_eq!(redact("dir /Q"), "dir /Q");
        assert_eq!(
            redact("grep --color -n foo file.txt"),
            "grep --color -n foo file.txt"
        );
        assert_eq!(
            redact("cargo build --release --output file.txt"),
            "cargo build --release --output file.txt"
        );
    }

    #[test]
    fn header_bearer_value_dropped_flag_kept() {
        // `curl -H "Authorization: Bearer abc.def"` → the quoted header is one
        // token here (already unquoted by the caller in practice).
        let redacted = redact("curl -H Authorization:Bearer-abcdefgh");
        assert!(redacted.starts_with("curl -H"));
        assert!(!redacted.contains("abcdefgh"));
    }

    #[test]
    fn suggestion_time_guard_detects_injected_secret() {
        assert!(contains_secret("deploy --token ghp_0123456789abcdefghij"));
        assert!(contains_secret("echo AKIAIOSFODNN7EXAMPLE"));
        assert!(!contains_secret("git commit -m message"));
        assert!(!contains_secret("dir /Q"));
    }
}
