//! Extended differential and memory probe for the parser, written by the
//! independent verifier of US-0073 and committed on their finding.
//!
//! It is the wide half of the proof that [`differential.rs`](differential.rs)
//! keeps narrow and fast: a second seed over 2 000 generated buffers, about
//! seventy hand-written adversarial sequences each replayed at 1-, 2-, 3-, 5-
//! and 7-byte chunking, every split point of seven terminator-bearing
//! sequences, and a counting allocator over the caps. It shares
//! `differential.rs`'s `accept()` verbatim, so only *new* disagreements
//! surface.
//!
//! `ext_memory_stays_bounded` is `#[ignore]`d: the counting `#[global_allocator]`
//! is process-wide, so any test running beside it pollutes the measurement.
//! Run it alone:
//!
//! ```text
//! cargo test -p oneterm-vt --release --test ext_differential -- \
//!     --ignored --nocapture --test-threads=1 ext_memory
//! ```

use std::alloc::{GlobalAlloc, Layout, System};
use std::path::{Path, PathBuf};
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

// ---------- action trace ----------
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
struct NewEngine {
    actions: Vec<Action>,
    puts: usize,
}

impl Dispatch for NewEngine {
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

#[derive(Default)]
struct Oracle {
    actions: Vec<Action>,
    puts: usize,
}

impl vte::Perform for Oracle {
    fn print(&mut self, c: char) {
        self.actions.push(Action::Print(c));
    }
    fn execute(&mut self, b: u8) {
        self.actions.push(Action::Execute(b));
    }
    fn esc_dispatch(&mut self, i: &[u8], _ig: bool, b: u8) {
        self.actions.push(Action::Esc(i.to_vec(), b));
    }
    fn csi_dispatch(&mut self, p: &vte::Params, i: &[u8], ignore: bool, a: char) {
        self.actions.push(Action::Csi {
            params: p.iter().map(<[u16]>::to_vec).collect(),
            intermediates: i.to_vec(),
            ignore,
            byte: a as u8,
        });
    }
    fn osc_dispatch(&mut self, p: &[&[u8]], bel: bool) {
        self.actions.push(Action::Osc {
            params: p.iter().map(|x| x.to_vec()).collect(),
            bel,
            truncated: false,
        });
    }
    fn hook(&mut self, p: &vte::Params, i: &[u8], ignore: bool, a: char) {
        self.actions.push(Action::Hook {
            params: p.iter().map(<[u16]>::to_vec).collect(),
            intermediates: i.to_vec(),
            ignore,
            byte: a as u8,
        });
    }
    fn put(&mut self, b: u8) {
        self.puts += 1;
        if self.puts <= 4096 {
            self.actions.push(Action::Put(b));
        }
    }
    fn unhook(&mut self) {
        self.actions.push(Action::Unhook);
    }
}

fn actions_new(bytes: &[u8]) -> Vec<Action> {
    let mut e = NewEngine::default();
    Parser::new().advance(&mut e, bytes);
    e.actions
}

fn actions_new_chunked(bytes: &[u8], chunk: usize) -> Vec<Action> {
    let mut e = NewEngine::default();
    let mut p = Parser::new();
    for piece in bytes.chunks(chunk.max(1)) {
        p.advance(&mut e, piece);
    }
    e.actions
}

fn actions_oracle(bytes: &[u8]) -> Vec<Action> {
    let mut o = Oracle::default();
    vte::Parser::new().advance(&mut o, bytes);
    o.actions
}

#[derive(Default, Debug, PartialEq, Eq, Clone, Copy)]
struct Filters {
    truncated_osc: usize,
    del_executed: usize,
    joined_osc_params: usize,
}

/// The implementer's `accept()` verbatim, so only NEW disagreements surface.
fn accept(mine: &Action, theirs: &Action, f: &mut Filters) -> bool {
    match (mine, theirs) {
        (Action::Execute(0x7F), Action::Print('\u{7f}')) => {
            f.del_executed += 1;
            true
        }
        (
            Action::Osc {
                params: m,
                truncated,
                ..
            },
            Action::Osc {
                params: t, bel: _, ..
            },
        ) => {
            let joined = m.len() == oneterm_vt::parser::MAX_OSC_PARAMS
                && t.len() == oneterm_vt::parser::MAX_OSC_PARAMS
                && m[..m.len() - 1] == t[..t.len() - 1];
            if joined {
                f.joined_osc_params += 1;
                return true;
            }
            if *truncated {
                f.truncated_osc += 1;
                return true;
            }
            false
        }
        _ => false,
    }
}

/// Compare and REPORT (never panic on the first hit).
fn diff(name: &str, bytes: &[u8], f: &mut Filters) -> Option<String> {
    let new = actions_new(bytes);
    let reference = actions_oracle(bytes);
    let mut i = 0;
    while i < new.len() && i < reference.len() {
        if new[i] == reference[i] || accept(&new[i], &reference[i], f) {
            i += 1;
            continue;
        }
        return Some(format!(
            "{name}: action {i}\n  new       {:?}\n  reference {:?}\n  new-ctx   {:?}\n  ref-ctx   {:?}",
            new[i],
            reference[i],
            &new[i.saturating_sub(2)..(i + 3).min(new.len())],
            &reference[i.saturating_sub(2)..(i + 3).min(reference.len())]
        ));
    }
    if new.len() != reference.len() {
        return Some(format!(
            "{name}: trace length {} vs {}\n  new-tail {:?}\n  ref-tail {:?}",
            new.len(),
            reference.len(),
            &new[i.min(new.len())..(i + 4).min(new.len())],
            &reference[i.min(reference.len())..(i + 4).min(reference.len())]
        ));
    }
    None
}

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/alacritty-ref")
}

