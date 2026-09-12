# Work: Benchmark and parity harness in `crates/tools`

ID: US-0072
Intake: IN-0029
Created: 2026-09-12

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability (test and measurement infrastructure only; no product behaviour changes)
- Risk lane: high_risk (the intake's lane; this packet itself ships no runtime code)
- Spec Intake, when required: IN-0029

## Outcome

The safety net the VT engine rewrite is measured against exists and is frozen, **before** any
engine code is written:

1. The 45 alacritty ref recordings are vendored with attribution, and every one of them has two
   expectation files blessed by the **old** (vendored `alacritty_terminal`) engine.
2. `vt-corpus` replays a recording through the old engine, blesses, checks a run against the
   frozen expectations with a readable row/column diff, cross-checks `grid.expect` once against
   upstream's `grid.json`, and greps the recordings for every deferred deviation.
3. `vt-bench` reports the five tiers the LLD defines for the old engine; those numbers are the
   baseline the new engine is compared against.
4. `cargo test --workspace` fails when the expectations drift, and a CI bench job records numbers
   without gating.

## Scope

- [ ] In scope:
  - `crates/vt/tests/corpus/alacritty-ref/<name>/{recording,size.json,config.json}` (45), the
    corpus `NOTICE`, `deviations.json`, and the generated `grid.expect` / `state.expect`.
  - `crates/tools`: a new `lib.rs` with `corpus` and `bench` modules, the `vt-corpus` and
    `vt-bench` binaries, and one integration test that is the drift gate.
  - `.github/workflows/ci.yml`: a Windows `vt-bench` job that records and never gates.
  - Owning-doc reconciliation: `docs/agents/structure.md`, `docs/license-analysis.md`,
    `THIRD-PARTY-NOTICES.md` (via `scripts/third-party-notices.py`).
  - Evidence under `docs/spec-intakes/IN-0029-vt-engine/evidence/`: the cross-check report, the
    bench baseline table, and the recording-risk report.
- [ ] Out of scope:
  - Any `oneterm-vt` or `oneterm-pty` source. `crates/vt/` gains **data only** in this packet;
    the crate manifest arrives with `US-0073` / `US-0074`.
  - `crates/tools/src/bin/pty-throughput.rs` and anything PTY-related (`US-0071` owns them).
  - `vt-diff.rs` and `vt-fixtures.rs` — see Evidence and Gaps for why neither is built here.
  - Editing the deviation tables in `low-level-design/dispatch-and-modes.md` and
    `low-level-design/grid-and-scrollback.md`: the measured answers are produced here as a
    report; the LLD cells are owned by the design agent working concurrently.
  - Fuzzing (`R-47`: Linux-only, scheduled, never a packet gate) and the `esctest` / `vttest`
    report job.

## Acceptance

- [x] All 45 recordings are vendored with `recording`, `size.json` and `config.json`, with an
      Apache-2.0 `NOTICE` naming the upstream revision. Upstream `grid.json` is **not** committed.
- [x] `vt-corpus bless --engine old` produced `grid.expect` and `state.expect` for all 45.
      `grid.expect` is cell-exact and nothing is trimmed: every column of every row, including
      trailing blanks, carries content, width class, fg, bg, attrs, underline colour and
      hyperlink; per grid the row count, column count and viewport position are recorded.
- [x] `bless` refuses to overwrite an existing expectation without `--deviation <id>`, and
      refuses `--engine new` outright. The deviation id must name a row in the LLD tables.
- [x] A per-recording, cell-level deviation overlay (`deviations.json`, keyed by deviation id)
      exists and is applied by `check`; a unit test proves an overlay entry turns a mismatch into
      a pass and that an unknown id is rejected.
- [x] `vt-corpus cross-check --grid-json <dir>` ran once over upstream's `grid.json` set; the
      per-recording match/mismatch result and an explanation for every mismatch are recorded in
      `evidence/US-0072-cross-check.md`. The comparison rezeroes the ring and ignores
      `occ` / `visible_lines` / `max_scroll_limit` / cursors, exactly as upstream's `Grid::eq`
      does (traps 44 and 45).
- [x] `vt-corpus grep-deviations` filled a recording-risk report
      (`evidence/US-0072-recording-risk.md`) for every deferred deviation in
      `dispatch-and-modes.md` and `grid-and-scrollback.md`.
- [x] `vt-bench` reports all five tiers for the old engine, from fixtures reproducible from the
      repository, as a Markdown or JSON table; the baseline is stored in
      `evidence/US-0072-bench-baseline.md`.
- [x] `cargo test --workspace` runs the corpus check and fails on drift.
- [x] A Windows CI bench job exists, records the table as an artifact, and cannot fail the build.
- [x] `pwsh scripts/ci-local.ps1` green.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0029-vt-engine/IN-0029.md` — packet list, stop condition for `US-0072`,
  intake-level acceptance ("parity gate green", "the benchmark is recorded, not gated"),
  design summary items 2 and 12 (parity-first; the old engine blesses).
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/testing-and-bench.md` — the spec for this
  packet: corpus layout and size, the two expectation files, who blesses, the cross-check, the
  differential runners, the five bench tiers, the CI wiring and the deviation grep.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/dispatch-and-modes.md` and
  `.../grid-and-scrollback.md` — the deviation tables D1-D15 and G1-G7, whose "Recording risk"
  cells this packet measures. **Read only**: owned by the design agent, edited concurrently.
- `docs/spec-intakes/IN-0029-vt-engine/research/engine-semantics.md` §7 (corpus format, what
  upstream actually compares) and §8 (traps 44, 45).
- `docs/spec-intakes/IN-0029-vt-engine/research/perf-baseline.md` — the scratch bench this
  packet promotes, its method, machine and numbers.
- `docs/license-analysis.md` §3 — third-party source policy: Apache-2.0 source may be reused
  with the notice retained.
- `docs/agents/structure.md` §1 tree, §3 responsibility table — `crates/tools` and the new
  `crates/vt/tests/corpus/` data directory.
- `docs/agents/crate-dependency-rules.md` — `crates/tools` sits outside the L0-L4 layering and
  depends on no OneTerm crate; this packet keeps that true.
- `docs/agents/dependencies.md` — every third-party dependency is declared once in the root
  `[workspace.dependencies]`.
- `docs/terminal-backend.md` §4 (vendored rev lock), §5 (pump layer) — read to reproduce
  `Processor::<StdSyncHandler>` + `Term` exactly as `crates/terminal/src/backend/pump.rs` does.
- `THIRD-PARTY-NOTICES.md` — generated; §2 lists the vendored forks. The corpus is a new
  reused-source row.
- `AGENTS.md` §4 — the quality gate this packet must pass.

### Documentation Action

Update required:

- `docs/agents/structure.md` — the tree gains `crates/vt/tests/corpus/` (data only, no crate yet)
  and the `crates/tools` responsibility row gains the two new binaries.
- `docs/license-analysis.md` §3 — the reused-source list gains the alacritty ref corpus with its
  licence, revision and the fact that `grid.json` is deliberately not carried.
- `THIRD-PARTY-NOTICES.md` / `scripts/third-party-notices.py` — a row for the vendored corpus, so
  `python scripts/third-party-notices.py --check` stays green and the attribution is in the
  generated file rather than only in a `NOTICE` nobody regenerates.

No contract change for `docs/terminal-backend.md`: this packet adds no runtime behaviour and
changes nothing the document describes. It was read to copy the engine construction exactly.

Reason: vendoring third-party data and adding two developer binaries are both facts the
structure and licence documents own; everything else this packet touches is test-only.

### Reconciliation

Changed: `docs/agents/structure.md`, `docs/license-analysis.md`,
`scripts/third-party-notices.py` + the regenerated `THIRD-PARTY-NOTICES.md`.
`docs/terminal-backend.md` no-change reason above still valid — verified at completion.

## Context

- The corpus is vendored at `crates/vt/tests/corpus/` — where
  `low-level-design/testing-and-bench.md` §2 says it belongs — even though `crates/vt/Cargo.toml`
  does not exist yet. Data placed at its final path now is zero moves later; the directory holds
  no Rust and no manifest, so `cargo metadata`, `verify-dependency-graph.py` and
  `check-english.py` (which only reads `.md/.ps1/.py/.rs/.sh/.toml/.yaml/.yml`) are unaffected.
- The tool lives in `crates/tools` as `lib` + `bin` rather than a single `bin`, because the drift
  gate has to be a `#[test]` that reuses the replay code.
- Upstream's comparison is narrower than its JSON (`research/engine-semantics.md` §7.3):
  `Grid::eq` ignores `max_scroll_limit`, `Storage::eq` ignores `visible_lines` and **panics**
  unless both sides have `zero == 0` (trap 45), and `Row::eq` ignores `occ`. The cross-check
  therefore replays, clones the grid, calls `initialize_all()` then `truncate()` exactly as
  `tests/ref.rs` does, and compares only the fields upstream compares.
- `Storage`'s ring maps `inner[i]` to `Line(screen_lines - i - 1)` once `zero == 0`, so
  `grid.expect` rows are emitted newest-first to match upstream's serialized order.
- `Cell::extra` fields are private; the public accessors (`zerowidth()`, `underline_color()`,
  `hyperlink()`, `graphic()`) are what `grid.expect` encodes.
- Tab stops and the scroll region have no public accessor on `Term`. They are recovered
  observationally on a second replay: `CR` + repeated `HT` reads the stops out of the cursor
  column, and `DECOM` + `CUP` to row 1 / row 999 reads the region bounds out of the cursor row.
  The title arrives as `Event::Title` on the event listener.

## Plan

- [x] Vendor the 45 recordings + `size.json` + `config.json`, with a corpus `NOTICE`.
- [x] `crates/tools/src/corpus/`: replay, `grid.expect` encode, `state.expect` capture, compare
      with a readable diff, deviation overlay, upstream `grid.json` conversion, deviation grep.
- [x] `vt-corpus` CLI: `bless`, `check`, `cross-check`, `grep-deviations`.
- [x] Bless all 45 with the old engine; review the output; freeze.
- [x] Run the cross-check against upstream `grid.json`; explain every mismatch.
- [x] Run `grep-deviations`; write the recording-risk report.
- [x] `crates/tools/src/bench/`: five tiers, deterministic fixtures, Markdown/JSON output.
- [x] Record the old-engine baseline.
- [x] `crates/tools/tests/corpus_check.rs` as the drift gate.
- [x] CI bench job; docs and notices reconciled.

## Decisions

None new. This packet implements `DEC-0014` / `DEC-0015` consequences and the intake's design
summary items 2 (parity-first with a written deviation table) and 12 (the old engine blesses,
the new engine never does) without reopening either.

## Verification Plan

- Focused: `cargo test -p oneterm-tools` — the corpus drift gate over all 45 recordings, plus
  unit tests for the RLE round trip, the deviation overlay, the `bless` guard and the flag-name
  parsing used by the cross-check.
- Integration: `cargo test --workspace` — the drift gate runs in the workspace test set.
- Manual proof, recorded as evidence: `vt-corpus cross-check --grid-json <scratch>` over all 45,
  `vt-corpus grep-deviations`, and `vt-bench all`.
- Quality gate: `pwsh scripts/ci-local.ps1`.
- No E2E and no platform proof: this packet ships no UI and no runtime path. The CI bench job is
  verified by inspection (it is `continue-on-error` and uploads an artifact) — it cannot be run
  locally.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### What was built

| Thing | Where |
| --- | --- |
| 45 vendored recordings + `NOTICE` | `crates/vt/tests/corpus/alacritty-ref/<name>/{recording,size.json,config.json}` |
| Frozen expectations | `.../<name>/{grid.expect,state.expect}` |
| `vt-corpus` | `crates/tools/src/bin/vt-corpus.rs` — `check`, `bless`, `cross-check`, `grep-deviations` |
| `vt-bench` | `crates/tools/src/bin/vt-bench.rs` — `all`, `parser`, `grid`, `render`, `resize`, `rss`, `fixtures` |
| Library half | `crates/tools/src/{lib,corpus,corpus_replay,corpus_upstream,corpus_grep,bench}.rs` |
| Drift gate | `crates/tools/tests/corpus_check.rs` |
| CI bench job | `.github/workflows/ci.yml` job `vt-bench` (`continue-on-error`, uploads `vt-bench.md`) |

### Repository size added

**2.84 MiB total**: recordings 936.1 KiB (958 551 B), `grid.expect` 1.90 MiB
(1 988 311 B), `state.expect` 24.2 KiB, `size.json` + `config.json` 2.3 KiB, `NOTICE`
1.9 KiB. Upstream's 44.0 MiB of `grid.json` is **not** committed. `.gitattributes` gains
`crates/vt/tests/corpus/** -text -whitespace`, because a CRLF conversion on a captured PTY
recording would silently invalidate the whole corpus.

### Cross-check against upstream `grid.json`

**45 of 45 match; zero mismatches.** Full method, the five caveats that had to be handled
(the `Storage::PartialEq` rezero panic, the ignored `occ`/`visible_lines`/`max_scroll_limit`,
row order, the process-global hyperlink-id counter, composite bitflags aliases) and the
oracle's SHA-256 are in [`evidence/US-0072-cross-check.md`](evidence/US-0072-cross-check.md).
Notable secondary result: **none of the fork's three alacritty patches changes anything the
45 recordings pin.**

### Recording risk

[`evidence/US-0072-recording-risk.md`](evidence/US-0072-recording-risk.md) answers every
`C1`-`C11`, plus `G3` and `D12`. Four findings the design should absorb:

1. **C11 (blink/overline) is free** — no recording sends `SGR 5 / 6 / 53 / 55`. N-03's
   reason for holding it out of the parity packet is not supported by the corpus.
2. **C9 (DECSTR) is not free** — `grid_reset` sends one `CSI ! p` the old engine drops, so
   it is the one correction certain to move `grid.expect`. The design said "none expected".
3. **C7, C8, C10 are free** — all three resolve to "none of the 45", including the three
   alt-screen recordings the design flagged for C8 (they use `? 1049`).
4. **C1 and C3 are wider than assumed** — C1 touches eight recordings, not one; C3's two
   real candidates (`vim_24bitcolors_bce`, `vttest_insert`) are not the two named.

### Benchmark baseline

[`evidence/US-0072-bench-baseline.md`](evidence/US-0072-bench-baseline.md) (raw:
`-raw.md` and `.json`). Headlines, all for the **old** engine at 160x45:

- Tier 1 parser alone: 66-1346 MiB/s. Tier 2 parse+grid: 38-181 MiB/s. The grid half costs
  1.2x-12.7x the parser half.
- Tier 3 snapshot build: 21-42 us per frame for 7200 cells (1-2.5 % of a 60 Hz budget).
- Tier 4 resize 80x24 to 100x40: 1.2 ms at 0 rows, 4.7 ms at 10 000, **53 ms at 100 000** —
  the one operation a user can feel.
- Tier 5: **4138 bytes per 160-column row (about 26 B/cell), identical for plain, unicode,
  styled and mixed content**; 39.5 MiB for 10 000 rows.

### Verification run

- `pwsh scripts/ci-local.ps1` — **all checks passed** (2026-09-12), including
  `cargo fmt --check`, `clippy --workspace --all-targets -D warnings`, `cargo test
  --workspace`, `verify-dependency-graph.py` (19 packages), `check-doc-paths.py`,
  `check-english.py` (648 files) and `third-party-notices.py --check`.
- Raw workspace test totals (`rtk proxy cargo test --workspace`, summed over all 50
  `test result:` lines including doctests): **1146 passed, 0 failed, 5 ignored**. The
  pre-packet baseline was 1131; this packet adds 15 (14 unit tests in
  `corpus_tests.rs`, 1 integration test that is the drift gate).
- `vt-corpus check` — 45 recordings, 45 passed, 0 failed. Old-against-old, which is the
  self-test of the comparator this packet owes.
- `vt-corpus bless --engine old` refuses a second time without `--deviation`; verified by
  running it after the freeze.

### Gaps

- **The LLD deviation tables are not updated.** `dispatch-and-modes.md` and
  `grid-and-scrollback.md` are owned by the design agent and were being edited
  concurrently, so the measured answers live in `evidence/US-0072-recording-risk.md`
  instead of in their "Affected recordings" cells. **Next owner action:** copy the four
  findings above into those tables.
- **No `expected-diffs.toml` exists yet**, because every correction lands in `US-0075` /
  `US-0076`. The mechanism is implemented and unit-tested (declared window passes,
  wrong row/column/field fails, stale window fails, unknown id fails, the design's own
  TOML example parses), but it has never run against a real correction.
- **`vt-diff.rs` and `vt-fixtures.rs` were not built.** `vt-diff` compares the old engine
  against the new one, and there is no new engine; `vt-corpus check` *is* old-against-old
  today, so a second binary would have been a copy with no second input. `vt-fixtures`
  collapsed into `vt-bench fixtures --out <dir>`, since the generators had to live in the
  bench anyway. Both should be reconsidered at `US-0073`.
- **Tier 5 reports live heap, not process RSS.** A counting global allocator answers the
  design's actual question (what a grid of N rows costs) deterministically and with no new
  dependency; true RSS would need a `windows-sys` feature and would fold in retained
  allocator pages. The LLD's subcommand name `rss` is kept.
- **`state.expect` cannot carry the title *stack*.** `Term` exposes no title state at all;
  the file records the `Event::Title` / `Event::ResetTitle` trail, which is everything the
  old engine can observably produce. Tab stops and the scroll region are recovered by a
  second, probe-only replay (`CR` + repeated `HT`; `DECOM` + `CUP` to rows 1 and 999).
- **The tab-stop probe's last column may be a clamp, not a stop.** `put_tab` moves to the
  final column when no stop remains. Both engines produce it identically, so the gate is
  unaffected, but the file is not a literal tab-stop set.
- **The CI bench job is unverified.** It cannot run locally; it was reviewed by
  inspection (`continue-on-error: true`, no assertion, artifact upload) and will first run
  when the branch reaches CI.
- **Bench variance.** Tiers 1-3 move up to 30 % (tier 3 up to 80 %) between runs on a
  loaded machine. Recorded, never gated — read as an order of magnitude.
- **Numbers differ from `research/perf-baseline.md`** (126 MB/s versus 94 MiB/s for plain
  ASCII) because that note measured 200x50 in MB and this measures 160x45 in MiB. This
  table, not that note, is the baseline `US-0073` onwards compares against.

## Handoff

Next owner: `US-0073` (parser core) and `US-0076` (dispatch and modes, whose exit criterion is
the parity gate). Both consume the frozen expectations; neither may re-bless them.
