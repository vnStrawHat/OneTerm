# Independent verification: US-0107, performance evidence in guide and README

Packet: [`US-0107-performance-evidence.md`](../US-0107-performance-evidence.md)
Design: [`low-level-design/performance-evidence.md`](../low-level-design/performance-evidence.md)
Rule under test: `IN-0029/low-level-design/testing-and-bench.md:272` (R-29)
Branch: `docs/vt-performance-evidence` @ `c05320fb`, base `main` @ `4437b98e`
Verified in a separate worktree, on the same machine class the baseline names
(Intel Core i7-12700, Windows 11 26200), 2026-09-16.

## Verdict

**Accept with findings.** Everything the packet claims about the mechanism is
true and reproduced here: the interleaving is real and correctly implemented,
the median is a sample, the timed region is clean, the published table is the
committed baseline field for field, the ten fixtures reproduce within a few
percent on an independent run, and all nine guard states behave as recorded.
R-29 is respected and CI is untouched. The findings below are two wrong
statements in **published** documentation (F1, F2), one missed update site in
the guide itself (F3), and record inaccuracies (F4, F5) -- none of which
invalidates the number, all of which a maintainer should fix before this is
read by the outsider it was written for.

## 1. Reproduction

Machine load at measurement time: **quiet**. `Get-Counter \Processor(_Total)`
over three samples gave 2.7 / 9.3 / 0.5 percent, and zero `cargo`, `rustc` or
`rust-analyzer` processes were running in this or any sibling worktree. The
two other worktrees mentioned in the task were idle for this run; they were
busy later, during the gate, which is stated where that matters.

The chapter's command, run verbatim
(`14-performance.md:102`), 26.4 s wall:

```console
$ cargo run -p oneterm-tools --release --bin vt-bench -- grid --mib 32
```

| Fixture | published MiB/s | measured here | delta | published spread | measured spread |
| --- | ---: | ---: | ---: | ---: | ---: |
| `plain_ascii` | 74.9 | 76.7 | +2.4% | 4% | 17% |
| `long_lines` | 80.9 | 80.1 | -1.0% | 2% | 5% |
| `heavy_sgr` | 226.5 | 222.4 | -1.8% | 5% | 4% |
| `tui_redraw` | 125.6 | 125.2 | -0.3% | 3% | 2% |
| `scroll_region` | 61.0 | 59.8 | -2.0% | 5% | 5% |
| `cjk_wide` | 115.0 | 116.6 | +1.4% | 4% | 2% |
| `dense_cells` | 195.6 | 194.6 | -0.5% | 8% | 5% |
| `scrolling` | 80.1 | 78.6 | -1.9% | 8% | 6% |
| `sixel` | 46.7 | 42.2 | -9.6% | 8% | 8% |
| `osc_9_7` | 102.2 | 105.5 | +3.2% | 31% | 26% |

Nine of ten land inside the published spread. `sixel` at -9.6% is just outside
its 8% band on this run; three further runs of the same fixture during the
guard work gave 47.2, 45.4 and 45.5 MiB/s, so `sixel` varies about 11%
**between** runs, wider than the 8% spread published **within** one. The table
is reproducible; the spread column understates run-to-run noise for at least
that fixture. The spread column is printed, as claimed.

`--check` against the committed baseline, release, quiet machine: **exit 0,
PASS**, ratios **0.88 to 1.07** (packet records 0.85-1.03). A scratch copy of
the baseline with every `mib_per_second` doubled: **exit 1, FAIL** -- see F5
for how that differs from what the packet recorded. A debug build is refused.
All scratch baselines were written to the session scratchpad; nothing was
committed and `crates/tools/bench-baseline.json` is untouched.

All nine guard states, reproduced:

| Case | Observed |
| --- | --- |
| `grid --mib 32 --check`, committed baseline | exit 0, PASS, 0.88-1.07 |
| baseline doubled in place | exit 1, FAIL, 8 of 10 named at 0.48-0.49 |
| `--baseline <missing>` | exit 1, "no baseline at ...: The system cannot find the file specified. (os error 2)" |
| baseline not JSON | exit 1, "... is not JSON: expected ident at line 1 column 2" |
| baseline with no `tiers.tier2_grid` | exit 1, names the key and the regenerating command |
| extra `retired_fixture` entry | exit 1, "baseline entry `retired_fixture` has no fixture" |
| `sixel` entry removed | exit 1, "fixture `sixel` has no baseline entry" |
| `render --check` | exit 1, "--check guards tier 2 only; run `vt-bench grid --check`" |
| debug build | exit 1, "--check needs a release build (cargo run --release)" |
| `--features oneterm-tools/vt-paranoid`, release | exit 1, "--check is meaningless under vt-paranoid" |

