//! The parser's memory bounds, measured rather than argued.
//!
//! This is what survives the differential oracle `US-0073`'s verifier ran once
//! against the fork. That oracle retired with the fork at `US-0087`; the
//! chunking invariant it proved is pinned by
//! `parser::props::arbitrary_bytes_never_panic_and_chunking_is_invariant`, and
//! nothing in the workspace depends on a second parser any more.
//!
//! What could not move into a unit test is this file: the counting allocator has
//! to be a `#[global_allocator]`, which only an integration test can install.
//! It is the measured form of the intake's headline defect (`IN-0029` P1 — the
//! reference accumulates an OSC payload into an unbounded `Vec<u8>` reachable
//! from any SSH session).
//!
//! `ext_memory_stays_bounded` is `#[ignore]`d because that allocator is
//! process-wide, so anything running beside it pollutes the measurement. Run it
//! alone:
//!
//! ```text
//! cargo test -p oneterm-vt --release --test parser_limits -- \
//!     --ignored --nocapture --test-threads=1
//! ```

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use oneterm_vt::parser::{Dispatch, OscParams, Params, Parser, StringTerm};

// ---------- peak-heap tracking allocator ----------
struct Tracking;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Tracking {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            let now = LIVE.fetch_add(l.size(), Ordering::Relaxed) + l.size();
            PEAK.fetch_max(now, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, l, new) };
        if !q.is_null() {
            let now = LIVE.fetch_add(new, Ordering::Relaxed) + new;
            PEAK.fetch_max(now, Ordering::Relaxed);
            LIVE.fetch_sub(l.size(), Ordering::Relaxed);
        }
        q
    }
}

#[global_allocator]
static A: Tracking = Tracking;

fn measure<R>(f: impl FnOnce() -> R) -> (R, usize) {
    let base = LIVE.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);
    let r = f();
    (r, PEAK.load(Ordering::Relaxed).saturating_sub(base))
}

// ---------- sinks ----------

/// Records what the parser dispatched, so case (f) below can prove the recorder
/// — not the parser — is what holds the memory between rounds.
#[derive(Debug, PartialEq, Eq, Clone)]
enum Action {
    Print(char),
    Execute(u8),
    Esc(Vec<u8>, u8),
    Csi {
        params: Vec<Vec<u16>>,
        intermediates: Vec<u8>,
        ignore: bool,
        byte: u8,
    },
    Osc {
        params: Vec<Vec<u8>>,
        bel: bool,
        truncated: bool,
    },
    Hook {
        params: Vec<Vec<u16>>,
        intermediates: Vec<u8>,
        ignore: bool,
        byte: u8,
    },
    Put(u8),
    Unhook,
}

#[derive(Default)]
struct Recorder {
    actions: Vec<Action>,
    puts: usize,
}

impl Dispatch for Recorder {
    fn print_str(&mut self, text: &str) {
        self.actions.extend(text.chars().map(Action::Print));
    }
    fn execute(&mut self, byte: u8) {
        self.actions.push(Action::Execute(byte));
    }
    fn esc(&mut self, i: &[u8], _ig: bool, b: u8) {
        self.actions.push(Action::Esc(i.to_vec(), b));
    }
    fn csi(&mut self, p: &Params, i: &[u8], ignore: bool, b: u8) {
        self.actions.push(Action::Csi {
            params: p.groups().map(<[u16]>::to_vec).collect(),
            intermediates: i.to_vec(),
            ignore,
            byte: b,
        });
    }
    fn osc(&mut self, _c: Option<u32>, p: &OscParams<'_>, t: StringTerm, tr: bool) {
        self.actions.push(Action::Osc {
            params: p.iter().map(<[u8]>::to_vec).collect(),
            bel: t == StringTerm::Bel,
            truncated: tr,
        });
    }
    fn dcs_hook(&mut self, p: &Params, i: &[u8], b: u8) {
        self.actions.push(Action::Hook {
            params: p.groups().map(<[u16]>::to_vec).collect(),
            intermediates: i.to_vec(),
            ignore: p.ignored(),
            byte: b,
        });
    }
    fn dcs_put(&mut self, b: u8) {
        self.puts += 1;
        if self.puts <= 4096 {
            self.actions.push(Action::Put(b));
        }
    }
    fn dcs_unhook(&mut self, _a: bool) {
        self.actions.push(Action::Unhook);
    }
    fn apc_start(&mut self, _i: u8) {}
    fn apc_put(&mut self, _b: u8) {}
    fn apc_end(&mut self, _a: bool) {}
}

