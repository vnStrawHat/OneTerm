//! Independent verification of `OscRoutes` and the built-in OSC handlers.
//!
//! Written against the crate's **public** API only, by somebody who had not
//! read the in-crate test module, so the two suites fail for different reasons.
//! Adopted verbatim apart from the three cases the verification itself changed:
//! table equality, a non-UTF-8 `OSC 7` URL, and the `OSC 9;4` percentage.

use oneterm_vt::{Config, EventBatch, OscRoute, OscRoutes, Size, Terminal, VtEvent};
use std::time::Instant;

// ── harness ────────────────────────────────────────────────────────────────

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

    /// Every event except `Repaint`, rendered as a stable tag + payload, so a
    /// whole sequence can be asserted at once (order included).
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
}

fn osc(seq: &str) -> String {
    format!("\x1b]{seq}\x07")
}

// ── 2(a): the four routes on a built-in and on an unknown number ───────────

#[test]
fn v_builtin_route_on_a_builtin_and_on_an_unknown_number() {
    // Built-in number, default route: typed event only.
    let mut run = Run::plain();
    let stats = run.feed(osc("0;hi").as_bytes());
    assert_eq!(run.tags(), vec!["Title(hi)"]);
    assert_eq!(stats.unhandled_sequences, 0);

    // Unknown number, default route: Drop. One count, no event.
    let mut run = Run::plain();
    let stats = run.feed(osc("633;hi").as_bytes());
    assert_eq!(run.tags(), Vec::<String>::new());
    assert_eq!(stats.unhandled_sequences, 1);
}

#[test]
fn v_forward_route_on_a_builtin_and_on_an_unknown_number() {
    let mut routes = OscRoutes::new();
    routes.route(0, OscRoute::Forward);
    routes.route(633, OscRoute::Forward);

    let mut run = Run::with(routes.clone());
    let stats = run.feed(osc("0;hi").as_bytes());
    assert_eq!(run.tags(), vec!["Osc(0:0|hi)"]);
    assert_eq!(run.term.title(), None, "the built-in did not run");
    assert_eq!(stats.unhandled_sequences, 0);

    let mut run = Run::with(routes);
    let stats = run.feed(osc("633;hi").as_bytes());
    assert_eq!(run.tags(), vec!["Osc(633:633|hi)"]);
    assert_eq!(stats.unhandled_sequences, 0);
}

#[test]
fn v_builtin_and_forward_is_typed_first_raw_second() {
    let mut routes = OscRoutes::new();
    routes.route(0, OscRoute::BuiltinAndForward);
    let mut run = Run::with(routes);
    run.feed(osc("0;hi").as_bytes());
    assert_eq!(run.tags(), vec!["Title(hi)", "Osc(0:0|hi)"]);
    assert_eq!(run.term.title(), Some("hi"));

    // Same on a non-built-in: BuiltinAndForward collapses to Forward.
    let mut routes = OscRoutes::new();
    routes.route(633, OscRoute::BuiltinAndForward);
    assert_eq!(routes.get(633), OscRoute::Forward);
    let mut run = Run::with(routes);
    run.feed(osc("633;hi").as_bytes());
    assert_eq!(run.tags(), vec!["Osc(633:633|hi)"]);
}

#[test]
fn v_drop_route_on_a_builtin_and_on_an_unknown_number() {
    let mut routes = OscRoutes::new();
    routes.route(0, OscRoute::Drop);
    routes.route(633, OscRoute::Drop);

    let mut run = Run::with(routes.clone());
    let stats = run.feed(osc("0;hi").as_bytes());
    assert_eq!(run.tags(), Vec::<String>::new());
    assert_eq!(run.term.title(), None);
    assert_eq!(stats.unhandled_sequences, 1, "2(b): Drop is counted");

    let mut run = Run::with(routes);
    let stats = run.feed(osc("633;hi").as_bytes());
    assert_eq!(run.tags(), Vec::<String>::new());
    assert_eq!(stats.unhandled_sequences, 1);
}