## 2. The interleaving change -- clean

`crates/tools/src/bench.rs:383-410`. Reviewed against every question asked:

- **Warm-up discarded.** `for cycle in 0..=RUNS` runs six cycles; `if cycle > 0`
  keeps five. The discarded cycle covers every fixture, not just the first.
- **Truly interleaved.** Cycles are the outer loop and fixtures the inner one,
  so each fixture is sampled once per cycle. This is not fixture-by-fixture.
- **Median correct.** `bench.rs:336-338` sorts and takes `samples[len/2]`. With
  `RUNS = 5` that is index 2, a real sample. `RUNS` is odd by construction and
  the doc comment at `bench.rs:46-52` says why; there is no even-`N` path to get
  wrong.
- **Timed region clean.** `bench.rs:364-374`: fixture generation, `Terminal`
  construction, `EventBatch::new()` and the drop all sit outside
  `Instant::now() .. start.elapsed()`. Only `feed`, `batch.clear()` and the
  chunk iterator are inside. Same for tier 1 at `bench.rs:349-356`.
- **Spread arithmetic correct.** `bench.rs:343-344` computes it in rate space:
  the fastest cycle's rate minus the slowest cycle's, over the median's. That
  is exactly what `14-performance.md:79` says it is.

No timing bug found.

## 3. R-29 -- held

- No acceptance criterion in `US-0107-performance-evidence.md:86-113` names a
  throughput figure. Every number is in Evidence, which is where R-29 wants it.
- `git diff --stat 4437b98e..HEAD` lists ten files; `.github/workflows/ci.yml`,
  `scripts/ci-local.ps1` and `scripts/ci-local.sh` are not among them. The
  guard is not wired into any gate.
- `14-performance.md:128-135` states that the check deliberately does not run in
  continuous integration, and why.

## 4. Prose audit

Verified true against the source: the five-tier table (`14-performance.md:22-28`)
against `RESIZE_DEPTHS` and `MEMORY_ROWS` at `bench.rs:57,59`; "64 KiB at a
time" against `bench.rs:369`; tier 3 reporting us/frame, not MiB/s
(`vt-bench.rs:319`, `bench.rs:460`), which is the substitution's whole
justification and it is correct; "median of five interleaved cycles after one
discarded warm-up" (`14-performance.md:58`); `--help` exists both as the first
argument and as a flag (`vt-bench.rs:49-52`, `vt-bench.rs:83`); "below **half**"
matches the strict `ratio < tolerance` at `vt-bench.rs:198`; the ceiling
sentence at `14-performance.md:93` holds (slowest fixture 46.7 > 30 MiB/s);
the `## Performance` section sits immediately after `## Dependencies`
(`README.md:49,71`) as the design specified.

**Standalone grep: zero matches.** CI's `The embedder guide must stand alone`
pattern (`ci.yml:246-248`) run by hand over `crates/vt/docs/guide` returns
nothing; chapter 14 cites no `crates/`, `docs/` or packet path. The chapter
carries no Rust fence, so it contributes no doctest -- disclosed in the packet.

No `thirteen` remains anywhere in `crates/vt`. `guide::ch14_performance` is
present (`guide.rs:95-96`) and the four strings the packet listed are all
updated.

### F1 -- medium -- vtebench provenance is overstated in published documentation

`crates/vt/docs/guide/14-performance.md:76` says "Most are ported from
vtebench's generators; `sixel` and `osc_9_7` are this engine's own." The
repository's own fixture table attributes exactly **two** of ten to vtebench --
`dense_cells` (`bench.rs:158`) and `scrolling` (`bench.rs:163`) -- and the
module doc at `bench.rs:123` says the set came from "the scratch harness **and**
vtebench's generators". `plain_ascii`, `long_lines`, `heavy_sgr`, `tui_redraw`,
`scroll_region` and `cjk_wide` carry no provenance at all and are not vtebench
generator names. This is a claim about an external project in the one document
written for a reader who will check it. "Two are ported from vtebench; the rest
are this engine's own" is what the source supports.

### F2 -- low/medium -- "one disturbed cycle" does not reproduce as noise

`14-performance.md:83` and `README.md:96` both explain `osc_9_7`'s 31% spread as
"one disturbed cycle", which reads as machine interference. On an independently
quiet machine `osc_9_7` came out at **26%** -- again the widest of the ten by a
factor of three. The evidence is that the fixture is intrinsically variable, not
that one cycle was disturbed. The column is right to be published; the
explanation beside it is the wrong one, and it is load-bearing, because the same
sentence is what tells a reader the other nine rows are trustworthy.

