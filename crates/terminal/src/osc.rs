//! The OSC work that is OneTerm's rather than a terminal's.
//!
//! The engine parses every standard OSC number itself and hands the result over
//! as a typed event ([`oneterm_vt::VtEvent::Cwd`], `Notification`, `Progress`,
//! `ShellMark`, `Title`, `ClipboardStore`, ...), so there is no second parser
//! here and no OSC number matched by spelling. What is left is the OSC 52 reply
//! encoder, because a terminal core never writes a clipboard. OSC 7501 program
//! status lives in [`crate::program_status`].
//!
//! Policy — what may be shown, pasted, followed or how often — lives in
//! [`crate::security_policy`], and the colour-query reply formatting in
//! [`crate::osc_color`].

use base64::Engine;

/// Decode an OSC 52 base64 payload → clipboard text. Returns None if base64 is invalid.
///
/// Test-only: the engine decodes an incoming OSC 52 itself, so nothing in the
/// adapter's production path ever decodes one. This is [`encode_osc52`]'s
/// inverse, and it exists so the round trip can be asserted.
#[cfg(test)]
pub(crate) fn decode_osc52(base64: &str) -> Option<String> {
    // OSC 52 allows skipping invalid characters; use the standard engine.
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(base64.trim())
        .ok()?;
    String::from_utf8(decoded).ok()
}

/// Encode text → an OSC 52 base64 payload (for a clipboard reply).
pub fn encode_osc52(text: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osc52_codec_roundtrip() {
        // The OSC 52 base64 helpers remain (used for clipboard replies).
        let s = "Héllo, 世界";
        let enc = encode_osc52(s);
        assert_eq!(decode_osc52(&enc).as_deref(), Some(s));
        // "hi" → "aGk="
        assert_eq!(decode_osc52("aGk=").as_deref(), Some("hi"));
    }

    #[test]
    fn the_security_policy_caps_what_the_engine_reports() {
        // The engine parses; the caps are this crate's.
        let policy = crate::security_policy::TerminalSecurityPolicy::default();
        let large_notification = "x".repeat(256 * 1024);
        let sanitized = policy.sanitize_notification(&large_notification);
        assert!(sanitized.is_some());
        assert!(sanitized.unwrap().len() <= 8 * 1024);

        // The policy caps clipboard write size (256 KiB limit).
        let large_clipboard = "c".repeat(256 * 1024 + 1);
        assert_eq!(
            policy.validate_clipboard_write(
                &large_clipboard,
                crate::security_policy::ClipboardOrigin::Local
            ),
            None
        );

        // Normal-sized clipboard write is allowed.
        let small_clipboard = "c".repeat(100);
        assert_eq!(
            policy.validate_clipboard_write(
                &small_clipboard,
                crate::security_policy::ClipboardOrigin::Local
            ),
            Some(small_clipboard.as_str())
        );
    }
}
