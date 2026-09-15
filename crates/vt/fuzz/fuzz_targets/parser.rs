//! Fuzz the VT state machine: it must not panic, and it must not grow without
//! a bound, whatever the remote sends.
//!
//! Run under a memory limit, which is where the value is:
//! `cargo +nightly fuzz run parser -- -rss_limit_mb=512`.

#![no_main]

use libfuzzer_sys::fuzz_target;
use oneterm_vt::parser::{Dispatch, OscParams, Params, Parser, StringTerm};
use oneterm_vt::{OscRoute, OscRoutes};

/// Consumes every action and keeps nothing, so the only memory in play is the
/// parser's own.
struct Sink {
    routes: OscRoutes,
}

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

    /// The 8 MiB tier is opt-in per number, so the table the fuzzer derived
    /// from its own input decides which numbers can reach it.
    fn osc_allows_large(&self, code: u32) -> bool {
        self.routes.allows_large(code)
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
    // The route table is part of the input: the first bytes pick a route and a
    // payload ceiling for a handful of numbers, so every combination is fuzzed
    // rather than only the default one.
    let mut routes = OscRoutes::new();
    for (index, &code) in [0u32, 4, 7, 8, 9, 52, 133, 1337, 31337].iter().enumerate() {
        let seed = data.get(index).copied().unwrap_or(0);
        let route = match seed % 4 {
            0 if OscRoutes::has_builtin(code) => OscRoute::Builtin,
            1 => OscRoute::BuiltinAndForward,
            2 => OscRoute::Forward,
            _ => OscRoute::Drop,
        };
        routes.route(code, route);
        if seed & 0x80 != 0 && routes.get(code) != OscRoute::Drop {
            routes.large(code, true);
        }
    }
    let mut sink = Sink { routes };
    let split = data.len() / 3;
    parser.advance(&mut sink, &data[..split]);
    parser.advance(&mut sink, &data[split..]);
});
