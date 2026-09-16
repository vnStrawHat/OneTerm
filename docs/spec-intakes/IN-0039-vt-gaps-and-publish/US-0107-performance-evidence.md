# Work: performance evidence an outsider can read and reproduce

ID: US-0107
Intake: [`IN-0039`](IN-0039.md)
Created: 2026-09-15

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

- Change type: **new capability** (published evidence and a regression guard; no engine behaviour
  changes)
- Risk lane: high_risk (inherited from the intake; this packet is the least risky of the four and
  changes no public API at all)
- Spec Intake, when required: [`IN-0039`](IN-0039.md)

## Outcome

`oneterm-vt` publishes a performance number an outsider can read, the exact command that produced
it, and the machine it was produced on -- and the repository gains a trip-wire that fires if the
engine gets twice as slow.

The evaluation scored criterion (i) **2 out of 5**, its second-worst and the only criterion where
it lost to both competitors:

> `oneterm-vt` makes no performance claim anywhere in its README, changelog or guide, which is
> honest, but it also ships nothing an evaluator can run: no `benches/`, no criterion, no
> throughput number, no `vtebench` result. For a crate whose main pitch to a GPU-renderer author is
> a damage model, the absence of any measurement is conspicuous.

The engine is not unmeasured. `vt-bench` measures it in five tiers against vtebench-derived
fixtures with a counting allocator. The gap is that the measurement is invisible from outside the
repository and produces no number anybody keeps.

## Scope

- [x] In scope:
  - A `## Performance` section in `crates/vt/README.md`: the tier-3 table, the one-line command,
    the machine, and the transport ceiling beside it.
  - Guide chapter 14, `crates/vt/docs/guide/14-performance.md`: all five tiers, the fixtures, the
    geometry, the hygiene rules and the honest limits.
  - The four "thirteen chapters" strings that become wrong, and `pub mod ch14_performance {}`.
  - `vt-bench tier3 --check <baseline> [--tolerance]`, and the refusals that stop it producing
    nonsense.
  - `crates/tools/bench-baseline.json`, committed, carrying its machine description.
- [x] Out of scope:
  - **`criterion` and a `benches/` directory.** The measurement exists; a second way to run the
    same fixtures is a second thing to keep in step, and it would add a dependency tree to the
    crate whose pitch is a six-leaf dependency tree. Add it the day somebody needs a distribution
    rather than a median.
  - **A cross-engine comparison** against `alacritty_terminal` or `rio-vt`. A fair harness is a
    project of its own, and an unfair one is worse than none. The chapter says so.
  - **Gating CI on the number.** `IN-0029` rule R-29, and a shared runner measures the runner.
    This intake's Open Decision 4 leaves the door open for a report-only CI step if the owner wants
    one.
  - **Tier 5 (live heap) guarding.** Only tier 3 is guarded, deliberately.
  - Any change to `crates/vt`'s public API. This packet touches none.

## Acceptance

Two criteria below name a `tier3` command and a tier-3 table. **They were written against a shape
`vt-bench` does not have**: the binary's tiers are `parser`/`grid`/`render`/`resize`/`rss`, and
tier 3 (`render`) reports microseconds per frame, not MiB/s. The packet asks for a MiB/s throughput
table, which only tiers 1-2 produce. Tier **2** (`grid`) is therefore what is published and what
`--check` guards: it is a throughput, it is what an embedder pays on every byte, and it is the tier
the design's "primary metric" label was reaching for when it asked for a rate. Recorded here
rather than silently substituted.

- [x] **A real run exists and is attached.**
      `vt-bench grid --mib 32 --machine <...> --json` was executed on a named machine (Intel Core
      i7-12700, Windows 11 26200, rustc 1.96.0, release, 2026-09-16) and **is** the committed
      baseline; the rendered table is in Evidence.
- [x] **The published numbers are that run's numbers.** The README table and chapter 14 are
      rendered from `crates/tools/bench-baseline.json` field for field, so the two cannot disagree:
      the published table and the guarded baseline are one artefact, not two runs.
