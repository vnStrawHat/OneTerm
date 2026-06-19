//! Parse side-channel OSC sequences mà alacritty VTE drop hoặc route qua
//! `EventListener`: OSC 7 (cwd), OSC 52 (clipboard). OSC 8 (hyperlink) được
//! alacritty lưu trực tiếp vào cell → xem `url.rs`.
//!
//! Tham chiếu: `freya-terminal/osc7.rs` + bổ sung OSC 52.

use std::path::PathBuf;

use alacritty_terminal::vte::Perform;
use base64::Engine;

/// Payload OSC đã capture (loại + dữ liệu thô).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OscPayload {
    /// OSC 7 — `file://host/path`.
    Cwd(String),
    /// OSC 52 — clipboard base64 payload (tham số `?` = query).
    Clipboard { query: bool, base64: String },
}

/// Sink chạy song song với `Term` để bắt OSC 7/52 (alacritty drop OSC 7,
/// OSC 52 route qua EventListener — sink này dùng khi muốn parse trực tiếp).
#[derive(Default)]
pub struct OscSink {
    latest: Option<OscPayload>,
}

impl OscSink {
    pub fn take(&mut self) -> Option<OscPayload> {
        self.latest.take()
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
                    // Chỉ quan tâm clipboard 'c' (system clipboard).
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
            _ => {}
        }
    }
}

/// Parse URL payload OSC 7 → `PathBuf`. Chấp nhận `file:///path`,
/// `file://host/path`, và path thường.
pub fn parse_cwd_url(url: &str) -> PathBuf {
    let Some(stripped) = url.strip_prefix("file://") else {
        return PathBuf::from(url);
    };
    match stripped.split_once('/') {
        Some((_, path)) => PathBuf::from(format!("/{path}")),
        None => PathBuf::from(stripped),
    }
}

/// Decode payload base64 OSC 52 → text clipboard. Trả None nếu base64 sai.
pub fn decode_osc52(base64: &str) -> Option<String> {
    // OSC 52 cho phép bỏ qua các ký tự không hợp lệ; dùng engine standard.
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(base64.trim())
        .ok()?;
    String::from_utf8(decoded).ok()
}

/// Encode text → payload base64 OSC 52 (cho reply clipboard).
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
}