fn recordings() -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<(String, Vec<u8>)> = std::fs::read_dir(corpus_dir())
        .expect("corpus")
        .filter_map(Result::ok)
        .filter_map(|e| {
            let p = e.path().join("recording");
            let n = e.file_name().to_string_lossy().into_owned();
            std::fs::read(&p).ok().map(|b| (n, b))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

struct Rng(u64);
impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n.max(1) as u64) as usize
    }
    fn fill(&mut self, out: &mut [u8]) {
        for b in out.iter_mut() {
            *b = (self.next_u64() >> 24) as u8;
        }
    }
}

// ============ 1. 2000 generated buffers, DIFFERENT seed ============
#[test]
fn ext_generated_2000_new_seed() {
    const BIAS: &[u8] = b"\x1b[]P_X^;:?0123456789<=>!\x20\x07\\mhlqrp\x18\x1a\x7f\xc2\x80\x9c\x9b\x9d\x90\x00\xf0\x9f\x98";
    let recs = recordings();
    let mut rng = Rng(0xDEAD_BEEF_1234_5677);
    let mut f = Filters::default();
    let mut bad = Vec::new();

    for round in 0..500 {
        let mut n = vec![0u8; 1 + rng.below(8192)];
        rng.fill(&mut n);
        if let Some(d) = diff(&format!("u-{round}"), &n, &mut f) {
            bad.push(d);
        }

        let mut b = vec![0u8; 1 + rng.below(8192)];
        rng.fill(&mut b);
        for x in &mut b {
            if usize::from(*x) % 3 != 0 {
                *x = BIAS[usize::from(*x) % BIAS.len()];
            }
        }
        if let Some(d) = diff(&format!("b-{round}"), &b, &mut f) {
            bad.push(d);
        }

        let mut s = Vec::new();
        for _ in 0..3 {
            let (_, r) = &recs[rng.below(recs.len())];
            let a = rng.below(r.len().max(1));
            let z = a + rng.below(r.len().saturating_sub(a).max(1));
            s.extend_from_slice(&r[a..z.min(r.len())]);
        }
        if let Some(d) = diff(&format!("s-{round}"), &s, &mut f) {
            bad.push(d);
        }

        let (name, src) = &recs[rng.below(recs.len())];
        let mut m = src.clone();
        for _ in 0..64 {
            if m.is_empty() {
                break;
            }
            let at = rng.below(m.len());
            m[at] = BIAS[rng.below(BIAS.len())];
        }
        if let Some(d) = diff(&format!("m-{name}-{round}"), &m, &mut f) {
            bad.push(d);
        }
    }

    println!(
        "ext_generated: 2000 buffers, filters={f:?}, divergences={}",
        bad.len()
    );
    for d in bad.iter().take(12) {
        println!("---\n{d}");
    }
    assert!(bad.is_empty(), "{} unfiltered divergences", bad.len());
}

