//! The OSC work that is OneTerm's rather than a terminal's.
//!
//! The engine parses every standard OSC number itself and hands the result over
//! as a typed event ([`oneterm_vt::VtEvent::Cwd`], `Notification`, `Progress`,
//! `ShellMark`, `Title`, `ClipboardStore`, ...), so there is no second parser
//! here and no OSC number matched by spelling. What is left is what no terminal
//! core can own:
//!
//! * the agent channel (`OSC 20308`, see `docs/osc-agent-status.md`), which is
//!   OneTerm's own proposal and reaches this crate as a raw
//!   [`oneterm_vt::VtEvent::Osc`] because [`crate::handle`] routes the number
//!   out of the engine;
//! * the answer to its support query, which names this product and its version;
//! * the OSC 52 reply encoder, because a terminal core never writes a clipboard.
//!
//! Policy — what may be shown, pasted, followed or how often — lives in
//! [`crate::security_policy`], and the colour-query reply formatting in
//! [`crate::osc_color`].

use crate::osc_agent::{
    AGENT_OSC, AGENT_PROTOCOL_VERSION, AgentStatusEvent, LEGACY_AGENT_OSC, LEGACY_AGENT_OSC_SUB,
    parse_agent_status,
};

use base64::Engine;
use oneterm_vt::StringTerm;

/// What an OSC the engine forwarded to this crate turned out to be.
#[derive(Debug)]
pub(crate) enum AgentOsc {
    /// `OSC 20308;1` (or its deprecated `OSC 9;7` alias) — a coding-agent
    /// status event, already base64-decoded and schema-validated. `seq` dedup
    /// is the router's. Boxed because the event is large and this is transient.
    Status(Box<AgentStatusEvent>),
    /// `OSC 20308;0` — the support query (spec §3.2). Carries nothing: the
    /// answer is a constant and the router writes it to the transport.
    SupportQuery,
}

/// Recognise the agent channel in a forwarded OSC. `params[0]` is the OSC
/// number and the rest are the raw `;`-separated fields.
///
/// `code` is matched numerically rather than by spelling, because an
/// xterm-derived parser accepts `020308` for `20308` and the engine has already
/// routed the number.
pub(crate) fn parse_agent_osc(code: u32, params: &[&[u8]]) -> Option<AgentOsc> {
    match (code, params.get(1).copied()) {
        // Sub-code `0` is the support query, `1` the status event; `2` and
        // above are reserved, so anything else is ignored (and counted by the
        // router, which is where a per-session counter lives).
        (AGENT_OSC, Some(b"1")) => parse_agent_status_param(params.get(2).copied()),
        (AGENT_OSC, Some(b"0")) => Some(AgentOsc::SupportQuery),
        // `OSC 9;7;<base64-json>` — the channel's **deprecated** spelling, kept
        // for one release (spec §3.1). Byte-for-byte the same payload, so it
        // lands in the same parser; the router logs the deprecation once per
        // session. `OSC 9` itself is the engine's, and the engine forwards it
        // here as well precisely so this sub-code stays OneTerm's knowledge.
        (LEGACY_AGENT_OSC, Some(sub)) if sub == LEGACY_AGENT_OSC_SUB => {
            parse_agent_status_param(params.get(2).copied())
        }
        _ => None,
    }
}

/// The agent-status half of both spellings: the base64 third parameter
/// (spec §3.3 — the payload is always base64-wrapped) → a validated event.
///
/// Both `OSC 20308;1` and the deprecated `OSC 9;7` land here, which is what
/// makes the alias byte-for-byte identical rather than merely similar. Any
/// malformed payload is dropped silently by `parse_agent_status` (spec §3.5).
fn parse_agent_status_param(base64_param: Option<&[u8]>) -> Option<AgentOsc> {
    let Some(b64) = base64_param else {
        log::debug!("agent status dropped: no base64 parameter (params had no index 2)");
        return None;
    };
    let Some(ev) = parse_agent_status(b64) else {
        log::debug!(
            "agent status dropped: parse_agent_status returned None \
             (bad base64/utf8/json/schema/type)"
        );
        return None;
    };
    log::debug!(
        "agent status parsed: agent={} type={} seq={}",
        ev.agent(),
        ev.type_name(),
        ev.seq()
    );
    Some(AgentOsc::Status(Box::new(ev)))
}

/// Whether a desktop notification the engine reported is really the deprecated
/// `OSC 9;7` agent alias wearing OSC 9's clothes.
///
/// The engine routes per number, never per sub-code — sub-codes are payload —
/// so `OSC 9;7;<base64>` reaches it as a notification whose body is the
/// remaining fields rejoined on `;`. That is exactly "the forwarded first
/// parameter is `7`", read off the body the engine built from it. The raw
/// sequence arrives too, and [`parse_agent_osc`] handles it there.
pub(crate) fn is_legacy_agent_notification(body: &str) -> bool {
    let sub = std::str::from_utf8(LEGACY_AGENT_OSC_SUB).unwrap_or("7");
    body == sub
        || body
            .strip_prefix(sub)
            .is_some_and(|rest| rest.starts_with(';'))
}