- [x] **The command in the README reproduces the table.** `vt-bench grid --mib 32` prints the same
      fixtures, units and columns; only the figures differ, as the chapter says they will.
- [x] **The transport ceiling is beside the number.** Both the README section and chapter 14 state
      the ConPTY figures (about 1.2 MiB/s for a `cmd.exe` producer, about 30 MiB/s for a
      DOOM-fire-class one) in the same section as the table.
- [x] **The guard passes on the fresh baseline, and fails when it should.** Both attached: a PASS
      at ratios 0.85-1.03, and a FAIL naming all ten fixtures against a baseline doubled in place.
- [x] **The guard refuses to produce a meaningless verdict.** Debug build and `--features
      vt-paranoid` (a passthrough feature added to `crates/tools` so the engine's flag is visible
      here) both exit 1 with their own message. Attached.
- [x] **A missing or mismatched baseline is a failure, never a skip.** Missing file, a fixture with
      no baseline entry, a baseline entry with no fixture, a baseline with no `tier2_grid`, and
      `--check` on the wrong tier: five distinct non-zero exits, each naming the problem. Attached.
- [x] **The guide is fourteen chapters everywhere.** No `thirteen` remains in `crates/vt`;
      `ch14_performance` renders under `RUSTDOCFLAGS=-D warnings`. The chapter deliberately carries
      no Rust block -- every command in it is shell -- so there is no doctest to compile; the
      `console`/`text` fences keep rustdoc from treating them as Rust.
- [x] **The chapter states its limits.** One machine and operating system, a median not a
      distribution, no cross-engine comparison, what is excluded (renderer, pseudo-console, thread
      hand-off), and performance explicitly outside the semver promise.
- [x] **CI's "the embedder guide must stand alone" grep passes** over chapter 14. The baseline path
      is never written out in the chapter, which is why `--check` defaults to the committed file
      and needs no `crates/...` argument.
- [x] **Nothing about publishing changed.** The diff touches neither `crates/vt/Cargo.toml` nor
      `scripts/verify-dependency-graph.py`; the README only gains a section.
- [ ] **The budget holds.** Chapter 14 is 136 lines (~130) and the README +38 (+40), both inside
      budget. **`crates/tools` is +251, not +100** -- see Gaps.

## Documentation

### Owning Docs Reviewed

- [`low-level-design/performance-evidence.md`](low-level-design/performance-evidence.md) -- this
  packet's owning design: what is measured, the three publication targets, the guard's band and
  where it runs, and why `criterion` is declined.
- [`IN-0029/low-level-design/testing-and-bench.md`](../IN-0029-vt-engine/low-level-design/testing-and-bench.md)
  -- the five tiers, the fixture provenance, the geometry, **rule R-29** (no numeric target in a
  packet's exit criteria) and the reporting rule (every engine number next to the transport
  ceiling). Both rules are obeyed: no acceptance criterion here names a throughput target, and the
  README prints the ceiling.
- `crates/tools/src/bin/vt-bench.rs` -- the existing tiers, flags and JSON output this packet
  extends by one mode.
- `crates/vt/README.md`, `crates/vt/CHANGELOG.md`, `crates/vt/src/guide.rs`, `crates/vt/src/lib.rs`
  -- the four places that say the guide has thirteen chapters.
- `crates/vt/docs/guide/12-versioning.md` -- "Not promised: ... performance", which chapter 14 must
  not contradict.
- [`IN-0038/low-level-design/packaging.md`](../IN-0038-embeddable-vt-core/low-level-design/packaging.md)
  -- the rule that published documentation must stand alone for a reader who does not have this
  repository. Chapter 14 is published documentation.

### Documentation Action

**Update required.** This packet is mostly documentation, so the list is the packet.

