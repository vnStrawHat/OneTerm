//! Parse side-channel OSC sequences that the alacritty VTE drops or routes through
//! `EventListener`: OSC 7 (cwd), OSC 52 (clipboard), OSC 133 (shell integration).
//! OSC 8 (hyperlink) is stored by alacritty directly in the cell → see `url.rs`.
//!
//! Reference: `freya-terminal/osc7.rs` + additions for OSC 52 and OSC 133.
//! OSC 133 spec: https://gitlab.freedesktop.org/Per_Bothner/specifications/blob/master/proposals/semantic-prompts.md

use std::path::PathBuf;

use alacritty_terminal::vte::{Params, Perform};
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

/// A captured OSC payload (kind + raw data).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OscPayload {
    /// OSC 7 — `file://host/path`.
    Cwd(String),
    /// OSC 52 — clipboard base64 payload (the `?` parameter = query).
    Clipboard { query: bool, base64: String },
    /// OSC 133 — shell integration marker (prompt/command boundary).
    ShellIntegration(Osc133Kind),
}

/// A sink that runs alongside `Term` to capture OSC 7/52/133.
/// Alacritty drops OSC 7 and OSC 133, and routes OSC 52 through the EventListener.
/// This sink parses the PTY byte stream directly, in parallel with alacritty's Processor.
#[derive(Default)]
pub struct OscSink {
    latest: Option<OscPayload>,
    /// Whether a full-screen clear sequence (`CSI 2J` / `CSI 3J` / `ESC c` = RIS)
    /// has been seen since the last `take_clear()`. Used to tell the upper layer
    /// to reset per-line timestamps (gutter), because a `clear` resets the
    /// absolute line counter → new content REUSES old indices, otherwise it
    /// would show stale times.
    clear_pending: bool,
}

impl OscSink {
    pub fn take(&mut self) -> Option<OscPayload> {
        self.latest.take()
    }

    /// Returns `true` (and resets the flag) if a full-screen clear sequence has
    /// been detected since the previous call.
    pub fn take_clear(&mut self) -> bool {
        std::mem::take(&mut self.clear_pending)
    }
}

impl Perform for OscSink {
    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        if params.is_empty() {
            return;
        }
        let kind = match std::str::from_utf8(params[0]) {
            Ok(s) => s,
            Err(_) => return,
        };
        match kind {
            // OSC 7: params = ["7", "file://..."]
            "7" if params.len() >= 2 => {
                if let Ok(url) = std::str::from_utf8(params[1]) {
                    self.latest = Some(OscPayload::Cwd(url.to_owned()));
                }
            }
            // OSC 52: params = ["52", "c", "<base64>" | "?"]
            "52" if params.len() >= 2 => {
                if let Ok(target) = std::str::from_utf8(params[1]) {
                    // Only care about clipboard 'c' (system clipboard).
                    if target.contains('c') {
                        let payload = params.get(2).copied().unwrap_or(&[]);
                        if payload == b"?" {
                            self.latest = Some(OscPayload::Clipboard {
                                query: true,
                                base64: String::new(),
                            });
                        } else if let Ok(b64) = std::str::from_utf8(payload) {
                            self.latest = Some(OscPayload::Clipboard {
                                query: false,
                                base64: b64.to_owned(),
                            });
                        }
                    }
                }
            }
            // OSC 133: shell integration markers
            // params = ["133", "A" | "B" | "C" | "D"]
            // params = ["133", "D", "exit_code"] (D with exit code)
            "133" if params.len() >= 2 => {
                let sub = match std::str::from_utf8(params[1]) {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let marker = match sub {
                    "A" => Some(Osc133Kind::PromptStart),
                    "B" => Some(Osc133Kind::PromptEnd),
                    "C" => Some(Osc133Kind::OutputStart),
                    "D" => {
                        // D;exit_code → params[2] = exit_code (if present).
                        let exit_code = params.get(2).and_then(|p| {
                            std::str::from_utf8(p)
                                .ok()
                                .and_then(|s| s.parse::<i32>().ok())
                        });
                        Some(Osc133Kind::OutputEnd { exit_code })
                    }
                    _ => None,
                };
                if let Some(m) = marker {
                    self.latest = Some(OscPayload::ShellIntegration(m));
                }
            }
            _ => {}
        }
    }

    /// Detect `CSI 2J` (clear whole screen) and `CSI 3J` (clear scrollback) —
    /// emitted by `clear` / `cls` / `tput clear`. `CSI 0J`/`CSI 1J`
    /// (partial erase) do NOT count as a clear.
    fn csi_dispatch(
        &mut self,
        params: &Params,
        _intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        if action == 'J' {
            let mode = params
                .iter()
                .next()
                .and_then(|sub| sub.first().copied())
                .unwrap_or(0);
            if mode == 2 || mode == 3 {
                self.clear_pending = true;
            }
        }
    }

    /// `ESC c` = RIS (Reset to Initial State) → clears everything → treated as a clear.
    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
        if byte == b'c' {
            self.clear_pending = true;
        }
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
    use alacritty_terminal::vte::Parser as VteParser;

    use super::*;

    fn sniff(chunks: &[&[u8]]) -> Option<OscPayload> {
        let mut parser = VteParser::new();
        let mut sink = OscSink::default();
        for chunk in chunks {
            parser.advance(&mut sink, chunk);
        }
        sink.take()
    }

