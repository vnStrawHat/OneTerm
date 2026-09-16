//! The VT engine benchmark, promoted from the scratch harness described in
//! `docs/spec-intakes/IN-0029-vt-engine/research/perf-baseline.md` § 1.
//!
//! Five tiers, each isolating a different cost:
//!
//! | Tier | What | Geometry |
//! | --- | --- | --- |
//! | 1 `parser` | the state machine alone, against a no-op `Dispatch` | 160x45 |
//! | 2 `grid` | parse plus grid mutation (`Terminal::feed`) | 160x45 |
//! | 3 `render` | tier 2 plus one `snapshot_update` per simulated frame | 160x45 |
//! | 4 `resize` | resize latency at three scrollback depths | its own, deliberately |
//! | 5 `rss` | heap held after filling scrollback with four content kinds | 160x45 |
//!
//! **Recorded, never gated in CI.** At realistic shell and SSH rates the engine
//! has two to three orders of magnitude of headroom (`perf-baseline.md` § 4,
//! § 5.9), so a threshold here would be a flaky test measuring the machine.
//! Every number is printed next to the ConPTY transport ceiling
//! `pty-throughput` measures (about 1.2 MiB/s for a `cmd.exe` producer), so none
//! is read in isolation.
//!
//! `vt-bench grid --check` is the one comparison that has a verdict, and it runs
//! by hand on the machine the committed baseline came from, with a band wide
//! enough (a fixture must get twice as slow) that it can only catch the class of
//! mistake that is a factor, not a percentage.
//!
//! Tier 4 uses its own geometry on purpose: it measures an operation, not a
//! stream, and is only ever compared against the other engine at the same
//! scrollback depth.
//!
//! Fixtures are deterministic functions in this file — no RNG, no captured
//! files — so a run is reproducible from the repository alone. `vt-bench
//! fixtures --out <dir>` writes them out when one needs inspecting.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use oneterm_vt::SnapshotState;
use oneterm_vt::parser::{Dispatch, OscParams, Params, Parser, StringTerm};
use oneterm_vt::{Config, EventBatch, ResizePolicy, Size, Terminal};

/// Benchmark grid width, matching `pty-throughput`'s geometry.
pub const COLS: usize = 160;
/// Benchmark grid height, matching `pty-throughput`'s geometry.
pub const ROWS: usize = 45;
/// Cycles per measurement; the reported figure is the median of them.
///
/// Tiers 1-2 take one sample of *every* fixture per cycle rather than `RUNS`
/// back-to-back samples of one, so a thermal or frequency drift over the length
/// of the run spreads across the whole table instead of biasing whichever
/// fixture happened to run while the machine was warm. Five is odd (so the
/// median is a sample, not a mean of two) and enough to show a spread.
pub const RUNS: usize = 5;
/// Default bytes per scenario for tiers 1-3.
pub const DEFAULT_MIB: usize = 100;
/// Scrollback depths tier 4 resizes at.
pub const RESIZE_DEPTHS: [usize; 3] = [0, 10_000, 100_000];
/// Scrollback rows tier 5 fills.
pub const MEMORY_ROWS: usize = 10_000;

// ---------------------------------------------------------------------------
// Allocation accounting
// ---------------------------------------------------------------------------

static ALLOCATED: AtomicUsize = AtomicUsize::new(0);

/// A pass-through allocator that tracks live bytes.
///
/// Tier 5 reports heap held rather than process RSS: RSS is polluted by the
/// allocator's own retained pages and by anything else in the process, while
/// the design claim tier 5 exists to check — what a grid of N rows costs — is
/// exactly live heap. The binary installs it as `#[global_allocator]`; without
/// that the tier reports zero, which `run_memory` says out loud.
pub struct CountingAllocator;

// SAFETY: every method forwards to `System` with the same layout it was given
// and returns the pointer `System` returned, so the allocator contract is the
// system allocator's. The counters are plain relaxed atomics and affect no
// pointer.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            ALLOCATED.fetch_add(layout.size(), Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        ALLOCATED.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let next = unsafe { System.realloc(pointer, layout, new_size) };
        if !next.is_null() {
            ALLOCATED.fetch_add(new_size, Ordering::Relaxed);
            ALLOCATED.fetch_sub(layout.size(), Ordering::Relaxed);
        }
        next
    }
}

