//! Terminal security policy — bounds and sanitizes terminal-controlled data
//! before it reaches persistent state or the OS.
//!
//! Without a central policy a local or remote program could set arbitrarily
//! long tab titles with control characters, flood notifications, overwrite the
//! clipboard at any time, or store unlimited cwd/URI strings. This module
//! provides a single `TerminalSecurityPolicy` with explicit defaults that all
//! terminal-controlled data must pass through.

use std::time::{Duration, Instant};

/// Where a clipboard operation originated.
///
/// Remote (SSH) sessions are treated more strictly than the local shell:
/// remote clipboard access exposes the local clipboard to a program running on
/// another host, so it is gated by dedicated policy flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardOrigin {
    /// The local shell running on this machine.
    Local,
    /// A remote (SSH) session.
    Remote,
}

impl ClipboardOrigin {
    /// Whether this origin is a remote (SSH) session.
    const fn is_remote(self) -> bool {
        matches!(self, Self::Remote)
    }
}

/// Default maximum title length in bytes (4 KiB).
const DEFAULT_MAX_TITLE_BYTES: usize = 4 * 1024;

/// Default maximum notification length in bytes (8 KiB).
const DEFAULT_MAX_NOTIFICATION_BYTES: usize = 8 * 1024;

/// Default maximum clipboard payload length in bytes (256 KiB).
const DEFAULT_MAX_CLIPBOARD_BYTES: usize = 256 * 1024;

/// Default maximum cwd/URI length in bytes (8 KiB).
const DEFAULT_MAX_CWD_BYTES: usize = 8 * 1024;

/// Default maximum notification rate (10 per second).
const DEFAULT_NOTIFICATION_RATE_PER_SEC: u32 = 10;

/// Default maximum queued notifications (5; older are coalesced/dropped).
const DEFAULT_MAX_QUEUED_NOTIFICATIONS: usize = 5;

/// Policy governing terminal-controlled side effects.
#[derive(Clone, Debug)]
pub struct TerminalSecurityPolicy {
    /// Maximum bytes for a terminal title (OSC 0/2).
    pub max_title_bytes: usize,
    /// Maximum bytes for a notification message (OSC 9/777).
    pub max_notification_bytes: usize,
    /// Maximum bytes for a clipboard write (OSC 52 set).
    pub max_clipboard_bytes: usize,
    /// Maximum bytes for a cwd/URI (OSC 7).
    pub max_cwd_bytes: usize,
    /// Maximum notifications per second before rate limiting kicks in.
    pub notification_rate_per_sec: u32,
    /// Maximum notifications queued for display (older are dropped).
    pub max_queued_notifications: usize,
    /// Whether remote (SSH) clipboard writes are allowed.
    pub allow_remote_clipboard_write: bool,
    /// Whether remote (SSH) clipboard reads are allowed.
    pub allow_remote_clipboard_read: bool,
}

impl Default for TerminalSecurityPolicy {
    fn default() -> Self {
        Self {
            max_title_bytes: DEFAULT_MAX_TITLE_BYTES,
            max_notification_bytes: DEFAULT_MAX_NOTIFICATION_BYTES,
            max_clipboard_bytes: DEFAULT_MAX_CLIPBOARD_BYTES,
            max_cwd_bytes: DEFAULT_MAX_CWD_BYTES,
            notification_rate_per_sec: DEFAULT_NOTIFICATION_RATE_PER_SEC,
            max_queued_notifications: DEFAULT_MAX_QUEUED_NOTIFICATIONS,
            // Remote clipboard operations are default-off for security.
            allow_remote_clipboard_write: false,
            allow_remote_clipboard_read: false,
        }
    }
}

/// Bounded one-second notification limiter shared by a terminal listener's clones.
#[derive(Debug, Default)]
pub struct NotificationRateLimiter {
    window_start: Option<Instant>,
    accepted: u32,
}

impl NotificationRateLimiter {
    /// Return whether one more notification may enter the session event stream.
    pub fn allow(&mut self, policy: &TerminalSecurityPolicy) -> bool {
        self.allow_at(policy, Instant::now())
    }

    /// Testable form of [`Self::allow`].
    pub fn allow_at(&mut self, policy: &TerminalSecurityPolicy, now: Instant) -> bool {
        if policy.notification_rate_per_sec == 0 {
            return false;
        }

        let reset = self
            .window_start
            .is_none_or(|started| now.duration_since(started) >= Duration::from_secs(1));
        if reset {
            self.window_start = Some(now);
            self.accepted = 0;
        }

        if self.accepted >= policy.notification_rate_per_sec {
            return false;
        }
        self.accepted += 1;
        true
    }
}

impl TerminalSecurityPolicy {
    /// Sanitize a terminal-controlled title string.
    ///
    /// - Removes C0 control characters (except tab and newline)
    /// - Removes C1 control characters
    /// - Removes BiDi override/embedding characters
    /// - Truncates to `max_title_bytes` at a UTF-8 boundary
    /// - Returns `None` if the result is empty
    pub fn sanitize_title(&self, title: &str) -> Option<String> {
        let cleaned = strip_unsafe_chars(title);
        let truncated = truncate_utf8(&cleaned, self.max_title_bytes);
        if truncated.is_empty() {
            None
        } else {
            Some(truncated)
        }
    }

