//! Parse side-channel OSC sequences the terminal engine forwards via
//! `Event::Osc` (OSC 7 cwd, OSC 9 notification/progress, OSC 133 shell
//! integration). These are the OSCs vte does not dispatch to a dedicated
//! `Handler` method; the OneTerm alacritty fork routes them through
//! `Handler::report_osc` → `Event::Osc`, so we parse the VT stream **once**
//! (no second `vte::Parser`).
//!
//! OSC 0/2 (title), OSC 4/10/11/12 (colors), OSC 8 (hyperlink) and OSC 52
//! (clipboard) are handled by the engine itself and surface via their own
//! events. Screen clears (`CSI 2J/3J`, RIS) arrive as `Event::ClearScreen`.
//!
//! OSC 133 spec: https://gitlab.freedesktop.org/Per_Bothner/specifications/blob/master/proposals/semantic-prompts.md

use std::path::PathBuf;

use crate::osc_agent::{
    AGENT_OSC, AGENT_PROTOCOL_VERSION, AgentStatusEvent, LEGACY_AGENT_OSC_SUB, parse_agent_status,
};

use base64::Engine;
use oneterm_vt::StringTerm;

/// OSC 133 marker kind — marks prompt/command/output boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Osc133Kind {
    /// `OSC 133;A` — prompt start (the shell is about to draw the prompt).
    PromptStart,
    /// `OSC 133;B` — prompt end / command input start (the user starts typing).
    PromptEnd,
    /// `OSC 133;C` — command output start (the user pressed Enter, command runs).
    OutputStart,
    /// `OSC 133;D[;exit_code]` — command finished (with exit code if present).
    OutputEnd { exit_code: Option<i32> },
}

/// OSC 9;4 progress state (ConEmu taskbar progress).
///
/// Sequence: `OSC 9 ; 4 ; st ; pr ST` where `st` is the state and `pr` a 0-100
/// percentage. Reference: ConEmu progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalProgress {
    /// `st=0` — remove/clear the progress indicator (`pr` ignored).
    Remove,
    /// `st=1` — normal progress at `pr` percent (0-100).
    Set(u8),
    /// `st=2` — error state at `pr` percent (0-100).
    Error(u8),
    /// `st=3` — indeterminate/busy (`pr` ignored).
    Indeterminate,
    /// `st=4` — paused/warning at `pr` percent (0-100).
    Paused(u8),
}

/// A captured OSC payload (kind + parsed data).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OscPayload {
    /// OSC 7 — `file://host/path`.
    Cwd(String),
    /// OSC 133 — shell integration marker (prompt/command boundary).
    ShellIntegration(Osc133Kind),
    /// OSC 9 — desktop notification. Payload = message.
    Notification(String),
    /// OSC 9;4 — taskbar progress (ConEmu).
    Progress(TerminalProgress),
    /// `OSC 20308;1` — coding-agent status event (see `docs/osc-agent-status.md`),
    /// or the same event under the deprecated `OSC 9;7` alias (spec §3.1).
    /// The payload is the base64-wrapped JSON event, already parsed +
    /// schema-validated. `seq` dedup is performed by the listener. Boxed to
    /// keep `OscPayload` small (the event is ~248 bytes; this enum is transient).
    AgentStatus(Box<AgentStatusEvent>),
    /// `OSC 20308;0` — the support query (spec §3.2). Carries nothing: the
    /// answer is a constant, and the router writes it to the transport.
    AgentSupportQuery,
}