// ── 2(c): the two debug assertions ─────────────────────────────────────────

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "has no built-in handler")]
fn v_route_builtin_on_a_non_builtin_asserts_in_debug() {
    OscRoutes::new().route(633, OscRoute::Builtin);
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "is dropped")]
fn v_large_on_a_dropped_number_asserts_in_debug() {
    OscRoutes::new().large(633, true);
}

/// In release the same two calls are inert, and `get` reports what really
/// happens. Run only when this test binary is built without debug assertions.
#[cfg(not(debug_assertions))]
#[test]
fn v_the_two_assertions_are_inert_in_release() {
    let mut routes = OscRoutes::new();
    routes.route(633, OscRoute::Builtin);
    assert_eq!(routes.get(633), OscRoute::Drop);
    routes.large(633, true);
    assert!(routes.allows_large(633));
    // And the engine still drops it.
    let mut run = Run::with(routes);
    assert_eq!(run.feed(osc("633;x").as_bytes()).unhandled_sequences, 1);
}

// ── 2(d): overrides() ──────────────────────────────────────────────────────

#[test]
fn v_overrides_lists_exactly_the_changed_numbers() {
    let mut routes = OscRoutes::new();
    assert_eq!(routes.overrides().count(), 0);

    routes.route(9, OscRoute::BuiltinAndForward);
    routes.route(20308, OscRoute::Forward);
    routes.large(52, true); // a ceiling is not a route change
    assert_eq!(
        routes.overrides().collect::<Vec<_>>(),
        vec![(9, OscRoute::BuiltinAndForward), (20308, OscRoute::Forward)]
    );

    // Routing a number back to its default removes it from the list, in both
    // the bitmap and the spill.
    routes.route(9, OscRoute::Builtin);
    routes.route(20308, OscRoute::Drop);
    assert_eq!(routes.overrides().count(), 0);

    // A dropped built-in is an override; a dropped unknown number is not.
    let mut routes = OscRoutes::new();
    routes.route(7, OscRoute::Drop);
    routes.route(633, OscRoute::Drop);
    assert_eq!(
        routes.overrides().collect::<Vec<_>>(),
        vec![(7, OscRoute::Drop)]
    );
}

// ── 2(e): the spill behaves like the bitmap ────────────────────────────────

#[test]
fn v_numbers_above_the_bitmap_behave_identically() {
    for &code in &[2048u32, 2049, 20308, 31337, u32::MAX] {
        let mut routes = OscRoutes::new();
        assert_eq!(routes.get(code), OscRoute::Drop);
        routes.route(code, OscRoute::Forward);
        assert_eq!(routes.get(code), OscRoute::Forward);
        assert_eq!(
            routes.overrides().collect::<Vec<_>>(),
            vec![(code, OscRoute::Forward)]
        );
        routes.large(code, true);
        assert!(routes.allows_large(code));

        let mut run = Run::with(routes.clone());
        run.feed(osc(&format!("{code};hi")).as_bytes());
        assert_eq!(run.tags(), vec![format!("Osc({code}:{code}|hi)")]);

        // And back to the default.
        routes.route(code, OscRoute::Drop);
        assert_eq!(routes.overrides().count(), 0);
    }
}

