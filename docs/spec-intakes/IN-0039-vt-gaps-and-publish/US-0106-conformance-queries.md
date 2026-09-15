# Work: DECRQCRA, DECRQSS and XTGETTCAP are answered, and esctest runs

ID: US-0106
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

- Change type: **new capability** (three answers the engine has never given, and a readback gate)
- Risk lane: high_risk -- both triggers: a public contract (new reply bytes are a contract under
  the crate's own clause 6) and a new capability at a trust boundary (screen readback)
- Spec Intake, when required: [`IN-0039`](IN-0039.md)

## Outcome

Three queries the engine parses, counts and never answers are answered, and an outside conformance
harness can finally score this engine.

The evaluation's gap 5:

> `DECRQCRA` is missing, so there is no third-party conformance score. ... `esctest` reads the
> screen back with rectangle checksums, so without `CSI * y` the harness cannot run, and no
> external number exists for this engine. `DECRQSS` and `XTGETTCAP` are absent for the same family
> of reasons and break tmux and neovim capability probing. Implementing `DECRQCRA` alone converts
> "we test ourselves thoroughly" into a number an outsider can compare against alacritty.

After this packet an `esctest` matrix exists, published as a CI artifact and pasted into Evidence,
and guide chapter 11's "Known gaps" table is rewritten from what the run actually found rather than
from what was never attempted.

## Scope

- [x] In scope:
  - A bounded DCS payload buffer, reusing the existing `DCS_MAX_BYTES` ceiling and abort path.
  - `DECRQCRA` (`CSI * y`) with the checksum variant pinned in the design, behind
    `Config::allow_screen_readback` (default **false**).
  - `DECRQSS` (`DCS $ q`) for `m` (SGR), `r` (`DECSTBM`), `SP q` (`DECSCUSR`), `" q` (`DECSCA`) and
    `" p` (`DECSCL`); everything else gets the invalid reply.
  - `XTGETTCAP` (`DCS + q`) over a fixed, compiled-in capability table, bounded and counted.
  - `#[non_exhaustive]` on `Config`, which the accepted IN-0038 design says should already be there.
  - `crates/tools/src/bin/vt-esctest.rs`, `#[cfg(unix)]`.
  - A Linux CI job that runs `esctest`, **report-only, never gating**.
  - Guide chapter 11 rewritten: its "Known gaps" table loses three rows and gains whatever the run
    finds.
- [x] Out of scope:
  - **`CSI Ps * x` (`XTERM_CHECKSUM`)**, xterm's runtime checksum-variant selector. One variant is
    implemented and pinned; xterm has a selector because it had its own history to reconcile.
  - **`DECRQSS` for `DECSLRM`, `DECSASD`, `DECSACE`, `DECSCPP`, `DECSNLS`.** The engine does not
    have those features, and answering would be the "claim a capability that does not exist"
    failure this whole intake is about. They take the invalid reply, which is the honest one.
  - **Left-right margins**, and therefore the `esctest` groups that need them. They fail correctly.
  - **Reading a terminfo database.** `XTGETTCAP` answers from a compiled-in table and touches no
    file, environment variable or network.
  - **Making `esctest` a gate.** `IN-0029`'s existing rule: conformance is a report.
  - **Running `esctest` on Windows.** The harness is `#[cfg(unix)]`.

## Acceptance

- [ ] **Each query answers the exact bytes, proven by feeding the exact request.** One test per
      arm, named after the sequence, asserting the reply byte for byte. The `DECRQCRA` set includes
      at minimum: a one-cell rectangle over a known character; a blank cell (answers `0020`); a
      rectangle clamped from outside the grid; a reversed rectangle (`Pb < Pt`, empty, no
      underflow); origin mode set (rows relative to the scrolling region); and the gate closed.
- [ ] **The gate is closed by default and is the status quo.** With `Config::default()`,
      `CSI 1;0;1;1;1;1*y` produces **no** `VtEvent::Reply` and increments
      `FeedStats::unhandled_sequences` -- byte-identical to the engine's behaviour on `main`.
      Asserted, not assumed.
- [ ] **The checksum variant is pinned where a reader sees it.** A doctest in guide chapter 11
      feeds a known rectangle and asserts the four hex digits, so the number cannot change without
      a documentation diff. The chapter states the variant in words beside it: positive sum of
      Unicode scalar values, attributes excluded, unwritten cells counted as `U+0020`, no trimming,
      masked to 16 bits -- and states that a program written against xterm's default (negated, with
      attributes) will disagree.
