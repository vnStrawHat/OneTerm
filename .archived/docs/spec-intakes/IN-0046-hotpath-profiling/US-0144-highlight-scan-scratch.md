# Work: highlight scanner reuses its per-line buffers

ID: US-0144
Intake: IN-0046
Created: 2026-09-25

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: maintenance (performance; no behaviour change)
- Risk lane: normal (`oneterm-highlight` is internal; the output is pinned by a recorded
  corpus)
- Spec Intake, when required: [`IN-0046`](IN-0046.md), candidate 2 of
  [`research/hotpath-evaluation.md`](research/hotpath-evaluation.md) § 5

## Outcome

`scan_line_into` allocates no per-line buffer of its own in steady state. Its per-line working buffers
(`chars`, the byte-to-char map) live in one caller-owned scratch type that is cleared, not
reallocated; a pure-ASCII line builds no byte-to-char map at all (byte index == char
index). Every class it produces is unchanged.

## Scope

- [x] In scope:
  - One `ScanScratch` type in `oneterm-highlight`, taken by `scan_line_into`; owned by
    the render `Scratch` in `oneterm-terminal-view` and by `highlight-bench`.
  - The ASCII fast path for the byte-to-char map.
  - Identifying every allocation hotpath counted inside `scan_line_into` (four per line).
  - A recorded-output test over ASCII, CJK, Vietnamese, emoji and mixed lines, and a
    zero-allocation test.
  - Before/after measurement: hotpath TUI load, `highlight-bench`, `frame_time_under_output`.
- [x] Out of scope: any matcher or class change; the other IN-0046 candidates.

## Acceptance

- [x] Classes identical to the pre-change scanner for every case of the recorded corpus
  (ASCII, CJK, Vietnamese, emoji, mixed; every role; four profiles).
- [x] Zero allocations per `scan_line_into` call in steady state for ASCII lines
  (counting-allocator test). A non-ASCII line keeps `regex`'s own 16-byte allocations, one
  per search that falls back from the lazy DFA; the test bounds them by size so that no
  scanner buffer can hide among them (see Evidence).
- [x] hotpath TUI load: `scan_line_into` allocations per call 0 (from 4), bytes per call
  14 B average and 0 B p95 (from 1.3 KB); avg time not worse.
- [x] Gates below green, full `ci-local` included.

## Documentation

### Owning Docs Reviewed

- `docs/terminal-semantic-highlighting.md` § 7 item 3 — "The scanner writes into a reused
  `Vec<u8>` scratch." Describes only the output buffer.
- `docs/spec-intakes/IN-0044-semantic-highlighting-phase-2/US-0135-highlighter-benchmark-and-scan-bounds.md`
  — the bench and the `class_scans`/`class_rows_scanned` bounds; unaffected (the number of
  scans does not change, only their cost).
- `docs/spec-intakes/IN-0046-hotpath-profiling/research/hotpath-evaluation.md` § 5 — the
  finding.
- `crates/tools/src/bin/highlight-bench.rs` header — names the byte-to-char map as one of
  the measured costs.

### Documentation Action

Update required: `docs/terminal-semantic-highlighting.md` § 7 item 3 (the scanner's
working buffers are caller-owned too; ASCII lines skip the map); `IN-0046.md` candidate
list (US-0144 created).

Reason: the allocation model of the scan is described there and changes.

### Reconciliation

Changed: `docs/terminal-semantic-highlighting.md` § 7 item 3 (the scan's buffers and the
ASCII path); `IN-0046.md` (US-0144 linked). The US-0135 packet and the bench header stay
correct (the bench still measures the whole per-line cost, map included for non-ASCII).

## Context

- `scan_line_into` (`crates/highlight/src/scanner/mod.rs`) built `chars: Vec<char>` and,
  on the output path, `byte_to_char: Vec<usize>` (8 B per byte) on every call; PERF-23 made
  only `out` caller-owned.
- Callers: `SemanticOverlay::scan_into` (render `Scratch`, via `class_rows_into`),
  `highlight-bench`, and the allocating `scan_line` wrapper (tests).

## Plan