    #[test]
    fn osc7_bel() {
        let p = sniff(&[b"\x1b]7;file:///home/marc\x07"]).unwrap();
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
    fn osc7_split() {
        let p = sniff(&[b"\x1b]7;file://", b"/var", b"/log\x07"]).unwrap();
        assert_eq!(p, OscPayload::Cwd("file:///var/log".into()));
    }

    #[test]
    fn ignores_other_oscs() {
        assert_eq!(sniff(&[b"\x1b]0;hello\x07"]), None);
        assert_eq!(sniff(&[b"\x1b]70;nope\x07"]), None);
    }

    #[test]
    fn osc52_set_clipboard() {
        // "hi" → base64 "aGk="
        let p = sniff(&[b"\x1b]52;c;aGk=\x07"]).unwrap();
        assert_eq!(
            p,
            OscPayload::Clipboard {
                query: false,
                base64: "aGk=".into()
            }
        );
        assert_eq!(decode_osc52("aGk=").as_deref(), Some("hi"));
    }

    #[test]
    fn osc52_query() {
        let p = sniff(&[b"\x1b]52;c;?\x07"]).unwrap();
        assert!(matches!(p, OscPayload::Clipboard { query: true, .. }));
    }

    #[test]
    fn osc52_roundtrip() {
        let s = "Héllo, 世界";
        let enc = encode_osc52(s);
        assert_eq!(decode_osc52(&enc).as_deref(), Some(s));
    }

    // ── OSC 133 tests ──────────────────────────────────────────────
    #[test]
    fn osc133_prompt_start() {
        let p = sniff(&[b"\x1b]133;A\x07"]).unwrap();
        assert_eq!(p, OscPayload::ShellIntegration(Osc133Kind::PromptStart));
    }

    #[test]
    fn osc133_prompt_end() {
        let p = sniff(&[b"\x1b]133;B\x07"]).unwrap();
        assert_eq!(p, OscPayload::ShellIntegration(Osc133Kind::PromptEnd));
    }

    #[test]
    fn osc133_output_start() {
        let p = sniff(&[b"\x1b]133;C\x07"]).unwrap();
        assert_eq!(p, OscPayload::ShellIntegration(Osc133Kind::OutputStart));
    }

    #[test]
    fn osc133_output_end_no_code() {
        let p = sniff(&[b"\x1b]133;D\x07"]).unwrap();
        assert_eq!(
            p,
            OscPayload::ShellIntegration(Osc133Kind::OutputEnd { exit_code: None })
        );
    }

    #[test]
    fn osc133_output_end_with_code() {
        // BEL terminated
        let p = sniff(&[b"\x1b]133;D;0\x07"]).unwrap();
        assert_eq!(
            p,
            OscPayload::ShellIntegration(Osc133Kind::OutputEnd { exit_code: Some(0) })
        );
        // ST terminated
        let p2 = sniff(&[b"\x1b]133;D;127\x1b\\"]).unwrap();
        assert_eq!(
            p2,
            OscPayload::ShellIntegration(Osc133Kind::OutputEnd {
                exit_code: Some(127)
            })
        );
    }

    #[test]
    fn osc133_full_cycle() {
        // Simulate a full prompt cycle: A → prompt text → B → command → C → output → D;0
        let bytes = b"\x1b]133;A\x07$ \x1b]133;B\x07echo hi\r\x1b]133;C\x07hi\n\x1b]133;D;0\x07";
        let mut parser = VteParser::new();
        let mut sink = OscSink::default();
        parser.advance(&mut sink, bytes);
        // Should capture the LAST OSC 133 payload (D;0).
        let p = sink.take().unwrap();
        assert_eq!(
            p,
            OscPayload::ShellIntegration(Osc133Kind::OutputEnd { exit_code: Some(0) })
        );
    }

    #[test]
    fn osc133_ignores_unknown_sub() {
        assert_eq!(sniff(&[b"\x1b]133;X\x07"]), None);
        assert_eq!(sniff(&[b"\x1b]133;Z;foo\x07"]), None);
    }

    // ── Clear detection tests ──────────────────────────────────────
    fn sniff_clear(chunks: &[&[u8]]) -> bool {
        let mut parser = VteParser::new();
        let mut sink = OscSink::default();
        for chunk in chunks {
            parser.advance(&mut sink, chunk);
        }
        sink.take_clear()
    }

    #[test]
    fn clear_csi_2j_detected() {
        assert!(sniff_clear(&[b"\x1b[2J"]));
    }

    #[test]
    fn clear_csi_3j_detected() {
        assert!(sniff_clear(&[b"\x1b[3J"]));
    }

    #[test]
    fn clear_full_sequence_detected() {
        // Typical `clear`: home + erase display + erase scrollback.
        assert!(sniff_clear(&[b"\x1b[H\x1b[2J\x1b[3J"]));
    }

    #[test]
    fn clear_ris_detected() {
        // ESC c = Reset to Initial State.
        assert!(sniff_clear(&[b"\x1bc"]));
    }

    #[test]
    fn clear_split_across_reads_detected() {
        // Sequence split mid-CSI across read boundaries — vte parser holds state.
        assert!(sniff_clear(&[b"\x1b[", b"2J"]));
    }

    #[test]
    fn clear_partial_erase_not_detected() {
        // CSI 0J / CSI 1J / CSI J (erase to end / to start) are NOT full clears.
        assert!(!sniff_clear(&[b"\x1b[0J"]));
        assert!(!sniff_clear(&[b"\x1b[1J"]));
        assert!(!sniff_clear(&[b"\x1b[J"]));
    }

    #[test]
    fn clear_flag_consumed_by_take() {
        let mut parser = VteParser::new();
        let mut sink = OscSink::default();
        parser.advance(&mut sink, b"\x1b[2J");
        assert!(sink.take_clear(), "first take should report clear");
        assert!(!sink.take_clear(), "flag should reset after take");
    }

    #[test]
    fn clear_does_not_leak_into_osc_payload() {
        // A clear sequence must not produce an OscPayload.
        assert_eq!(sniff(&[b"\x1b[2J"]), None);
    }
}