/// Two tables that route every number the same way are the same table.
///
/// The verification found this **false**: `route()` stored the bits of the
/// route that was asked for rather than the one the number would get, so
/// `route(633, Drop)` on a number that was already dropped compared unequal to
/// an untouched table, and a spilled number routed back to its default stayed
/// in the spill list for ever. Fixed; this is the pin.
#[test]
fn v_table_equality_is_semantic() {
    let default = OscRoutes::new();

    let mut redundant = OscRoutes::new();
    redundant.route(633, OscRoute::Drop); // already the default for 633
    assert_eq!(redundant.get(633), default.get(633));
    assert_eq!(redundant, default, "same behaviour, same value");

    let mut spilled = OscRoutes::new();
    spilled.route(31337, OscRoute::Drop);
    assert_eq!(spilled.get(31337), default.get(31337));
    assert_eq!(spilled, default);

    // Asking for a built-in route on a number with no built-in gets `Forward`,
    // and is the same table as asking for `Forward`.
    let mut asked = OscRoutes::new();
    asked.route(633, OscRoute::BuiltinAndForward);
    let mut got = OscRoutes::new();
    got.route(633, OscRoute::Forward);
    assert_eq!(asked.get(633), OscRoute::Forward);
    assert_eq!(asked, got);

    // `overrides()` is the semantic view and agrees with `get()`.
    assert_eq!(redundant.overrides().count(), 0);
    assert_eq!(spilled.overrides().count(), 0);
}

#[test]
fn v_the_spill_stays_sorted_and_deduped() {
    let mut routes = OscRoutes::new();
    for &code in &[40000u32, 3000, 9999, 3000, 2048] {
        routes.route(code, OscRoute::Forward);
    }
    let seen: Vec<u32> = routes.overrides().map(|(code, _)| code).collect();
    assert_eq!(seen, vec![2048, 3000, 9999, 40000]);
}

// ── 2(f): a cloned table keeps two terminals independent ───────────────────

#[test]
fn v_a_cloned_table_keeps_two_terminals_independent() {
    let mut routes = OscRoutes::new();
    routes.route(0, OscRoute::Forward);
    let mut a = Run::with(routes.clone());

    // Mutating the original after the clone must not reach `a`.
    routes.route(0, OscRoute::Builtin);
    let mut b = Run::with(routes.clone());

    a.feed(osc("0;one").as_bytes());
    b.feed(osc("0;two").as_bytes());
    assert_eq!(a.tags(), vec!["Osc(0:0|one)"]);
    assert_eq!(b.tags(), vec!["Title(two)"]);
    assert_eq!(a.term.title(), None);
    assert_eq!(b.term.title(), Some("two"));

    // Config equality survives the clone, which is what keeps it `PartialEq`.
    let mut same = OscRoutes::new();
    same.route(0, OscRoute::Builtin);
    assert_eq!(routes, same);
}

// ── 2(g): route changes between feeds, no state leakage ────────────────────

#[test]
fn v_a_route_change_between_feeds_takes_effect_on_the_next_osc() {
    // `Config` is only read at construction, so the honest form of "change the
    // route between feeds" is a second terminal fed the same continuation.
    // What must not leak is parser state: a half-fed OSC.
    let mut routes = OscRoutes::new();
    routes.route(0, OscRoute::Forward);
    let mut run = Run::with(routes);
    // First feed leaves the parser inside an OSC.
    let stats = run.feed(b"\x1b]0;par");
    assert_eq!(run.tags(), Vec::<String>::new(), "nothing is emitted yet");
    assert_eq!(stats.unhandled_sequences, 0);
    // The continuation completes it under the same route.
    run.feed(b"tial\x07");
    assert_eq!(run.tags(), vec!["Osc(0:0|partial)"]);
    // And the next OSC is unaffected by the previous one's state.
    run.feed(osc("0;second").as_bytes());
    assert_eq!(run.tags(), vec!["Osc(0:0|second)"]);
}

// ── 3: the built-ins ───────────────────────────────────────────────────────

fn one(seq: &str) -> Vec<String> {
    let mut run = Run::plain();
    run.feed(osc(seq).as_bytes());
    run.tags()
}

