//! Bracketed paste encoding with ESC sanitization.
//!
//! Before Phase 1, `paste()` used `format!("\x1b[200~{}\x1b[201~", text)` which
//! allowed pasted content containing `\x1b[201~` to terminate bracketed-paste
//! mode early, causing the remainder to be interpreted as keystrokes/commands.
//!
//! The fix strips embedded paste markers and — following alacritty — every
//! remaining ESC (`\x1b`) byte from the payload before wrapping it in the outer
//! bracketed-paste delimiters. Removing ESC itself (not only marker sequences)
//! makes it impossible for nested or overlapping fragments to reassemble into
//! `\x1b[201~` after filtering. `\x03` is also removed because some shells
//! incorrectly terminate bracketed paste on receiving it.

/// Default maximum paste size (1 MiB). Larger pastes are rejected to prevent
/// unbounded allocation from terminal-controlled content.
pub const DEFAULT_MAX_PASTE_BYTES: usize = 1024 * 1024;

/// Policy governing paste behavior.
#[derive(Clone, Debug)]
pub struct PastePolicy {
    /// Maximum number of bytes allowed in a single paste. Zero = unlimited.
    pub max_bytes: usize,
}

impl Default for PastePolicy {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_MAX_PASTE_BYTES,
        }
    }
}

/// Whether pasted text is wrapped in bracketed-paste delimiters.
///
/// Selected from the terminal's `TermMode::BRACKETED_PASTE` state at the call
/// site (see [`crate::TerminalSession::paste`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteMode {
    /// Wrap the payload in `ESC[200~…ESC[201~` and strip embedded markers.
    Bracketed,
    /// Pass the payload through unchanged.
    Plain,
}

/// Outcome of [`encode_paste`].
#[derive(Debug, PartialEq, Eq)]
pub enum PasteResult {
    /// Encoded bytes ready to write to the PTY.
    Ok(Vec<u8>),
    /// The paste exceeded `max_bytes`; the payload size is included.
    TooLarge(usize),
}

/// Encode text for pasting into the terminal.
///
/// - [`PasteMode::Bracketed`]: wraps the text in `ESC[200~…ESC[201~` after
///   stripping embedded markers and every remaining ESC / `\x03` byte.
/// - [`PasteMode::Plain`]: passes the text through as-is.
///
/// Stripping ESC (not just the marker sequences) guarantees the payload cannot
/// contain `ESC[201~` in any form, so pasted content cannot terminate
/// bracketed-paste mode early or re-enter it spuriously.
pub fn encode_paste(text: &str, mode: PasteMode, policy: &PastePolicy) -> PasteResult {
    // Enforce size cap.
    if policy.max_bytes > 0 && text.len() > policy.max_bytes {
        return PasteResult::TooLarge(text.len());
    }

    if mode == PasteMode::Plain {
        return PasteResult::Ok(text.as_bytes().to_vec());
    }

    // Strip ESC / ETX so the payload cannot forge a paste terminator.
    let sanitized = strip_paste_escapes(text);

    let mut result = Vec::with_capacity(sanitized.len() + b"\x1b[200~".len() + b"\x1b[201~".len());
    result.extend_from_slice(b"\x1b[200~");
    result.extend_from_slice(sanitized.as_bytes());
    result.extend_from_slice(b"\x1b[201~");
    PasteResult::Ok(result)
}