### F3 -- low/medium -- chapter 14 is missing from the guide's own roadmap

`crates/vt/docs/guide/01-overview.md:90-97`, "How to read the rest", walks the
reader from chapter 2 to chapter 13 and stops. Chapter 14 is not mentioned, so a
reader following the guide in order never learns the performance chapter exists.
The packet enumerated four "thirteen" strings to fix (README, CHANGELOG,
`guide.rs`, `lib.rs`) and ticked "The guide is fourteen chapters everywhere"
(`US-0107-performance-evidence.md:98`); this is the fifth site, inside the
published guide, and no script covers it.

### F4 -- low/medium -- chapter 14's line count is misstated, hiding a second overrun

`US-0107-performance-evidence.md:112` reads "Chapter 14 is 136 lines (~130) and
the README +38 (+40), both inside budget", and `:272` repeats "chapter 14 136
lines". The file is **170 lines** (`git diff --numstat`: `170 0
crates/vt/docs/guide/14-performance.md`). Against the design's "about 130 lines"
(`performance-evidence.md:109`) that is +31% -- a second budget overrun,
undisclosed, while the packet asserts the item is inside budget. README `+38`
and `crates/tools` `+251` (6 + 62 + 183) are both exact.

The length itself is defensible: `13-pty.md` is 220 lines and
`11-conformance.md` 203, so 170 is ordinary for this guide. The finding is the
wrong number in the record, and the "both inside budget" it produces.

### F5 -- low/medium -- the recorded FAIL figures do not follow from a doubled baseline

`US-0107-performance-evidence.md:260` records "against a baseline doubled in
place | exit 1, FAIL, all ten fixtures named at 0.27-0.35 x". Doubling puts
every ratio at approximately 0.50, and the test at `vt-bench.rs:198` is strict
(`ratio < tolerance`), so on a quiet machine the observed result is **8 of 10**
named at 0.48-0.49 with two landing at 0.51 and passing. Ratios of 0.27-0.35
require the measuring run to have been roughly 40% slower than the baseline,
which is consistent with the packet's own note that sibling worktrees were
compiling -- so the attached figures are load-contaminated and the "all ten"
claim is not a property of an exactly-doubled baseline. The guard still exits 1,
which is the part that matters; the record should say what it actually shows.

### F6 -- low -- `vt-bench all` prints one unlabelled spread for two tiers

`crates/tools/src/bin/vt-bench.rs:281-284`: `spread` is reassigned for each
printed tier, so when tiers 1 and 2 both run the single `spread` column carries
tier 2's and tier 1's is silently dropped. Harmless for the published
single-tier table, misleading for anyone running `vt-bench all`.

### F7 -- low -- `--tolerance` and `--baseline` swallow bad input

`vt-bench.rs:67-73`: `--tolerance abc`, and `--tolerance` with no value, both
fall back silently to 0.5; `--baseline` with no value silently keeps the
committed default. A mistyped band yields a confident verdict against a band the
operator did not ask for. Every other bad flag is a hard error
(`vt-bench.rs:87-90`), so this is inconsistent with the tool's own posture.

### F8 -- low -- stray tier-3 sentence in a tier-2 chapter

`14-performance.md:42`, inside "What is not measured", a section about the
tier-2 number published below, reads "Tier 3 measures the cost of *producing*
what a renderer would paint". Leftover from the tier-3 plan; a reader has to
work out which tier the exclusions apply to.

### F9 -- informational -- pre-existing tier-3 measurement artifact, not in scope

`crates/tools/src/bench.rs:454-456`: `cells = render.rows().iter().map(|row|
row.cells.len()).sum();` sits **inside** the timed span, so `us/frame` includes a
full walk of every snapshot row. Tier 3 also takes a single sample with no
warm-up and no median, unlike the interleaved tiers 1-2. Untouched by this
packet and not published, but it is the tier the packet originally intended to
publish, and it would need this fixed first.

## 5. Gates

All green. `pwsh scripts/ci-local.ps1 -Full` reported **"ci-local: all checks
passed."**, exit 0 -- every step including `cargo deny check licenses bans
advisories`. The two sibling worktrees were idle for the reproduction run above
but compiling during part of this gate; nothing here is a timing measurement, so
that does not affect the result.

| Gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| ... `--features oneterm-app/terminal-diagnostics` | pass |
| `cargo test --workspace` | pass |
| `cargo test -p oneterm-vt --features vt-paranoid` | pass |
| `cargo test -p oneterm-vt --features regex` | pass |
| `cargo build -p oneterm-vt --no-default-features --examples` | pass |
| `cargo test -p oneterm-vt --no-default-features` | pass |
| `cargo build -p oneterm-vt --all-features --examples` | pass |
| `cargo doc -p oneterm-vt --no-deps` / `--all-features`, `RUSTDOCFLAGS=-D warnings` | pass -- chapter 14 renders warning-free |
| `vt-public-api.py --check --no-doc` | pass -- **public surface unchanged**, the new `pub mod ch14_performance` is invisible to it |
| `vt-public-api.py --check-nameable --no-doc` | pass |
| `vt-public-api.py --diff-platforms` | pass |
| rustdoc self-containment, `crates/vt/src` and `crates/vt/docs/guide` | pass |
| `verify-dependency-graph.py`, `check-doc-paths.py`, `check-english.py`, `test_check_english.py`, `completion-catalog.py validate`, `third-party-notices.py --check` | pass |
| `cargo deny check licenses bans advisories` | pass |

Run separately, outside `ci-local`:

| Gate | Result |
| --- | --- |
| `cargo test -p oneterm-vt --all-features` | pass -- 496 unit + every integration suite, 0 failed |
| `cargo test -p oneterm-tools` | pass -- 16 tests, 0 failed |
| `cargo test -p oneterm-vt --all-features --doc` | pass -- **37 doctests** |
| `cargo test -p oneterm-vt --doc` (default) | pass -- 36 doctests |
| `cargo test -p oneterm-vt --no-default-features --doc` | pass -- 34 doctests |

The "37 doctests" claim is the `--all-features` figure and is exact. Chapter 14
contributes none of them -- it carries only `console` fences
(`14-performance.md:101,103,120,122`), as disclosed.

## 6. Records

- Packet ticks match reality for every criterion except the two noted in F4.
  The tier-3-vs-tier-2 substitution is disclosed prominently
  (`US-0107-performance-evidence.md:86-95`) and its justification is factually
  correct: tier 3 reports us/frame and cannot carry a MiB/s table.
- The `crates/tools` +251 vs +100 overrun is disclosed
  (`US-0107-performance-evidence.md:276`) and the arithmetic checks out.
- The interleaving break with earlier packets' tier-1/2 figures is disclosed
  (`:284`).
- Harness snippet: `unit_proof=0, integration_proof=1, e2e_proof=0,
  platform_proof=1` is consistent with what was actually proved (no new
  `#[test]`; nine guard states; a platform-specific run). `intake_id=44` matches
  the sibling `BUG-0059` snippet (`BUG-0059-unnameable-public-types.md:356`).
  The database itself is not reachable from this worktree, so the row could not
  be checked against it.