/// Keeps only the payload length, and claims the OSC numbers in `.1` as large.
struct Large(usize, Vec<u32>);
impl Dispatch for Large {
    fn print_str(&mut self, _t: &str) {}
    fn execute(&mut self, _b: u8) {}
    fn esc(&mut self, _i: &[u8], _ig: bool, _b: u8) {}
    fn csi(&mut self, _p: &Params, _i: &[u8], _ig: bool, _b: u8) {}
    fn osc(&mut self, _c: Option<u32>, p: &OscParams<'_>, _t: StringTerm, _tr: bool) {
        self.0 = p.iter().map(<[u8]>::len).sum::<usize>();
    }
    fn osc_allows_large(&self, code: u32) -> bool {
        self.1.contains(&code)
    }
    fn dcs_hook(&mut self, _p: &Params, _i: &[u8], _b: u8) {}
    fn dcs_put(&mut self, _b: u8) {}
    fn dcs_unhook(&mut self, _a: bool) {}
    fn apc_start(&mut self, _i: u8) {}
    fn apc_put(&mut self, _b: u8) {}
    fn apc_end(&mut self, _a: bool) {}
}

/// Counts streamed payload bytes and terminations without buffering any.
struct CountStream {
    puts: usize,
    ends: usize,
    aborted: usize,
}
impl Dispatch for CountStream {
    fn print_str(&mut self, _t: &str) {}
    fn execute(&mut self, _b: u8) {}
    fn esc(&mut self, _i: &[u8], _ig: bool, _b: u8) {}
    fn csi(&mut self, _p: &Params, _i: &[u8], _ig: bool, _b: u8) {}
    fn osc(&mut self, _c: Option<u32>, _p: &OscParams<'_>, _t: StringTerm, _tr: bool) {}
    fn dcs_hook(&mut self, _p: &Params, _i: &[u8], _b: u8) {}
    fn dcs_put(&mut self, _b: u8) {
        self.puts += 1;
    }
    fn dcs_unhook(&mut self, a: bool) {
        self.ends += 1;
        self.aborted += usize::from(a);
    }
    fn apc_start(&mut self, _i: u8) {}
    fn apc_put(&mut self, _b: u8) {
        self.puts += 1;
    }
    fn apc_end(&mut self, a: bool) {
        self.ends += 1;
        self.aborted += usize::from(a);
    }
}

#[test]
#[ignore = "the counting allocator is process-wide; run with --test-threads=1"]
fn ext_memory_stays_bounded() {
    const MIB: usize = 1024 * 1024;
    let chunk = vec![b'A'; 64 * 1024];
    let rounds = 20 * MIB / chunk.len();

    // (a) unterminated OSC, 20 MiB, unclaimed code
    let mut e = Recorder::default();
    let (_, peak_osc) = measure(|| {
        let mut p = Parser::new();
        p.advance(&mut e, b"\x1b]0;");
        for _ in 0..rounds {
            p.advance(&mut e, &chunk);
        }
        p
    });
    println!("(a) unterminated OSC 20 MiB, code 0 UNCLAIMED : peak heap = {peak_osc} B");

    // (b) unterminated OSC, 20 MiB, claimed large
    let mut lg = Large(0, vec![52]);
    let (_, peak_large) = measure(|| {
        let mut p = Parser::new();
        p.advance(&mut lg, b"\x1b]52;c;");
        for _ in 0..rounds {
            p.advance(&mut lg, &chunk);
        }
        p
    });
    println!(
        "(b) unterminated OSC 20 MiB, code 52 CLAIMED   : peak heap = {peak_large} B ({:.2} MiB)",
        peak_large as f64 / MIB as f64
    );

    // (c) unterminated DCS, 20 MiB
    let mut d = CountStream {
        puts: 0,
        ends: 0,
        aborted: 0,
    };
    let (_, peak_dcs) = measure(|| {
        let mut p = Parser::new();
        p.advance(&mut d, b"\x1bPq");
        for _ in 0..rounds {
            p.advance(&mut d, &chunk);
        }
        p
    });
    println!(
        "(c) unterminated DCS 20 MiB                    : peak heap = {peak_dcs} B; dcs_put = {} (cap {}); unhook = {} (aborted {})",
        d.puts,
        oneterm_vt::parser::DCS_MAX_BYTES,
        d.ends,
        d.aborted
    );

    // (d) unterminated APC, 20 MiB
    let mut a = CountStream {
        puts: 0,
        ends: 0,
        aborted: 0,
    };
    let (_, peak_apc) = measure(|| {
        let mut p = Parser::new();
        p.advance(&mut a, b"\x1b_G");
        for _ in 0..rounds {
            p.advance(&mut a, &chunk);
        }
        p
    });
    println!(
        "(d) unterminated APC 20 MiB                    : peak heap = {peak_apc} B; apc_put = {}; end = {} (aborted {})",
        a.puts, a.ends, a.aborted
    );

    // (e) retained heap after a claimed 9 MiB OSC completes
    let mut lg2 = Large(0, vec![52]);
    let mut p2 = Parser::new();
    let before_big = LIVE.load(Ordering::Relaxed);
    let big = vec![b'C'; 9 * MIB];
    p2.advance(&mut lg2, b"\x1b]52;c;");
    p2.advance(&mut lg2, &big);
    p2.advance(&mut lg2, b"\x07");
    drop(big);
    let after_big = LIVE.load(Ordering::Relaxed);
    println!(
        "(e) parser live heap after a completed 8 MiB claimed OSC: {} B (was {} B before)",
        after_big, before_big
    );
    println!("    payload kept = {} B", lg2.0);

    // (f) 1000 hostile rounds: no monotonic growth
    let mut e2 = Recorder::default();
    let mut p3 = Parser::new();
    let osc_body = vec![b'x'; 5000];
    let dcs_body = vec![b'~'; 5000];
    p3.advance(&mut e2, b"\x1b]999;");
    p3.advance(&mut e2, &osc_body);
    p3.advance(&mut e2, b"\x07");
    e2.actions.clear();
    e2.actions.shrink_to_fit();
    let before = LIVE.load(Ordering::Relaxed);
    for _ in 0..1000 {
        p3.advance(&mut e2, b"\x1b]999;");
        p3.advance(&mut e2, &osc_body);
        p3.advance(&mut e2, b"\x07\x1bPq");
        p3.advance(&mut e2, &dcs_body);
        p3.advance(&mut e2, b"\x1b\\");
        e2.actions.clear();
        e2.puts = 0;
    }
    e2.actions.shrink_to_fit();
    let after = LIVE.load(Ordering::Relaxed);
    println!(
        "(f) 1000 hostile OSC+DCS rounds: live heap {before} -> {after} B (delta {})",
        after as i64 - before as i64
    );

    assert_eq!(
        d.puts,
        oneterm_vt::parser::DCS_MAX_BYTES,
        "dcs_put stops at the cap"
    );
    assert_eq!(d.ends, 1, "abort reported exactly once");
    assert_eq!(d.aborted, 1);
    assert_eq!(
        a.puts,
        oneterm_vt::parser::DCS_MAX_BYTES,
        "apc_put stops at the cap"
    );
    assert_eq!(a.ends, 1);
    assert!(peak_osc < 4 * MIB, "unclaimed OSC allocated {peak_osc}");
    assert!(peak_large < 24 * MIB, "claimed OSC allocated {peak_large}");
    assert!(peak_dcs < MIB, "DCS buffered {peak_dcs}");
    assert!(peak_apc < MIB, "APC buffered {peak_apc}");
}
