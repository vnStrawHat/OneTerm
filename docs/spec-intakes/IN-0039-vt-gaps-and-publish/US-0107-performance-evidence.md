# Work: performance evidence an outsider can read and reproduce

ID: US-0107
Intake: [`IN-0039`](IN-0039.md)
Created: 2026-09-15

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [ ] Implemented
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

- [ ] **A real run exists and is attached.**
      `cargo run -p oneterm-tools --release --bin vt-bench -- tier3` was executed on a named
      machine (CPU, operating system, toolchain version, date), and its complete output is in
      Evidence.
- [ ] **The published numbers are that run's numbers.** The README table and guide chapter 14 carry
      the same figures as the attached output. A published number that does not match the attached
      run makes this packet worse than doing nothing, and this criterion is the one a hostile
      verifier should check first: read both, compare, fail on any difference.
- [ ] **The command in the README reproduces the table.** A verifier runs the printed command on
      their own machine and gets the same shape of output -- same fixtures, same units, same
      columns. Different numbers are expected; a different structure is a failure.
- [ ] **The transport ceiling is beside the number.** The README states the ConPTY figures
      `pty-throughput` reports (about 1.2 MiB/s for a `cmd.exe` producer, about 30 MiB/s for a
      DOOM-fire-class one) in the same section, so no engine number is read in isolation. This is
      `IN-0029`'s existing reporting rule, applied to a public document for the first time.
- [ ] **The guard passes on the fresh baseline, and fails when it should.** Attach both: `--check`
      exiting zero against the committed baseline, and `--check` exiting non-zero against a
      baseline hand-edited to twice the throughput. A guard never seen to fail has not been tested.
- [ ] **The guard refuses to produce a meaningless verdict.** `--check` exits non-zero with a clear
      message in a debug build and under the `vt-paranoid` feature. Both messages attached.
- [ ] **A missing or mismatched baseline is a failure, never a skip.** No baseline file, a fixture
      with no baseline entry, and a baseline entry with no fixture each exit non-zero and name the
      problem. Attached.
- [ ] **The guide is fourteen chapters everywhere.** `grep -rn thirteen crates/vt` returns nothing
      about the guide, `ch14_performance` renders, and its Rust blocks compile as doctests like
      every other chapter's.
- [ ] **The chapter states its limits.** One machine, one operating system, medians not
      distributions, no cross-engine comparison, and performance explicitly **not** covered by the
      semver promise ("Not promised: ... allocation behaviour, and performance"). A performance
      chapter that omits its limits undoes the credibility chapter 11 earns by listing gaps.
- [ ] **CI's "the embedder guide must stand alone" grep passes** over chapter 14: no `US-`,
      `BUG-`, `DEC-`, `IN-`, `crates/` or `docs/` reference except an
      `https://github.com/vnStrawHat/OneTerm/...` link.
- [ ] **Nothing about publishing changed.** `git diff` touches neither `crates/vt/Cargo.toml`'s
      `publish` line nor `scripts/verify-dependency-graph.py`. The README gains a section and loses
      none of its git-dependency instructions.
- [ ] **The budget holds**: `crates/tools` +100, chapter 14 about 130 Markdown lines, README +40.
      `git diff --stat` attached.

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
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

E2E proof is **not applicable**: no user-visible behaviour changes and no engine code is touched.
Record it as not applicable rather than blank. Platform proof is the run itself, which is
platform-specific by nature and says which platform it was.

## Evidence and Gaps

After implementation, record: the full tier-3 run with the machine named; the baseline file; the
passing and the deliberately failing guard runs; the three refusal messages; the README and chapter
14 figures next to the run's figures; `git diff --stat` against budget.

Gaps to state rather than discover:

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

## Handoff

Independent of `US-0105` and `US-0106`; all three sit behind `BUG-0059` only. This one touches
`crates/tools` and documentation and does not regenerate the public-surface files, so it never
conflicts with the other two.