- `CHANGELOG.md:56-64` places both entries under `[Unreleased] / ### Added` and
  the "fourteen Markdown chapters" correction is in the same block.
- Minor inconsistency, no finding raised: `BUG-0059`'s harness `evidence` column
  is a path to its verification file, `US-0107`'s is a prose blob.

## 7. What could not be verified

- **The transport ceiling** (about 1.2 MiB/s for `cmd.exe`, about 30 MiB/s for a
  DOOM-fire-class producer). Quoted from `pty-throughput` per `IN-0029`; not
  re-measured here. The chapter's conclusion depends on it.
- **"A debug build is an order of magnitude out"** (`14-performance.md:110`).
  The debug build refuses to `--check`, so no debug figure was produced.
- **`harness.db`** -- not present in or reachable from this worktree.
- **The published run itself.** It cannot be re-run; what is verified is that
  the committed baseline is genuine tool output (its key order is serde_json's
  alphabetical `BTreeMap` order) and that an independent run on the same machine
  class lands on it.

## Commands

```console
$ git reset --hard docs/vt-performance-evidence          # c05320fb
$ cargo build -p oneterm-tools --release --bin vt-bench
$ cargo run -p oneterm-tools --release --bin vt-bench -- grid --mib 32
$ cargo run -q -p oneterm-tools --release --bin vt-bench -- grid --mib 32 --check
$ cargo run -q -p oneterm-tools --release --bin vt-bench -- grid --mib 4 --check --baseline <scratch>/{doubled,stale,missing-entry,no-tier,bad,nope}.json
$ cargo run -q -p oneterm-tools --release --bin vt-bench -- render --check
$ cargo run -q -p oneterm-tools --bin vt-bench -- grid --mib 1 --check          # debug
$ cargo run -q -p oneterm-tools --release --features oneterm-tools/vt-paranoid --bin vt-bench -- grid --mib 1 --check
$ grep -rnE "(US|BUG|IN|DEC)-[0-9]{4}|docs/|crates/" crates/vt/docs/guide --include='*.md' | grep -v 'https://github.com/'
$ grep -rni thirteen crates/vt
$ pwsh scripts/ci-local.ps1 -Full
```