// ============ 2. hand-written nasties ============
fn nasties() -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<(String, Vec<u8>)> = Vec::new();
    let mut add = |n: &str, b: &[u8]| v.push((n.to_string(), b.to_vec()));

    add("esc-inside-osc", b"\x1b]0;ti\x1btle\x07rest");
    add("esc-esc-inside-osc", b"\x1b]0;a\x1b\x1b\\b\x07");
    add("esc-bracket-inside-osc", b"\x1b]0;a\x1b[31mb\x07");
    add("can-mid-csi", b"\x1b[31\x18m");
    add("sub-mid-csi", b"\x1b[31;4\x1am");
    add("can-mid-osc", b"\x1b]0;abc\x18def\x07");
    add("sub-mid-dcs", b"\x1bP1;2q payload \x1a rest");
    add("can-mid-dcs", b"\x1bPq\x18\x1b[1m");
    add("can-mid-apc", b"\x1b_Gf=100\x18tail");
    add("can-in-escape", b"\x1b\x18[1m");
    add("del-after-esc", b"\x1b\x7f[1m");
    add("del-in-ground", b"a\x7fb");
    add("del-in-csi-param", b"\x1b[1\x7f2m");
    add("del-in-csi-entry", b"\x1b[\x7f1m");
    add("del-in-osc", b"\x1b]0;a\x7fb\x07");
    add("del-in-dcs", b"\x1bPq a\x7fb \x1b\\");
    add("nul-bytes", b"a\x00b\x00\x00\x1b[\x001m");
    add("nul-in-osc-and-dcs", b"\x1b]0;a\x00b\x07\x1bPq\x00\x1b\\");

    add("c1-8bit-raw", b"a\x9bb\x9dc\x90d\x9ce");
    add("c1-in-utf8", b"\xc3\x9b\xc2\x9b\x9b\xe2\x82\xac");
    add("c1-mid-multibyte", b"\xe2\x82\x9b\xac");
    add("c1-two-byte-form", b"a\xc2\x9bb\xc2\x80c\xc2\xa0d");
    add("c1-in-dcs-passthrough", b"\x1bPq\x9b\x9c");
    add("c1-9c-in-osc", b"\x1b]0;a\x9cb\x07");
    add("c1-9c-in-apc", b"\x1b_a\x9cb\x1b\\");

    add("overlong-2byte-slash", b"\xc0\xaf");
    add("overlong-3byte", b"\xe0\x80\xaf");
    add("overlong-4byte", b"\xf0\x80\x80\xaf");
    add("surrogate", b"\xed\xa0\x80");
    add("f5-out-of-range", b"\xf5\x80\x80\x80");
    add("fe-ff", b"\xfe\xff\xfe\xff");
    add("lone-continuations", b"\x80\x81\xbf");
    add("truncated-4byte", b"\xf0\x9f\x98");
    add("truncated-then-ascii", b"\xc5\x93\x40\x97");
    add("valid-then-bad-tail", b"ok\xe2\x82");
    add("utf8-then-esc", b"\xe2\x82\x1b[1m");

    add("osc-bel", b"\x1b]0;title\x07");
    add("osc-st", b"\x1b]0;title\x1b\\");
    add("osc-esc-only-eof", b"\x1b]0;title\x1b");
    add("osc-unterminated-eof", b"\x1b]0;title");
    add("osc-empty-bel", b"\x1b]\x07");
    add("osc-empty-st", b"\x1b]\x1b\\");
    add("osc-only-semis", b"\x1b];;;;\x07");
    add("dcs-st-eof", b"\x1bPq~~~\x1b\\");
    add("dcs-unterminated-eof", b"\x1bPq~~~");
    add("dcs-8bit-st", b"\x1bPq~~~\x9c");

    add("leading-zeros", b"\x1b[0000000038;00005;00001m");
    add("empty-params", b"\x1b[;;;;m");
    add("colon-run", b"\x1b[::::::::::::::::::::::::::::::::x");
    add(
        "semi-run-40",
        b"\x1b[;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;m",
    );
    add("mixed-sep-run", b"\x1b[1;2:3;4:5:6;7m");
    add("sgr-38-2-colon-empty", b"\x1b[38:2::10:20:30m");
    add("sgr-38-2-colon", b"\x1b[38:2:10:20:30m");
    add("sgr-38-5-semi", b"\x1b[38;5;123m");
    add("u16-saturate", b"\x1b[99999999999999m");
    add("csi-no-params", b"\x1b[m");
    add("markers", b"\x1b[?25h\x1b[>4;2m\x1b[=5c\x1b[<1m");
    add("marker-mid-param", b"\x1b[12?5h");
    add("three-intermediates", b"\x1b[ !#p");
    add("three-intermediates-esc", b"\x1b !#A");
    add("two-intermediates", b"\x1b[ !p");
    add("csi-intermediate-then-digit", b"\x1b[ 1p");
    add("esc-idempotent", b"\x1b\x1b\x1b[A");
    add("sync-2026", b"\x1b[?1;2026h\x1b[?2026l");

    let mut p33 = b"\x1b[".to_vec();
    for i in 0..33 {
        if i > 0 {
            p33.push(b';');
        }
        p33.push(b'7');
    }
    p33.push(b'm');
    v.push(("params-33".to_string(), p33));

    let mut p33c = b"\x1b[1".to_vec();
    for _ in 0..33 {
        p33c.extend_from_slice(b":2");
    }
    p33c.push(b'm');
    v.push(("subparams-33".to_string(), p33c));

    for n in [15usize, 16, 17, 24] {
        let mut o = b"\x1b]8".to_vec();
        for i in 0..n {
            o.push(b';');
            o.extend_from_slice(format!("p{i}").as_bytes());
        }
        o.push(0x07);
        v.push((format!("osc-{n}-params"), o));
    }

    v.push((
        "osc8-hyperlink".to_string(),
        b"\x1b]8;;http://a/b?c=1;d=2\x1b\\text\x1b]8;;\x1b\\".to_vec(),
    ));

    v
}