/// Parse the OSC parameters forwarded by the engine (`Event::Osc { params, .. }`)
/// into an [`OscPayload`]. `params[0]` is the OSC number; the rest are the raw
/// semicolon-separated parameter fields. Returns `None` for OSCs we don't handle.
///
/// Only OSC 7 / 9 / 133 / 20308 are recognised here — every other OSC is either
/// handled by the engine directly (title/colors/clipboard/hyperlink) or ignored.
pub fn parse_osc(params: &[&[u8]]) -> Option<OscPayload> {
    if params.is_empty() {
        return None;
    }
    // Match the OSC **number**, not its spelling. `params[0]` is raw wire bytes,
    // and an xterm-derived parser accepts `020308` for `20308`; matching the
    // string dropped the zero-padded form after the engine had already claimed
    // and forwarded it by number. Parsing once here is also what keeps
    // `AGENT_OSC` a single source of truth rather than a `u32` for the claim and
    // a `&str` for the dispatch. Pre-existing for 7 / 9 / 133, fixed for all
    // four at once.
    let kind = std::str::from_utf8(params[0]).ok()?.parse::<u32>().ok()?;
    // Debug-trace every OSC the engine forwards, so you can confirm the VT
    // pump delivered it (e.g. `RUST_LOG=oneterm_terminal=trace`). The first
    // param is the OSC number; the second (when present) is the sub-code
    // (e.g. `1` for OSC 20308;1, `4` for OSC 9;4, `A`/`B`/`C`/`D` for OSC 133).
    let sub = params
        .get(1)
        .and_then(|p| std::str::from_utf8(p).ok())
        .unwrap_or("");
    log::debug!("OSC recv: {kind};{sub} ({} params)", params.len());
    match kind {
        // OSC 20308 — the agent channel (`docs/osc-agent-status.md` §3).
        // Sub-code `0` is the support query, `1` the status event; `2` and
        // above are reserved, so anything else is ignored (and counted by the
        // router, which is the only place a per-session counter lives).
        AGENT_OSC => match params.get(1) {
            Some(sub) if *sub == b"1" => parse_agent_status_param(params.get(2).copied()),
            Some(sub) if *sub == b"0" => Some(OscPayload::AgentSupportQuery),
            _ => None,
        },
        // OSC 7: params = ["7", "file://..."]
        7 if params.len() >= 2 => {
            let url = std::str::from_utf8(params[1]).ok()?;
            Some(OscPayload::Cwd(url.to_owned()))
        }
        // OSC 9: notification (`9;msg`) OR taskbar progress (`9;4;st;pr`).
        // Sub-param "4" = progress, else notify.
        9 if params.len() >= 2 => {
            if params[1] == LEGACY_AGENT_OSC_SUB {
                // OSC 9;7;<base64-json> — the agent channel's **deprecated**
                // spelling, kept for one release (spec §3.1). Byte-for-byte the
                // same payload, so it lands in the same parser; the router logs
                // the deprecation once per session.
                parse_agent_status_param(params.get(2).copied())
            } else if params[1] == b"4" {
                // OSC 9;4;state;percent — taskbar progress.
                let parse = |p: &[u8]| std::str::from_utf8(p).ok()?.parse::<u8>().ok();
                let state = params.get(2).and_then(|p| parse(p)).unwrap_or(0);
                let pct = params.get(3).and_then(|p| parse(p)).unwrap_or(0).min(100);
                let progress = match state {
                    0 => TerminalProgress::Remove,
                    1 => TerminalProgress::Set(pct),
                    2 => TerminalProgress::Error(pct),
                    3 => TerminalProgress::Indeterminate,
                    4 => TerminalProgress::Paused(pct),
                    _ => return None,
                };
                Some(OscPayload::Progress(progress))
            } else {
                // OSC 9;message — desktop notification. The message may itself
                // contain ';', so rejoin the remaining params.
                let body = params[1..]
                    .iter()
                    .map(|p| String::from_utf8_lossy(p))
                    .collect::<Vec<_>>()
                    .join(";");
                if body.is_empty() {
                    None
                } else {
                    Some(OscPayload::Notification(body))
                }
            }
        }
        // OSC 133: shell integration markers.
        // params = ["133", "A" | "B" | "C" | "D"] or ["133", "D", "exit_code"].
        133 if params.len() >= 2 => {
            let sub = std::str::from_utf8(params[1]).ok()?;
            let marker = match sub {
                "A" => Osc133Kind::PromptStart,
                "B" => Osc133Kind::PromptEnd,
                "C" => Osc133Kind::OutputStart,
                "D" => {
                    // D;exit_code → params[2] = exit_code (if present).
                    let exit_code = params.get(2).and_then(|p| {
                        std::str::from_utf8(p)
                            .ok()
                            .and_then(|s| s.parse::<i32>().ok())
                    });
                    Osc133Kind::OutputEnd { exit_code }
                }
                _ => return None,
            };
            Some(OscPayload::ShellIntegration(marker))
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
fn parse_agent_status_param(base64_param: Option<&[u8]>) -> Option<OscPayload> {
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
    Some(OscPayload::AgentStatus(Box::new(ev)))
}

/// The support-query answer (spec §3.2):
/// `ESC ] 20308 ; 0 ; <protocol version> ; <name> ; <version> ST`.
///
/// Terminated the way the question was terminated — the same rule the OSC
/// 4/10/11/12 colour replies follow, and the reason the agent may pair the
/// query with a DA1 request and need no timeout.
pub fn agent_support_reply(terminator: StringTerm) -> String {
    let terminator = match terminator {
        StringTerm::Bel => "\x07",
        StringTerm::St => "\x1b\\",
    };
    format!(
        "\x1b]{AGENT_OSC};0;{AGENT_PROTOCOL_VERSION};OneTerm;{}{terminator}",
        env!("CARGO_PKG_VERSION")
    )
}

/// Parse an OSC 7 URL payload → `PathBuf`. Accepts `file:///path`,
/// `file://host/path`, and a plain path.
///
/// The path part of a `file://` URL is percent-decoded (`%20` → space, as
/// shells emit it) and a Windows drive path (`file:///C:/Users`) loses the
/// leading `/` so it becomes `C:/Users` (CORR-46). Invalid percent escapes
/// and non-UTF-8 sequences are kept verbatim.
pub fn parse_cwd_url(url: &str) -> PathBuf {
    let Some(stripped) = url.strip_prefix("file://") else {
        return PathBuf::from(url);
    };
    let path = match stripped.split_once('/') {
        Some((_, path)) => format!("/{path}"),
        None => stripped.to_string(),
    };
    let decoded = percent_decode(&path);
    let path = strip_windows_drive_slash(&decoded);
    PathBuf::from(path)
}

/// Decode `%XX` escapes; malformed escapes are kept as-is and the result is
/// interpreted as UTF-8 leniently.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let high = (bytes[i + 1] as char).to_digit(16);
            let low = (bytes[i + 2] as char).to_digit(16);
            if let (Some(high), Some(low)) = (high, low) {
                out.push((high * 16 + low) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `/C:/Users` → `C:/Users` (a `file:///C:/...` URL on Windows); other paths
/// are returned unchanged.
fn strip_windows_drive_slash(path: &str) -> &str {
    let bytes = path.as_bytes();
    if bytes.len() >= 3
        && bytes[0] == b'/'
        && bytes[1].is_ascii_alphabetic()
        && bytes[2] == b':'
        && (bytes.len() == 3 || bytes[3] == b'/' || bytes[3] == b'\\')
    {
        &path[1..]
    } else {
        path
    }
}

/// Decode an OSC 52 base64 payload → clipboard text. Returns None if base64 is invalid.
pub fn decode_osc52(base64: &str) -> Option<String> {
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

    // ── OSC 7 (cwd) ────────────────────────────────────────────────
    #[test]
    fn osc7_cwd() {
        let p = parse_osc(&[b"7", b"file:///home/marc"]).unwrap();
        assert_eq!(p, OscPayload::Cwd("file:///home/marc".into()));
        assert_eq!(
            parse_cwd_url("file:///home/marc"),
            PathBuf::from("/home/marc")
        );
    }

    #[test]
    fn osc7_host() {
        assert_eq!(
            parse_cwd_url("file://host/var/log"),
            PathBuf::from("/var/log")
        );
    }

    #[test]
    fn osc7_bare_path() {
        assert_eq!(parse_cwd_url("/tmp/x"), PathBuf::from("/tmp/x"));
    }

    /// CORR-46: percent escapes are decoded and a Windows drive URL loses the
    /// leading slash; malformed escapes stay verbatim.
    #[test]
    fn osc7_percent_decoding_and_windows_drive() {
        assert_eq!(
            parse_cwd_url("file://host/home/me/My%20Docs"),
            PathBuf::from("/home/me/My Docs")
        );
        assert_eq!(
            parse_cwd_url("file:///C:/Users/me/src"),
            PathBuf::from("C:/Users/me/src")
        );
        assert_eq!(parse_cwd_url("file:///C:"), PathBuf::from("C:"));
        assert_eq!(
            parse_cwd_url("file:///tmp/100%25/x%zz"),
            PathBuf::from("/tmp/100%/x%zz")
        );
        assert_eq!(
            parse_cwd_url("file:///home/%C3%A9t%C3%A9"),
            PathBuf::from("/home/été")
        );
        // A plain absolute path that happens to start with `/C:` is left alone.
        assert_eq!(parse_cwd_url("/Cx/y"), PathBuf::from("/Cx/y"));
    }

    #[test]
    fn ignores_other_oscs() {
        // OSC 0 (title) and unknown OSC 70 are not our concern.
        assert_eq!(parse_osc(&[b"0", b"hello"]), None);
        assert_eq!(parse_osc(&[b"70", b"nope"]), None);
        // OSC 52 is handled by the engine, not here.
        assert_eq!(parse_osc(&[b"52", b"c", b"aGk="]), None);
        // Empty / lone number.
        assert_eq!(parse_osc(&[]), None);
        assert_eq!(parse_osc(&[b"7"]), None);
    }

    #[test]
    fn osc52_codec_roundtrip() {
        // The OSC 52 base64 helpers remain (used for clipboard replies).
        let s = "Héllo, 世界";
        let enc = encode_osc52(s);
        assert_eq!(decode_osc52(&enc).as_deref(), Some(s));
        // "hi" → "aGk="
        assert_eq!(decode_osc52("aGk=").as_deref(), Some("hi"));
    }

    // ── OSC 133 shell integration ──────────────────────────────────
    #[test]
    fn osc133_markers() {
        assert_eq!(
            parse_osc(&[b"133", b"A"]),
            Some(OscPayload::ShellIntegration(Osc133Kind::PromptStart))
        );
        assert_eq!(
            parse_osc(&[b"133", b"B"]),
            Some(OscPayload::ShellIntegration(Osc133Kind::PromptEnd))
        );
        assert_eq!(
            parse_osc(&[b"133", b"C"]),
            Some(OscPayload::ShellIntegration(Osc133Kind::OutputStart))
        );
    }

    #[test]
    fn osc133_output_end() {
        assert_eq!(
            parse_osc(&[b"133", b"D"]),
            Some(OscPayload::ShellIntegration(Osc133Kind::OutputEnd {
                exit_code: None
            }))
        );
        assert_eq!(
            parse_osc(&[b"133", b"D", b"0"]),
            Some(OscPayload::ShellIntegration(Osc133Kind::OutputEnd {
                exit_code: Some(0)
            }))
        );
        assert_eq!(
            parse_osc(&[b"133", b"D", b"127"]),
            Some(OscPayload::ShellIntegration(Osc133Kind::OutputEnd {
                exit_code: Some(127)
            }))
        );
    }

    #[test]
    fn osc133_unknown_sub() {
        assert_eq!(parse_osc(&[b"133", b"X"]), None);
        assert_eq!(parse_osc(&[b"133", b"Z", b"foo"]), None);
    }

    // ── OSC 9 notification ─────────────────────────────────────────
    #[test]
    fn osc9_notification() {
        assert_eq!(
            parse_osc(&[b"9", b"Build finished"]),
            Some(OscPayload::Notification("Build finished".into()))
        );
    }

    #[test]
    fn osc9_notification_with_semicolons() {
        // A message split on ';' by the parser is rejoined verbatim.
        assert_eq!(
            parse_osc(&[b"9", b"done: 3 tests", b" 0 failed"]),
            Some(OscPayload::Notification("done: 3 tests; 0 failed".into()))
        );
    }

    #[test]
    fn osc_payloads_are_capped_by_security_policy() {
        // parse_osc itself doesn't cap — the TerminalSecurityPolicy does.
        let notification = vec![b'x'; 256 * 1024];
        let parsed = parse_osc(&[b"9", notification.as_slice()]);
        assert_eq!(
            parsed,
            Some(OscPayload::Notification("x".repeat(notification.len())))
        );

        // The policy caps notification size.
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

        // encode_osc52/decode_osc52 still work for normal sizes.
        let clipboard = "c".repeat(100);
        let encoded = encode_osc52(&clipboard);
        assert_eq!(decode_osc52(&encoded).as_deref(), Some(clipboard.as_str()));
    }

    // ── OSC 9;4 progress ───────────────────────────────────────────
    #[test]
    fn osc9_4_progress() {
        assert_eq!(
            parse_osc(&[b"9", b"4", b"1", b"42"]),
            Some(OscPayload::Progress(TerminalProgress::Set(42)))
        );
        assert_eq!(
            parse_osc(&[b"9", b"4", b"0"]),
            Some(OscPayload::Progress(TerminalProgress::Remove))
        );
        assert_eq!(
            parse_osc(&[b"9", b"4", b"2", b"80"]),
            Some(OscPayload::Progress(TerminalProgress::Error(80)))
        );
        assert_eq!(
            parse_osc(&[b"9", b"4", b"3"]),
            Some(OscPayload::Progress(TerminalProgress::Indeterminate))
        );
        assert_eq!(
            parse_osc(&[b"9", b"4", b"4", b"10"]),
            Some(OscPayload::Progress(TerminalProgress::Paused(10)))
        );
    }

    #[test]
    fn osc9_4_progress_clamps_percent() {
        assert_eq!(
            parse_osc(&[b"9", b"4", b"1", b"250"]),
            Some(OscPayload::Progress(TerminalProgress::Set(100)))
        );
    }

    #[test]
    fn osc9_4_unknown_state_ignored() {
        assert_eq!(parse_osc(&[b"9", b"4", b"9", b"50"]), None);
    }

    // ── OSC 20308 agent channel ────────────────────────────────────
    const AGENT_JSON: &str = stringify!(
        {"v":1,"agent":"pi","type":"state","seq":9,"ts":1700000000000,"state":"working"}
    );

    /// The test-support prefixes are the numbers the dispatch matches. There is
    /// no second source of truth left to drift — the match is numeric — but the
    /// fixtures are still hand-written bytes, so pin them.
    #[test]
    fn the_agent_osc_prefixes_spell_the_claimed_numbers() {
        let [new, legacy] = crate::osc_agent::AGENT_OSC_PREFIXES;
        assert_eq!(new[0], crate::osc_agent::AGENT_OSC.to_string().as_bytes());
        assert_eq!(
            legacy[0],
            crate::osc_agent::LEGACY_AGENT_OSC.to_string().as_bytes()
        );
        assert_eq!(legacy[1], LEGACY_AGENT_OSC_SUB);
    }

    /// The claim is numeric and so is the dispatch, so a zero-padded number —
    /// which xterm-derived parsers accept — reaches the same handler instead of
    /// being claimed by the engine and then dropped here.
    #[test]
    fn a_zero_padded_number_reaches_the_same_arm() {
        assert_eq!(
            parse_osc(&[b"020308", b"0"]),
            Some(OscPayload::AgentSupportQuery)
        );
        assert_eq!(
            parse_osc(&[b"007", b"file:///tmp"]),
            Some(OscPayload::Cwd("file:///tmp".into()))
        );
        // Still not a number, still ignored.
        assert_eq!(parse_osc(&[b"20308x", b"0"]), None);
        assert_eq!(parse_osc(&[b"", b"0"]), None);
    }

    /// Both spellings produce the *same* payload — the alias is identical, not
    /// merely similar (spec §3.1).
    #[test]
    fn both_encodings_parse_to_the_same_agent_status() {
        let [new, legacy] = crate::osc_agent::AGENT_OSC_PREFIXES.map(|prefix| {
            let params = crate::osc_agent::encode_agent_osc_params(prefix, AGENT_JSON);
            let refs: Vec<&[u8]> = params.iter().map(Vec::as_slice).collect();
            parse_osc(&refs).expect("both spellings parse")
        });
        assert_eq!(new, legacy);
        match new {
            OscPayload::AgentStatus(ev) => {
                assert_eq!(ev.agent(), "pi");
                assert_eq!(ev.seq(), 9);
                assert_eq!(ev.type_name(), "state");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn agent_support_query_is_recognised() {
        assert_eq!(
            parse_osc(&[b"20308", b"0"]),
            Some(OscPayload::AgentSupportQuery)
        );
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
        assert_eq!(parse_osc(&[b"20308"]), None);
        assert_eq!(parse_osc(&[b"20308", b"2", b"x"]), None);
        assert_eq!(parse_osc(&[b"20308", b"7", b"x"]), None);
        assert_eq!(parse_osc(&[b"20308", b""]), None);
        // Right sub-code, no payload / undecodable payload.
        assert_eq!(parse_osc(&[b"20308", b"1"]), None);
        assert_eq!(parse_osc(&[b"20308", b"1", b"!!not base64!!"]), None);
        assert_eq!(parse_osc(&[b"9", b"7", b"!!not base64!!"]), None);
    }

    /// The alias must not eat OSC 9: notifications and `9;4` progress are
    /// unrelated sub-codes of the same number and still route the old way.
    #[test]
    fn the_alias_does_not_swallow_the_rest_of_osc9() {
        assert_eq!(
            parse_osc(&[b"9", b"4", b"1", b"10"]),
            Some(OscPayload::Progress(TerminalProgress::Set(10)))
        );
        assert_eq!(
            parse_osc(&[b"9", b"71 bottles"]),
            Some(OscPayload::Notification("71 bottles".into())),
            "a message that merely starts with 7 is not the alias"
        );
    }
}
