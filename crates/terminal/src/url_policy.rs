//! External target (URL) policy for terminal-controlled links.
//!
//! OSC 8 hyperlinks and plain-text URLs are terminal-controlled: a remote
//! program could embed a `custom-app://` URI that launches a local application,
//! a `file://` URI that opens a local file, or an OSC 8 link whose visible text
//! names one site while the target points at another. This module is the
//! single policy layer that every external-target opening must pass through.

/// Default maximum URL length (256 KiB).
const DEFAULT_MAX_URL_LEN: usize = 256 * 1024;

/// Outcome of [`ExternalTargetPolicy::validate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetDecision {
    /// The target is safe to open directly.
    Allow,
    /// The target is denied; the reason explains why.
    Deny(DenyReason),
    /// The target should be shown to the user for confirmation before opening.
    Confirm(ConfirmReason),
}

/// Why a target was denied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DenyReason {
    /// The scheme is not in the allowlist.
    SchemeNotAllowed(String),
    /// The target contains C0/C1 control characters.
    ControlCharacters,
    /// The target exceeds the maximum length.
    TooLong(usize),
    /// The target is not a valid URL.
    InvalidUrl(String),
    /// The target contains credentials in the authority section.
    HasCredentials,
}

/// Why a target requires confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmReason {
    /// The display text differs from the actual target URI.
    DisplayTargetMismatch,
    /// The scheme is allowed but requires confirmation (e.g. http).
    RequiresConfirmation,
    /// Non-default port.
    NonDefaultPort,
}

/// Policy governing which external targets may be opened.
#[derive(Clone, Debug)]
pub struct ExternalTargetPolicy {
    /// Schemes that are opened without confirmation.
    pub allowed_schemes: Vec<String>,
    /// Schemes that are opened after user confirmation.
    pub confirm_schemes: Vec<String>,
    /// Maximum URL length in bytes.
    pub max_url_len: usize,
}

impl Default for ExternalTargetPolicy {
    fn default() -> Self {
        Self {
            allowed_schemes: vec!["https".to_string()],
            confirm_schemes: vec!["http".to_string()],
            max_url_len: DEFAULT_MAX_URL_LEN,
        }
    }
}

impl ExternalTargetPolicy {
    /// Validate a target URI for opening.
    ///
    /// Returns `Allow`, `Deny(reason)`, or `Confirm(reason)`.
    pub fn validate(&self, target: &str) -> TargetDecision {
        // Length check.
        if target.len() > self.max_url_len {
            return TargetDecision::Deny(DenyReason::TooLong(target.len()));
        }

        // Control character check — reject C0 (except tab/newline in path) and C1.
        if has_control_chars(target) {
            return TargetDecision::Deny(DenyReason::ControlCharacters);
        }

        // Parse the URL.
        let parsed = match parse_url(target) {
            Some(p) => p,
            None => return TargetDecision::Deny(DenyReason::InvalidUrl(target.to_string())),
        };

        // Scheme check (case-insensitive).
        let scheme_lower = parsed.scheme.to_lowercase();
        if self
            .allowed_schemes
            .iter()
            .any(|s| s.eq_ignore_ascii_case(&scheme_lower))
        {
            // Check for credentials in authority.
            if parsed.has_credentials {
                return TargetDecision::Deny(DenyReason::HasCredentials);
            }
            // Non-default port requires confirmation.
            if let Some(port) = parsed.port {
                let default = default_port(&scheme_lower);
                if port != default {
                    return TargetDecision::Confirm(ConfirmReason::NonDefaultPort);
                }
            }
            return TargetDecision::Allow;
        }

        if self
            .confirm_schemes
            .iter()
            .any(|s| s.eq_ignore_ascii_case(&scheme_lower))
        {
            if parsed.has_credentials {
                return TargetDecision::Deny(DenyReason::HasCredentials);
            }
            return TargetDecision::Confirm(ConfirmReason::RequiresConfirmation);
        }

        TargetDecision::Deny(DenyReason::SchemeNotAllowed(parsed.scheme))
    }

    /// Validate a target where the display text may differ from the actual URI
    /// (OSC 8 hyperlinks: the visible cell text is chosen by the program).
    ///
    /// Descriptive text (`click here`, `release notes`) is not suspicious.
    /// Text that *looks like a URL or host* is compared against the target
    /// with [`display_matches_target`]; when it names a different host the
    /// decision becomes `Confirm(DisplayTargetMismatch)` even for allowed
    /// schemes, so `https://good.com` shown over `https://evil.com` never
    /// opens silently.
    pub fn validate_with_display(&self, target: &str, display: Option<&str>) -> TargetDecision {
        let base = self.validate(target);
        match &base {
            TargetDecision::Allow => match display {
                Some(display) if !display_matches_target(display, target) => {
                    TargetDecision::Confirm(ConfirmReason::DisplayTargetMismatch)
                }
                _ => TargetDecision::Allow,
            },
            _ => base,
        }
    }
}

