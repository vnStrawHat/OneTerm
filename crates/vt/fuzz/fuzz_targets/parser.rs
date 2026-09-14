//! Fuzz the VT state machine: it must not panic, and it must not grow without
//! a bound, whatever the remote sends.
//!
//! Run under a memory limit, which is where the value is:
//! `cargo +nightly fuzz run parser -- -rss_limit_mb=512`.

#![no_main]

use libfuzzer_sys::fuzz_target;
use oneterm_vt::parser::{Dispatch, OscParams, Params, Parser, StringTerm};

/// Consumes every action and keeps nothing, so the only memory in play is the
/// parser's own.
struct Sink;

impl Dispatch for Sink {
    fn print_str(&mut self, _text: &str) {}
    fn execute(&mut self, _byte: u8) {}
    fn esc(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
    fn csi(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _byte: u8) {}

    fn osc(
        &mut self,
        _code: Option<u32>,
        _params: &OscParams<'_>,
        _term: StringTerm,
        _truncated: bool,
    ) {
    }

    /// Claim the two numbers `crates/terminal` claims, so the 8 MiB tier is
    /// reachable and the fuzzer can aim at it.
    fn osc_allows_large(&self, code: u32) -> bool {
        matches!(code, 8 | 52)
    }

    fn dcs_hook(&mut self, _params: &Params, _intermediates: &[u8], _byte: u8) {}
    fn dcs_put(&mut self, _byte: u8) {}
    fn dcs_unhook(&mut self, _aborted: bool) {}
    fn apc_start(&mut self, _introducer: u8) {}
    fn apc_put(&mut self, _byte: u8) {}
    fn apc_end(&mut self, _aborted: bool) {}
}

fuzz_target!(|data: &[u8]| {
    // One parser fed in several chunks, because parser state, the OSC
    // accumulator and the UTF-8 carry all have to survive a chunk boundary.
    let mut parser = Parser::new();
    let mut sink = Sink;
    let split = data.len() / 3;
    parser.advance(&mut sink, &data[..split]);
    parser.advance(&mut sink, &data[split..]);
});
