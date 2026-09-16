# 14. Performance: one number, and what it is worth

This crate makes one measured claim, and this chapter is all of it: the command
that produced the number, the machine it ran on, what the number covers, and
the several things it is not evidence of.

A throughput figure with no machine beside it is marketing. A figure with no
statement of what it excludes is worse, because a reader fills the gap with an
assumption the measurement does not support.

## What measures it

The engine's repository ships a benchmark binary, `vt-bench`. It is a developer
diagnostic and is **not** part of this crate: depending on the engine does not
pull it in, and it adds no dependency to anything an embedder compiles. There
is no `benches/` directory and no `criterion`, deliberately -- see "Why there is
no criterion" below.

It runs five tiers, each isolating a different cost. Every one drives the engine
directly, in one process, from a byte buffer already in memory.

| Tier | Command | What it measures |
| --- | --- | --- |
| 1 | `parser` | the state machine alone, against a dispatch that does nothing |
| 2 | `grid` | parse plus grid mutation: `Terminal::feed`, 64 KiB at a time |
| 3 | `render` | tier 2 plus one snapshot update per simulated frame |
| 4 | `resize` | reflow latency at 0, 10 000 and 100 000 scrollback rows |
| 5 | `rss` | live heap held after filling 10 000 scrollback rows |

Tier 2 is the tier published below. It is what an embedder pays on every byte
that arrives, and unlike tiers 3 and 4 it is a throughput rather than a latency,
so it is the one that can honestly be stated as a rate.

Tier 5 counts **live heap** through a counting allocator rather than process
resident size, because the design claim it exists to check -- what a grid of N
rows costs -- is exactly live heap, while resident size also counts the
allocator's own retained pages.

## What is not measured

The number below is the engine and nothing else. It does **not** include:

- **Any renderer.** No glyph shaping, no atlas upload, no GPU work, no window.
  Tier 3 measures the cost of *producing* what a renderer would paint, not the
  cost of painting it. In a real application the renderer is usually the larger
  half.
- **Any pseudo-console.** The bytes come from a buffer, not from a child
  process through a transport. See the ceiling below for why that matters.
- **Reading, scheduling or thread hand-off.** One thread, no channel, no wake.
- **Your terminal's own input handling, scrollback UI or search.**

It also is not a comparison. There is no measurement here against
`alacritty_terminal`, `rio-vt` or anything else. A fair cross-engine harness is
a project of its own -- identical fixtures, identical geometry, identical
scrollback policy, identical build flags -- and an unfair one is worse than
none. If you need that comparison, build it; do not infer it from this table.

## The number

Measured on an Intel Core i7-12700 (12 cores / 20 threads), Windows 11 build
26200, rustc 1.96.0, release profile, on 2026-09-16. Grid geometry 160x45,
32 MiB of input per fixture, median of five interleaved cycles after one
discarded warm-up cycle.

| Fixture | MiB/s | ns/byte | spread | What the stream is |
| --- | ---: | ---: | ---: | --- |
| `plain_ascii` | 74.9 | 12.73 | 4% | plain ASCII lines ended by CRLF |
| `long_lines` | 80.9 | 11.79 | 2% | lines five times the grid width, so every one wraps |
| `heavy_sgr` | 226.5 | 4.21 | 5% | a 24-bit colour change per character |
| `tui_redraw` | 125.6 | 7.59 | 3% | cursor placement plus a short string, a TUI repaint |
| `scroll_region` | 61.0 | 15.64 | 5% | a scrolling region set once, then bare line feeds |
| `cjk_wide` | 115.0 | 8.29 | 4% | wide CJK characters filling every line |
| `dense_cells` | 195.6 | 4.88 | 8% | a 256-colour change for every cell |
| `scrolling` | 80.1 | 11.91 | 8% | exactly-grid-width plain lines |
| `sixel` | 46.7 | 20.43 | 8% | repeated small Sixel images |
| `osc_9_7` | 102.2 | 9.33 | 31% | an application status channel between output lines |

The fixtures are deterministic generators in the benchmark's own source -- no
recorded files, no random data -- so a run is reproducible from the repository
alone. Most are ported from vtebench's generators; `sixel` and `osc_9_7` are
this engine's own.