    /// Sanitize a notification message.
    ///
    /// - Removes C0/C1 controls and BiDi overrides
    /// - Truncates to `max_notification_bytes`
    /// - Returns `None` if the result is empty
    pub fn sanitize_notification(&self, msg: &str) -> Option<String> {
        let cleaned = strip_unsafe_chars(msg);
        let truncated = truncate_utf8(&cleaned, self.max_notification_bytes);
        if truncated.is_empty() {
            None
        } else {
            Some(truncated)
        }
    }

    /// Validate a clipboard write payload.
    ///
    /// - Truncates to `max_clipboard_bytes`
    /// - Returns `None` if it exceeds the limit (reject, don't truncate
    ///   security-sensitive data)
    pub fn validate_clipboard_write<'a>(
        &self,
        text: &'a str,
        origin: ClipboardOrigin,
    ) -> Option<&'a str> {
        if origin.is_remote() && !self.allow_remote_clipboard_write {
            return None;
        }
        if text.len() > self.max_clipboard_bytes {
            return None;
        }
        Some(text)
    }

    /// Check whether a clipboard read is allowed.
    pub fn allow_clipboard_read(&self, origin: ClipboardOrigin) -> bool {
        if origin.is_remote() {
            self.allow_remote_clipboard_read
        } else {
            true
        }
    }

    /// Sanitize a cwd path from OSC 7.
    ///
    /// - Removes every C0/C1 control (including `\n`, `\r`, `\t` — a path
    ///   never legitimately contains a line break, SEC-07) and BiDi controls
    /// - Truncates to `max_cwd_bytes`
    pub fn sanitize_cwd(&self, cwd: &str) -> Option<String> {
        let cleaned = strip_control_chars(cwd);
        let truncated = truncate_utf8(&cleaned, self.max_cwd_bytes);
        if truncated.is_empty() {
            None
        } else {
            Some(truncated)
        }
    }
}

/// Remove C0 control characters (except `\t` `\n` `\r`), DEL, C1 controls,
/// and BiDi override/embedding/isolate/mark characters from a string.
pub(crate) fn strip_unsafe_chars(s: &str) -> String {
    s.chars()
        .filter(|&c| matches!(c, '\t' | '\n' | '\r') || !is_unsafe_char(c))
        .collect()
}

/// Like [`strip_unsafe_chars`] but also removes `\t`, `\n` and `\r` — for
/// values that are single-line by construction (cwd paths, URIs).
pub(crate) fn strip_control_chars(s: &str) -> String {
    s.chars().filter(|&c| !is_unsafe_char(c)).collect()
}

/// C0 (including tab/newline/CR), DEL, C1, or BiDi control character.
fn is_unsafe_char(c: char) -> bool {
    let code = c as u32;
    code < 0x20
        || code == 0x7f
        || (0x80..=0x9f).contains(&code)
        || crate::url_policy::is_bidi_control(c)
}

