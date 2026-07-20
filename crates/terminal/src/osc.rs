//! Parse side-channel OSC sequences the terminal engine forwards via
//! `Event::Osc` (OSC 7 cwd, OSC 9 notification/progress, OSC 133 shell
//! integration). These are the OSCs vte does not dispatch to a dedicated
//! `Handler` method; the OneTerm alacritty fork routes them through
//! `Handler::report_osc` → `Event::Osc`, so we parse the VT stream **once**
//! (no second `vte::Parser`). See `docs/terminal-fullscreen-perf/09-*.md`.
//!
//! OSC 0/2 (title), OSC 4/10/11/12 (colors), OSC 8 (hyperlink) and OSC 52
//! (clipboard) are handled by the engine itself and surface via their own
//! events. Screen clears (`CSI 2J/3J`, RIS) arrive as `Event::ClearScreen`.
//!
//! OSC 133 spec: https://gitlab.freedesktop.org/Per_Bothner/specifications/blob/master/proposals/semantic-prompts.md

use std::path::PathBuf;

use crate::osc_agent::{AgentStatusEvent, parse_agent_status};

use base64::Engine;

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

/// OSC 9;4 progress state (ConEmu / Windows Terminal taskbar progress).
///
/// Sequence: `OSC 9 ; 4 ; st ; pr ST` where `st` is the state and `pr` a 0-100
/// percentage. Reference: ConEmu progress + Windows Terminal `DispatchTypes::
/// TaskbarState`.
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
    /// OSC 9 — desktop notification (iTerm2 / Windows Terminal). Payload = message.
    Notification(String),
    /// OSC 9;4 — taskbar progress (ConEmu / Windows Terminal).
    Progress(TerminalProgress),
    /// OSC 9;7 — coding-agent status event (see `docs/osc-agent-status.md`).
    /// The payload is the base64-wrapped JSON event, already parsed +
    /// schema-validated. `seq` dedup is performed by the listener. Boxed to
    /// keep `OscPayload` small (the event is ~248 bytes; this enum is transient).
    AgentStatus(Box<AgentStatusEvent>),
}

/// Parse the OSC parameters forwarded by the engine (`Event::Osc { params, .. }`)
/// into an [`OscPayload`]. `params[0]` is the OSC number; the rest are the raw
/// semicolon-separated parameter fields. Returns `None` for OSCs we don't handle.
///
/// Only OSC 7 / 9 / 133 are recognised here — every other OSC is either handled
/// by the engine directly (title/colors/clipboard/hyperlink) or ignored.
pub fn parse_osc(params: &[&[u8]]) -> Option<OscPayload> {
    if params.is_empty() {
        return None;
    }
    let kind = std::str::from_utf8(params[0]).ok()?;
    // Debug-trace every OSC the engine forwards, so you can confirm the VT
    // pump delivered it (e.g. `RUST_LOG=oneterm_terminal=trace`). The first
    // param is the OSC number; the second (when present) is the sub-code
    // (e.g. `7` for OSC 9;7, `4` for OSC 9;4, `A`/`B`/`C`/`D` for OSC 133).
    let sub = params
        .get(1)
        .and_then(|p| std::str::from_utf8(p).ok())
        .unwrap_or("");
    log::debug!("OSC recv: {kind};{sub} ({} params)", params.len());
    match kind {
        // OSC 7: params = ["7", "file://..."]
        "7" if params.len() >= 2 => {
            let url = std::str::from_utf8(params[1]).ok()?;
            Some(OscPayload::Cwd(url.to_owned()))
        }
        // OSC 9: notification (`9;msg`) OR taskbar progress (`9;4;st;pr`).
        // Windows Terminal disambiguates: sub-param "4" = progress, else notify.
        "9" if params.len() >= 2 => {
            if params[1] == b"7" {
                // OSC 9;7;<base64-json> — coding-agent status event
                // (see `docs/osc-agent-status.md`). The third parameter is the
                // base64-wrapped JSON payload (spec §3.1). Malformed payloads
                // are dropped silently by `parse_agent_status` (spec §3.3).
                if let Some(b64) = params.get(2) {
                    let ev = parse_agent_status(b64);
                    if let Some(ref ev) = ev {
                        log::debug!(
                            "OSC 9;7 parsed: agent={} type={} seq={}",
                            ev.agent(),
                            ev.type_name(),
                            ev.seq()
                        );
                    } else {
                        log::debug!(
                            "OSC 9;7 dropped: parse_agent_status returned None (bad base64/utf8/json/schema/type)"
                        );
                    }
                    ev.map(|ev| OscPayload::AgentStatus(Box::new(ev)))
                } else {
                    log::debug!("OSC 9;7 dropped: no base64 parameter (params had no index 2)");
                    None
                }
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
        "133" if params.len() >= 2 => {
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

/// Parse an OSC 7 URL payload → `PathBuf`. Accepts `file:///path`,
/// `file://host/path`, and a plain path.
pub fn parse_cwd_url(url: &str) -> PathBuf {
    let Some(stripped) = url.strip_prefix("file://") else {
        return PathBuf::from(url);
    };
    match stripped.split_once('/') {
        Some((_, path)) => PathBuf::from(format!("/{path}")),
        None => PathBuf::from(stripped),
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
    fn phase1_osc_payloads_are_capped_by_security_policy() {
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
            policy.validate_clipboard_write(&large_clipboard, false),
            None
        );

        // Normal-sized clipboard write is allowed.
        let small_clipboard = "c".repeat(100);
        assert_eq!(
            policy.validate_clipboard_write(&small_clipboard, false),
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
}