- [ ] **`DECRQSS` for SGR round-trips as a property, not a golden string.** Feed an SGR sequence,
      request it back, feed the answer into a second fresh terminal, assert the two cell templates
      are equal. Run over a set of SGR states including 256-colour, truecolour, colon
      sub-parameters and the underline styles.
- [ ] **`XTGETTCAP` ceilings hold at the boundary.** 16 names answered; a 17th dropped and counted;
      a 128-byte name dropped; odd-length hex answered unknown; non-hex answered unknown; none of
      them panics and none allocates unboundedly.
- [ ] **The DCS payload buffer is bounded and reused.** A payload past `DCS_MAX_BYTES` aborts,
      increments `FeedStats::aborted_dcs`, and answers nothing. A loop of 10 000 `XTGETTCAP`
      requests allocates the buffer once -- asserted by a counting-allocator test or, if that is
      impractical in the unit harness, by a stated code-reading argument in Evidence rather than
      silently dropped.
- [ ] **`esctest` runs, and its matrix is attached.** Pass/fail per test group, from the Linux CI
      job, with the exact command line including `--expected-terminal xterm --xterm-checksum 334`.
      A "before" count is attached too and is expected to be near zero by construction -- every
      rectangle assertion times out on `main` -- and the packet says so rather than presenting it
      as a measurement.
- [ ] **The CI job cannot fail the build.** Read the job definition and state that it is
      report-only; and demonstrate it by attaching a run in which `esctest` reports failures and
      the job is green.
- [ ] **Guide chapter 11's "Known gaps" table is rewritten from the run.** The three rows for these
      sequences are gone; whatever `esctest` found that the engine does not do is listed by name.
      A gaps table that shrinks without gaining the newly discovered gaps is a failed criterion.
- [ ] **The corpus does not move.** 46 recordings replay byte-identically; none contains any of
      these three sequences, so any difference is a bug in this packet.
- [ ] **`cargo test -p oneterm-vt --features vt-paranoid` passes** -- `DECRQCRA` reads the grid, and
      a reader running off the end is exactly what the whole-history walk catches.
- [ ] **The surface diff is one `Config` field plus the `#[non_exhaustive]` mark.**
- [ ] **Nothing about publishing changed.** `git diff` touches neither `crates/vt/Cargo.toml`'s
      `publish` line nor `scripts/verify-dependency-graph.py`'s publish assertion. Stated because a
      packet that adds a CI job is the kind of packet where such a line drifts in.
- [ ] **The budget holds**: `crates/vt` +470 production, `crates/tools` +140, CI +40 YAML,
      tests +380. `git diff --stat` attached.

## Documentation

### Owning Docs Reviewed

- [`low-level-design/conformance-queries.md`](low-level-design/conformance-queries.md) -- this
  packet's owning design: the payload buffer, all three sequences, the checksum variant and its
  four-reason justification, the capability table, the ceilings and the `esctest` run plan.
- [`IN-0038/evidence/US-0102-esctest.md`](../IN-0038-embeddable-vt-core/evidence/US-0102-esctest.md)
  -- why the previous attempt stopped, and the three-step list this packet executes. **It is the
  document this packet closes**; it should gain a pointer here on merge.
- [`IN-0029/low-level-design/dispatch-and-modes.md`](../IN-0029-vt-engine/low-level-design/dispatch-and-modes.md)
  -- the CSI and DCS dispatch rules and the `DECRQM` honesty rule this packet must not break.
- [`IN-0029/low-level-design/testing-and-bench.md`](../IN-0029-vt-engine/low-level-design/testing-and-bench.md)
  section 6 -- "Conformance as a report, not a gate", which the CI job implements verbatim.
- [`IN-0038/low-level-design/api-surface.md`](../IN-0038-embeddable-vt-core/low-level-design/api-surface.md)
  -- the semver promise (clause 6, reply bytes) and the `#[non_exhaustive]` doctrine, including the
  `Config` mark that was designed and never applied.