#[test]
fn v_osc_7_forms() {
    assert_eq!(one("7;file://host/path"), vec!["Cwd(host|/path)"]);
    assert_eq!(one("7;file:///C:/x"), vec!["Cwd(|C:/x)"]);
    assert_eq!(one("7;file:///C:"), vec!["Cwd(|C:)"]);
    assert_eq!(
        one("7;file://host/home/me/My%20Docs"),
        vec!["Cwd(host|/home/me/My Docs)"]
    );
    assert_eq!(one("7;/tmp/x"), vec!["Cwd(|/tmp/x)"], "no scheme");
    assert_eq!(
        one("7;file:///tmp/100%25/x%zz"),
        vec!["Cwd(|/tmp/100%/x%zz)"]
    );
    assert_eq!(one("7;file:///home/%C3%A9t%C3%A9"), vec!["Cwd(|/home/été)"]);
    assert_eq!(one("7;/Cx/y"), vec!["Cwd(|/Cx/y)"], "not a drive letter");
    // `file://host` with no path: the host is not split out. Documented shape,
    // pinned here because the adapter only ever reads `path`.
    assert_eq!(one("7;file://host"), vec!["Cwd(|host)"]);
    // Escaped `..` is decoded but never resolved: that is the embedder's job.
    assert_eq!(
        one("7;file://evil/%2e%2e/%2e%2e/etc"),
        vec!["Cwd(evil|/../../etc)"]
    );
    // No payload at all.
    let mut run = Run::plain();
    assert_eq!(run.feed(osc("7").as_bytes()).unhandled_sequences, 1);
    assert_eq!(run.tags(), Vec::<String>::new());
}

#[test]
fn v_osc_7_hostile_8_mib_payload_hits_the_ceiling_and_is_refused() {
    // OSC 7 has no `large` opt-in, so the parser caps it at OSC_INLINE and the
    // built-in refuses a truncated path rather than reporting a cut one.
    let mut run = Run::plain();
    let payload = "a".repeat(8 * 1024 * 1024);
    let stats = run.feed(osc(&format!("7;file:///{payload}")).as_bytes());
    assert_eq!(stats.truncated_osc, 1);
    assert_eq!(stats.unhandled_sequences, 1);
    assert_eq!(run.tags(), Vec::<String>::new());
}

#[test]
fn v_osc_7_utf8_boundary_cut_never_panics() {
    // A payload whose truncation lands mid-codepoint, both with and without a
    // ceiling that lets it through.
    let mut run = Run::plain();
    let body = "é".repeat(8 * 1024);
    let stats = run.feed(osc(&format!("7;file:///{body}")).as_bytes());
    assert_eq!(stats.truncated_osc, 1);
    assert_eq!(run.tags(), Vec::<String>::new());

    // With the ceiling raised, the whole thing is parsed and reported.
    let mut routes = OscRoutes::new();
    routes.large(7, true);
    let mut run = Run::with(routes);
    let stats = run.feed(osc(&format!("7;file:///{body}")).as_bytes());
    assert_eq!(stats.truncated_osc, 0);
    assert_eq!(run.tags(), vec![format!("Cwd(|/{body})")]);

    // Raw bytes that are not UTF-8 at all: dropped and counted, never a panic.
    // The verification found this reporting a lossy path where the adapter it
    // replaced dropped the sequence; the adapter's behaviour was restored.
    let mut run = Run::plain();
    let mut bytes = b"\x1b]7;file:///tmp/".to_vec();
    bytes.extend_from_slice(&[0xC3, 0xA9, 0xC3]); // a trailing lead byte
    bytes.push(0x07);
    let stats = run.feed(&bytes);
    assert_eq!(run.tags(), Vec::<String>::new());
    assert_eq!(stats.unhandled_sequences, 1);

    // A percent escape that decodes to something that is not UTF-8 is still
    // reported leniently: by then the sequence has been accepted, and a
    // directory name is not required to be UTF-8.
    let mut run = Run::plain();
    run.feed(osc("7;file:///tmp/%C3%A9%C3").as_bytes());
    assert_eq!(run.tags(), vec!["Cwd(|/tmp/é\u{FFFD})"]);
}

