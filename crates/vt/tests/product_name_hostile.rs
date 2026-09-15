//! Hostile `product_name` values, beyond the ones `product_name.rs` covers.
//!
//! Adopted from the independent verification of the packet that added the
//! field: the cases it found by attacking the sanitiser rather than by reading
//! it. Every one goes through the public API.

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

fn engine_xtversion() -> Vec<u8> {
    format!("\x1bP>|oneterm-vt({})\x1b\\", env!("CARGO_PKG_VERSION")).into_bytes()
}

/// A four-byte character straddling byte 64 must be dropped whole, not split.
/// The adopted suite only covers a three-byte character, whose arithmetic lands
/// differently.
#[test]
fn verify_a_four_byte_character_straddling_the_cap_is_dropped_whole() {
    for head in 61..=63 {
        let name = format!("{}\u{1f600}", "a".repeat(head));
        let reply = xtversion(Some(&name));
        // head + 4 is 65..=67, so the emoji never fits and nothing follows it.
        assert_eq!(
            reply,
            format!("\x1bP>|{}\x1b\\", "a".repeat(head)).as_bytes(),
            "head={head}"
        );
        assert!(
            std::str::from_utf8(&reply).is_ok(),
            "reply stays valid UTF-8"
        );
    }
    // Exactly 60 + 4 = 64 fits, and is the boundary case on the other side.
    let name = format!("{}\u{1f600}", "a".repeat(60));
    assert_eq!(
        xtversion(Some(&name)),
        format!("\x1bP>|{}\u{1f600}\x1b\\", "a".repeat(60)).as_bytes()
    );
}

/// `break`, not `continue`: once a character does not fit, nothing after it is
/// taken either, even if a shorter character would have fitted.
#[test]
fn verify_the_cap_stops_rather_than_skipping_to_a_shorter_character() {
    let name = format!("{}\u{1f600}zzz", "a".repeat(62));
    assert_eq!(
        xtversion(Some(&name)),
        format!("\x1bP>|{}\x1b\\", "a".repeat(62)).as_bytes(),
        "the three ASCII bytes after the oversized character are not appended"
    );
}

/// Names made of nothing but controls, in every band the sanitiser names.
#[test]
fn verify_control_only_names_all_fall_back_to_the_engine() {
    let engine = engine_xtversion();
    // Every C0, then DEL, then every C1.
    let c0: String = (0u8..=0x1f).map(char::from).collect();
    let c1: String = (0x80u32..=0x9f)
        .map(|c| char::from_u32(c).unwrap())
        .collect();
    assert_eq!(xtversion(Some(&c0)), engine);
    assert_eq!(xtversion(Some("\u{7f}")), engine);
    assert_eq!(xtversion(Some(&c1)), engine);
    assert_eq!(xtversion(Some(&format!("{c0}\u{7f}{c1}"))), engine);
    // And DA2 follows the same fallback, so the two answers cannot disagree.
    assert_eq!(da2(Some(&format!("{c0}\u{7f}{c1}"))), da2(None));
}

/// Version components at and beyond `u32::MAX`, in both the parsable and the
/// unparsable direction. Neither may panic in a debug build.
#[test]
fn verify_da2_handles_u32_max_and_beyond() {
    // Parses as u32, then saturates at 99 per component.
    assert_eq!(
        da2(Some("P(4294967295.4294967295.4294967295)")),
        b"\x1b[>0;999999;1c"
    );
    // One past u32::MAX does not parse, so the leading-component guard sends
    // DA2 back to the engine's own number.
    assert_eq!(da2(Some("P(4294967296.0.0)")), da2(None));
    assert_eq!(da2(Some("P(99999999999999999999.0.0)")), da2(None));
    // A parsable major with unparsable tail components: those become 0.
    assert_eq!(da2(Some("P(7.4294967296.4294967296)")), b"\x1b[>0;70000;1c");
    // Negative and empty components.
    assert_eq!(da2(Some("P(-1.2.3)")), da2(None));
    assert_eq!(da2(Some("P(..)")), da2(None));
    assert_eq!(da2(Some("P()")), da2(None));
}

/// The 64-byte cut is applied to the *sanitised* name, so controls must not buy
/// budget: a name of 64 controls followed by "Prod" still reports "Prod".
#[test]
fn verify_controls_do_not_consume_the_byte_budget() {
    let name = format!("{}Prod", "\x1b".repeat(64));
    assert_eq!(xtversion(Some(&name)), b"\x1bP>|Prod\x1b\\");
}

/// Nesting parentheses cannot make `DA2` read something that is not the
/// trailing version.
#[test]
fn verify_da2_reads_only_the_trailing_parenthesis_group() {
    assert_eq!(da2(Some("P(1.2.3)(4.5.6)")), b"\x1b[>0;40506;1c");
    assert_eq!(da2(Some("(1.2.3)P")), da2(None));
}

/// The sanitiser runs **once**, in `Terminal::new`, so a query costs the same
/// whatever the embedder configured. The verification measured the opposite
/// before the fix: 2 000 queries against a 1 MiB control-only name walked a
/// megabyte per query, because a dropped control never advanced the byte
/// budget that ends the loop.
///
/// No timing assertion -- a shared runner is not a stopwatch. What is pinned is
/// that the work is not in the query path: the name the terminal keeps is the
/// sanitised one, so there is nothing left for a query to walk.
#[test]
fn verify_a_hostile_name_costs_nothing_per_query() {
    let hostile = "\x1b".repeat(1 << 20);
    let mut probe = Vec::new();
    for _ in 0..2_000 {
        probe.extend_from_slice(b"\x1b[>0q");
    }

    // It sanitises to nothing, so all 2 000 replies are the engine's own.
    let out = replies(Some(&hostile), &probe);
    assert_eq!(out.len(), engine_xtversion().len() * 2_000);
    assert!(out.starts_with(&engine_xtversion()));

    let term = Terminal::new(
        Size { rows: 4, cols: 20 },
        Config {
            product_name: Some(hostile.into()),
            ..Config::default()
        },
    );
    assert_eq!(term.config().product_name, None);
}