/// Truncate a string to at most `max_bytes` at a UTF-8 character boundary.
pub(crate) fn truncate_utf8(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    // Find the largest valid UTF-8 boundary <= max_bytes.
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_strips_control_chars() {
        let policy = TerminalSecurityPolicy::default();
        let result = policy
            .sanitize_title("hello\u{0007}world\u{0001}!")
            .unwrap();
        assert_eq!(result, "helloworld!");
    }

    #[test]
    fn title_strips_bidi_overrides() {
        let policy = TerminalSecurityPolicy::default();
        let result = policy
            .sanitize_title("hello\u{202e}evil\u{202d}world")
            .unwrap();
        assert_eq!(result, "helloevilworld");
    }

    #[test]
    fn title_truncates_at_utf8_boundary() {
        let policy = TerminalSecurityPolicy {
            max_title_bytes: 5,
            ..Default::default()
        };
        // "héllo" = 6 bytes (é is 2 bytes); should truncate to "héll" (5 bytes).
        let result = policy.sanitize_title("héllo").unwrap();
        assert_eq!(result, "héll");
    }

    #[test]
    fn title_empty_returns_none() {
        let policy = TerminalSecurityPolicy::default();
        assert_eq!(policy.sanitize_title(""), None);
        assert_eq!(policy.sanitize_title("\u{0001}\u{0002}"), None);
    }

    #[test]
    fn title_preserves_tab_and_newline() {
        let policy = TerminalSecurityPolicy::default();
        let result = policy.sanitize_title("line1\tcol\nline2").unwrap();
        assert_eq!(result, "line1\tcol\nline2");
    }

    #[test]
    fn title_preserves_unicode() {
        let policy = TerminalSecurityPolicy::default();
        let result = policy.sanitize_title("Héllo, 世界 🎉").unwrap();
        assert_eq!(result, "Héllo, 世界 🎉");
    }

    #[test]
    fn title_c1_controls_stripped() {
        let policy = TerminalSecurityPolicy::default();
        // C1 control: 0x9b (CSI)
        let result = policy.sanitize_title("text\u{009b}more").unwrap();
        assert_eq!(result, "textmore");
    }

    #[test]
    fn notification_strips_controls_and_truncates() {
        let policy = TerminalSecurityPolicy {
            max_notification_bytes: 10,
            ..Default::default()
        };
        let result = policy
            .sanitize_notification("hello\u{0007}world this is long")
            .unwrap();
        assert_eq!(result, "helloworld");
    }

    #[test]
    fn notification_empty_returns_none() {
        let policy = TerminalSecurityPolicy::default();
        assert_eq!(policy.sanitize_notification(""), None);
    }

    #[test]
    fn clipboard_write_local_allowed() {
        let policy = TerminalSecurityPolicy::default();
        assert_eq!(
            policy.validate_clipboard_write("hello", ClipboardOrigin::Local),
            Some("hello")
        );
    }

    #[test]
    fn clipboard_write_remote_denied_by_default() {
        let policy = TerminalSecurityPolicy::default();
        assert_eq!(
            policy.validate_clipboard_write("hello", ClipboardOrigin::Remote),
            None
        );
    }

    #[test]
    fn clipboard_write_remote_allowed_when_enabled() {
        let policy = TerminalSecurityPolicy {
            allow_remote_clipboard_write: true,
            ..Default::default()
        };
        assert_eq!(
            policy.validate_clipboard_write("hello", ClipboardOrigin::Remote),
            Some("hello")
        );
    }

    #[test]
    fn clipboard_write_oversized_denied() {
        let policy = TerminalSecurityPolicy {
            max_clipboard_bytes: 10,
            ..Default::default()
        };
        let large = "x".repeat(100);
        assert_eq!(
            policy.validate_clipboard_write(&large, ClipboardOrigin::Local),
            None
        );
    }

    #[test]
    fn clipboard_read_local_allowed() {
        let policy = TerminalSecurityPolicy::default();
        assert!(policy.allow_clipboard_read(ClipboardOrigin::Local));
    }

    #[test]
    fn clipboard_read_remote_denied_by_default() {
        let policy = TerminalSecurityPolicy::default();
        assert!(!policy.allow_clipboard_read(ClipboardOrigin::Remote));
    }

    #[test]
    fn cwd_strips_controls_and_truncates() {
        let policy = TerminalSecurityPolicy {
            max_cwd_bytes: 10,
            ..Default::default()
        };
        let result = policy.sanitize_cwd("/home/user\u{0007}/dir").unwrap();
        assert_eq!(result, "/home/user");
    }

    /// SEC-07: line breaks and tabs never survive in a cwd; BiDi isolates are
    /// stripped from every terminal-controlled string.
    #[test]
    fn cwd_rejects_line_breaks_and_bidi_isolates() {
        let policy = TerminalSecurityPolicy::default();
        assert_eq!(
            policy.sanitize_cwd("/home/us\ner\r/di\tr").unwrap(),
            "/home/user/dir"
        );
        assert_eq!(
            policy.sanitize_cwd("/home/\u{2066}user\u{2069}").unwrap(),
            "/home/user"
        );
        assert_eq!(policy.sanitize_title("a\u{2067}b\u{2068}c").unwrap(), "abc");
        // Titles and notifications may still span lines.
        assert_eq!(
            policy.sanitize_title("line1\nline2").unwrap(),
            "line1\nline2"
        );
    }

    #[test]
    fn large_title_within_limit_preserved() {
        let policy = TerminalSecurityPolicy::default();
        let title = "x".repeat(4000);
        let result = policy.sanitize_title(&title).unwrap();
        assert_eq!(result.len(), 4000);
    }

    #[test]
    fn large_title_over_limit_truncated() {
        let policy = TerminalSecurityPolicy::default();
        let title = "x".repeat(5000);
        let result = policy.sanitize_title(&title).unwrap();
        assert_eq!(result.len(), DEFAULT_MAX_TITLE_BYTES);
    }
}

#[cfg(test)]
mod notification_limiter_tests {
    use super::*;

    #[test]
    fn notification_rate_is_bounded_and_resets() {
        let policy = TerminalSecurityPolicy {
            notification_rate_per_sec: 2,
            ..Default::default()
        };
        let start = Instant::now();
        let mut limiter = NotificationRateLimiter::default();

        assert!(limiter.allow_at(&policy, start));
        assert!(limiter.allow_at(&policy, start + Duration::from_millis(10)));
        assert!(!limiter.allow_at(&policy, start + Duration::from_millis(20)));
        assert!(limiter.allow_at(&policy, start + Duration::from_secs(1)));
    }

    #[test]
    fn zero_rate_rejects_every_notification() {
        let policy = TerminalSecurityPolicy {
            notification_rate_per_sec: 0,
            ..Default::default()
        };
        let mut limiter = NotificationRateLimiter::default();
        assert!(!limiter.allow_at(&policy, Instant::now()));
    }
}
