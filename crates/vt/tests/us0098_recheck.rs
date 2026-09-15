//! Verifier re-check of the US-0098 rework: the pieces that did not exist at
//! `82680ce`. Public API only; independent of the adopted suite.

use oneterm_vt::{Config, EventBatch, OscRoute, OscRoutes, Size, Terminal, VtEvent};
use std::time::Instant;

struct Run {
    term: Terminal,
    batch: EventBatch,
}

impl Run {
    fn with(routes: OscRoutes) -> Run {
        Run {
            term: Terminal::new(
                Size { rows: 4, cols: 20 },
                Config {
                    osc_routes: routes,
                    ..Config::default()
                },
            ),
            batch: EventBatch::new(),
        }
    }

    fn plain() -> Run {
        Run::with(OscRoutes::new())
    }

    fn feed(&mut self, bytes: &[u8]) -> oneterm_vt::FeedStats {
        self.batch.clear();
        self.term.feed(bytes, &mut self.batch, Instant::now())
    }

    fn tags(&self) -> Vec<String> {
        self.batch
            .iter()
            .filter(|e| !matches!(e, VtEvent::Repaint))
            .map(|e| match e {
                VtEvent::Title(s) => format!("Title({})", self.batch.str(*s)),
                VtEvent::IconName(s) => format!("IconName({})", self.batch.str(*s)),
                VtEvent::Pointer(s) => format!("Pointer({})", self.batch.str(*s)),
                VtEvent::Cwd { host, path } => {
                    format!("Cwd({}|{})", self.batch.str(*host), self.batch.str(*path))
                }
                VtEvent::Notification { title, body } => format!(
                    "Notification({}|{})",
                    self.batch.str(*title),
                    self.batch.str(*body)
                ),
                VtEvent::Progress(p) => format!("Progress({p:?})"),
                VtEvent::ShellMark(m) => format!("ShellMark({m:?})"),
                VtEvent::CursorStyleChanged => "CursorStyleChanged".to_owned(),
                VtEvent::Osc { code, params, .. } => {
                    let parts: Vec<String> = self
                        .batch
                        .params(*params)
                        .map(|p| String::from_utf8_lossy(p).into_owned())
                        .collect();
                    format!("Osc({code}:{})", parts.join("|"))
                }
                other => format!("{other:?}"),
            })
            .collect()
    }

    fn caps(&self) -> (usize, usize, usize) {
        (
            self.batch.arena_capacity(),
            self.batch.param_capacity(),
            self.batch.event_capacity(),
        )
    }
}

fn osc(seq: &str) -> String {
    format!("\x1b]{seq}\x07")
}

fn one(seq: &str) -> Vec<String> {
    let mut run = Run::plain();
    run.feed(osc(seq).as_bytes());
    run.tags()
}

// ── Finding 3: the built-in set is a bitmap, and the lookup is bit tests ────

/// `has_builtin` must answer exactly what `BUILTIN` lists, for every number in
/// and out of the bitmap. If the const bitmap and the array ever disagree, the
/// routing decision and the dispatch match disagree with them.
#[test]
fn r_has_builtin_is_the_published_array_for_every_number() {
    for code in 0..4096u32 {
        assert_eq!(
            OscRoutes::has_builtin(code),
            OscRoutes::BUILTIN.contains(&code),
            "OSC {code}"
        );
    }
    for &code in &[4096u32, 20308, 31337, 65535, u32::MAX] {
        assert!(!OscRoutes::has_builtin(code), "OSC {code}");
    }
    assert_eq!(OscRoutes::BUILTIN.len(), 18);
    // Every built-in must be inside the bitmap, which is what the const
    // assertion in the crate relies on.
    for code in OscRoutes::BUILTIN {
        assert!(code < 2048, "OSC {code} is outside the bitmap");
    }
}

/// The lookup has no per-call state and no scan-order dependence: asking in a
/// different order, or many times, cannot change the answer.
#[test]
fn r_get_is_a_pure_function_of_the_bits() {
    let mut routes = OscRoutes::new();
    routes.route(9, OscRoute::BuiltinAndForward);
    routes.route(20308, OscRoute::Forward);
    let forwards: Vec<OscRoute> = (0..2100).map(|code| routes.get(code)).collect();
    let backwards: Vec<OscRoute> = (0..2100).rev().map(|code| routes.get(code)).rev().collect();
    assert_eq!(forwards, backwards);
    for _ in 0..8 {
        assert_eq!(routes.get(9), OscRoute::BuiltinAndForward);
        assert_eq!(routes.get(20308), OscRoute::Forward);
        assert_eq!(routes.get(2047), OscRoute::Drop);
    }
}