/// Whether OSC 8 display text is consistent with the link target.
///
/// Returns `true` (no mismatch) when the display text is empty, is not
/// URL-shaped (plain words), or is URL-shaped and names the same host as the
/// target — with or without a scheme, `www.`, path, or trailing slash. Returns
/// `false` when the display text is URL-shaped and its host differs from the
/// target host, which is the classic phishing pattern.
pub fn display_matches_target(display: &str, target: &str) -> bool {
    let display = display.trim();
    if display.is_empty() || display.eq_ignore_ascii_case(target.trim()) {
        return true;
    }
    let Some(display_host) = url_like_host(display) else {
        // Descriptive text: nothing to compare against.
        return true;
    };
    match url_like_host(target) {
        Some(target_host) => display_host == target_host,
        // A target without a recognisable host (mailto:, data:) shown as a
        // URL-looking label is a mismatch.
        None => false,
    }
}

/// The lower-cased host of a URL-shaped string, without a leading `www.` and
/// without a port. `None` when the text is not URL-shaped: no `scheme://`
/// prefix and no `host.tld` first segment.
fn url_like_host(text: &str) -> Option<String> {
    let text = text.trim();
    let after_scheme = match text.find("://") {
        Some(idx) => {
            let scheme = &text[..idx];
            let scheme_ok = scheme
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.');
            if !scheme_ok {
                return None;
            }
            &text[idx + 3..]
        }
        None => {
            // `mailto:x@y`, `data:...`: a scheme without an authority has no
            // host. Distinguish it from `host:port` by the dot-less scheme.
            if let Some((scheme, rest)) = text.split_once(':') {
                let opaque_scheme = scheme
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic())
                    && scheme
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-')
                    && !rest.starts_with('/')
                    && !rest.split(['/', '?', '#']).next().is_some_and(|port| {
                        !port.is_empty() && port.chars().all(|c| c.is_ascii_digit())
                    });
                if opaque_scheme {
                    return None;
                }
            }
            text
        }
    };
    let authority_end = after_scheme
        .find(|c| c == '/' || c == '?' || c == '#')
        .unwrap_or(after_scheme.len());
    let authority = &after_scheme[..authority_end];
    // Drop userinfo.
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    let host = if let Some(rest) = host_port.strip_prefix('[') {
        // IPv6 literal.
        rest.split(']').next().unwrap_or(rest)
    } else {
        host_port
            .rsplit_once(':')
            .map_or(host_port, |(host, port)| {
                if port.chars().all(|c| c.is_ascii_digit()) {
                    host
                } else {
                    host_port
                }
            })
    };
    let host = host.trim_end_matches('.').to_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
    // A bare word without a dot (and no scheme) is not URL-shaped; a host must
    // contain only host characters.
    let has_scheme = text.contains("://");
    if host.is_empty()
        || (!has_scheme && !host.contains('.'))
        || host
            .chars()
            .any(|c| c.is_whitespace() || c == '/' || c == '\\' || c == '"' || c == '\'')
    {
        return None;
    }
    Some(host)
}

/// Parsed URL components (minimal, dependency-free).
struct ParsedUrl {
    scheme: String,
    has_credentials: bool,
    port: Option<u16>,
}

/// Minimal URL parser — extracts scheme, credentials flag, and port.
/// Deliberately hand-written so this engine crate does not carry the `url`
/// crate for three fields.
fn parse_url(input: &str) -> Option<ParsedUrl> {
    // Find scheme: must be alpha+digits+'+'+'-'+'.' followed by ':'
    let colon = input.find(':')?;
    let scheme = &input[..colon];
    if scheme.is_empty() {
        return None;
    }
    // Scheme must start with a letter and contain only allowed chars.
    let chars = scheme.chars();
    let mut iter = chars.clone();
    if !iter
        .next()
        .map(|c| c.is_ascii_alphabetic())
        .unwrap_or(false)
    {
        return None;
    }
    if !scheme
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
    {
        return None;
    }

    let after_scheme = &input[colon + 1..];

    // Check for authority section (//...).
    let (has_credentials, port) = if let Some(auth) = after_scheme.strip_prefix("//") {
        // Find end of authority (first /, ?, or #).
        let auth_end = auth
            .find(|c| c == '/' || c == '?' || c == '#')
            .unwrap_or(auth.len());
        let authority = &auth[..auth_end];

        // Check for credentials: user:pass@host
        let has_credentials = authority.contains('@');

        // Extract port from the last segment after the last @.
        let host_part = authority.rsplit('@').next().unwrap_or(authority);
        // For an IPv6 literal (`[::1]:8443`) the port can only follow the
        // closing bracket; the colons inside the brackets are not a port
        // separator (CORR-45).
        let port_source = if host_part.starts_with('[') {
            host_part
                .split_once(']')
                .map(|(_, rest)| rest)
                .unwrap_or("")
        } else {
            host_part
        };
        let port = port_source.rfind(':').and_then(|idx| {
            let port_str = &port_source[idx + 1..];
            if port_str.is_empty() {
                None
            } else {
                // Validate: only digits.
                if port_str.chars().all(|c| c.is_ascii_digit()) {
                    port_str.parse::<u16>().ok()
                } else {
                    None
                }
            }
        });

        (has_credentials, port)
    } else {
        (false, None)
    };

    Some(ParsedUrl {
        scheme: scheme.to_string(),
        has_credentials,
        port,
    })
}