**`spread` is the width of the whole sample, fastest cycle to slowest, as a
percentage of the median.** It is published because it is the only thing that
tells you whether a difference between two runs means anything. Nine of the ten
rows above sit between two and eight percent, which is about the floor for this
kind of machine; `osc_9_7` at thirty-one percent had one disturbed cycle and is
the honest illustration of why the column is here at all. Read no meaning into a
difference smaller than the spread beside it -- which is also why the guard
below fires at a factor rather than a percentage.

### Read it next to the transport ceiling

No engine number should be read in isolation. On the same machine, a ConPTY
delivering the output of `cmd.exe` moves about **1.2 MiB/s**, and a
full-screen, flat-out producer of the DOOM-fire class about **30 MiB/s**. The
slowest fixture above is faster than the second of those, so on this machine
the engine is not the bottleneck at any rate a shell can actually produce. That
is the claim the table supports, and it is the only one.

## Reproducing it

In a checkout of <https://github.com/vnStrawHat/OneTerm>, one command:

```console
$ cargo run -p oneterm-tools --release --bin vt-bench -- grid --mib 32
```

Your numbers will differ; the shape of the output should not. `all` runs every
tier, `--json` writes the same figures machine-readably, and `--help` lists the
rest.

Three hygiene rules, and they are not optional if the result is to mean
anything: run it in **release** (a debug build is an order of magnitude out),
run it with **nothing else running** on the machine, and never pipe a generator
into it -- the fixtures are built in-process for exactly that reason. The
spread column is how you tell whether the second rule was actually kept.

## The regression trip-wire

The repository commits one measured run as `bench-baseline.json`, and the
benchmark can compare against it:

```console
$ cargo run -p oneterm-tools --release --bin vt-bench -- grid --check
```

It exits non-zero when any fixture has dropped below **half** its baseline
figure, and prints the full table of ratios either way. `--tolerance` moves the
band and `--baseline` points at a different file.

The band is deliberately that wide, and the check deliberately does not run in
continuous integration:

- A shared runner varies by more than a factor of two between machine classes,
  so a gate on it would measure the runner, and a flaky performance gate is
  worse than none -- people learn to re-run it.
- The baseline is one machine's. Comparing another machine against it says
  nothing.
- What a factor-of-two band actually catches is the class of mistake that is a
  factor rather than a percentage: an accidental clone in the print path, a
  linear scan added to a per-cell loop, a snapshot update that stopped being
  incremental. Those are the regressions worth an automatic verdict.

The check refuses to answer rather than answer wrongly: it exits non-zero in a
debug build, under the engine's paranoid-integrity feature, when the baseline
file is missing or unreadable, when a fixture has no baseline entry, and when a
baseline entry has no fixture behind it. A missing baseline is a failure, never
a skip.

One rule about that file is social, and no script can enforce it: **a baseline
is refreshed only in a commit that says why the number moved.** A baseline
quietly regenerated to make the check pass is the exact failure this whole
section exists to prevent, and only reading the diff catches it.

## Why there is no criterion

The obvious move is a `benches/` directory and the `criterion` crate, because
that is what a reviewer expects to see. It is not here on purpose. The
measurement already exists in a binary that is already built, tested and
documented, and a second way to run the same fixtures is a second thing to keep
in step. `criterion` earns its dependency tree when you need a distribution and
a confidence interval on a microbenchmark; tier 2 is a throughput over a fixed
32 MiB input whose noise floor is the machine, not the sampling. Add it the day
somebody needs a distribution rather than a median -- not before.

## What this number is not

- **One machine, one operating system.** Everything above is a Windows figure
  from one desktop CPU. A Linux embedder on other silicon gets a different
  number and has no way to know how different. Naming the machine is the honest
  mitigation, not a fix.
- **A median, not a distribution.** Five cycles and a spread, which is enough
  to see a factor and not enough to see a few percent.
- **Not a promise.** Performance is explicitly outside this crate's stability
  promise, alongside grid internals and allocation behaviour. A published
  measurement is evidence about one build on one machine on one day; it is not
  a contract, and a later version is free to be slower without that being a
  breaking change. What the promise does cover is in chapter 12.