// ── Finding 6: route() normalisation ───────────────────────────────────────

#[test]
fn r_route_writes_the_bits_of_the_route_the_number_actually_gets() {
    // `Drop` on a number with no built-in writes nothing at all.
    let mut routes = OscRoutes::new();
    for code in [633u32, 1337, 2048, 31337, u32::MAX] {
        routes.route(code, OscRoute::Drop);
    }
    assert_eq!(routes, OscRoutes::new());
    assert_eq!(routes.overrides().count(), 0);

    // `BuiltinAndForward` on a number with no built-in is `Forward`, and the
    // two spellings build the same table — in the bitmap and in the spill.
    for code in [633u32, 31337] {
        let mut asked = OscRoutes::new();
        asked.route(code, OscRoute::BuiltinAndForward);
        let mut got = OscRoutes::new();
        got.route(code, OscRoute::Forward);
        assert_eq!(asked.get(code), OscRoute::Forward, "OSC {code}");
        assert_eq!(asked, got, "OSC {code}");
    }

    // A spilled number routed away and back leaves no trace.
    let mut routes = OscRoutes::new();
    routes.route(31337, OscRoute::Forward);
    assert_ne!(routes, OscRoutes::new());
    routes.route(31337, OscRoute::Drop);
    assert_eq!(routes, OscRoutes::new(), "the spill entry was removed");

    // A built-in routed away and back likewise.
    let mut routes = OscRoutes::new();
    routes.route(7, OscRoute::Forward);
    routes.route(7, OscRoute::Drop);
    routes.route(7, OscRoute::Builtin);
    assert_eq!(routes, OscRoutes::new());

    // `Drop` on a built-in is a real change and stays visible.
    let mut routes = OscRoutes::new();
    routes.route(7, OscRoute::Drop);
    assert_ne!(routes, OscRoutes::new());
    assert_eq!(
        routes.overrides().collect::<Vec<_>>(),
        vec![(7, OscRoute::Drop)]
    );
}

/// Equality is now semantic, so any two tables built by different routes to the
/// same answer must compare equal. Brute-forced over every route on a mixed set
/// of numbers.
#[test]
fn r_equal_behaviour_means_equal_tables() {
    const KINDS: [OscRoute; 4] = [
        OscRoute::Builtin,
        OscRoute::BuiltinAndForward,
        OscRoute::Forward,
        OscRoute::Drop,
    ];
    for code in [0u32, 7, 9, 633, 2048, 31337] {
        let mut built: Vec<(OscRoute, OscRoutes)> = Vec::new();
        for kind in KINDS {
            if kind == OscRoute::Builtin && !OscRoutes::has_builtin(code) {
                continue; // the documented debug assertion
            }
            let mut table = OscRoutes::new();
            table.route(code, kind);
            let effective = table.get(code);
            for (other_effective, other) in &built {
                if *other_effective == effective {
                    assert_eq!(&table, other, "OSC {code} via {kind:?}");
                } else {
                    assert_ne!(&table, other, "OSC {code} via {kind:?}");
                }
            }
            built.push((effective, table));
        }
    }
}

// ── Finding 4: the arena assembly must not grow the batch ──────────────────

fn steady(mut feed: impl FnMut(&mut Run), warm: usize, then: usize) {
    let mut run = Run::with({
        let mut routes = OscRoutes::new();
        routes.route(20308, OscRoute::Forward).large(20308, true);
        routes.route(9, OscRoute::BuiltinAndForward).large(9, true);
        routes.large(52, true);
        routes
    });
    for _ in 0..warm {
        feed(&mut run);
    }
    let before = run.caps();
    for _ in 0..then {
        feed(&mut run);
    }
    assert_eq!(run.caps(), before, "the batch grew after warm-up");
}