/// The support-query answer (spec §3.2):
/// `ESC ] 20308 ; 0 ; <protocol version> ; <name> ; <version> ST`.
///
/// Terminated the way the question was terminated — the same rule the OSC
/// 4/10/11/12 colour replies follow, and the reason the agent may pair the
/// query with a DA1 request and need no timeout.
pub(crate) fn agent_support_reply(terminator: StringTerm) -> String {
    let terminator = match terminator {
        StringTerm::Bel => "\x07",
        StringTerm::St => "\x1b\\",
    };
    format!(
        "\x1b]{AGENT_OSC};0;{AGENT_PROTOCOL_VERSION};OneTerm;{}{terminator}",
        env!("CARGO_PKG_VERSION")
    )
}

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

    const AGENT_JSON: &str = stringify!(
        {"v":1,"agent":"pi","type":"state","seq":9,"ts":1700000000000,"state":"working"}
    );

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

    /// The test-support prefixes are the numbers the dispatch matches. There is
    /// no second source of truth left to drift — the match is numeric — but the
    /// fixtures are still hand-written bytes, so pin them.
    #[test]
    fn the_agent_osc_prefixes_spell_the_routed_numbers() {
        let [new, legacy] = crate::osc_agent::AGENT_OSC_PREFIXES;
        assert_eq!(new[0], AGENT_OSC.to_string().as_bytes());
        assert_eq!(legacy[0], LEGACY_AGENT_OSC.to_string().as_bytes());
        assert_eq!(legacy[1], LEGACY_AGENT_OSC_SUB);
    }

    /// Both spellings produce the *same* payload — the alias is identical, not
    /// merely similar (spec §3.1).
    #[test]
    fn both_encodings_parse_to_the_same_agent_status() {
        let [new, legacy] = crate::osc_agent::AGENT_OSC_PREFIXES.map(|prefix| {
            let params = crate::osc_agent::encode_agent_osc_params(prefix, AGENT_JSON);
            let refs: Vec<&[u8]> = params.iter().map(Vec::as_slice).collect();
            let code = std::str::from_utf8(refs[0])
                .expect("the prefix is ASCII")
                .parse::<u32>()
                .expect("the prefix is a number");
            match parse_agent_osc(code, &refs).expect("both spellings parse") {
                AgentOsc::Status(ev) => *ev,
                other => panic!("unexpected {other:?}"),
            }
        });
        assert_eq!(new.agent(), legacy.agent());
        assert_eq!(new.seq(), legacy.seq());
        assert_eq!(new.type_name(), legacy.type_name());
        assert_eq!(new.agent(), "pi");
        assert_eq!(new.seq(), 9);
        assert_eq!(new.type_name(), "state");
    }

    #[test]
    fn agent_support_query_is_recognised() {
        assert!(matches!(
            parse_agent_osc(20308, &[b"20308", b"0"]),
            Some(AgentOsc::SupportQuery)
        ));
        assert_eq!(
            agent_support_reply(StringTerm::Bel),
            format!("\x1b]20308;0;1;OneTerm;{}\x07", env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(
            agent_support_reply(StringTerm::St),
            format!("\x1b]20308;0;1;OneTerm;{}\x1b\\", env!("CARGO_PKG_VERSION"))
        );
    }

    /// `2` and above are reserved (spec §3), and a malformed payload on the
    /// right sub-code is still dropped silently (spec §3.5).
    #[test]
    fn unknown_agent_subcodes_and_bad_payloads_are_dropped() {
        for params in [
            vec![b"20308".as_slice()],
            vec![b"20308".as_slice(), b"2", b"x"],
            vec![b"20308".as_slice(), b"7", b"x"],
            vec![b"20308".as_slice(), b""],
            vec![b"20308".as_slice(), b"1"],
            vec![b"20308".as_slice(), b"1", b"!!not base64!!"],
        ] {
            assert!(
                parse_agent_osc(20308, &params).is_none(),
                "{params:?} is not an agent event"
            );
        }
        assert!(parse_agent_osc(9, &[b"9", b"7", b"!!not base64!!"]).is_none());
        // Every other OSC the engine forwards here is not the agent channel.
        assert!(parse_agent_osc(9, &[b"9", b"4", b"1", b"10"]).is_none());
        assert!(parse_agent_osc(70, &[b"70", b"nope"]).is_none());
    }

    /// The alias must not eat OSC 9: a notification and `9;4` progress are
    /// unrelated sub-codes of the same number, and the engine parses both.
    #[test]
    fn only_the_alias_body_is_taken_from_osc_9() {
        assert!(is_legacy_agent_notification("7;anything"));
        assert!(is_legacy_agent_notification("7"));
        assert!(
            !is_legacy_agent_notification("71 bottles"),
            "a message that merely starts with 7 is not the alias"
        );
        assert!(!is_legacy_agent_notification("Build finished"));
    }
}