#[test]
fn v_osc_9_4_progress_states_and_out_of_range() {
    assert_eq!(one("9;4;0"), vec!["Progress(Remove)"]);
    assert_eq!(one("9;4;1;50"), vec!["Progress(Set(50))"]);
    assert_eq!(one("9;4;2;80"), vec!["Progress(Error(80))"]);
    assert_eq!(one("9;4;3"), vec!["Progress(Indeterminate)"]);
    assert_eq!(one("9;4;4;10"), vec!["Progress(Paused(10))"]);
    assert_eq!(one("9;4;1;250"), vec!["Progress(Set(100))"], "clamped");
    // Out of range, non-numeric and overflowing states.
    for seq in ["9;4;5;10", "9;4;9;50", "9;4;255", "9;4;300", "9;4;x;1"] {
        let mut run = Run::plain();
        let stats = run.feed(osc(seq).as_bytes());
        if seq == "9;4;x;1" {
            // A non-numeric state parses as 0 (`unwrap_or(0)`), which is
            // `Remove`. Pinned as the observed behaviour, matching the adapter.
            assert_eq!(run.tags(), vec!["Progress(Remove)"], "{seq}");
        } else {
            assert_eq!(run.tags(), Vec::<String>::new(), "{seq}");
            assert_eq!(stats.unhandled_sequences, 1, "{seq}");
        }
    }
    // A percentage that does not fit a `u8` clamps like any other. The
    // verification found this falling back to `Set(0)`, because both fields
    // were parsed as `u8` and the parse failed before the clamp could run; the
    // sequence's own definition says "clamped", and the engine now does that.
    assert_eq!(one("9;4;1;1000"), vec!["Progress(Set(100))"]);
    // Bare `9;4`: no state at all.
    assert_eq!(one("9;4"), vec!["Progress(Remove)"]);
}

#[test]
fn v_osc_9_notification_with_semicolons_in_the_body() {
    assert_eq!(
        one("9;done: 3 tests; 0 failed"),
        vec!["Notification(|done: 3 tests; 0 failed)"]
    );
    assert_eq!(one("9;71 bottles"), vec!["Notification(|71 bottles)"]);
    assert_eq!(one("9;7"), vec!["Notification(|7)"]);
    assert_eq!(one("9;7;payload"), vec!["Notification(|7;payload)"]);
    // Empty body, and no body at all, are both counted.
    for seq in ["9;", "9"] {
        let mut run = Run::plain();
        assert_eq!(
            run.feed(osc(seq).as_bytes()).unhandled_sequences,
            1,
            "{seq}"
        );
        assert_eq!(run.tags(), Vec::<String>::new(), "{seq}");
    }
}

#[test]
fn v_osc_133_markers_and_exit_codes() {
    assert_eq!(one("133;A"), vec!["ShellMark(PromptStart)"]);
    assert_eq!(one("133;B"), vec!["ShellMark(PromptEnd)"]);
    assert_eq!(one("133;C"), vec!["ShellMark(OutputStart)"]);
    assert_eq!(
        one("133;D"),
        vec!["ShellMark(OutputEnd { exit_code: None })"]
    );
    assert_eq!(
        one("133;D;0"),
        vec!["ShellMark(OutputEnd { exit_code: Some(0) })"]
    );
    assert_eq!(
        one("133;D;-1"),
        vec!["ShellMark(OutputEnd { exit_code: Some(-1) })"],
        "a negative exit code parses"
    );
    assert_eq!(
        one("133;D;not-a-number"),
        vec!["ShellMark(OutputEnd { exit_code: None })"]
    );
    assert_eq!(
        one("133;D;99999999999999999999"),
        vec!["ShellMark(OutputEnd { exit_code: None })"],
        "an i32 overflow is not an exit code"
    );
    // The whole parameter is the sub-code now: `Abc` is not `A`.
    for seq in ["133;X", "133;Z;foo", "133;Abc", "133;", "133"] {
        let mut run = Run::plain();
        assert_eq!(
            run.feed(osc(seq).as_bytes()).unhandled_sequences,
            1,
            "{seq}"
        );
        assert_eq!(run.tags(), Vec::<String>::new(), "{seq}");
    }
}