/// The case the packet documents as the one repair: a percent escape that
/// decodes to bytes that are not UTF-8. `finish_lossy` allocates a `String`
/// there, but the *batch* must still reach a steady state.
#[test]
fn r_osc_7_with_an_invalid_utf8_escape_reaches_a_steady_state() {
    assert_eq!(one("7;file:///tmp/%C3%A9%C3"), vec!["Cwd(|/tmp/é\u{FFFD})"]);
    steady(
        |run| {
            run.feed(osc("7;file:///tmp/%C3%A9%C3").as_bytes());
        },
        256,
        4096,
    );
}

/// OSC 0/2 keep one owned `String` for `Terminal::title`; the batch must not
/// grow for it.
#[test]
fn r_titles_reach_a_steady_state() {
    steady(
        |run| {
            run.feed(osc("0;a title with ; in it  ").as_bytes());
            run.feed(osc("2;another").as_bytes());
        },
        256,
        2048,
    );
    assert_eq!(
        one("0;a title with ; in it  "),
        vec!["Title(a title with ; in it)"]
    );
}

/// Every new arm, alternating, so a per-kind growth cannot hide behind a
/// single-kind warm-up.
#[test]
fn r_alternating_kinds_reach_a_steady_state() {
    let kinds = [
        "7;file://host/home/me/My%20Docs",
        "0;title",
        "1;icon",
        "2;other",
        "9;a notification; with a semicolon",
        "9;4;1;10",
        "22;text",
        "50;CursorShape=1",
        "133;A",
        "133;D;0",
        "20308;1;eyJ2IjoxfQ==",
        "7;file:///C:/Users/me",
        "7;/bare/path",
        "9;7;eyJ2IjoxfQ==",
    ];
    steady(
        |run| {
            for kind in kinds {
                run.feed(osc(kind).as_bytes());
            }
        },
        64,
        512,
    );
}

/// A 2 MiB body on the wrapped route: both events, the body intact, and the
/// batch bounded by the payload rather than by the number of sequences.
#[test]
fn r_a_2_mib_osc_9_body_under_builtin_and_forward() {
    let mut routes = OscRoutes::new();
    routes.route(9, OscRoute::BuiltinAndForward).large(9, true);
    let mut run = Run::with(routes);
    let body = "b".repeat(2 * 1024 * 1024);

    let stats = run.feed(osc(&format!("9;{body}")).as_bytes());
    assert_eq!(stats.truncated_osc, 0, "2 MiB fits under the 8 MiB tier");
    let tags = run.tags();
    assert_eq!(tags.len(), 2);
    assert_eq!(tags[0], format!("Notification(|{body})"));
    assert!(tags[1].starts_with("Osc(9:"));

    // Repeating it does not accumulate: the batch is cleared between feeds and
    // the arena settles at one payload's worth.
    let settled = run.batch.arena_capacity();
    for _ in 0..4 {
        run.feed(osc(&format!("9;{body}")).as_bytes());
    }
    assert_eq!(run.batch.arena_capacity(), settled);
}

// ── Finding 10: OSC 7 and invalid UTF-8 ────────────────────────────────────

#[test]
fn r_osc_7_drops_a_non_utf8_url_and_counts_it() {
    for tail in [
        &[0xC3u8][..],           // a lone lead byte
        &[0x80][..],             // a lone continuation byte
        &[0xFF, 0xFE][..],       // never valid
        &[0xED, 0xA0, 0x80][..], // a surrogate
    ] {
        let mut run = Run::plain();
        let mut bytes = b"\x1b]7;file:///tmp/".to_vec();
        bytes.extend_from_slice(tail);
        bytes.push(0x07);
        let stats = run.feed(&bytes);
        assert_eq!(run.tags(), Vec::<String>::new(), "{tail:?}");
        assert_eq!(stats.unhandled_sequences, 1, "{tail:?}");
    }
    // A percent escape is decoded after the URL was accepted, so it stays
    // lenient — the documented split.
    assert_eq!(one("7;file:///tmp/%FF"), vec!["Cwd(|/tmp/\u{FFFD})"]);
    assert_eq!(one("7;file:///tmp/%80x"), vec!["Cwd(|/tmp/\u{FFFD}x)"]);
    // And a valid escape still decodes.
    assert_eq!(one("7;file:///home/%C3%A9t%C3%A9"), vec!["Cwd(|/home/été)"]);
    assert_eq!(
        one("7;file:///tmp/100%25/x%zz"),
        vec!["Cwd(|/tmp/100%/x%zz)"]
    );
    // The drive-slash rule survives the span-cut rewrite.
    assert_eq!(one("7;file:///C:/x"), vec!["Cwd(|C:/x)"]);
    assert_eq!(one("7;file:///C:"), vec!["Cwd(|C:)"]);
    assert_eq!(one("7;file:///C:\\x"), vec!["Cwd(|C:\\x)"]);
    assert_eq!(one("7;/Cx/y"), vec!["Cwd(|/Cx/y)"]);
    // A decoded drive slash, so the cut runs after the decode.
    assert_eq!(one("7;file:///C%3A/x"), vec!["Cwd(|C:/x)"]);
    // A host that is not UTF-8 is caught by the URL check, not by the host span.
    assert_eq!(one("7;file://host/a"), vec!["Cwd(host|/a)"]);
}

