//! External target (URL) policy for terminal-controlled links.
//!
//! Before Phase 1, OSC 8 hyperlinks and plain-text URLs were passed directly
//! to `cx.open_url` without scheme validation. A remote terminal could embed
//! a `custom-app://` URI that launches a local application, or a `file://`
//! URI that opens a local file.
//!
//! This module provides a single policy layer that all external-target
//! openings must pass through.

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
    /// (e.g. OSC 8 hyperlinks). If the display text differs from the target,
    /// confirmation is required even for allowed schemes.
    pub fn validate_with_display(&self, target: &str, display: Option<&str>) -> TargetDecision {
        let base = self.validate(target);
        match &base {
            TargetDecision::Allow => {
                if let Some(disp) = display {
                    if !disp.eq_ignore_ascii_case(target) && !disp.trim().is_empty() {
                        return TargetDecision::Confirm(ConfirmReason::DisplayTargetMismatch);
                    }
                }
                TargetDecision::Allow
            }
            _ => base,
        }
    }
}

/// Parsed URL components (minimal, dependency-free).
struct ParsedUrl {
    scheme: String,
    has_credentials: bool,
    port: Option<u16>,
}

/// Minimal URL parser — extracts scheme, credentials flag, and port.
/// Does not depend on the `url` crate to keep core dependency-free.
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
        // Strip IPv6 brackets.
        let host_part = host_part
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .unwrap_or(host_part);
        let port = host_part.rfind(':').and_then(|idx| {
            let port_str = &host_part[idx + 1..];
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

/// Check for C0 control characters (0x00-0x1F, except \t \n \r) and
/// C1 controls (0x80-0x9F), plus RTL/LTR override characters.
fn has_control_chars(s: &str) -> bool {
    s.chars().any(|c| {
        let code = c as u32;
        // C0 controls (0x00-0x1F), except tab (0x09), newline (0x0A), CR (0x0D).
        (code < 0x20 && code != 0x09 && code != 0x0a && code != 0x0d)
        // C1 controls (0x80-0x9F).
        || (0x80..=0x9f).contains(&code)
        // BiDi override characters.
        || code == 0x202e // RIGHT-TO-LEFT OVERRIDE
        || code == 0x202d // LEFT-TO-RIGHT OVERRIDE
        || code == 0x202a // LEFT-TO-RIGHT EMBEDDING
        || code == 0x202b // RIGHT-TO-LEFT EMBEDDING
        || code == 0x200e // LEFT-TO-RIGHT MARK
        || code == 0x200f // RIGHT-TO-LEFT MARK
    })
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
