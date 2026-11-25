//! Fuzz the VT engine: it must not panic, and it must not grow without a
//! bound, whatever the remote sends and whatever the embedder's OSC routes say.
//!
//! Run under a memory limit, which is where the value is:
//! `cargo +nightly fuzz run parser -- -rss_limit_mb=512`.

#![no_main]

use libfuzzer_sys::fuzz_target;
use oneterm_vt::{Config, EventBatch, OscRoute, OscRoutes, Size, Terminal, VtEvent};
use std::time::Instant;

/// The numbers whose route the input picks: every kind of number the table
/// distinguishes — built-ins with and without a payload ceiling, one below the
/// bitmap that has no built-in, and two above it in the sorted spill.
const ROUTED: [u32; 9] = [0, 4, 7, 8, 9, 52, 133, 1337, 31337];

/// Build the embedder's table out of the head of the input, so the route
/// lookup, the six built-in arms and `forward_osc` are all fuzzed rather than
/// only the parser under a fixed table.
fn routes(data: &[u8]) -> OscRoutes {
    let mut routes = OscRoutes::new();
    for (index, &code) in ROUTED.iter().enumerate() {
        let seed = data.get(index).copied().unwrap_or(0);
        let route = match seed % 4 {
            0 if OscRoutes::has_builtin(code) => OscRoute::Builtin,
            1 => OscRoute::BuiltinAndForward,
            2 => OscRoute::Forward,
            _ => OscRoute::Drop,
        };
        routes.route(code, route);
        // The ceiling is bought after the route, which is the order `large`
        // asserts on, and never for a number that is dropped.
        if seed & 0x80 != 0 && routes.get(code) != OscRoute::Drop {
            routes.large(code, true);
        }
    }
    routes
}

fuzz_target!(|data: &[u8]| {
    // A real terminal, not a sink: the mechanism this target exists to fuzz —
    // the route lookup and the built-in arms — lives in the dispatch layer, and
    // a `Dispatch` that ignores everything would never reach it. The grid is
    // small and the batch is reused, so the only memory that can grow without
    // a bound is the engine's own.
    let mut term = Terminal::new(
        Size { rows: 8, cols: 40 },
        Config {
            osc_routes: routes(data),
            ..Config::default()
        },
    );
    let mut batch = EventBatch::new();
    let now = Instant::now();

    // Fed in several chunks, because parser state, the OSC accumulator and the
    // UTF-8 carry all have to survive a chunk boundary.
    let split = data.len() / 3;
    for chunk in [&data[..split], &data[split..]] {
        term.feed(chunk, &mut batch, now);
        // Drain the way an embedder does, and **read** every payload rather
        // than merely looking at the event. A span is an index, so a bound that
        // fell off a character boundary — `StrSpan::skip` cutting the drive
        // slash, or a `finish_lossy` repair — only shows up when somebody
        // resolves it.
        for event in batch.iter() {
            match event {
                VtEvent::Title(span) | VtEvent::IconName(span) | VtEvent::Pointer(span) => {
                    std::hint::black_box(batch.str(*span));
                }
                VtEvent::ClipboardStore { text, .. } => {
                    std::hint::black_box(batch.str(*text));
                }
                VtEvent::Cwd { host, path } => {
                    std::hint::black_box(batch.str(*host));
                    std::hint::black_box(batch.str(*path));
                }
                VtEvent::Notification { title, body } => {
                    std::hint::black_box(batch.str(*title));
                    std::hint::black_box(batch.str(*body));
                }
                VtEvent::Reply(span) => {
                    std::hint::black_box(batch.bytes(*span));
                }
                VtEvent::Osc { params, .. } => {
                    for param in batch.params(*params) {
                        std::hint::black_box(param);
                    }
                }
                other => {
                    std::hint::black_box(other);
                }
            }
        }
    }
});