#[test]
fn v_osc_22_and_50_values() {
    assert_eq!(one("22;pointer"), vec!["Pointer(pointer)"]);
    assert_eq!(one("22;text"), vec!["Pointer(text)"]);
    assert_eq!(
        one("22;;"),
        Vec::<String>::new(),
        "an empty shape name is not a shape"
    );
    let mut run = Run::plain();
    assert_eq!(run.feed(osc("22").as_bytes()).unhandled_sequences, 1);

    assert_eq!(one("50;CursorShape=1"), vec!["CursorStyleChanged"]);
    assert_eq!(one("50;CursorShape=2"), vec!["CursorStyleChanged"]);
    let mut run = Run::plain();
    let stats = run.feed(osc("50;nonsense").as_bytes());
    assert_eq!(run.tags(), Vec::<String>::new());
    assert_eq!(stats.unhandled_sequences, 1);
}

#[test]
fn v_osc_1_versus_0_and_2_title_interplay() {
    // OSC 1 is the icon name and must not touch the title.
    let mut run = Run::plain();
    run.feed(osc("2;the title").as_bytes());
    assert_eq!(run.tags(), vec!["Title(the title)"]);
    run.feed(osc("1;the icon").as_bytes());
    assert_eq!(run.tags(), vec!["IconName(the icon)"]);
    assert_eq!(
        run.term.title(),
        Some("the title"),
        "OSC 1 did not overwrite the title"
    );
    run.feed(osc("0;both").as_bytes());
    assert_eq!(
        run.tags(),
        vec!["Title(both)"],
        "OSC 0 nominally sets icon+title; the engine reports only the title"
    );
    assert_eq!(run.term.title(), Some("both"));
    // Rejoining and trimming, the same rule OSC 0/2 use.
    assert_eq!(one("1;  a;b  "), vec!["IconName(a;b)"]);
    let mut run = Run::plain();
    assert_eq!(run.feed(osc("1").as_bytes()).unhandled_sequences, 1);
}

// ── 3/hostile: counted, never a panic ──────────────────────────────────────

#[test]
fn v_hostile_osc_input_is_counted_and_never_panics() {
    let numbers = [
        "",
        "0",
        "1",
        "2",
        "4",
        "7",
        "8",
        "9",
        "22",
        "50",
        "52",
        "104",
        "133",
        "633",
        "20308",
        "4294967295",
        "99999999999999999999",
        "007",
        "020308",
        "-1",
        "x",
    ];
    let bodies = [
        "",
        ";",
        ";;;;",
        ";4",
        ";4;9;999",
        ";D;\u{FFFD}",
        ";file://",
        ";file:///%",
        ";\u{1}\u{7f}",
        ";A",
        ";\u{00e9}",
    ];
    let route_kinds = [
        OscRoute::Builtin,
        OscRoute::BuiltinAndForward,
        OscRoute::Forward,
        OscRoute::Drop,
    ];
    for (index, number) in numbers.iter().enumerate() {
        for body in bodies {
            for (kind, route) in route_kinds.iter().enumerate() {
                let mut routes = OscRoutes::new();
                let code = (index as u32 * 7 + kind as u32) % 300;
                if *route != OscRoute::Builtin || OscRoutes::has_builtin(code) {
                    routes.route(code, *route);
                }
                let mut run = Run::with(routes);
                let stats = run.feed(format!("\x1b]{number}{body}\x07").as_bytes());
                // Never more sequences than bytes, and never a panic.
                assert!(stats.unhandled_sequences <= stats.bytes as u32);
                assert!(stats.malformed_sequences <= stats.bytes as u32);
                let _ = run.tags();
            }
        }
    }
}