- [x] Record the current scanner's output over the corpus (before any change).
- [x] `ScanScratch { chars, byte_to_char }`; ASCII fast path; account for the other
  allocations.
- [x] Thread it through the overlay, the render `Scratch` and the bench.
- [x] Tests; measurements; docs.

## Decisions

None.

## Verification Plan

- Recorded-corpus test and zero-allocation test in `oneterm-highlight`.
- `cargo test -p oneterm-highlight -p oneterm-terminal-view`.
- hotpath release builds (time, alloc count, alloc bytes) under `hotpath-measure.ps1 -Mode
  Tui` (180 s), before and after; `highlight-bench` before and after;
  `frame_time_under_output` before and after.
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `python scripts/check-doc-paths.py`, `python scripts/check-english.py`, full
  `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### What changed

- `ScanScratch` (`crates/highlight/src/scanner/mod.rs`, exported): `chars: Vec<char>` and
  `byte_to_char: Vec<usize>`, cleared and refilled per line. `scan_line_into` takes it
  (`&mut ScanScratch` before `out`); `scan_line` makes a throwaway one. Owners: the render
  `Scratch::scan` (`crates/terminal-view/src/render/row_plan.rs`) via
  `SemanticOverlay::scan_into`, and `highlight-bench`.
- `LineText` borrows the map; an ASCII line (`str::is_ascii`) leaves it empty and
  `char_index(byte)` returns `byte.min(len)`, which is what the identity map plus its
  sentinel returned.

### The four allocations hotpath counted

Read from the code (hotpath reports the count, not the call sites); the new counting-allocator test confirms none of them is left:

1. `chars: Vec<char>` — every call. Now the scratch.
2. `byte_to_char: Vec<usize>` — every output-mode call. Now the scratch; none for ASCII.
3. and 4. Growth reallocations of `chars`: `collect()` on a `Chars` iterator starts from
   its size-hint lower bound (`len / 4`), so a typical line reallocates it about twice on
   the way to its length. The map was `with_capacity(len + 1)` and did not grow. Gone with
   the Vecs.

Remaining, justified: `regex`'s own allocations, one 16-byte boxed `MatchError` per
search whose lazy DFA quits on a Unicode `\b` next to a non-ASCII byte, after which
`regex` retries on another engine. Both the date/time and the MAC patterns do this. The
count depends on the line, not on the scanner: 0 for a line that is only `é`, 2 for a
non-ASCII line with a MAC or ending in CJK, 114 (1.8 KB) for a 5,000-char CJK line (the
verifier's figures, [`evidence/US-0144-verify.md`](evidence/US-0144-verify.md) § 4); main's
map alone for that line was about 120 KB. Removing them would mean an ASCII-only
`(?-u:\b)`, which was **rejected**: it changes classes (`日本語2026-09-22` gains a date,
`caféMon` a month, `é00:1a:2b:3c:4d:5e` a MAC). The golden corpus pins this with the base
line `日本語2026-09-22 caféMon é00:1a:2b:3c:4d:5e`, recorded on main's scanner; swapping both
`\b` to `(?-u:\b)` fails it. The allocation test asserts 0 for ASCII lines and, for
non-ASCII lines, that no allocation exceeds 32 B (every test line is at least 20 chars, so
any per-line scanner buffer would be at least 80 B), which does not tie the gate to
`regex`'s internal count. `out` growth is the caller's and happens once per longest line.

Capacity: the scratch keeps the capacity of the longest line it has scanned (4 B per char
for `chars`, 8 B per UTF-8 byte for the map), the same policy as the neighbouring render
`Scratch` buffers. The logical line is bounded by the viewport, so the worst case is about
420 KB per view (a 250x60 all-CJK screen, about 15k chars: 60 KB + 360 KB).

### Measurements

Machine: the IN-0046 machine, Windows 11, toolchain 1.96.0, release (fat LTO). Two other
agents were building in parallel throughout, so timings carry that noise; counts and bytes
do not. Before = main `7d8de84b`; after = this branch. hotpath-measure.ps1 `-Mode Tui` (two
tabs of `tui-mimic.py`, 180 s, 1280x800), one run per metric per side. Raw reports:
[`research/raw/us-0144/`](research/raw/us-0144/).

| `scan_line_into`, 2-tab TUI 180 s | before | after | change |
| --- | ---: | ---: | ---: |
| avg time (timing build) | 2.92 us (77,127 calls) | 1.88 us (80,434 calls) | -36 % |
| p95 time (timing build) | 6.70 us | 3.50 us | -48 % |
| total time (timing build) | 225.4 ms | 151.5 ms | -33 % |
| allocations per call (count build) | 4 | 0 (avg and p95) | -4 |
| allocations, whole run | 319,913 | 1,607 | -99.5 % |
| bytes per call (bytes build) | 1.3 KB | 14 B avg, 0 B p95 | -1.3 KB |
| bytes, whole run | 106.7 MB | 1.1 MB | -99 % |
| `class_rows_into` avg (timing build) | 141.3 us | 92.7 us | within noise |
| `PlanCache::update` avg (timing build) | 598.0 us | 518.3 us | within noise |

Only the `scan_line_into` rows are robust: its avg improves in all three builds (before
2.10 / 2.69 / 2.92 us, after 1.80 / 1.91 / 1.88 us). The `class_rows_into` and
`PlanCache::update` gains come from the single timing pair; the count and bytes builds of
the same runs give `class_rows_into` 117.5 / 92.2 -> 93.9 / 88.4 us and `update`
532 / 435 -> 514 / 487 us, which is run-to-run noise (verifier finding F4).

`highlight-bench` (release, 5 runs per cell; median ns/char over three profiles and three
wrap widths per cell; the committed baseline is **not** refreshed, see Gaps):

| shape | chars | before ns/char | after ns/char | change |
| --- | ---: | ---: | ---: | ---: |
| prompt | 80 | 3.51 | 2.11 | -40 % |
| prompt | 2000 | 1.17 | 1.06 | -10 % |
| plain | 80 | 12.89 | 9.50 | -26 % |
| plain | 2000 | 9.65 | 8.48 | -12 % |
| keyword-log | 80 | 17.29 | 12.28 | -29 % |
| keyword-log | 2000 | 14.74 | 12.91 | -12 % |
| cjk | 80 | 60.95 | 52.09 | -15 % |
| cjk | 2000 | 76.93 | 73.19 | -5 % |

The ASCII shapes reproduce under the verifier's paired runs (-7 % to -42 %); the CJK gain
does not (-4 % to +7 %, noise), and `keyword-log` at 2000 chars is noisy (verifier finding
F5).

`frame_time_under_output` (`fast-dev`, 3 runs each, flood avg / p50 / p95): before
1657/1635/1815, 1887/1761/2957, 1664/1634/1909 us; after 1684/1649/1797, 1665/1637/1834,
1670/1624/1865 us. No measurable change: its output is `cmd`-style numbered lines where the
scan is a small share of a debug-assertion frame; the second before-run is an outlier.

### Commands

- `cargo test -p oneterm-highlight -p oneterm-terminal-view`: pass (includes
  `tests/scan_line_output.rs`, 144 recorded hashes over 24 base lines x 6 ASCII / CJK /
  Vietnamese / emoji / mixed / non-ASCII-path variants x 4 profiles x 5 role cases, and
  `tests/scan_allocations.rs`). The recorded-output test was recorded on main's scanner
  before any change (the 24th base line, from verifier finding F3, was recorded afterwards
  with main's `scanner/mod.rs` and `lib.rs` restored), and fails when `char_index` is
  shifted by one or when both `\b` become `(?-u:\b)` (mutation checks).
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `python scripts/check-doc-paths.py`, `python scripts/check-english.py`: pass.
- Full `pwsh scripts/ci-local.ps1`: `ci-local: all checks passed.`

### Gaps

- `crates/tools/highlight-bench-baseline.json` not refreshed: the before/after pair above
  was taken on a loaded machine; refresh it on a quiet one as its own act.
- One run per metric per side; no repeat runs of the TUI load.
- `regex`'s fallback allocations on non-ASCII lines stay (above).

## Handoff

None.
