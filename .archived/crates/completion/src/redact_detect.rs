//! Secret-classification heuristics (docs 08 §3–4).
//!
//! Pure predicates that answer "is this flag/key/value a credential?" — the
//! vocabulary of sensitive flag names, credential-shaped token detection
//! (known prefixes, JWTs, high-entropy blobs), and URL userinfo stripping.
//! Kept separate from the tokenizing/scrubbing pipeline in [`super`] so the
//! detection rules can grow independently.

/// Flag names (normalized) whose following/inline value is a secret.
///
/// Single-letter flags are deliberately absent: `-p` means "password" for
/// `mysql`, "parents" for `mkdir`, "publish" for `docker run` and "port" for
/// `ssh`, so short flags are resolved per command by [`short_secret_flags`].
const SECRET_FLAG_VOCAB: &[&str] = &[
    "password",
    "passwd",
    "pwd",
    "pass",
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

/// Normalize a flag name for vocabulary comparison: strip leading triggers,
/// lowercase, and map `_` → `-`.
fn normalize_flag(name: &str) -> String {
    name.trim_start_matches(['-', '/'])
        .to_ascii_lowercase()
        .replace('_', "-")
}

/// Whether a long (or Windows `/FLAG`) option name carries a secret value:
/// either in the vocabulary or a compound ending in a secret noun
/// (`--http-password`, `--proxy-passwd`, `--client-secret`, `--access-token`).
pub(super) fn is_secret_flag(name: &str) -> bool {
    let n = normalize_flag(name);
    SECRET_FLAG_VOCAB.contains(&n.as_str())
        || n.ends_with("-password")
        || n.ends_with("-passwd")
        || n.ends_with("-secret")
        || n.ends_with("-token")
}

/// Commands whose `-p` short flag takes a password (attached `-pSECRET` or
/// separate `-p SECRET`).
const PASSWORD_P_COMMANDS: &[&str] = &[
    "mysql",
    "mysqladmin",
    "mysqldump",
    "mysqlimport",
    "mariadb",
    "mariadb-dump",
    "sshpass",
    "az",
];

/// Container CLIs whose `login` subcommand takes `-p <password>` while other
/// subcommands use `-p` for port publishing.
const CONTAINER_LOGIN_COMMANDS: &[&str] = &["docker", "podman", "nerdctl"];

/// The single-letter short flags whose value is a secret for `command`
/// (already normalized by [`command_name`]), given its first subcommand.
///
/// `curl -u user:pass` embeds a password in the credential pair, so it is
/// treated as secret regardless of the value's shape.
pub(super) fn short_secret_flags(command: &str, subcommand: Option<&str>) -> &'static [char] {
    if PASSWORD_P_COMMANDS.contains(&command) {
        return &['p'];
    }
    if CONTAINER_LOGIN_COMMANDS.contains(&command) && subcommand == Some("login") {
        return &['p'];
    }
    if command == "curl" {
        return &['u'];
    }
    &[]
}

/// Long option names whose value is a secret only for `command` (`curl --user
/// user:pass`).
pub(super) fn is_command_secret_long_flag(command: &str, name: &str) -> bool {
    command == "curl" && normalize_flag(name) == "user"
}

/// Normalize a command token for per-command rules: basename, lowercase, no
/// `.exe` suffix (`C:\Tools\MySQL.EXE` → `mysql`).
pub(super) fn command_name(token: &str) -> String {
    let base = token
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(token)
        .to_ascii_lowercase();
    base.strip_suffix(".exe").unwrap_or(&base).to_string()
}

pub(super) fn is_secret_key(key: &str) -> bool {
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
pub(super) fn strip_url_userinfo(tok: &str) -> Option<String> {
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
pub(super) fn looks_like_secret(tok: &str) -> bool {
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