/// 64 random tables x 64 random byte streams, the stand-in the packet claims
/// for the fuzz hour. Independent of the implementer's own random test.
#[test]
fn v_random_tables_and_random_bytes_never_panic() {
    let mut rng: u64 = 0xDEAD_BEEF_0098_0098;
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
    const BIAS: &[u8] = b"\x1b]0123456789;:PX_\x07\\file://%C";

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
            let value = (next() >> 27) as u8;
            *byte = if index % 2 == 0 {
                BIAS[usize::from(value) % BIAS.len()]
            } else {
                value
            };
        }
        for chunk in input.chunks(1 + (round as usize) * 11) {
            let stats = run.term.feed(chunk, &mut run.batch, Instant::now());
            assert_eq!(stats.bytes, chunk.len());
            assert!(stats.unhandled_sequences <= chunk.len() as u32);
            let _ = run.tags();
            run.batch.clear();
        }
    }
}

// ── 4: OneTerm's shipped table as a black box ──────────────────────────────

/// The exact table `crates/terminal/src/handle.rs::adapter_config` builds.
fn oneterm_routes() -> OscRoutes {
    let mut routes = OscRoutes::new();
    routes.route(20308, OscRoute::Forward).large(20308, true);
    routes.route(9, OscRoute::BuiltinAndForward).large(9, true);
    routes.large(52, true);
    routes
}

#[test]
fn v_oneterm_shipped_table_emits_what_the_adapter_expects() {
    // OSC 20308: forwarded raw, never parsed.
    let mut run = Run::with(oneterm_routes());
    run.feed(osc("20308;1;eyJ2IjoxfQ==").as_bytes());
    assert_eq!(run.tags(), vec!["Osc(20308:20308|1|eyJ2IjoxfQ==)"]);

    // The support query.
    let mut run = Run::with(oneterm_routes());
    run.feed(osc("20308;0").as_bytes());
    assert_eq!(run.tags(), vec!["Osc(20308:20308|0)"]);

    // OSC 9;7: a bogus Notification whose body starts `7;`, then the raw bytes.
    let mut run = Run::with(oneterm_routes());
    run.feed(osc("9;7;eyJ2IjoxfQ==").as_bytes());
    assert_eq!(
        run.tags(),
        vec!["Notification(|7;eyJ2IjoxfQ==)", "Osc(9:9|7|eyJ2IjoxfQ==)"]
    );

    // A plain notification: the typed event plus a raw one the adapter ignores.
    let mut run = Run::with(oneterm_routes());
    run.feed(osc("9;hello").as_bytes());
    assert_eq!(run.tags(), vec!["Notification(|hello)", "Osc(9:9|hello)"]);

    // OSC 9;4 progress, likewise.
    let mut run = Run::with(oneterm_routes());
    run.feed(osc("9;4;1;50").as_bytes());
    assert_eq!(run.tags(), vec!["Progress(Set(50))", "Osc(9:9|4|1|50)"]);

    // A zero-padded number still routes: the parser reads the number, not the
    // spelling. The adapter's old `007;file:///tmp` literal did not move to any
    // new test, so it is pinned here.
    let mut run = Run::with(oneterm_routes());
    run.feed(osc("020308;0").as_bytes());
    assert_eq!(run.tags(), vec!["Osc(20308:020308|0)"]);
    let mut run = Run::with(oneterm_routes());
    run.feed(osc("007;file:///tmp").as_bytes());
    assert_eq!(run.tags(), vec!["Cwd(|/tmp)"]);
    let mut run = Run::with(oneterm_routes());
    run.feed(osc("0133;A").as_bytes());
    assert_eq!(run.tags(), vec!["ShellMark(PromptStart)"]);

    // The agent channel's published 8 KiB allowance survives the ceiling.
    let mut run = Run::with(oneterm_routes());
    let payload = "A".repeat(8 * 1024);
    let stats = run.feed(osc(&format!("20308;1;{payload}")).as_bytes());
    assert_eq!(stats.truncated_osc, 0);
    assert_eq!(run.tags(), vec![format!("Osc(20308:20308|1|{payload})")]);
}