/// Remove embedded paste markers, then every remaining ESC (`\x1b`) and ETX
/// (`\x03`) byte from the text.
///
/// Whole `ESC[200~` / `ESC[201~` sequences are dropped in full so pasted text
/// that merely contains a marker loses only the marker. Every other ESC is
/// dropped as well (alacritty's filtering): a marker-only stripper is
/// bypassable by nesting (`ESC[20` + `ESC[201~` + `1~` collapses to `ESC[201~`
/// after one pass), whereas removing ESC itself is provably safe because no
/// ESC can survive into the payload.
fn strip_paste_escapes(text: &str) -> String {
    const START_MARKER: &[u8] = b"\x1b[200~";
    const END_MARKER: &[u8] = b"\x1b[201~";

    let bytes = text.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            0x1b => {
                if bytes[i..].starts_with(START_MARKER) {
                    i += START_MARKER.len();
                } else if bytes[i..].starts_with(END_MARKER) {
                    i += END_MARKER.len();
                } else {
                    i += 1;
                }
            }
            0x03 => i += 1,
            byte => {
                result.push(byte);
                i += 1;
            }
        }
    }

    // Only ASCII bytes were removed from valid UTF-8 input, so the result is
    // still valid UTF-8; the lossy fallback is unreachable in practice.
    String::from_utf8(result)
        .unwrap_or_else(|e| String::from_utf8_lossy(&e.into_bytes()).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bracketed_paste_wraps_text() {
        let result = encode_paste("hello", PasteMode::Bracketed, &PastePolicy::default());
        assert_eq!(result, PasteResult::Ok(b"\x1b[200~hello\x1b[201~".to_vec()));
    }

    #[test]
    fn plain_paste_passes_through() {
        let result = encode_paste("hello", PasteMode::Plain, &PastePolicy::default());
        assert_eq!(result, PasteResult::Ok(b"hello".to_vec()));
    }

    #[test]
    fn empty_text_bracketed() {
        let result = encode_paste("", PasteMode::Bracketed, &PastePolicy::default());
        assert_eq!(result, PasteResult::Ok(b"\x1b[200~\x1b[201~".to_vec()));
    }

    #[test]
    fn empty_text_plain() {
        let result = encode_paste("", PasteMode::Plain, &PastePolicy::default());
        assert_eq!(result, PasteResult::Ok(b"".to_vec()));
    }

    #[test]
    fn embedded_end_marker_is_stripped() {
        let text = "safe\x1b[201~malicious";
        let result = encode_paste(text, PasteMode::Bracketed, &PastePolicy::default());
        assert_eq!(
            result,
            PasteResult::Ok(b"\x1b[200~safemalicious\x1b[201~".to_vec())
        );
    }

    #[test]
    fn embedded_start_marker_is_stripped() {
        let text = "text\x1b[200~more";
        let result = encode_paste(text, PasteMode::Bracketed, &PastePolicy::default());
        assert_eq!(
            result,
            PasteResult::Ok(b"\x1b[200~textmore\x1b[201~".to_vec())
        );
    }

    #[test]
    fn multiple_embedded_markers_stripped() {
        let text = "a\x1b[201~b\x1b[200~c\x1b[201~d";
        let result = encode_paste(text, PasteMode::Bracketed, &PastePolicy::default());
        assert_eq!(result, PasteResult::Ok(b"\x1b[200~abcd\x1b[201~".to_vec()));
    }

    #[test]
    fn nested_end_marker_cannot_reassemble() {
        // SEC-01: a single-pass marker stripper would turn this into a bare
        // `ESC[201~` (strip the inner marker, the halves join). ESC removal
        // leaves no way to reassemble a terminator.
        let text = "\x1b[20\x1b[201~1~";
        let result = encode_paste(text, PasteMode::Bracketed, &PastePolicy::default());
        let PasteResult::Ok(bytes) = result else {
            panic!("expected Ok");
        };
        assert_eq!(bytes, b"\x1b[200~[201~\x1b[201~".to_vec());
        let payload = &bytes[b"\x1b[200~".len()..bytes.len() - b"\x1b[201~".len()];
        assert!(!payload.contains(&0x1b), "payload must not contain ESC");
    }

    #[test]
    fn partial_marker_loses_esc() {
        // Even a partial marker has its ESC removed, since any ESC could be
        // combined with later bytes to form a terminator.
        let text = "text\x1b[201x";
        let result = encode_paste(text, PasteMode::Bracketed, &PastePolicy::default());
        assert_eq!(
            result,
            PasteResult::Ok(b"\x1b[200~text[201x\x1b[201~".to_vec())
        );
    }

    #[test]
    fn etx_is_stripped_in_bracketed_mode() {
        let text = "a\x03b";
        let result = encode_paste(text, PasteMode::Bracketed, &PastePolicy::default());
        assert_eq!(result, PasteResult::Ok(b"\x1b[200~ab\x1b[201~".to_vec()));
    }

    #[test]
    fn nul_and_control_chars_preserved() {
        let text = "nul:\0ctrl:\u{0001}bell:\u{0007}";
        let result = encode_paste(text, PasteMode::Bracketed, &PastePolicy::default());
        assert_eq!(
            result,
            PasteResult::Ok(b"\x1b[200~nul:\0ctrl:\x01bell:\x07\x1b[201~".to_vec())
        );
    }

    #[test]
    fn multiline_text_preserved() {
        let text = "line one\nline two\r\nline three";
        let result = encode_paste(text, PasteMode::Bracketed, &PastePolicy::default());
        assert_eq!(
            result,
            PasteResult::Ok(b"\x1b[200~line one\nline two\r\nline three\x1b[201~".to_vec())
        );
    }

    #[test]
    fn unicode_preserved() {
        let text = "Héllo, 世界 🎉";
        let result = encode_paste(text, PasteMode::Bracketed, &PastePolicy::default());
        assert_eq!(
            result,
            PasteResult::Ok(format!("\x1b[200~{text}\x1b[201~").into_bytes())
        );
    }

    #[test]
    fn large_text_within_limit() {
        let large = "x".repeat(1024 * 1024);
        let result = encode_paste(&large, PasteMode::Bracketed, &PastePolicy::default());
        match result {
            PasteResult::Ok(bytes) => {
                assert_eq!(bytes.len(), large.len() + 12);
            }
            _ => panic!("expected Ok for 1MB paste"),
        }
    }

    #[test]
    fn text_exceeding_max_is_rejected() {
        let policy = PastePolicy { max_bytes: 100 };
        let large = "x".repeat(200);
        let result = encode_paste(&large, PasteMode::Bracketed, &policy);
        assert_eq!(result, PasteResult::TooLarge(200));
    }

    #[test]
    fn zero_max_means_unlimited() {
        let policy = PastePolicy { max_bytes: 0 };
        let large = "x".repeat(1024 * 1024 + 1);
        let result = encode_paste(&large, PasteMode::Plain, &policy);
        assert!(matches!(result, PasteResult::Ok(_)));
    }

    #[test]
    fn plain_mode_does_not_strip_markers() {
        // In plain mode, markers are left in place — the terminal is not in
        // bracketed paste mode, so ESC sequences are interpreted normally.
        let text = "text\x1b[201~more";
        let result = encode_paste(text, PasteMode::Plain, &PastePolicy::default());
        assert_eq!(result, PasteResult::Ok(text.as_bytes().to_vec()));
    }

    #[test]
    fn esc_byte_without_marker_is_stripped() {
        // Every ESC is removed in bracketed mode (alacritty behaviour); the
        // remaining bytes are passed through unchanged.
        let text = "text\x1b[0mcolor";
        let result = encode_paste(text, PasteMode::Bracketed, &PastePolicy::default());
        assert_eq!(
            result,
            PasteResult::Ok(b"\x1b[200~text[0mcolor\x1b[201~".to_vec())
        );
    }
}