/// Check for C0 control characters (0x00-0x1F, including `\t`, `\n` and
/// `\r` — a URL never legitimately contains them, SEC-07), DEL, C1 controls
/// (0x80-0x9F), and BiDi override/embedding/isolate/mark characters that can
/// visually reorder the address.
fn has_control_chars(s: &str) -> bool {
    s.chars().any(|c| {
        let code = c as u32;
        code < 0x20 || code == 0x7f || (0x80..=0x9f).contains(&code) || is_bidi_control(c)
    })
}

/// BiDi override, embedding, isolate and mark code points (U+202A..U+202E,
/// U+2066..U+2069, U+200E/U+200F).
pub(crate) fn is_bidi_control(c: char) -> bool {
    matches!(
        c,
        '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200e}' | '\u{200f}'
    )
}

/// Default port for a scheme.
fn default_port(scheme: &str) -> u16 {
    match scheme {
        "https" => 443,
        "http" => 80,
        "ftp" => 21,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_allowed_by_default() {
        let policy = ExternalTargetPolicy::default();
        assert_eq!(
            policy.validate("https://example.com/path"),
            TargetDecision::Allow
        );
    }

    #[test]
    fn http_requires_confirmation_by_default() {
        let policy = ExternalTargetPolicy::default();
        assert_eq!(
            policy.validate("http://example.com/path"),
            TargetDecision::Confirm(ConfirmReason::RequiresConfirmation)
        );
    }

    #[test]
    fn custom_scheme_denied() {
        let policy = ExternalTargetPolicy::default();
        assert_eq!(
            policy.validate("custom-app://run?action=delete"),
            TargetDecision::Deny(DenyReason::SchemeNotAllowed("custom-app".to_string()))
        );
    }

    #[test]
    fn file_scheme_denied() {
        let policy = ExternalTargetPolicy::default();
        assert_eq!(
            policy.validate("file:///C:/Windows/System32/cmd.exe"),
            TargetDecision::Deny(DenyReason::SchemeNotAllowed("file".to_string()))
        );
    }

    #[test]
    fn ssh_scheme_denied() {
        let policy = ExternalTargetPolicy::default();
        assert_eq!(
            policy.validate("ssh://user@host/path"),
            TargetDecision::Deny(DenyReason::SchemeNotAllowed("ssh".to_string()))
        );
    }

    #[test]
    fn mixed_case_scheme_allowed() {
        let policy = ExternalTargetPolicy::default();
        assert_eq!(
            policy.validate("HtTpS://Example.COM/Path"),
            TargetDecision::Allow
        );
    }

    #[test]
    fn unicode_host_allowed() {
        let policy = ExternalTargetPolicy::default();
        assert_eq!(
            policy.validate("https://例え.テスト/path"),
            TargetDecision::Allow
        );
    }

    #[test]
    fn credentials_denied() {
        let policy = ExternalTargetPolicy::default();
        assert_eq!(
            policy.validate("https://user:secret@example.com/private"),
            TargetDecision::Deny(DenyReason::HasCredentials)
        );
    }

    #[test]
    fn control_characters_denied() {
        let policy = ExternalTargetPolicy::default();
        assert_eq!(
            policy.validate("https://example.com/\u{0007}control"),
            TargetDecision::Deny(DenyReason::ControlCharacters)
        );
    }

    #[test]
    fn oversized_url_denied() {
        let policy = ExternalTargetPolicy {
            max_url_len: 100,
            ..Default::default()
        };
        let oversized = format!("https://example.com/{}", "x".repeat(200));
        assert_eq!(
            policy.validate(&oversized),
            TargetDecision::Deny(DenyReason::TooLong(oversized.len()))
        );
    }

    #[test]
    fn non_default_port_requires_confirmation() {
        let policy = ExternalTargetPolicy::default();
        assert_eq!(
            policy.validate("https://example.com:8443/path"),
            TargetDecision::Confirm(ConfirmReason::NonDefaultPort)
        );
    }

    #[test]
    fn default_port_allowed() {
        let policy = ExternalTargetPolicy::default();
        assert_eq!(
            policy.validate("https://example.com:443/path"),
            TargetDecision::Allow
        );
    }

    #[test]
    fn display_target_mismatch_requires_confirmation() {
        let policy = ExternalTargetPolicy::default();
        assert_eq!(
            policy.validate_with_display("https://evil.com/steal", Some("https://google.com")),
            TargetDecision::Confirm(ConfirmReason::DisplayTargetMismatch)
        );
    }

    #[test]
    fn display_matches_target_allowed() {
        let policy = ExternalTargetPolicy::default();
        assert_eq!(
            policy.validate_with_display("https://example.com", Some("https://example.com")),
            TargetDecision::Allow
        );
    }

    /// SEC-03: descriptive labels and same-host spellings are not mismatches;
    /// a URL-looking label naming another host is.
    #[test]
    fn display_text_comparison_is_host_based() {
        for (display, target) in [
            ("click here", "https://example.com/x"),
            ("Release notes", "https://example.com/x"),
            ("", "https://example.com/x"),
            ("example.com", "https://example.com/x"),
            ("EXAMPLE.com/path", "https://example.com/other"),
            ("www.example.com", "https://example.com"),
            ("https://example.com:443/", "https://example.com/"),
            ("[::1]/", "https://[::1]:8443/"),
            ("https://user@example.com", "https://example.com"),
        ] {
            assert!(
                display_matches_target(display, target),
                "{display:?} vs {target:?}"
            );
        }
        for (display, target) in [
            ("https://good.com", "https://evil.com"),
            ("good.com/login", "https://evil.com/login"),
            ("www.good.com", "https://evil.com"),
            ("https://example.com", "mailto:someone@example.com"),
        ] {
            assert!(
                !display_matches_target(display, target),
                "{display:?} vs {target:?}"
            );
        }
        let policy = ExternalTargetPolicy::default();
        assert_eq!(
            policy.validate_with_display("https://example.com/x", Some("click here")),
            TargetDecision::Allow
        );
        assert_eq!(
            policy.validate_with_display("https://evil.com/login", Some("good.com/login")),
            TargetDecision::Confirm(ConfirmReason::DisplayTargetMismatch)
        );
    }

    /// CORR-45: an IPv6 host without a port is not a non-default port.
    #[test]
    fn ipv6_host_port_detection() {
        let policy = ExternalTargetPolicy::default();
        assert_eq!(policy.validate("https://[::1]/"), TargetDecision::Allow);
        assert_eq!(
            policy.validate("https://[2001:db8::1]:443/path"),
            TargetDecision::Allow
        );
        assert_eq!(
            policy.validate("https://[::1]:8443/"),
            TargetDecision::Confirm(ConfirmReason::NonDefaultPort)
        );
    }

    /// SEC-07: newline, carriage return, tab and BiDi isolates are rejected.
    #[test]
    fn line_breaks_tabs_and_bidi_isolates_denied() {
        let policy = ExternalTargetPolicy::default();
        for url in [
            "https://example.com/a\nb",
            "https://example.com/a\rb",
            "https://example.com/a\tb",
            "https://example.com/\u{2066}evil\u{2069}",
            "https://example.com/\u{7f}",
        ] {
            assert_eq!(
                policy.validate(url),
                TargetDecision::Deny(DenyReason::ControlCharacters),
                "{url:?}"
            );
        }
    }

    #[test]
    fn invalid_url_denied() {
        let policy = ExternalTargetPolicy::default();
        // No scheme.
        assert_eq!(
            policy.validate("not a url"),
            TargetDecision::Deny(DenyReason::InvalidUrl("not a url".to_string()))
        );
    }

    #[test]
    fn rtl_override_denied() {
        let policy = ExternalTargetPolicy::default();
        assert_eq!(
            policy.validate("https://example.com/\u{202e}evil"),
            TargetDecision::Deny(DenyReason::ControlCharacters)
        );
    }

    #[test]
    fn valid_wrapped_https_allowed() {
        // A long valid HTTPS URL that wraps across terminal lines.
        let policy = ExternalTargetPolicy::default();
        let url = "https://example.com/very/long/path/that/wraps/across/terminal/lines?q=1&r=2";
        assert_eq!(policy.validate(url), TargetDecision::Allow);
    }

    #[test]
    fn www_url_prepend_https() {
        // The UI prepends https:// to www. URLs before calling validate.
        let policy = ExternalTargetPolicy::default();
        assert_eq!(
            policy.validate("https://www.google.com"),
            TargetDecision::Allow
        );
    }
}