#[test]
fn ext_nasties_whole_and_byte_at_a_time() {
    let mut f = Filters::default();
    let mut bad = Vec::new();
    let mut chunk_bad = Vec::new();
    for (name, bytes) in nasties() {
        if let Some(d) = diff(&name, &bytes, &mut f) {
            bad.push(d);
        }
        let whole = actions_new(&bytes);
        for c in [1usize, 2, 3, 5, 7] {
            let got = actions_new_chunked(&bytes, c);
            if got != whole {
                chunk_bad.push(format!(
                    "{name}: chunk {c} differs\n  whole   {whole:?}\n  chunked {got:?}"
                ));
            }
        }
    }
    println!("ext_nasties: filters={f:?}");
    println!("--- oracle divergences ({}) ---", bad.len());
    for d in &bad {
        println!("{d}");
    }
    println!("--- chunk-invariance failures ({}) ---", chunk_bad.len());
    for d in &chunk_bad {
        println!("{d}");
    }
    assert!(chunk_bad.is_empty(), "chunk invariance broken");
    assert!(bad.is_empty(), "{} unfiltered divergences", bad.len());
}

// ============ 3. every split point ============
#[test]
fn ext_split_terminators_every_offset() {
    let cases: Vec<&[u8]> = vec![
        b"\x1bPq#0;2;0;0;0#0~~\x1b\\after",
        b"\x1b]52;c;SGVsbG8=\x1b\\after",
        b"\x1b]0;t\x07after",
        b"\x1b_Gf=100,a=T;AAAA\x1b\\after",
        b"\x1b[38:2:1:2:3;48;5;9mX",
        b"\xf0\x9f\x98\x80\xe2\x82\xac\xc2\x9b\x1b[1m",
        b"\x1b]0;a\x9cb\x1b\\\x1bPq\x9c",
    ];
    let mut bad = Vec::new();
    for (ci, case) in cases.iter().enumerate() {
        let whole = actions_new(case);
        for split in 0..=case.len() {
            let mut e = NewEngine::default();
            let mut p = Parser::new();
            p.advance(&mut e, &case[..split]);
            p.advance(&mut e, &case[split..]);
            if e.actions != whole {
                bad.push(format!(
                    "case {ci} split at {split}\n  whole {whole:?}\n  split {:?}",
                    e.actions
                ));
            }
        }
    }
    println!("ext_split: failures={}", bad.len());
    for d in bad.iter().take(5) {
        println!("{d}");
    }
    assert!(bad.is_empty());
}

// ============ 4. memory bounds ============
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
    let mut e = NewEngine::default();
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
    let mut e2 = NewEngine::default();
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

