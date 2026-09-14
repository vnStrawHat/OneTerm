# Work: The corpus lives with its reader and the byte budget has one home

ID: US-0093
Intake: IN-0032
Created: 2026-09-14

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

- Change type: maintenance
- Risk lane: normal
- Spec Intake, when required: `IN-0032`

## Outcome

Two boundaries that are wrong today are right, and neither costs a crate move.

**B3.** The VT parity corpus — 3.2 MB, 45 alacritty reference recordings plus one OneTerm
recording — lives in `crates/tools/`, the only crate that reads it, instead of in
`crates/vt/tests/` where nothing reads it. `corpus_root()` resolves from its own
`CARGO_MANIFEST_DIR` instead of reaching into a sibling crate's `tests/` directory through
`.parent().join("vt/tests/corpus")`, an undeclared filesystem assumption between two crates.
`cargo test -p oneterm-vt` stops carrying 3.2 MB it never opens.

**B5.** The one genuinely duplicated piece of backend code — about 35 lines of byte-budget
reservation, written twice in the same idiom against two identically valued 4 MiB constants — is
one `ByteBudget` type in `crates/terminal/src/backend/`, which both backends already depend on.
Each backend keeps its own constant, because the value is each backend's policy; only the
mechanism is shared.

**No behaviour changes.** The corpus bytes are identical after the move and the parity gate
compares the same recordings; the budget enforces the same limit at the same points.

## Scope

### In scope

**B3 — the corpus move:**

- [ ] Move `crates/vt/tests/corpus/` → `crates/tools/corpus/`, including `alacritty-ref/`,
  `oneterm/` and `NOTICE`. Use `git mv` so the move is recorded as a rename and the bytes are
  provably unchanged.
- [ ] `crates/tools/src/corpus.rs:87-92` — `corpus_root()` resolves from `CARGO_MANIFEST_DIR`
  directly; delete the `.parent().expect("crates/tools has a parent").join("vt/tests/corpus")`
  hack and the stale doc comment above it (*"The corpus lives at its final home under
  `crates/vt/` even though that crate does not exist yet"* — `crates/vt` has existed since
  `US-0073`).
- [ ] Every other path reference, all of which are real and none of which the compiler catches:
  - `.gitattributes:11` — `crates/vt/tests/corpus/** -text -whitespace`. This one matters: if it
    is not moved with the data, Git may normalise line endings in binary-ish recordings and the
    parity gate fails for a reason that looks nothing like its cause.
  - `scripts/third-party-notices.py:81` — the attribution row naming
    `crates/vt/tests/corpus/alacritty-ref/` and `crates/vt/tests/corpus/NOTICE`. This script is a
    CI gate (`--check` against `THIRD-PARTY-NOTICES.md`), so the generated file changes with it.
  - `crates/tools/src/bin/vt-corpus.rs:87` — the `--dir` default in the help text.
  - `crates/vt/fuzz/Cargo.toml:17` — the `cp ../tests/corpus/...` comment.
  - `crates/tools/src/corpus.rs:723`, `crates/tools/src/lib.rs:9` and
    `crates/vt/src/terminal/terminal_tests.rs:6` — prose naming the corpus or the drift gate.
    Check each; update only what is now wrong.

**B5 — the shared byte budget:**

- [ ] Add one `ByteBudget` (an `AtomicUsize` with reserve and release) to
  `crates/terminal/src/backend/`, alongside `pump.rs`, `osc_router.rs`, `state.rs`,
  `event_sink.rs` and `transport.rs`.
- [ ] `crates/local-shell/src/event_loop.rs:120-140` and `crates/ssh/src/transport.rs:121-134`
  use it. Both currently write the same
  `fetch_update(|n| n.checked_add(len).filter(|&next| next <= BUDGET))` reservation plus its
  release. `LOCAL_COMMAND_BYTE_BUDGET` (`event_loop.rs:65`) and `SSH_COMMAND_BYTE_BUDGET`
  (`transport.rs:25`) both stay where they are, each 4 MiB: the constants are per-backend policy
  and the tests name them (`local-shell/src/event_loop_tests.rs:53, :63`,
  `ssh/src/transport_tests.rs:98-110`).

### Out of scope

- [ ] Unifying the two constants into one value, or moving them into `crates/terminal`. They are
  identical by coincidence of policy, not by contract, and either backend may need to change its
  own without touching the other.