/// Live heap bytes, as counted by [`CountingAllocator`].
pub fn live_heap_bytes() -> usize {
    ALLOCATED.load(Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// One deterministic byte-stream generator.
pub struct Fixture {
    /// Short name used in the report and as the dumped file name.
    pub name: &'static str,
    /// What the stream is meant to stress.
    pub about: &'static str,
    /// Produces at least `target` bytes.
    pub make: fn(usize) -> Vec<u8>,
}

/// Every tier 1-3 fixture, ported from the scratch harness and vtebench's
/// generators.
pub const FIXTURES: &[Fixture] = &[
    Fixture {
        name: "plain_ascii",
        about: "plain ASCII lines terminated by CRLF",
        make: plain_ascii,
    },
    Fixture {
        name: "long_lines",
        about: "lines five times the grid width, so every one implicitly wraps",
        make: long_lines,
    },
    Fixture {
        name: "heavy_sgr",
        about: "a 24-bit SGR per character",
        make: heavy_sgr,
    },
    Fixture {
        name: "tui_redraw",
        about: "CUP plus a short string, like a full-screen TUI repaint",
        make: tui_redraw,
    },
    Fixture {
        name: "scroll_region",
        about: "DECSTBM once, then bare line feeds",
        make: scroll_region,
    },
    Fixture {
        name: "cjk_wide",
        about: "wide CJK characters filling every line",
        make: cjk_wide,
    },
    Fixture {
        name: "dense_cells",
        about: "vtebench dense_cells: a 256-colour SGR for every cell",
        make: dense_cells,
    },
    Fixture {
        name: "scrolling",
        about: "vtebench scrolling: exactly-grid-width plain lines",
        make: scrolling,
    },
    Fixture {
        name: "sixel",
        about: "OneTerm's own: repeated small Sixel images (IN-0028)",
        make: sixel,
    },
    Fixture {
        name: "osc_9_7",
        about: "OneTerm's own: the agent channel between output lines",
        make: osc_9_7,
    },
];

fn repeat_to(target: usize, unit: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(target + unit.len());
    while out.len() < target {
        out.extend_from_slice(unit);
    }
    out
}

fn plain_ascii(target: usize) -> Vec<u8> {
    repeat_to(
        target,
        b"the quick brown fox jumps over the lazy dog 0123456789 -- log line\r\n",
    )
}

fn long_lines(target: usize) -> Vec<u8> {
    let mut line: Vec<u8> = (0..COLS * 5).map(|i| b'a' + (i % 26) as u8).collect();
    line.extend_from_slice(b"\r\n");
    repeat_to(target, &line)
}

fn heavy_sgr(target: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(target + 32);
    let mut index: u32 = 0;
    while out.len() < target {
        let (r, g, b) = (index % 256, (index / 3) % 256, (index / 7) % 256);
        out.extend_from_slice(format!("\x1b[38;2;{r};{g};{b}m").as_bytes());
        out.push(b'a' + (index % 26) as u8);
        index += 1;
        if index % COLS as u32 == 0 {
            out.extend_from_slice(b"\r\n");
        }
    }
    out
}

fn tui_redraw(target: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(target + 32);
    let mut index: u64 = 0;
    while out.len() < target {
        let row = index % ROWS as u64 + 1;
        let col = (index * 7) % COLS as u64 + 1;
        out.extend_from_slice(format!("\x1b[{row};{col}H").as_bytes());
        out.extend_from_slice(b"stat: 42");
        index += 1;
    }
    out
}

fn scroll_region(target: usize) -> Vec<u8> {
    let mut out = format!("\x1b[2;{}r", ROWS - 1).into_bytes();
    out.extend_from_slice(&repeat_to(
        target,
        b"tail output line ------------------------------\r\n",
    ));
    out
}

fn cjk_wide(target: usize) -> Vec<u8> {
    let mut line = "\u{56fd}".repeat(COLS / 2);
    line.push_str("\r\n");
    repeat_to(target, line.as_bytes())
}

fn dense_cells(target: usize) -> Vec<u8> {
    let mut frame = b"\x1b[H".to_vec();
    for row in 0..ROWS {
        for col in 0..COLS {
            let index = (row * COLS + col) % 256;
            frame.extend_from_slice(format!("\x1b[48;5;{index}m").as_bytes());
            frame.push(b'a' + ((row + col) % 26) as u8);
        }
    }
    repeat_to(target, &frame)
}

fn scrolling(target: usize) -> Vec<u8> {
    let mut line = vec![b'.'; COLS];
    line.extend_from_slice(b"\r\n");
    repeat_to(target, &line)
}

/// A 12x12 Sixel image plus a newline. Small on purpose: the cost being
/// measured is the DCS path and the placement bookkeeping, not one huge decode.
fn sixel(target: usize) -> Vec<u8> {
    let mut image = b"\x1bPq#0;2;100;0;0#1;2;0;100;0".to_vec();
    for band in 0..2 {
        image.extend_from_slice(format!("#{band}").as_bytes());
        image.extend_from_slice(&[b'~'; 12]);
        image.push(b'-');
    }
    image.extend_from_slice(b"\x1b\\\r\n");
    repeat_to(target, &image)
}

/// The fixture **name** is deliberately left at `osc_9_7` even though the agent
/// channel moved to `OSC 20308;1` in `US-0088`: it is the key three recorded
/// baseline tables (`US-0072`, `US-0073`, `US-0076`) compare against, and what
/// it measures is OSC-payload throughput, which the number does not change.
/// These bytes are fed to an in-process parser and never reach a terminal, so
/// the ConEmu collision that moved the protocol does not apply here.
fn osc_9_7(target: usize) -> Vec<u8> {
    repeat_to(
        target,
        b"\x1b]9;7;state=running;title=build\x07building the workspace\r\n",
    )
}

// ---------------------------------------------------------------------------
// Tiers
// ---------------------------------------------------------------------------

/// A `Dispatch` that does nothing with anything, which is what isolates the
/// state machine from the grid.
struct NullSink;

impl Dispatch for NullSink {
    fn print_str(&mut self, _text: &str) {}
    fn execute(&mut self, _byte: u8) {}
    fn esc(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
    fn csi(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
    fn osc(&mut self, _code: Option<u32>, _p: &OscParams<'_>, _t: StringTerm, _truncated: bool) {}
    fn dcs_hook(&mut self, _params: &Params, _intermediates: &[u8], _byte: u8) {}
    fn dcs_put(&mut self, _byte: u8) {}
    fn dcs_unhook(&mut self, _aborted: bool) {}
    fn apc_start(&mut self, _introducer: u8) {}
    fn apc_put(&mut self, _byte: u8) {}
    fn apc_end(&mut self, _aborted: bool) {}
}

const BENCH_SIZE: Size = Size {
    rows: ROWS as u16,
    cols: COLS as u16,
};

fn new_term() -> Terminal {
    Terminal::new(BENCH_SIZE, Config::default())
}

/// One throughput measurement.
#[derive(Debug, Clone)]
pub struct Throughput {
    /// Fixture name.
    pub fixture: &'static str,
    /// Mebibytes per second, median of [`RUNS`] cycles.
    pub mib_per_second: f64,
    /// Nanoseconds per input byte at that rate.
    pub ns_per_byte: f64,
    /// Fastest minus slowest cycle, as a percentage of the median.
    ///
    /// The honest companion to a median: it says how much of a difference
    /// between two runs is the machine rather than the engine. A published
    /// number whose spread is wider than the difference being argued about is
    /// not evidence of anything.
    pub spread_percent: f64,
}

fn median_throughput(fixture: &'static str, len: usize, mut samples: Vec<Duration>) -> Throughput {
    samples.sort_unstable();
    let elapsed = samples[samples.len() / 2];
    let rate = |d: Duration| (len as f64 / (1024.0 * 1024.0)) / d.as_secs_f64();
    Throughput {
        fixture,
        mib_per_second: rate(elapsed),
        ns_per_byte: elapsed.as_nanos() as f64 / len as f64,
        spread_percent: (rate(samples[0]) - rate(samples[samples.len() - 1])) / rate(elapsed)
            * 100.0,
    }
}

/// Tier 1: the state machine with no grid behind it.
pub fn run_parser(mib: usize) -> Vec<Throughput> {
    tier(mib, |bytes| {
        let mut sink = NullSink;
        let mut parser = Parser::new();
        let start = Instant::now();
        parser.advance(&mut sink, bytes);
        start.elapsed()
    })
}

/// Tier 2: parse plus grid mutation — the real cost centre.
///
/// The event batch is cleared per chunk, as the pump clears it per drain: the
/// tier measures the feed, not an arena that grows for the length of the run.
pub fn run_grid(mib: usize) -> Vec<Throughput> {
    tier(mib, |bytes| {
        let mut term = new_term();
        let mut batch = EventBatch::new();
        let now = Instant::now();
        let start = Instant::now();
        for chunk in bytes.chunks(64 * 1024) {
            term.feed(chunk, &mut batch, now);
            batch.clear();
        }
        start.elapsed()
    })
}

/// Runs `pass` over every fixture, [`RUNS`] cycles, one sample per fixture per
/// cycle.
///
/// The fixture bytes are regenerated inside the cycle rather than all held at
/// once: ten fixtures of `--mib 100` would be a gigabyte of resident input, and
/// generation is outside the timed region anyway.
fn tier(mib: usize, mut pass: impl FnMut(&[u8]) -> Duration) -> Vec<Throughput> {
    let mut samples: Vec<Vec<Duration>> = vec![Vec::with_capacity(RUNS); FIXTURES.len()];
    let mut len = vec![0usize; FIXTURES.len()];
    for cycle in 0..=RUNS {
        for (index, fixture) in FIXTURES.iter().enumerate() {
            let bytes = (fixture.make)(mib * 1024 * 1024);
            let sample = pass(&bytes);
            // Cycle zero is a warm-up and is thrown away: it pays the cold
            // instruction cache, the first-touch page faults on a freshly
            // allocated input buffer, and the CPU's turbo ramp, none of which
            // the engine will pay again on a stream that is already running.
            if cycle > 0 {
                len[index] = bytes.len();
                samples[index].push(sample);
            }
        }
    }
    FIXTURES
        .iter()
        .enumerate()
        .map(|(index, fixture)| {
            median_throughput(
                fixture.name,
                len[index],
                std::mem::take(&mut samples[index]),
            )
        })
        .collect()
}

/// One tier 3 measurement: the per-frame cost of building what the view paints.
#[derive(Debug, Clone)]
pub struct FrameCost {
    /// Fixture name.
    pub fixture: &'static str,
    /// Microseconds per simulated frame.
    pub us_per_frame: f64,
    /// Cells copied per frame.
    pub cells: usize,
}

/// Tier 3: tier 2 plus one `snapshot_update` per simulated frame.
///
/// This is the primary metric: it is the tier that would have caught a
/// per-frame viewport copy, because it is the only one where a bigger viewport
/// costs anything. The `SnapshotState` is reused across frames, exactly as the
/// view reuses its own — a fresh one every frame would measure a `Full` rebuild
/// and nothing the damage model does.
pub fn run_render(frames: usize) -> Vec<FrameCost> {
    FIXTURES
        .iter()
        .map(|fixture| {
            let mut term = new_term();
            let mut batch = EventBatch::new();
            let mut render = SnapshotState::new();
            let now = Instant::now();
            // Prime a full frame so the update is never over an empty grid.
            term.feed(&dense_cells(COLS * ROWS * 20), &mut batch, now);
            batch.clear();
            term.snapshot_update(&mut render, now);

            // Bytes arriving between paints, as a PTY would deliver them.
            let stream = (fixture.make)(frames * 512 + 1024);
            let mut cells = 0;
            let mut spent = Duration::ZERO;
            for frame in 0..frames {
                let start = frame * 512;
                term.feed(&stream[start..start + 512], &mut batch, now);
                batch.clear();

                let timed = Instant::now();
                term.snapshot_update(&mut render, now);
                cells = render.rows().iter().map(|row| row.cells.len()).sum();
                spent += timed.elapsed();
            }
            FrameCost {
                fixture: fixture.name,
                us_per_frame: spent.as_secs_f64() * 1_000_000.0 / frames as f64,
                cells,
            }
        })
        .collect()
}

/// One tier 4 measurement.
#[derive(Debug, Clone)]
pub struct ResizeCost {
    /// Scrollback rows held when the resize ran.
    pub depth: usize,
    /// Microseconds to grow 80x24 to 100x40.
    pub grow_us: f64,
    /// Microseconds to shrink back.
    pub shrink_us: f64,
}

/// Tier 4: resize latency at three scrollback depths.
///
/// Its geometry is deliberately its own (80x24 to 100x40): this tier measures
/// an operation, not a stream, and is only ever compared against the other
/// engine at the same depth — never against tiers 1-3.
pub fn run_resize(depths: &[usize]) -> Vec<ResizeCost> {
    depths
        .iter()
        .map(|&depth| {
            let small = Size { rows: 24, cols: 80 };
            let large = Size {
                rows: 40,
                cols: 100,
            };
            let config = Config {
                scrollback_limit: depth as u32,
                ..Default::default()
            };
            let mut term = Terminal::new(small, config);
            let mut batch = EventBatch::new();
            let now = Instant::now();
            // Fill the history with content that actually has to reflow.
            let filler = long_lines((depth + 24) * 96);
            term.feed(&filler, &mut batch, now);
            batch.clear();

            let mut grow = Vec::with_capacity(RUNS);
            let mut shrink = Vec::with_capacity(RUNS);
            for _ in 0..RUNS {
                let start = Instant::now();
                term.resize(large, ResizePolicy::BottomAnchor);
                grow.push(start.elapsed());
                let start = Instant::now();
                term.resize(small, ResizePolicy::BottomAnchor);
                shrink.push(start.elapsed());
            }
            grow.sort_unstable();
            shrink.sort_unstable();
            ResizeCost {
                depth,
                grow_us: grow[grow.len() / 2].as_secs_f64() * 1_000_000.0,
                shrink_us: shrink[shrink.len() / 2].as_secs_f64() * 1_000_000.0,
            }
        })
        .collect()
}

/// One full row of each tier 5 content kind, so `rows` lines fill exactly
/// `rows` scrollback rows.
fn memory_row_plain() -> Vec<u8> {
    let mut row = vec![b'.'; COLS];
    row.extend_from_slice(b"\r\n");
    row
}

fn memory_row_unicode() -> Vec<u8> {
    let mut row = "\u{56fd}".repeat(COLS / 2);
    row.push_str("\r\n");
    row.into_bytes()
}

fn memory_row_styled() -> Vec<u8> {
    let mut row = Vec::new();
    for index in 0..COLS {
        let (r, g, b) = (index % 256, (index * 3) % 256, (index * 7) % 256);
        row.extend_from_slice(format!("\x1b[38;2;{r};{g};{b}m").as_bytes());
        row.push(b'a' + (index % 26) as u8);
    }
    row.extend_from_slice(b"\x1b[0m\r\n");
    row
}

fn memory_row_mixed() -> Vec<u8> {
    let mut row = Vec::new();
    for index in 0..COLS {
        if index % 2 == 0 {
            row.extend_from_slice(format!("\x1b[48;5;{}m", index % 256).as_bytes());
        }
        row.push(if index % 8 == 0 {
            b' '
        } else {
            b'a' + (index % 26) as u8
        });
    }
    row.extend_from_slice(b"\x1b[0m\r\n");
    row
}

/// One tier 5 measurement.
#[derive(Debug, Clone)]
pub struct MemoryCost {
    /// Content kind filled into scrollback.
    pub content: &'static str,
    /// Live heap bytes the terminal holds.
    pub bytes: usize,
    /// Heap bytes per scrollback row.
    pub bytes_per_row: f64,
}

/// Tier 5: heap held after filling `rows` of scrollback with four content
/// kinds. A report, never a `#[test]`: an allocation assertion inside a test
/// harness running other tests in parallel is unreliable.
pub fn run_memory(rows: usize) -> Vec<MemoryCost> {
    // One *row* per line, not one fixed byte budget: the throughput fixtures
    // produce wildly different row counts for the same number of bytes, which
    // would make bytes-per-row meaningless across content kinds.
    let kinds: [(&str, fn() -> Vec<u8>); 4] = [
        ("plain", memory_row_plain),
        ("unicode", memory_row_unicode),
        ("styled", memory_row_styled),
        ("mixed", memory_row_mixed),
    ];
    kinds
        .iter()
        .map(|(content, row)| {
            let bytes = repeat_to(rows * row().len(), &row());
            let baseline = live_heap_bytes();
            let mut term = new_term();
            let mut batch = EventBatch::new();
            let now = Instant::now();
            for chunk in bytes.chunks(64 * 1024) {
                term.feed(chunk, &mut batch, now);
                batch.clear();
            }
            let held = live_heap_bytes().saturating_sub(baseline);
            // Keep the terminal alive across the measurement.
            drop(term);
            MemoryCost {
                content,
                bytes: held,
                bytes_per_row: held as f64 / rows as f64,
            }
        })
        .collect()
}