/// The shipped table buys OSC 9 the 8 MiB tier for the agent alias's sake, and
/// the notification arm now *also* interns the whole body. A hostile 8 MiB
/// `OSC 9` therefore costs the join, the arena copy and the raw params at once.
/// It must still be bounded and must not panic.
#[test]
fn v_a_hostile_8_mib_osc_9_under_the_shipped_table_is_bounded() {
    let mut run = Run::with(oneterm_routes());
    let body = "n".repeat(8 * 1024 * 1024);
    let stats = run.feed(osc(&format!("9;{body}")).as_bytes());
    assert_eq!(stats.truncated_osc, 1, "the 8 MiB tier is itself a ceiling");
    let tags = run.tags();
    assert_eq!(tags.len(), 2, "the wrap still produces both events");
    assert!(tags[0].starts_with("Notification("));
    assert!(tags[1].starts_with("Osc(9:"));
    // The arena holds the body once for the typed event and once for the raw
    // params, so the batch is O(payload) and not O(1) here.
    assert!(run.batch.arena_capacity() >= body.len());
}

// ── 5: invariants ──────────────────────────────────────────────────────────

/// `Config` is `Clone` but **not** `PartialEq` — contrary to DEC-0017 and
/// `osc-extension.md`, which both assert it is. The table itself is both, which
/// is what the decision actually needed.
#[test]
fn v_config_is_clone_and_the_table_is_clone_plus_partial_eq() {
    let a = Config {
        osc_routes: oneterm_routes(),
        ..Config::default()
    };
    let b = a.clone();
    assert_eq!(a.osc_routes, b.osc_routes);
    assert_ne!(a.osc_routes, Config::default().osc_routes);
    // If `Config` ever gains `PartialEq`, this line stops compiling and the
    // docs can be believed again.
    let _: fn(&OscRoutes, &OscRoutes) -> bool = PartialEq::eq;
}

/// The hot path must not grow the batch's arenas once warm: an OSC per feed
/// interns into the same storage. (This replaces the deleted
/// `claimed_osc_reaches_the_batch_without_allocating_per_osc`.)
#[test]
fn v_the_osc_path_stops_growing_the_batch_once_warm() {
    let mut run = Run::with(oneterm_routes());
    for _ in 0..256 {
        run.batch.clear();
        run.term
            .feed(b"\x1b]20308;1;warm\x07", &mut run.batch, Instant::now());
    }
    let before = (
        run.batch.arena_capacity(),
        run.batch.param_capacity(),
        run.batch.event_capacity(),
    );
    for _ in 0..4096 {
        run.batch.clear();
        run.term
            .feed(b"\x1b]20308;1;warm\x07", &mut run.batch, Instant::now());
    }
    assert_eq!(
        (
            run.batch.arena_capacity(),
            run.batch.param_capacity(),
            run.batch.event_capacity()
        ),
        before,
        "the batch stopped growing"
    );
}

/// The same for the built-in arms that now allocate a `String` while parsing.
#[test]
fn v_the_builtin_arms_stop_growing_the_batch_once_warm() {
    let mut run = Run::plain();
    let feed = |run: &mut Run| {
        for seq in [
            "7;file:///home/me/My%20Docs",
            "9;a notification",
            "9;4;1;10",
            "133;A",
            "133;D;0",
            "1;icon",
            "22;text",
            "0;title",
        ] {
            run.batch.clear();
            run.term
                .feed(osc(seq).as_bytes(), &mut run.batch, Instant::now());
        }
    };
    for _ in 0..64 {
        feed(&mut run);
    }
    let before = (
        run.batch.arena_capacity(),
        run.batch.param_capacity(),
        run.batch.event_capacity(),
    );
    for _ in 0..1024 {
        feed(&mut run);
    }
    assert_eq!(
        (
            run.batch.arena_capacity(),
            run.batch.param_capacity(),
            run.batch.event_capacity()
        ),
        before
    );
}