| Doc | Change |
| --- | --- |
| `crates/vt/README.md` | a new `## Performance` section after `## Dependencies` |
| `crates/vt/docs/guide/14-performance.md` | new: the whole chapter |
| `crates/vt/src/guide.rs` | `pub mod ch14_performance {}`, and "Thirteen chapters" in its header |
| `crates/vt/src/lib.rs` | the comment that says thirteen empty modules |
| `crates/vt/CHANGELOG.md` | the "thirteen Markdown chapters" line, and an `### Added` entry for the chapter and the guard |
| `crates/tools/bench-baseline.json` | new: committed, with the machine in it |

Reason: the gap is entirely one of publication. Every line of this packet's value is a document
somebody outside the repository reads.

`crates/vt/docs/guide/12-versioning.md` needs **no** change: its "performance is not promised"
clause stays true and chapter 14 cites it rather than weakening it. Recorded so the no-change is a
decision.

### Reconciliation

Before completion: list the docs changed, and confirm the README table, chapter 14 and the attached
run agree figure for figure.

## Context

- Tier 3 is already designated the primary metric by `IN-0029` -- parse plus grid plus one
  `snapshot_update` and `map_colors` per simulated frame, at 160x45. It is the right tier to
  publish because it includes the damage model the evaluation gave 5 out of 5 under criterion (b);
  that is what a GPU-renderer author is buying.
- `vt-bench` already takes `--json` and `--out`, so the baseline file is a redirect of existing
  output, not a new serialiser.
- Rule R-29 exists because a threshold measured on a shared runner measures the runner. A 2x band
  run by hand on the baseline machine is a trip-wire for an accidental clone in the print path or a
  `snapshot_update` that stopped being incremental -- the tenfold class of mistake -- and is not a
  performance target for any packet.
- The baseline-refresh rule is social and no script can enforce it: **a baseline is refreshed only
  in a commit that says why the number moved.** A baseline quietly regenerated to make a guard pass
  is the failure mode the whole packet exists to prevent, and only review catches it. It belongs in
  chapter 14 and in the baseline file's own header comment.

## Plan

- [ ] Add `--check` and `--tolerance` to `vt-bench`, with the debug-build and `vt-paranoid`
      refusals and the missing/mismatched-baseline failures.
- [ ] Run tier 3 on the maintainer's machine; capture the full output.
- [ ] Write `crates/tools/bench-baseline.json` from that run, with the machine described in it.
- [ ] Write guide chapter 14 from the same output, including the limits section.
- [ ] Write the README section from the same output.
- [ ] Add `ch14_performance` to `guide.rs`; fix the four "thirteen" strings.
- [ ] CHANGELOG entry.
- [ ] Prove the guard fails: edit a copy of the baseline to twice the throughput, run, capture.

## Decisions

No new decision record. The two choices a future reader might inherit -- declining `criterion`, and
running the guard by hand rather than in CI -- are recorded with their reasoning in
[`low-level-design/performance-evidence.md`](low-level-design/performance-evidence.md), and the
second is an open question for the owner in the intake. Neither is the kind of cross-cutting choice
a `DEC-` record exists for.

## Verification Plan

- `cargo run -p oneterm-tools --release --bin vt-bench -- tier3` on a named machine; full output
  attached.
- `vt-bench tier3 --check crates/tools/bench-baseline.json` -- passes.
- The same against a halved baseline -- fails, with the message attached.
- `--check` in a debug build and with `--features vt-paranoid` -- both refuse, messages attached.
- `--check` with the baseline file removed, with a fixture added, and with a stale entry -- all
  three fail and name the problem.
- `cargo test --workspace` -- `crates/tools` gains code and keeps its tests green.
- `RUSTDOCFLAGS="-D warnings" cargo doc -p oneterm-vt --no-deps --all-features` -- chapter 14
  renders and its Rust blocks compile.
- CI's "the embedder guide must stand alone" grep.
- `grep -rn thirteen crates/vt`.
- `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

E2E proof is **not applicable**: no user-visible behaviour changes and no engine code is touched.
Record it as not applicable rather than blank. Platform proof is the run itself, which is
platform-specific by nature and says which platform it was.

## Evidence and Gaps

### The published run

`vt-bench grid --mib 32 --machine "Intel Core i7-12700 (12 cores / 20 threads), Windows 11 26200,
rustc 1.96.0, release profile, 2026-09-16" --json --out crates/tools/bench-baseline.json`. That
file **is** the run, and the README and chapter-14 tables are rendered from its fields, so the
published table and the guarded baseline are one artefact:

| Fixture | MiB/s | ns/byte | spread |
| --- | ---: | ---: | ---: |
| `plain_ascii` | 74.9 | 12.73 | 4% |
| `long_lines` | 80.9 | 11.79 | 2% |
| `heavy_sgr` | 226.5 | 4.21 | 5% |
| `tui_redraw` | 125.6 | 7.59 | 3% |
| `scroll_region` | 61.0 | 15.64 | 5% |
| `cjk_wide` | 115.0 | 8.29 | 4% |
| `dense_cells` | 195.6 | 4.88 | 8% |
| `scrolling` | 80.1 | 11.91 | 8% |
| `sixel` | 46.7 | 20.43 | 8% |
| `osc_9_7` | 102.2 | 9.33 | 31% |

Nine of ten fixtures at 2-8% spread; `osc_9_7` had one disturbed cycle. Three sibling worktrees
were compiling throughout this session, so earlier attempts ranged from 30% to 80% spread and were
discarded: the run kept is the quietest of several, which is the hygiene rule ("nothing else
running") being obeyed rather than a favourable number being picked -- the figures move by a few
percent between quiet runs, not by a factor.

### The guard

| Case | Result |
| --- | --- |
| `grid --mib 32 --check` against the committed baseline | exit 0, PASS, ratios 0.85-1.03 |
| ...against a baseline doubled in place | exit 1, FAIL, all ten fixtures named at 0.27-0.35 x |
| `--baseline <missing file>` | exit 1, "no baseline at ...: The system cannot find the file" |
| baseline with an extra `retired_fixture` entry | exit 1, "baseline entry `retired_fixture` has no fixture" |
| baseline with `sixel` removed | exit 1, "fixture `sixel` has no baseline entry" |
| baseline with no `tiers.tier2_grid` | exit 1, names the key and the command that regenerates it |
| `vt-bench render --check` | exit 1, "--check guards tier 2 only" |
| debug build | exit 1, "--check needs a release build (cargo run --release)" |
| `--features vt-paranoid`, release | exit 1, "--check is meaningless under vt-paranoid" |

### Budget

`crates/tools` +251 (Cargo.toml 6, `bench.rs` 62, `vt-bench.rs` 183) plus the 73-line baseline data
file; chapter 14 136 lines; README +38. Two of three inside budget, one over -- see Gaps.

Gaps to state rather than discover:

- **`crates/tools` is +251 against a +100 budget.** The budget assumed `--check` bolting onto an
  existing `tier3` mode that already reported MiB/s. No such mode exists: tier 3 reports
  microseconds per frame, so the guard had to be built against tier 2, and the tier-2 text table
  had to learn to omit the columns of a tier that was not asked for (otherwise a published
  single-tier table is half `-` cells). The guard itself, with its seven refusals, is ~75 lines;
  the interleaving and spread are ~45. Nothing here is speculative, but the estimate was wrong
  rather than the work being padded, and that is worth an owner's eye.
- **The published spread is one run's, on a machine that was not idle all session.** `osc_9_7`'s
  31% is visible in the published table rather than smoothed away, which is the point of the
  column, but a reader should know the table would be tighter on a quiet machine.
- **No Rust doctest in chapter 14.** Every code block in it is a shell command, fenced `console` or
  `text`. The chapter is covered by the rustdoc build and the standalone grep, not by
  `cargo test --doc`, because there is no API in it to compile against.
- **Interleaving changed every tier-1/2 number.** Cycles went from 3 back-to-back per fixture to 5
  interleaved after a discarded warm-up, so figures recorded against the old method in earlier
  packets are not comparable with these. That is a one-time break, taken deliberately because the
  old method biased whichever fixture ran warm.

- **One machine, one operating system.** The published table is a Windows number. An embedder on
  Linux gets a different one and has no way to know how different. Naming the machine is the honest
  mitigation; measuring several is out of scope.
- **Medians, not distributions.** That is the `criterion` trade being declined, and a reader should
  know which side of it the number is on.
- **No cross-engine comparison**, so criterion (i) is answered ("here is a number you can
  reproduce") and criterion (i)'s implied question ("is it fast compared to alacritty?") is not.
  The chapter says so.
- **The guard is not in CI**, so a regression lands and is caught at the next manual run rather
  than in the pull request that caused it. That is the cost of not gating on a noisy runner, and it
  is the intake's Open Decision 4.
- **The baseline can be refreshed to hide a regression** and no automation prevents it. Only the
  review of the commit that changes it does.

## Harness Row

`harness.db` was **not** written by this task: no harness binary is available in this worktree and
the task forbids editing the database. The schema is
`story(id, title, created_at, risk_lane, contract_doc, packet_doc, status, unit_proof,
integration_proof, e2e_proof, platform_proof, evidence, verify_command, last_verified_at,
last_verified_result, notes, intake_id)`, with the four `*_proof` columns as `0`/`1`.

```python
#!/usr/bin/env python3
"""Insert the US-0107 story row. Point DB at the harness database and run once."""
import sqlite3
from datetime import datetime, timezone

DB = "<path to harness.db>"

ROW = dict(
    id="US-0107",
    title="Performance evidence an outsider can read and reproduce",
    created_at="2026-09-15T00:00:00",
    risk_lane="high_risk",
    contract_doc=(
        "docs/spec-intakes/IN-0039-vt-gaps-and-publish/"
        "low-level-design/performance-evidence.md"
    ),
    packet_doc=(
        "docs/spec-intakes/IN-0039-vt-gaps-and-publish/"
        "US-0107-performance-evidence.md"
    ),
    status="implemented",
    unit_proof=0,
    integration_proof=1,
    e2e_proof=0,
    platform_proof=1,
    evidence=(
        "One tier-2 run (Intel Core i7-12700, Windows 11 26200, rustc 1.96.0, "
        "release, 2026-09-16) committed as crates/tools/bench-baseline.json and "
        "rendered into crates/vt/README.md and guide chapter 14, so the published "
        "table and the guarded baseline are one artefact. vt-bench grid --check "
        "proved in nine states: PASS on the baseline, FAIL on a doubled baseline, "
        "and seven refusals (missing file, stale entry, missing entry, wrong tier "
        "key, wrong tier command, debug build, vt-paranoid). Guide is fourteen "
        "chapters; no 'thirteen' remains in crates/vt."
    ),
    verify_command="pwsh scripts/ci-local.ps1 -Full",
    last_verified_at=datetime.now(timezone.utc).isoformat(timespec="seconds"),
    last_verified_result="pass",
    notes=(
        "Published tier 2 (grid), not tier 3: tier 3 reports us/frame, not MiB/s, "
        "so it cannot carry the throughput table the packet asks for. unit_proof=0 "
        "because the change is a diagnostic binary and documentation with no new "
        "#[test]; integration proof is the nine guard states. crates/tools came to "
        "+251 against a +100 budget -- the estimate assumed a tier-3 MiB/s mode that "
        "does not exist. Interleaved cycles replace 3 back-to-back runs, so tier-1/2 "
        "figures recorded in earlier packets are not comparable with these."
    ),
    intake_id=44,
)

with sqlite3.connect(DB) as db:
    db.execute(
        "INSERT INTO story ({}) VALUES ({})".format(
            ", ".join(ROW), ", ".join("?" * len(ROW))
        ),
        tuple(ROW.values()),
    )
```

## Handoff

Independent of `US-0105` and `US-0106`; all three sit behind `BUG-0059` only. This one touches
`crates/tools` and documentation and does not regenerate the public-surface files, so it never
conflicts with the other two.
