# Work: The corpus lives with its reader and the byte budget has one home

ID: US-0093
Intake: IN-0032
Created: 2026-09-14

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [x] Implemented
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

- [x] Move `crates/vt/tests/corpus/` → `crates/tools/corpus/`, including `alacritty-ref/`,
  `oneterm/` and `NOTICE`. Use `git mv` so the move is recorded as a rename and the bytes are
  provably unchanged.
- [x] `crates/tools/src/corpus.rs:87-92` — `corpus_root()` resolves from `CARGO_MANIFEST_DIR`
  directly; delete the `.parent().expect("crates/tools has a parent").join("vt/tests/corpus")`
  hack and the stale doc comment above it (*"The corpus lives at its final home under
  `crates/vt/` even though that crate does not exist yet"* — `crates/vt` has existed since
  `US-0073`).
- [x] Every other path reference, all of which are real and none of which the compiler catches:
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

- [x] Add one `ByteBudget` (an `AtomicUsize` with reserve and release) to
  `crates/terminal/src/backend/`, alongside `pump.rs`, `osc_router.rs`, `state.rs`,
  `event_sink.rs` and `transport.rs`.
- [x] `crates/local-shell/src/event_loop.rs:120-140` and `crates/ssh/src/transport.rs:121-134`
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

- [x] **The data is byte-identical.** `git log --follow --stat` (or `git show --stat` on the move
  commit) shows pure renames with no content change for all 46 recordings and the `NOTICE`.
  Additionally record `find crates/tools/corpus -type f | wc -l` and a directory checksum before
  and after.
- [x] **`crates/vt` no longer carries the corpus**: `crates/vt/tests/corpus` does not exist, and
  `grep -rn "vt/tests/corpus" .` returns hits only in `docs/spec-intakes/` and
  `docs/decisions/` history.
- [x] **No sibling-path resolution anywhere**: `grep -rn "\.parent()" crates/tools/src/corpus.rs`
  returns nothing.
- [x] **The parity gate passes with the same result**: `cargo test -p oneterm-tools`, including
  `tests/corpus_check.rs`, the drift gate that runs inside `cargo test --workspace`. Record the
  count before and after; it must be identical, not merely passing — a corpus that silently
  stopped being found would also "pass" if the gate skips a missing directory, so confirm the
  per-recording count.
- [x] **`python scripts/third-party-notices.py --check` passes** against a regenerated
  `THIRD-PARTY-NOTICES.md` whose corpus row names the new paths.
- [x] **`.gitattributes` covers the new location** and no recording's bytes changed in the working
  tree after the move (`git status` clean, `git diff --stat` empty on a fresh checkout).

**B5:**

- [x] **One implementation.** `grep -rn "fetch_update" crates/local-shell/src crates/ssh/src`
  returns nothing; the idiom exists once, in `crates/terminal/src/backend/`.
- [x] **Net delta at least −20 lines** across the two backends (the audit's figure for the
  duplication is about 35 lines; the shared type costs some of it back). Record the measured
  number.
