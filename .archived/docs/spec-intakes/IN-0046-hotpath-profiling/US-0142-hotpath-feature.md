# Work: hotpath profiling feature

ID: US-0142
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

- Change type: new capability (developer-only build feature)
- Risk lane: normal (off by default; no runtime behaviour, persisted data or public item
  changes; `oneterm-vt` gains a default-off feature)
- Spec Intake, when required: [`IN-0046`](IN-0046.md); design:
  [`high-level-design.md`](high-level-design.md)

## Outcome

`cargo build -p oneterm-app --release --features hotpath-profiling` (or
`hotpath-profiling-alloc`) produces a OneTerm that reports per-function time (or
allocations) for the hot paths listed in the HLD. Without the feature nothing changes:
no `hotpath` in the graph, the same release binary.

## Scope

- [x] In scope:
  - `hotpath = "0.26"` in root `[workspace.dependencies]`, optional in seven crates.
  - `hotpath-profiling` features and the fan-out in `oneterm-app`;
    `hotpath-profiling-alloc` in `oneterm-app`.
  - `#[hotpath::main]` on `oneterm_app::run` wrapping `OomResilientAlloc`.
  - `cfg_attr` sites listed in the HLD; one extracted `pty_read` helper in
    `oneterm-local-shell` so the read has a function to carry the attribute.
  - `docs/agents/dependencies.md` row; `THIRD-PARTY-NOTICES.md` checked, no change (it lists
    the default feature graph, and the feature is off by default).
  - Measurements and findings in `research/hotpath-evaluation.md`.
- [x] Out of scope: every optimization the findings propose (US-0143 and later).

## Acceptance

- [x] Without the feature: `cargo tree -p oneterm-app -e normal` has no `hotpath`, and
  the release binary built from this branch has the same size as main's.
- [x] With the feature: the app starts, the report names every site under a load that
  reaches it, and `OomResilientAlloc` is still the inner allocator in the alloc build.
- [x] `cargo clippy --workspace --all-targets -- -D warnings` passes with no feature, with
  `oneterm-app/hotpath-profiling` and with `oneterm-app/hotpath-profiling-alloc`.
- [x] Overhead quantified with the `frame_time_under_output` test, with and without.
- [x] `python scripts/third-party-notices.py --check` and `cargo deny check licenses bans
  advisories` pass.
- [x] Full `pwsh scripts/ci-local.ps1` passes.

## Documentation

### Owning Docs Reviewed

- `docs/agents/dependencies.md` — dependency policy; § 3 table gains a row.
- `docs/agents/crate-dependency-rules.md` — R7: `oneterm-vt` may add only the `pty`
  platform crates; `hotpath` is not a platform crate, is optional and default-off, so R7
  and the six-leaf `--no-default-features` check still hold. No change.
- `docs/terminal-backend.md` — the pipeline the sites measure; no behaviour change.
- `docs/spec-intakes/IN-0045-memory-usage-review/` — the measurement protocol reused.
- `crates/app/src/oom.rs` — the global allocator the alloc build wraps.

### Documentation Action

- Update required: `docs/agents/dependencies.md` § 3 (new row), `crates/vt/CHANGELOG.md` and
  `crates/vt/README.md` Features table (new default-off feature). `THIRD-PARTY-NOTICES.md`:
  no change, it lists the default feature graph.

Reason: a new third-party dependency and a new `oneterm-vt` feature.

### Reconciliation

Changed: `docs/agents/dependencies.md` § 3 (new row), `crates/vt/CHANGELOG.md` (Unreleased,
Added), this intake's HLD. `THIRD-PARTY-NOTICES.md` needed no change: it lists the default
graph. `crate-dependency-rules.md` R7 still holds (no platform crate added; the six-leaf
`--no-default-features` set is unchanged).

## Context

`hotpath` 0.26.1: `#[hotpath::measure]` is a no-op unless hotpath's own `hotpath` feature
is on. `hotpath-alloc` needs a counting global allocator; `#[hotpath::main(allocator =
X)]` emits `static: CountingAllocator<X> = CountingAllocator::with(X)` as the global
allocator, so the plain `GLOBAL_ALLOC` static in `crates/app/src/lib.rs` is compiled out
under that feature.

## Plan

- [x] Manifests and features.
- [x] Sites.
- [x] Clippy with and without; binary-size comparison.
- [x] Measurements (a) to (c), frame-time overhead. (d) SFTP skipped: see Evidence and Gaps.
- [x] Findings; gates; commit.

## Decisions

None. The HLD records the wiring.

## Verification Plan

- `cargo clippy --workspace --all-targets -- -D warnings` (no feature, each feature).
- `cargo test --workspace`.
- `cargo test -p oneterm-terminal-view --release frame_time_under_output -- --ignored
  --nocapture`, with and without `--features hotpath-profiling,hotpath/hotpath`.
- Release build size with and without the feature against main.
- `python scripts/verify-dependency-graph.py`, `check-doc-paths.py`, `check-english.py`,
  `third-party-notices.py --check`, `cargo deny check licenses bans advisories`.
- Full `pwsh scripts/ci-local.ps1` with `CARGO_BUILD_JOBS=4`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- Zero cost off: release `oneterm.exe` 45,976,064 B on main `d27f06ca` and on this branch
  without the feature; 92 bytes differ (PE timestamp, panic-location line numbers shifted by
  the inserted lines). `cargo tree -p oneterm-app -e normal -i hotpath`: no match. With
  `hotpath-profiling` 47,505,920 B, with `-alloc` 47,796,736 B.
- Clippy `-D warnings`: clean with no feature, `oneterm-app/hotpath-profiling` and
  `oneterm-app/hotpath-profiling-alloc`.
- Alloc build: `#[hotpath::main(allocator = oom::OomResilientAlloc)]` declares the only
  global allocator; the plain static is `cfg`'d out. The app ran every load with it.
- Overhead (`frame_time_under_output`, `fast-dev`, 3 runs each): flood frame 1569 -> 1749 us
  mean (+11.5 %), idle frame unchanged; about 0.3 us per measured call with `hotpath`
  unoptimized. hotpath's thread sampler takes about 10 % of a core on its own thread.
- Measurements (a) idle, (b) 300k-line flood, (c) two-tab TUI: `research/raw/*.json`,
  tables in `research/tables.md`, findings in `research/hotpath-evaluation.md`.
- `cargo deny check licenses bans advisories`: ok; `cargo deny --all-features check`: ok.
  `python scripts/third-party-notices.py --check`: up to date (the file lists the default
  graph, and `hotpath` is not in it).
- `verify-dependency-graph.py`, `check-doc-paths.py`, `check-english.py`: pass.
- Gaps: (d) SFTP load not run (GUI driving not cheap; `copy_sequential` is async, so alloc
  mode reports `N/A` for it); release-build per-site overhead not measured; frame counts
  vary between runs of one load (window not foreground).
- Full gate: `CARGO_BUILD_JOBS=4 pwsh scripts/ci-local.ps1` (after deleting `target/release`),
  135 `test result: ok` lines, none failed. Final line: `ci-local: all checks passed.`

## Handoff

None.