- [ ] Anything else in the two read loops. They are structurally different for good reasons: the
  local loop holds the engine guard across reads and caps each read at `MAX_LOCKED_READ`; the SSH
  loop cannot hold a guard across reads at all. This packet touches the budget arithmetic and
  nothing around it.
- [ ] Re-recording, re-blessing, adding to or pruning the corpus. `US-0072`'s rule is "nothing
  blesses"; the data is frozen and must be byte-identical after the move.
- [ ] Splitting `doom-fire.rs` out of `crates/tools` for cleaner licence metadata (audit §1.4).
  Real, optional, and the audit says it would not spend a packet on it. Note that this packet
  moves an Apache-2.0-attributed corpus **into** the crate carrying the `GPL-3.0-only` term, so the
  `deny.toml` exception and the licence analysis must be re-read — see Documentation.
- [ ] `crates/vt`'s `pub` surface and every other item in `US-0090`.
- [ ] The session macro — `US-0091`, which must land first.

## Acceptance

**B3:**

- [ ] **The data is byte-identical.** `git log --follow --stat` (or `git show --stat` on the move
  commit) shows pure renames with no content change for all 46 recordings and the `NOTICE`.
  Additionally record `find crates/tools/corpus -type f | wc -l` and a directory checksum before
  and after.
- [ ] **`crates/vt` no longer carries the corpus**: `crates/vt/tests/corpus` does not exist, and
  `grep -rn "vt/tests/corpus" .` returns hits only in `docs/spec-intakes/` and
  `docs/decisions/` history.
- [ ] **No sibling-path resolution anywhere**: `grep -rn "\.parent()" crates/tools/src/corpus.rs`
  returns nothing.
- [ ] **The parity gate passes with the same result**: `cargo test -p oneterm-tools`, including
  `tests/corpus_check.rs`, the drift gate that runs inside `cargo test --workspace`. Record the
  count before and after; it must be identical, not merely passing — a corpus that silently
  stopped being found would also "pass" if the gate skips a missing directory, so confirm the
  per-recording count.
- [ ] **`python scripts/third-party-notices.py --check` passes** against a regenerated
  `THIRD-PARTY-NOTICES.md` whose corpus row names the new paths.
- [ ] **`.gitattributes` covers the new location** and no recording's bytes changed in the working
  tree after the move (`git status` clean, `git diff --stat` empty on a fresh checkout).

**B5:**

- [ ] **One implementation.** `grep -rn "fetch_update" crates/local-shell/src crates/ssh/src`
  returns nothing; the idiom exists once, in `crates/terminal/src/backend/`.
- [ ] **Net delta at least −20 lines** across the two backends (the audit's figure for the
  duplication is about 35 lines; the shared type costs some of it back). Record the measured
  number.
