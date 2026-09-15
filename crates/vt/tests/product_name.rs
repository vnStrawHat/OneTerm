//! The `Config::product_name` contract, driven through the public API.
//!
//! Adopted from the independent verification of the packet that added the
//! field: four tests re-measure the documented replies, and the rest are the
//! hostile values an embedder should not pass but might, which is why the
//! engine sanitises the name instead of trusting it.

use std::time::Instant;

use oneterm_vt::{Config, EventBatch, Size, Terminal, VtEvent};

fn replies(name: Option<&str>, probe: &[u8]) -> Vec<u8> {
    let mut term = Terminal::new(
        Size { rows: 4, cols: 20 },
        Config {
            product_name: name.map(|name| name.to_owned().into()),
            ..Config::default()
        },
    );
    let mut batch = EventBatch::new();
    term.feed(probe, &mut batch, Instant::now());
    let mut out = Vec::new();
    for event in batch.iter() {
        if let VtEvent::Reply(span) = event {
            out.extend_from_slice(batch.bytes(*span));
        }
    }
    out
}

fn xtversion(name: Option<&str>) -> Vec<u8> {
    replies(name, b"\x1b[>0q")
}

fn da2(name: Option<&str>) -> Vec<u8> {
    replies(name, b"\x1b[>c")
}

/// The documented defaults, measured rather than assumed.
#[test]
fn default_identity_is_the_engine() {
    let expected = format!("\x1bP>|oneterm-vt({})\x1b\\", env!("CARGO_PKG_VERSION"));
    assert_eq!(xtversion(None), expected.as_bytes());
    assert_eq!(xtversion(Some("X")), b"\x1bP>|X\x1b\\");
    assert_eq!(da2(Some("OneTerm(0.5.2)")), b"\x1b[>0;502;1c");
}

/// A name carrying `ESC \` must not end the DCS string early. The controls are
/// dropped, so the reply is one well-formed answer naming the whole product.
#[test]
fn an_injected_string_terminator_cannot_break_the_framing() {
    let reply = xtversion(Some("Ev\x1b\\il"));
    assert_eq!(reply, b"\x1bP>|Ev\\il\x1b\\");
    let terminators = reply.windows(2).filter(|pair| pair == b"\x1b\\").count();
    assert_eq!(terminators, 1, "exactly one terminator, at the end");
}

/// The 8-bit C1 terminator and `BEL` go the same way: both are controls, and
/// controls never reach the reply.
#[test]
fn c1_st_and_bel_are_stripped() {
    assert_eq!(xtversion(Some("A\u{9c}B")), b"\x1bP>|AB\x1b\\");
    assert_eq!(xtversion(Some("A\x07B")), b"\x1bP>|AB\x1b\\");
    assert_eq!(xtversion(Some("A\x7fB")), b"\x1bP>|AB\x1b\\");
}

/// The name is cut at 64 bytes, so no embedder configuration can turn one
/// `XTVERSION` query into a reply the embedder has to write back by the
/// kilobyte.
#[test]
fn a_huge_name_is_capped_at_64_bytes() {
    let huge = "x".repeat(65536);
    let reply = xtversion(Some(&huge));
    assert_eq!(reply.len(), 64 + 6, "prefix + 64 bytes + ST");
    assert_eq!(reply, format!("\x1bP>|{}\x1b\\", "x".repeat(64)).as_bytes());
}

/// The cut lands on a character boundary, never inside one.
#[test]
fn the_cap_does_not_split_a_character() {
    // 21 three-byte characters is 63 bytes; the 22nd would be 66.
    let name = "\u{4e2d}".repeat(30);
    let reply = xtversion(Some(&name));
    assert_eq!(
        reply,
        format!("\x1bP>|{}\x1b\\", "\u{4e2d}".repeat(21)).as_bytes()
    );
    assert_eq!(reply.len(), 63 + 6);
}

/// A name that is empty, or that is nothing but controls, is the same as no
/// name at all: the engine answers for itself rather than with an empty one.
#[test]
fn an_empty_name_falls_back_to_the_engine() {
    let engine = format!("\x1bP>|oneterm-vt({})\x1b\\", env!("CARGO_PKG_VERSION"));
    assert_eq!(xtversion(Some("")), engine.as_bytes());
    assert_eq!(xtversion(Some("\x1b\x07\u{9c}")), engine.as_bytes());
    assert_eq!(da2(Some("")), da2(None));
}

/// `DA2` answers one number, so each component saturates at 99. The collision
/// that buys is documented on `Config::product_name`; what matters is that the
/// arithmetic cannot overflow whatever an embedder puts in the name.
#[test]
fn da2_version_saturates_instead_of_overflowing() {
    assert_eq!(da2(Some("P(0.0.0)")), b"\x1b[>0;0;1c");
    assert_eq!(da2(Some("P(1.2.3)")), b"\x1b[>0;10203;1c");
    assert_eq!(da2(Some("P(999.999.999)")), b"\x1b[>0;999999;1c");
    // The saturation is visible here: 1.0.100 reports what 1.0.99 reports.
    assert_eq!(da2(Some("P(1.0.100)")), da2(Some("P(1.0.99)")));
    assert_ne!(da2(Some("P(1.1.0)")), da2(Some("P(1.0.100)")));
    // A major nobody would write, and no panic in a debug build.
    assert_eq!(da2(Some("P(429497.0.0)")), b"\x1b[>0;990000;1c");
}

/// A name with no parsable trailing version leaves `DA2` on the engine's own
/// number rather than reporting zero.
#[test]
fn a_name_without_a_version_keeps_the_engine_number() {
    assert_eq!(da2(Some("MyTerm")), da2(None));
    assert_eq!(da2(Some("MyTerm(latest)")), da2(None));
}