- [x] **Both backends' budget tests pass untouched**: `local-shell/src/event_loop_tests.rs:53,
  :63` and `ssh/src/transport_tests.rs:98-110`, which exercise reserving exactly the budget,
  exceeding it, and releasing. No assertion may be edited. If one must be, the semantics moved
  and the packet is wrong.
- [x] **Overflow behaviour is preserved and pinned.** The current idiom uses `checked_add`, so a
  reservation that would overflow `usize` fails rather than wrapping. The shared type must keep
  that, and a test must prove it — this is the one place where "tidying" a duplicated idiom could
  quietly remove a guard.
- [x] **R10 holds**: the shared type lives in the lowest crate both backends already depend on
  (`crates/terminal`), not duplicated and not pushed up. `python scripts/verify-dependency-graph.py`
  passes and no manifest gains a dependency.

**Both:**

- [x] **No test lost.** Baselines recorded for `cargo test -p oneterm-tools`, `-p oneterm-vt`,
  `-p oneterm-terminal`, `-p oneterm-local-shell`, `-p oneterm-ssh`; each after-count greater than
  or equal to baseline.
- [x] **`pwsh scripts/ci-local.ps1` exits 0**, with totals recorded. It runs both gates that matter
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

- [x] Record branch-point baselines: the five `cargo test -p …` counts, `corpus_check.rs`'s
  per-recording count, and a checksum of `crates/vt/tests/corpus`.
- [x] `git mv crates/vt/tests/corpus crates/tools/corpus`. Commit the move **alone**, so the
  rename is reviewable and provably content-free.
- [x] Fix `corpus_root()` and its doc comment; run `cargo test -p oneterm-tools` and compare the
  per-recording count against the baseline.
- [x] Move the `.gitattributes` rule; verify on a fresh clone or `git stash`-clean tree that no
  recording's bytes differ.
- [x] Update `scripts/third-party-notices.py`, regenerate `THIRD-PARTY-NOTICES.md`, run
  `--check`.
- [x] Fix the remaining path references (`vt-corpus.rs` help, `crates/vt/fuzz/Cargo.toml`, the
  three prose sites).
- [x] Read `deny.toml` and `docs/license-analysis.md`; record that the crate-scoped exception
  still describes the truth, or fix it.
- [x] B5: add `ByteBudget` to `crates/terminal/src/backend/`, including the overflow test; move
  both backends onto it; run each backend's tests against baseline.
- [x] Update `docs/terminal-backend.md`, `docs/agents/structure.md` and
  `testing-and-bench.md`.
- [x] `cargo test --workspace`; `pwsh scripts/ci-local.ps1`.

## Decisions

- **Whether `corpus/oneterm/` moves with the rest.** **Taken: yes, it moved** —
  `crates/tools/corpus/oneterm/sixel_basic/`. Its only reader is `corpus::oneterm_dir()`, in the
  same crate and the same module as `alacritty_ref_dir()`, and `tests/corpus_check.rs` gates the
  two directories in one file; splitting them would have left half the gate's data under a crate
  that opens neither half, re-creating the exact sibling-path reach this packet removes. The
  provenance argument is answered by the `NOTICE` travelling with the data: its `oneterm/`
  section, written at `US-0080`, states that these recordings are synthetic OneTerm files blessed
  by the engine being replaced and then frozen — which is the fact `R-58` needs recorded, and is
  not a fact about which directory holds them. No `DEC` record: a file location inside one crate.
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
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Branch point `394f075` (`main`; `US-0090`, `US-0091` and `US-0092` all merged). Commits:

| Commit | Scope |
| --- | --- |
| `ba0e5d9` | `refactor(tools)`: the corpus move and every path reference (B3) |
| `7dbb1f8` | `refactor(terminal)`: one `ByteBudget` for both backends (B5) |
| *(docs)* | this packet, `IN-0032`, `docs/terminal-backend.md` |

### B3 — the move is provably content-free

Immediately after `git mv crates/vt/tests/corpus crates/tools/corpus`, before any other edit:

```
git diff --cached --name-status -M --find-renames=100% | grep -vc '^R100'  ->  0
git diff --cached --stat -M  ->  231 files changed, 0 insertions(+), 0 deletions(-)
```

All 231 files — the 45 `alacritty-ref/` recordings with their `size.json`, `config.json`,
`grid.expect` and `state.expect`, `oneterm/sixel_basic/`, and the `NOTICE` — are `R100` with a
zero diff. File count 231 before, 231 after. A content checksum taken **relative to the corpus
root** (blob hash plus path with the root prefix stripped, sorted) is identical on both sides:

```
before   ls-tree -r HEAD crates/vt/tests/corpus, prefix stripped
after    ls-files -s crates/tools/corpus,        prefix stripped
both  ->  eb63fbcc661aca35b7dc2a4c02b6309c392a8c280fd8725636031f1e215da385
```

The whole-branch `git diff --stat -M main..HEAD` is
`247 files changed, 120 insertions(+), 60 deletions(-)`: 231 renames carrying nothing, plus 16
edited files.

### B3 — the `.gitattributes` rule really did travel

The rule is the one thing here that fails silently, and in a way that looks unrelated to its
cause, so it was checked against Git itself rather than by reading the file. Same attributes
before and after, on a raw recording, a frozen expectation and the `NOTICE`:

```
before  check-attr crates/vt/tests/corpus/alacritty-ref/alt_reset/recording  -> text: unset, whitespace: unset
after   check-attr crates/tools/corpus/alacritty-ref/alt_reset/recording     -> text: unset, whitespace: unset
after   check-attr crates/tools/corpus/alacritty-ref/alt_reset/grid.expect   -> text: unset, whitespace: unset
after   check-attr crates/tools/corpus/oneterm/sixel_basic/recording         -> text: unset, whitespace: unset
after   check-attr crates/tools/corpus/NOTICE                                -> text: unset, whitespace: unset
```

### B3 — the gate passes with the same result, and still bites

```
cargo run -q -p oneterm-tools --release --bin vt-corpus -- check
  -> 45 recordings, 45 passed, 0 failed
cargo run -q -p oneterm-tools --release --bin vt-corpus -- check --dir crates/tools/corpus/oneterm
  -> 1 recordings, 1 passed, 0 failed   (ok sixel_basic)