- [ ] **Both backends' budget tests pass untouched**: `local-shell/src/event_loop_tests.rs:53,
  :63` and `ssh/src/transport_tests.rs:98-110`, which exercise reserving exactly the budget,
  exceeding it, and releasing. No assertion may be edited. If one must be, the semantics moved
  and the packet is wrong.
- [ ] **Overflow behaviour is preserved and pinned.** The current idiom uses `checked_add`, so a
  reservation that would overflow `usize` fails rather than wrapping. The shared type must keep
  that, and a test must prove it — this is the one place where "tidying" a duplicated idiom could
  quietly remove a guard.
- [ ] **R10 holds**: the shared type lives in the lowest crate both backends already depend on
  (`crates/terminal`), not duplicated and not pushed up. `python scripts/verify-dependency-graph.py`
  passes and no manifest gains a dependency.

**Both:**

- [ ] **No test lost.** Baselines recorded for `cargo test -p oneterm-tools`, `-p oneterm-vt`,
  `-p oneterm-terminal`, `-p oneterm-local-shell`, `-p oneterm-ssh`; each after-count greater than
  or equal to baseline.
- [ ] **`pwsh scripts/ci-local.ps1` exits 0**, with totals recorded. It runs both gates that matter
  here — the workspace tests (which include `corpus_check.rs`) and
  `python scripts/third-party-notices.py --check`.

## Documentation

### Owning Docs Reviewed

- `IN-0029/low-level-design/testing-and-bench.md` — owns the parity corpus contract: what the
  recordings are, how the drift gate compares them, `US-0072`'s blessing rules (`R-58`, "nothing
  blesses"), and the five bench tiers. It is the document that states where the corpus lives.
  **Must change** if it names the path.
- `docs/terminal-backend.md` — owns `crates/terminal/src/backend/`: `pump.rs`, `osc_router.rs`,
  `state.rs`, `event_sink.rs`, `transport.rs`, and the two read loops including the command byte
  budget. **Must change**: it gains a module, and the budget stops being described twice.
- `docs/agents/crate-dependency-rules.md` — R10 ("new shared types go in the lowest crate that
  needs them") for B5, and the Layers note that `crates/tools` may only reach down to L0 leaves.
  B3 *reduces* `tools`' reach into another crate's directory tree; no edge changes either way.
- `docs/agents/structure.md` — the directory tree, which will show `crates/tools/corpus/` and no
  longer `crates/vt/tests/corpus/`. Note that `US-0090` also edits this file's `vt/` subtree
  listing; rebase carefully.
- `THIRD-PARTY-NOTICES.md` and `scripts/third-party-notices.py` — the Apache-2.0 attribution for
  the 45 alacritty reference captures, their upstream commit, and the `NOTICE` path. The script is
  the generator and the CI check. **Must change.**
- `deny.toml` and `docs/license-analysis.md` — `crates/tools` carries
  `license = "Apache-2.0 AND GPL-3.0-only"` because of `doom-fire.rs`, with a crate-scoped
  `deny.toml` exception. This packet moves Apache-2.0-attributed data into that crate. Nothing
  ships from `crates/tools`, so nothing is at risk, but the packet must read both and record that
  the existing exception still describes the truth — or fix it.
- `crates/vt/fuzz/Cargo.toml` — the fuzz corpus seeding comment points at the old path.

### Documentation Action

Update required:

- `IN-0029/low-level-design/testing-and-bench.md` — the corpus location, if named.
- `docs/terminal-backend.md` — `crates/terminal/src/backend/`'s file list gains the budget module,
  and the byte-budget description becomes one shared mechanism with two per-backend constants.
- `docs/agents/structure.md` — the directory tree.
- `THIRD-PARTY-NOTICES.md` (regenerated) and `scripts/third-party-notices.py` — the corpus
  attribution paths.
- `crates/tools/src/corpus.rs` — `corpus_root()`'s doc comment, which currently says the corpus
  lives under `crates/vt/` "even though that crate does not exist yet". Both halves are now false.
- `crates/vt/fuzz/Cargo.toml` and `crates/tools/src/bin/vt-corpus.rs` — path comments and help
  text.

Reason: B3 is a file move whose correctness lives almost entirely **outside** the compiler — a
`.gitattributes` rule, a licence-attribution generator that CI diffs, a help string, and two
prose references. Every one of those is documentation-shaped, so "no contract change" is not
available. B5 changes the backend's documented file layout.

### Reconciliation

Before completion, list the docs changed, and record explicitly that `docs/PROJECT.md` needed no
edit (no persisted file, no boundary and no verification command changes) and that
`docs/agents/crate-dependency-rules.md` needed none (no crate edge moves).

## Context

**B3, the evidence.** `grep -rn corpus crates/vt/src crates/vt/tests/*.rs` returns only prose:
nothing in `crates/vt` reads the 3.2 MB sitting in its own `tests/` directory. Its only reader is
`crates/tools/src/corpus.rs:88-92`:

```rust
pub fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/tools has a parent")
        .join("vt/tests/corpus")
}
```

That is an undeclared filesystem-sibling assumption between two crates: it survives only because
both happen to sit directly under `crates/`, and nothing in the build system enforces it.

The audit flags one judgement call (§(d).4): the `oneterm/` subdirectory (one Sixel recording) has
the same single reader as `alacritty-ref/`, but `US-0072`'s blessing rules may make the owner
prefer it stay adjacent to the engine for provenance. **Default: move it with the rest** and carry
the provenance in the moved `NOTICE`. `IN-0032` § Open Decisions records this as this packet's
implementer's call; record which way it went and why.

**B5, the evidence.** The audit's §1.2 table compared the two backends concern by concern and
found exactly one duplicate:

| Concern | `local-shell` | `ssh` | Duplicated? |
| --- | --- | --- | --- |
| Read loop | `event_loop.rs:399-545`, a `polling::Poller` loop holding the engine guard across reads | `task.rs:55-138`, a `tokio::select!` locking per chunk | **No** — structurally different |
| Pump usage | `advance` + manual guard + `finish_batch_blocking` | `process_chunk` + `finish_batch().await` | Shared already |
| Transport | `transport.rs`, 74 lines | `transport.rs`, 222 lines | Partly |
| Byte budget | `event_loop.rs:120-140` | `transport.rs:121-129` | **Yes, about 35 lines** |
| `TerminalSession` | one macro call + `capabilities()` | one macro call + `capabilities()` | `US-0091` |

*"Total duplicated code between the two backends: about 35 lines, plus the macro-call shape. That
is the entire prize"* — and this packet plus `US-0091` collects all of it without moving a crate.

**Why after `US-0091`.** B5 adds a type to `crates/terminal/src/backend/` and edits both backends'
transports; `US-0091` rewrites both backends' session types against that same layer. Sequencing
them avoids a conflict in one directory and keeps each packet's test evidence attributable.

## Plan

- [ ] Record branch-point baselines: the five `cargo test -p …` counts, `corpus_check.rs`'s
  per-recording count, and a checksum of `crates/vt/tests/corpus`.
- [ ] `git mv crates/vt/tests/corpus crates/tools/corpus`. Commit the move **alone**, so the
  rename is reviewable and provably content-free.
- [ ] Fix `corpus_root()` and its doc comment; run `cargo test -p oneterm-tools` and compare the
  per-recording count against the baseline.
- [ ] Move the `.gitattributes` rule; verify on a fresh clone or `git stash`-clean tree that no
  recording's bytes differ.
- [ ] Update `scripts/third-party-notices.py`, regenerate `THIRD-PARTY-NOTICES.md`, run
  `--check`.
- [ ] Fix the remaining path references (`vt-corpus.rs` help, `crates/vt/fuzz/Cargo.toml`, the
  three prose sites).
- [ ] Read `deny.toml` and `docs/license-analysis.md`; record that the crate-scoped exception
  still describes the truth, or fix it.
- [ ] B5: add `ByteBudget` to `crates/terminal/src/backend/`, including the overflow test; move
  both backends onto it; run each backend's tests against baseline.
- [ ] Update `docs/terminal-backend.md`, `docs/agents/structure.md` and
  `testing-and-bench.md`.
- [ ] `cargo test --workspace`; `pwsh scripts/ci-local.ps1`.

## Decisions

- **Whether `corpus/oneterm/` moves with the rest.** Default yes; see Context. Record the choice
  and its reason in this packet. No `DEC` record: it is a file location inside one crate, not a
  rationale future work must inherit.
- Nothing else. B5's shape (one type in the lowest shared crate, constants left with their
  backends) is R10 applied literally.

## Verification Plan

Focused proof:

- `cargo test -p oneterm-tools`, with `tests/corpus_check.rs`'s per-recording count compared
  against the baseline — equal, not merely green.
- `cargo test -p oneterm-local-shell` and `-p oneterm-ssh`, with the four existing budget
  assertions unedited, plus the new overflow test.
- `cargo test -p oneterm-vt` and `cargo test -p oneterm-vt --features vt-paranoid` — unchanged by
  this packet, and that is the point: the engine's tests must be unaffected by losing 3.2 MB they
  never read.

Integration:

- `cargo test -p oneterm-core -p oneterm-terminal -p oneterm-local-shell -p oneterm-ssh` — the
  portable backend contracts, which is where B5 lands.

Regression:

- `cargo test --workspace` (this is what runs the parity drift gate).
- `cargo clippy --workspace --all-targets -- -D warnings`.
- `python scripts/third-party-notices.py --check` and
  `python scripts/verify-dependency-graph.py`.
- A clean-tree check that the move introduced no content change: `git diff --stat` empty, and the
  move commit's `--stat` showing renames only.

Platform:

- `pwsh scripts/ci-local.ps1`, exit 0, totals recorded.

No E2E and no GUI walk. B3 moves test data that the application never opens, and B5 changes
arithmetic already covered by four focused tests on both backends plus the three-platform
portable contract suite.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

After implementation, record: the branch point and the move commit; the rename-only `--stat` and
the before/after corpus checksum and file count; `corpus_check.rs`'s per-recording count before
and after; the `third-party-notices.py --check` result and the regenerated rows; the `deny.toml`
and licence-analysis finding; the B5 net line delta; the five test counts before and after;
whether `corpus/oneterm/` moved and why; and `ci-local.ps1`'s totals.

## Handoff

Last packet of `IN-0032`. On completion, reconcile the intake: mark all four packets, confirm the
`terminal-diagnostics` decision from `US-0090` is recorded, and note anything the audit listed
that was deliberately not done — `B2`, the `frame.rs` mirror, the `doom-fire` licence split, the
per-OSC `Vec` allocation, and Option C in full.