// ============ 5. V6: the reference carry bug ============
#[test]
fn ext_v6_reference_carry_bug_reproducer() {
    let head: &[u8] = b"\xc5";
    let tail: &[u8] = b"\x93\x40\x97";

    let mut o = Oracle::default();
    let mut vp = vte::Parser::new();
    vp.advance(&mut o, head);
    vp.advance(&mut o, tail);
    println!("vte     chunked : {:?}", o.actions);

    let mut o2 = Oracle::default();
    vte::Parser::new().advance(&mut o2, b"\xc5\x93\x40\x97");
    println!("vte     whole   : {:?}", o2.actions);

    let mut n = NewEngine::default();
    let mut np = Parser::new();
    np.advance(&mut n, head);
    np.advance(&mut n, tail);
    println!("oneterm chunked : {:?}", n.actions);

    let mut n2 = NewEngine::default();
    Parser::new().advance(&mut n2, b"\xc5\x93\x40\x97");
    println!("oneterm whole   : {:?}", n2.actions);

    assert_ne!(
        o.actions, o2.actions,
        "vte is chunk-DEPENDENT here (the bug)"
    );
    assert_eq!(n.actions, n2.actions, "oneterm must be chunk-invariant");
    assert!(
        !o.actions.contains(&Action::Print('@')),
        "vte drops the '@'"
    );
    assert!(n.actions.contains(&Action::Print('@')), "oneterm keeps it");

    // Second reference bug the packet does NOT claim: a C1 completing across a
    // chunk boundary is printed by vte and executed in ground.
    let mut o3 = Oracle::default();
    let mut vp3 = vte::Parser::new();
    vp3.advance(&mut o3, b"\xc2");
    vp3.advance(&mut o3, b"\x9b");
    let mut o4 = Oracle::default();
    vte::Parser::new().advance(&mut o4, b"\xc2\x9b");
    println!(
        "vte     C1 split {:?} vs whole {:?}",
        o3.actions, o4.actions
    );

    let mut n3 = NewEngine::default();
    let mut np3 = Parser::new();
    np3.advance(&mut n3, b"\xc2");
    np3.advance(&mut n3, b"\x9b");
    let mut n4 = NewEngine::default();
    Parser::new().advance(&mut n4, b"\xc2\x9b");
    println!(
        "oneterm C1 split {:?} vs whole {:?}",
        n3.actions, n4.actions
    );
}

// ============ 6. OSC cap behaviour ============
struct Rec {
    kept: usize,
    truncated: bool,
    nparams: usize,
    code: Option<u32>,
    large: Vec<u32>,
}
impl Dispatch for Rec {
    fn print_str(&mut self, _t: &str) {}
    fn execute(&mut self, _b: u8) {}
    fn esc(&mut self, _i: &[u8], _ig: bool, _b: u8) {}
    fn csi(&mut self, _p: &Params, _i: &[u8], _ig: bool, _b: u8) {}
    fn osc(&mut self, c: Option<u32>, p: &OscParams<'_>, _t: StringTerm, tr: bool) {
        self.kept = p.iter().map(<[u8]>::len).sum();
        self.truncated = tr;
        self.nparams = p.len();
        self.code = c;
    }
    fn osc_allows_large(&self, code: u32) -> bool {
        self.large.contains(&code)
    }
    fn dcs_hook(&mut self, _p: &Params, _i: &[u8], _b: u8) {}
    fn dcs_put(&mut self, _b: u8) {}
    fn dcs_unhook(&mut self, _a: bool) {}
    fn apc_start(&mut self, _i: u8) {}
    fn apc_put(&mut self, _b: u8) {}
    fn apc_end(&mut self, _a: bool) {}
}

#[test]
fn ext_osc_caps() {
    for (label, code, claimed, payload) in [
        ("unclaimed 0,  8 KiB", 0u32, false, 8 * 1024usize),
        ("claimed  52, 8 KiB", 52, true, 8 * 1024),
        ("claimed  52, 9 MiB", 52, true, 9 * 1024 * 1024),
        ("unclaimed 52, 9 MiB", 52, false, 9 * 1024 * 1024),
        ("unclaimed 0,  2047 B", 0, false, 2047),
    ] {
        let mut r = Rec {
            kept: 0,
            truncated: false,
            nparams: 0,
            code: None,
            large: if claimed { vec![52] } else { vec![] },
        };
        let mut bytes = format!("\x1b]{code};c;").into_bytes();
        bytes.extend(std::iter::repeat_n(b'x', payload));
        bytes.push(0x07);
        Parser::new().advance(&mut r, &bytes);
        println!(
            "{label}: code={:?} nparams={} payload_kept={} truncated={}",
            r.code, r.nparams, r.kept, r.truncated
        );
    }
}