- `crates/vt/docs/guide/11-conformance.md` -- lists all three as known gaps and states that there
  is no `esctest` score. Both statements become false.
- `crates/vt/src/terminal/dispatch.rs` -- `dcs_hook`'s header comment names `DECRQSS` and
  `XTGETTCAP` as counted-unhandled; it describes the behaviour this packet replaces.
- `docs/osc-sequences-checklist.md` -- reviewed and **not changed**: it is about OSC, and nothing
  here is OSC. Recorded so the no-change is a decision.

### Documentation Action

**Update required.**

| Doc | Change |
| --- | --- |
| `crates/vt/docs/guide/11-conformance.md` | the "Supported" DCS paragraph, the "Known gaps" table (three rows out, the run's findings in), the "How conformance is checked" section (an `esctest` matrix now exists), and the new checksum doctest with its variant paragraph |
| `crates/vt/docs/guide/12-versioning.md` | clause 6's list gains `DECRQSS`, `DECRQCRA` and `XTGETTCAP`; the `#[non_exhaustive]` count gains `Config` |
| `crates/vt/CHANGELOG.md` | `### Added` for the three answers and the `Config` field; a note that the reply bytes are a contract from here on |
| `crates/vt/src/terminal/dispatch.rs` | `dcs_hook`'s header comment, which currently describes the old behaviour |
| [`IN-0038/evidence/US-0102-esctest.md`](../IN-0038-embeddable-vt-core/evidence/US-0102-esctest.md) | a closing pointer to this packet. The evidence record is not rewritten -- it was accurate when written |
| `.github/workflows/ci.yml` | the new report-only job, with a comment saying why it never gates |

Reason: guide chapter 11 currently tells a reader, correctly, that this engine has no `esctest`
score and cannot get one. After this packet that is false in both halves, and chapter 11 is the
document an evaluator reads to decide whether to trust the crate.

### Reconciliation

Before completion: list the docs changed, and confirm by grep that no file under `crates/vt/`
still says `DECRQCRA`, `DECRQSS` or `XTGETTCAP` is unimplemented.

## Context

- `BUG-0058` already made `dcs_hook` route on (intermediates, final byte), so `DCS $ q` and
  `DCS + q` no longer open the Sixel decoder. The parsing half is done; only the answering half is
  missing.
- `dcs_put` today feeds the Sixel parser or discards the byte. It needs a second sink, not a second
  state machine.
- The checksum variant is the one xterm reaches with `checksumExtension: 7`. It is chosen because
  it is the only variant `esctest` scores without a per-cell correction: `escutil.py` compares a
  one-cell checksum against `ord(char)` and applies the negation inverse only under
  `--xterm-checksum < 279`, and `esc.py`'s `empty()` returns a space only under
  `--xterm-checksum >= 334`. Both conditions are met by this variant and by no other. The design
  carries the xterm source excerpt the bit meanings come from.
- `Config` is **not** `#[non_exhaustive]` today, although
  [`IN-0038/low-level-design/api-surface.md`](../IN-0038-embeddable-vt-core/low-level-design/api-surface.md)
  says it should be. This packet adds a `Config` field and therefore discovers it. Every
  construction site in this repository already uses `Config { ..Config::default() }`, so the mark
  is compatible.
- xterm gates `DECRQCRA` behind `allowWindowOps` and requires `vtXX_level >= 4`; WezTerm gates it
  behind `enable_checksum_rectangular_area`, default off. This engine's `DA1` claims VT220, not
  VT420 -- worth knowing, and not worth changing: `DA1`'s reply is a contract and no client is
  known to gate `DECRQCRA` on it.

## Plan

- [ ] Add the DCS payload sink to the terminal state; extend `dcs_hook`, `dcs_put`, `dcs_unhook`;
      clear it at `RIS` and `DECSTR`.
- [ ] `Config::allow_screen_readback` plus the `#[non_exhaustive]` mark.
- [ ] `DECRQCRA`: parameter parsing with defaults, rectangle clamping, origin-mode handling, the
      checksum walk, the `DCS Pid ! ~ xxxx ST` reply.
- [ ] `DECRQSS`: the six arms and the default; the SGR serialiser is the bulk.
- [ ] `XTGETTCAP`: hex decode, the const table, hex encode, the per-name reply, the two ceilings.
- [ ] `crates/tools/src/bin/vt-esctest.rs` -- it cannot be compiled on the maintainer's host, so it
      is written against the CI job and the job is part of the same commit.
- [ ] The CI job, report-only, uploading the log as an artifact.
- [ ] Run it; read the matrix; rewrite guide chapter 11 from what it says.
- [ ] Chapter 12, CHANGELOG, `dcs_hook`'s comment, the IN-0038 evidence pointer.
- [ ] Regenerate both surface files.

## Decisions

No new decision record, and that is a judgement worth stating. The checksum variant is the kind of
choice future work must inherit -- changing it later breaks every harness that stored a number --
but it is a single implementation detail of one sequence in one crate, fully recorded with its
justification in
[`low-level-design/conformance-queries.md`](low-level-design/conformance-queries.md), which is an
accepted design document of a high-risk intake. A `DEC-` record would duplicate it.

If the owner rules that `allow_screen_readback` should default **true** (this intake's Open
Decision 2), **that** is a decision record: it would be a deliberate choice to give every program
in every OneTerm terminal a screen-readback primitive, and the reasoning should outlive this
packet.

## Verification Plan

- `cargo test -p oneterm-vt` -- one test per arm with exact bytes; the `DECRQCRA` edge set; the SGR
  round-trip property; the `XTGETTCAP` boundary set; the DCS abort path.
- `cargo test -p oneterm-vt --features vt-paranoid` -- the whole-history walk, because `DECRQCRA`
  reads the grid.
- `cargo test -p oneterm-vt --no-default-features` -- none of this is behind `pty`, so the
  transport-free build must have it all.
- The 46-recording corpus replay, byte-identical.
- The existing `cargo-fuzz` parser target, which gains the new DCS state for free. No new target.
- `python scripts/vt-public-api.py --check`, `--check-nameable`, `--diff-platforms`.
- `RUSTDOCFLAGS="-D warnings" cargo doc -p oneterm-vt --no-deps --all-features` -- chapter 11's new
  doctest compiles and runs.
- CI's "the embedder guide must stand alone" grep over the rewritten chapter 11.
- The Linux CI `esctest` job: matrix attached, job green even when tests fail.
- `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

E2E proof is **not applicable**: nothing a OneTerm user can see changes, because the adapter does
not set the gate and OneTerm sends none of these queries. Record it as not applicable rather than
leaving it blank. Platform proof is the Linux `esctest` job plus the `--no-default-features` build.

## Evidence and Gaps

After implementation, record: every exact-bytes test result; the SGR round-trip set; the
`XTGETTCAP` boundary results; the `esctest` matrix before and after with its full command line and
artifact path; the green-job-with-failures demonstration; `git diff --stat` against budget; the
corpus replay; the surface diff.

Gaps to state rather than discover:

- **The checksum variant disagrees with xterm's default.** Deliberate, justified four ways in the
  design, and documented in the guide -- but it means a hypothetical program written against
  xterm's negated-with-attributes form gets a different number here. No such program is known;
  `DECRQCRA` is a harness primitive.
- **`vt-esctest.rs` is `#[cfg(unix)]` and invisible to every check the maintainer runs.** It
  compiles in CI and nowhere else, so a change to it is only ever proven there. That was the exact
  argument for *not* writing it in `US-0102`; it is written now because step 1 has landed and the
  CI job exercises it in the same commit.
- **`esctest` is GPL-2.0.** It is executed, never linked or vendored, which is the same
  relationship CI has with every other tool it runs. Confirm the job fetches it rather than
  committing it, and say where from.
- **The `esctest` groups that fail for correct reasons** (no left-right margins, VT220-level `DA1`)
  must be listed as such, with the reason, or the matrix reads as a quality score rather than a
  capability map.
- **The DCS buffer's single-allocation claim** may be argued from code rather than measured; say
  which.

## Handoff

Independent of `US-0105` and `US-0107`; all three sit behind `BUG-0059` only. If `US-0105` lands
first, both regenerate the surface files and the second rebases its snapshot -- mechanical, not
semantic. Blocked on the owner for Open Decision 2 (the readback default) **at acceptance**, not at
start: the implementation is identical either way and only the default constant moves.