// ── Finding 14: the OSC 9;4 clamp ──────────────────────────────────────────

#[test]
fn r_osc_9_4_clamps_instead_of_falling_back_to_zero() {
    assert_eq!(one("9;4;1;1000"), vec!["Progress(Set(100))"]);
    assert_eq!(one("9;4;1;250"), vec!["Progress(Set(100))"]);
    assert_eq!(one("9;4;1;101"), vec!["Progress(Set(100))"]);
    assert_eq!(one("9;4;1;100"), vec!["Progress(Set(100))"]);
    assert_eq!(one("9;4;2;4294967295"), vec!["Progress(Error(100))"]);
    assert_eq!(one("9;4;4;99"), vec!["Progress(Paused(99))"]);
    // A percentage that does not fit a `u32` is not a number: back to 0.
    assert_eq!(one("9;4;1;99999999999999999999"), vec!["Progress(Set(0))"]);

    // A state above the five defined ones is counted, never silently `Remove`.
    for seq in ["9;4;5", "9;4;255", "9;4;256", "9;4;300", "9;4;4294967295"] {
        let mut run = Run::plain();
        let stats = run.feed(osc(seq).as_bytes());
        assert_eq!(run.tags(), Vec::<String>::new(), "{seq}");
        assert_eq!(stats.unhandled_sequences, 1, "{seq}");
    }
    // A state that is not a number still reads as 0, as the adapter did.
    assert_eq!(one("9;4;x;1"), vec!["Progress(Remove)"]);
    assert_eq!(one("9;4"), vec!["Progress(Remove)"]);
}

// ── Hostile input, again, against the reworked arms ────────────────────────

#[test]
fn r_the_reworked_arms_never_panic_and_always_count() {
    let mut rng: u64 = 0xA5A5_0098_1234_5678;
    let mut next = move || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    const KINDS: [OscRoute; 4] = [
        OscRoute::Builtin,
        OscRoute::BuiltinAndForward,
        OscRoute::Forward,
        OscRoute::Drop,
    ];
    const BIAS: &[u8] = b"\x1b]0123456789;:%file:/\\C\x07\\\xc3\xa9\xff\x80";

    for round in 0..64u64 {
        let mut routes = OscRoutes::new();
        for _ in 0..24 {
            let code = (next() % 40000) as u32;
            let mut route = KINDS[(next() % 4) as usize];
            if route == OscRoute::Builtin && !OscRoutes::has_builtin(code) {
                route = OscRoute::Forward;
            }
            routes.route(code, route);
            if next() % 3 == 0 && routes.get(code) != OscRoute::Drop {
                routes.large(code, true);
            }
        }
        let mut run = Run::with(routes);
        let mut input = vec![0u8; 8192];
        for (index, byte) in input.iter_mut().enumerate() {
            let value = (next() >> 29) as u8;
            *byte = if index % 2 == 0 {
                BIAS[usize::from(value) % BIAS.len()]
            } else {
                value
            };
        }
        for chunk in input.chunks(1 + (round as usize) * 13) {
            let stats = run.term.feed(chunk, &mut run.batch, Instant::now());
            assert_eq!(stats.bytes, chunk.len());
            assert!(stats.unhandled_sequences <= chunk.len() as u32);
            let _ = run.tags();
            run.batch.clear();
        }
    }
}