cargo test -p oneterm-tools --test corpus_check   -> 2 passed
```

The per-recording counts are 45 and 1 before and after, not merely "green": `corpus_check.rs`
asserts `recordings.len() == 45` and `!recordings.is_empty()`, so a corpus that silently stopped
being found fails rather than skips.

**Tamper proof.** One byte changed in one frozen expectation —
`crates/tools/corpus/alacritty-ref/alt_reset/grid.expect`, offset 383, the first cell of row 0
from `0020` to `0021` — and the gate failed, naming the recording:

```
test the_engine_matches_the_frozen_alacritty_expectations ... FAILED
panicked at crates\tools\tests\corpus_check.rs:68:5:  alt_reset:
test result: FAILED. 1 passed; 1 failed
```

Restored with a path-scoped `git checkout --`; SHA-256 back to
`ab3c9c366cfef420b83e3d0c0cc6cab64f19a60309c8aec569a3b2816e922555`.

### B3 — attribution and licence

`python scripts/third-party-notices.py` regenerated `THIRD-PARTY-NOTICES.md`; the only change is
the § 2 row, now naming `crates/tools/corpus/alacritty-ref/` and `crates/tools/corpus/NOTICE`.
`python scripts/third-party-notices.py --check` then prints
`THIRD-PARTY-NOTICES.md is up to date.`

One reference the packet did not list was found by grep and fixed: the **root `NOTICE`**, line 19,
pointed at `crates/vt/tests/corpus/NOTICE`.

**`deny.toml` and `docs/license-analysis.md`: the existing exception still describes the truth,
and was made explicit rather than changed.** `cargo-deny` scores *packages* by their declared SPDX
expression. `oneterm-tools` declares `Apache-2.0 AND GPL-3.0-only` because `doom-fire.rs` is a
GPL-3.0 port, and the crate-scoped `[[licenses.exceptions]] name = "oneterm-tools"` allows exactly
that term. Moving Apache-2.0 **data** into the package changes neither the declared expression nor
anything `cargo-deny` reads, so no `deny.toml` edit is required and none was made. What could
mislead a future reader is a package-level `AND GPL-3.0-only` sitting beside 45 third-party
Apache-2.0 captures, so `crates/tools/Cargo.toml` now says in one sentence that `corpus/` is
third-party Apache-2.0 test data carried under its own `corpus/NOTICE`, is not source, is compiled
into nothing, and is not reached by the GPL term. Nothing in `crates/tools` ships and no binary
links the data, so the Apache-2.0 § 4 obligation — attribution travelling with the files — is met
by the `NOTICE` that moved with them. `docs/license-analysis.md` § 3 was repathed; its claim is
unchanged.

### B5 — one implementation, and the guard is pinned

```
git grep -n "fetch_update" -- crates/local-shell/src crates/ssh/src  ->  (nothing)
git grep -n "\.parent()"   -- crates/tools/src/corpus.rs             ->  (nothing)
```

`ByteBudget<const LIMIT: usize>` lives in `crates/terminal/src/backend/byte_budget.rs`, the lowest
crate both backends already depend on (R10). No manifest gained a dependency and no crate edge
moved; `python scripts/verify-dependency-graph.py` passes inside `ci-local.ps1`.

The ceiling is the **type parameter**, not a field and not a second argument. Three reasons, in
order of weight: each backend keeps its own constant exactly as the Scope requires, and its own
tests keep naming it; a second call site cannot reserve against a different limit by accident; and
`ShellControl` keeps its derived `Default`, which a `ByteBudget::new(limit)` field would have cost
about ten hand-written lines to replace. It also keeps both call sites on one line — with the
constant as a second argument, rustfmt's `chain_width` split each reservation across four or five
lines and the measured delta was **−16**, missing this packet's gate for a purely cosmetic reason.

**Net line delta across the two backends: −23** (`event_loop.rs` +6/−19, `transport.rs` +6/−16),
against the ≥ −20 gate. Honest accounting of the whole change: the shared file is 77 lines — 48
production, 29 test — plus 4 lines in `backend/mod.rs`, so **workspace production is +29** and the
workspace total +58. The audit's `−35` for B5 counted the duplicated lines, not the doc comment a
shared type in a shared layer has to carry. The duplication really was about 35 lines and 23 of
them are gone, but B5 is a de-duplication, not a deletion: it does not pay for itself in lines. It
pays in the `checked_add` guard existing once and being tested.

**Overflow is preserved and pinned.**
`a_reservation_that_would_overflow_fails_instead_of_wrapping` reserves 1 against `usize::MAX`,
then asserts that a `usize::MAX` reservation is refused and the total is still 1. Without
`checked_add` that reservation would wrap the total to 0 and hand out an unbounded budget —
the one place where tidying a duplicated idiom could quietly remove a guard.
`a_reservation_is_refused_once_it_would_cross_the_limit` pins the boundary, the refusal reserving
nothing, and the release.

**No assertion was edited.** `git diff --stat main..HEAD` over
`crates/local-shell/src/event_loop_tests.rs` and `crates/ssh/src/transport_tests.rs` is empty:
both files are byte-identical to `main`. That is why `ByteBudget::load(Ordering)` keeps the
atomic's own method name and signature — the local-shell assertions read
`control.queued_input_bytes.load(Ordering::Acquire)` and still compile verbatim.

### Test counts, before and after

| Target | Before (`394f075`) | After | Δ |
| --- | --- | --- | --- |
| `oneterm-tools` lib | 14 | 14 | 0 |
| `oneterm-tools` `tests/corpus_check.rs` | 2 (45 + 1 recordings) | 2 (45 + 1 recordings) | 0 |
| `oneterm-vt` lib | 364 (+2 ignored) | 364 (+2 ignored) | 0 |
| `oneterm-vt` `parser_limits.rs` | 0 (+1 ignored) | 0 (+1 ignored) | 0 |
| `oneterm-vt` `us0087_cleanup_rows.rs` | 6 | 6 | 0 |
| `oneterm-terminal` | 285 | **287** | **+2** (the two `ByteBudget` tests) |
| `oneterm-local-shell` | 33 (+2 ignored) | 33 (+2 ignored) | 0 |
| `oneterm-ssh` | 68 | 68 | 0 |

No count shrank. The local-shell suite — which carries the lock-wait measurements — stays green at
a comparable runtime (9.5 s before, 10.3 s after, both well inside their own bounds).
`cargo test -p oneterm-vt` no longer carries 3.2 MB it never opened, and its counts are identical:
that was the point.

### Platform

`pwsh scripts/ci-local.ps1` exits **0** — `ci-local: all checks passed.` Every step ran: `fmt
--check`, both clippy passes (plain and `--features oneterm-app/terminal-diagnostics`),
`cargo test --workspace`, the `vt-paranoid` walk, `verify-dependency-graph.py`,
`check-doc-paths.py`, the English checks, `completion-catalog.py validate`, and
`third-party-notices.py --check`.

| | sections | passed | failed | ignored |
| --- | ---: | ---: | ---: | ---: |
| `cargo test --workspace` | 56 | 1 603 | 0 | 11 |
| `cargo test -p oneterm-vt --features vt-paranoid` | 4 | 370 | 0 | 3 |
| **total** | **60** | **1 973** | **0** | **14** |

`main`'s baseline is 60 / 1 971 / 0 / 14, so sections and ignored are identical and passed is
**+2** — exactly the two `ByteBudget` tests. Nothing else moved.

`cargo deny check licenses bans advisories`: **licenses ok, bans ok, advisories FAILED**. The
failure is `RUSTSEC-2026-0xxx` on `rustls 0.23.40` (TLS 1.3 handshake messages accepted across
encryption-level boundaries), which the advisory database gained after `deny.toml`'s `ignore`
list was last written. It is **not** caused by this packet: `git diff --name-only main..HEAD --
Cargo.lock` is empty and the only manifests touched are two comments (`crates/tools/Cargo.toml`,
`crates/vt/fuzz/Cargo.toml`), so the dependency graph is byte-identical to `main` and this fails
the same way on `main`. `licenses ok` is the half this packet could have broken — it did not:
moving Apache-2.0 data into the crate carrying the `GPL-3.0-only` term leaves the crate-scoped
exception satisfied and no new licence encountered. The `rustls` advisory needs a dependency bump
or a `deny.toml` entry and belongs to whoever owns that upgrade, not here.

### Gaps

- **B5 does not reduce workspace lines** (+29 production). Recorded above rather than hidden. The
  intake's `−35` estimate for B5 is not reachable by any shape of shared type that carries a doc
  comment; the packet's own gate (−20 across the two backends) is the one that was met.
- **No fresh-clone check.** The `.gitattributes` evidence is `git check-attr` plus a clean
  `git status`, not a second clone. `check-attr` is the same machinery the checkout consults, so
  this is a formality gap rather than an unverified claim.
- **The `oneterm/` corpus provenance now sits one crate further from the engine.** Deliberate; see
  Decisions. If a future owner disagrees, moving that one subdirectory back is a `git mv` plus a
  one-line change to `corpus::oneterm_dir()`.
- **`crates/vt/fuzz` seeding was repointed by reading, not by running.** Its `cp` comment now says
  `../../tools/corpus/alacritty-ref/*/recording`; `cargo-fuzz` is Linux-and-nightly only and is
  deliberately outside the workspace, so nothing here executes it.

### Harness story row

No `harness` binary is available in this worktree. Apply against `harness.db` with the real
schema:

```python
import sqlite3

db = sqlite3.connect("harness.db")
db.execute(
    "UPDATE story SET status = ?, unit_proof = ?, integration_proof = ?, e2e_proof = ?,"
    " platform_proof = ?, evidence = ?, last_verified_at = ?, last_verified_result = ?"
    " WHERE id = ?",
    (
        "implemented",
        "cargo test: oneterm-terminal 287 (+2 ByteBudget), oneterm-local-shell 33/2 ignored,"
        " oneterm-ssh 68, oneterm-tools 14 + corpus_check 2 (45+1 recordings),"
        " oneterm-vt 364/2 ignored + 6; no count shrank",
        "cargo test --workspace inside ci-local.ps1 covers oneterm-core, oneterm-terminal,"
        " oneterm-local-shell, oneterm-ssh (the portable backend contracts) plus the parity"
        " drift gate",
        "none - no automated E2E exists; B3 moves data the app never opens and B5 is"
        " arithmetic already covered by four focused assertions per backend",
        "pwsh scripts/ci-local.ps1 exit 0",
        "docs/spec-intakes/IN-0032-terminal-crate-tidy/"
        "US-0093-corpus-move-and-shared-byte-budget.md",
        "2026-09-14",
        "pass",
    ),
    ("US-0093",),
)
db.commit()
```

## Reconciliation

Docs changed by this packet:

| Doc | Change |
| --- | --- |
| `IN-0029/low-level-design/testing-and-bench.md` | the corpus contract's five path references: the vendored set, the `NOTICE`, the `expected-diffs.json` example, `oneterm/`, and `sixel_basic` |
| `docs/terminal-backend.md` | § 5.3's tree gains `byte_budget.rs`; § 6.5 gains the bullet saying the budget is one mechanism with two per-backend `LIMIT`s |
| `docs/agents/structure.md` | the tree: `crates/tools/corpus/` added, `crates/vt/tests/` no longer lists a corpus |
| `docs/license-analysis.md` | § 3's two corpus paths |
| `THIRD-PARTY-NOTICES.md` | regenerated; the § 2 corpus row repathed |
| `scripts/third-party-notices.py` | the `HEADER` row that generates it (a CI gate) |
| `NOTICE` (repository root) | the corpus pointer — **not listed in the packet**, found by grep |
| `crates/tools/Cargo.toml` | one sentence scoping the GPL term away from `corpus/` |
| `crates/tools/src/corpus.rs` | `corpus_root()`'s doc comment, both halves of which were false |
| `crates/tools/src/bin/vt-corpus.rs`, `crates/vt/fuzz/Cargo.toml` | the `--dir` help text and the fuzz seeding comment |
| `IN-0032.md` | `US-0093` ticked; the intake's closing status line |

Checked and **left unchanged**, with the reason, as this packet requires:

- **`docs/PROJECT.md`** — no edit needed. No persisted file, no boundary, and no verification
  command changed: `ci-local.ps1`'s step list is identical, `TerminalSession` is untouched, and
  the corpus is test data the application never opens.
- **`docs/agents/crate-dependency-rules.md`** — no edit needed. No crate edge moves in either
  direction. B5 puts the shared type in the lowest crate both backends already depend on, which is
  R10 applied as written; B3 *reduces* `crates/tools`' reach, since it no longer resolves a path
  into another crate's `tests/` directory, so the Layers note about `tools` reaching down to L0
  leaves needs no amendment.
- **`crates/tools/src/corpus.rs:723`, `crates/tools/src/lib.rs:9` and
  `crates/vt/src/terminal/terminal_tests.rs:6`** — the three prose sites the Scope said to check.
  All three name `crates/tools/tests/corpus_check.rs`, which did not move. Correct as written.
- **`deny.toml`** — see above: the crate-scoped exception still describes the truth.
- **Nothing was re-blessed.** `US-0072`'s `R-58` holds: the data is byte-identical and no blessing
  path exists to run.

## Handoff

Last packet of `IN-0032`. On completion, reconcile the intake: mark all four packets, confirm the
`terminal-diagnostics` decision from `US-0090` is recorded, and note anything the audit listed
that was deliberately not done — `B2`, the `frame.rs` mirror, the `doom-fire` licence split, the
per-OSC `Vec` allocation, and Option C in full.
