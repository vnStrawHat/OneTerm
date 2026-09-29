# Work: PTY test scratch directories are never removed

ID: BUG-0082
Intake: IN-0029
Created: 2026-09-29

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

> No `harness` CLI is available in this session (it is a `pi` extension; see the project memory
> `harness-and-gui-verification`). Per this packet's own instruction, `harness.db` is not touched
> either — the status/proof blocks below are hand-maintained instead of CLI-mirrored.

## Classification

- Change type: bug (test-only resource leak, not a product defect)
- Risk lane: normal
- Spec Intake, when required: `IN-0029` — `docs/spec-intakes/IN-0029-vt-engine/IN-0029.md`, which
  owns the ConPTY host resolution (`DEC-0013`) and its test support.

## Outcome

`crates/vt/src/pty/windows/conpty.rs`'s `scratch_directory` test helper created a directory under
`%LOCALAPPDATA%\Temp` per test run and never removed it. Owner's report (2026-09-29): 4,388
`oneterm-vt-pty-bundled-<pid>` directories accumulated in `%LOCALAPPDATA%\Temp`, one per historical
`cargo test -p oneterm-vt` process. Two smaller, unrelated families were reported alongside it —
`oneterm-crash-report-test-<pid>-outside-store` (`crates/app/src/crash_report.rs`) and
`oneterm-dock-document-test-<pid>-<nanos>` / `-recovery-test-` / `-schema-test-` (`crates/state/src/dock_persistence.rs`)
— both fixed here too, since the same guard pattern closed them in a few lines each.

After this packet: none of the three families grow across repeated `cargo test` runs (measured
below); the `oneterm-vt-pty-bundled-*` family is additionally self-healing — each new run sweeps
away stale siblings left by earlier, provably-exited test processes, so the pre-existing backlog on
a developer machine drains on its own over normal use instead of needing a manual purge.

**Rework note (2026-09-29, second pass).** Independent verification
(`docs/spec-intakes/IN-0029-vt-engine/evidence/BUG-0082-verify.md`) found the first pass's sweep
(commit `a7854507`) unsafe: it deleted any same-prefix sibling directory by name alone, with no check
that the PID in its name was actually dead. Two real concurrent `cargo test -p oneterm-vt` processes
— exactly the shape of running several worktree agents on one machine, as this repository's own
present setup often does — spuriously failed `conpty_api_prefers_the_bundled_host` in 13 of 15 runs
(F1b), and a fake sibling named after a real live PID was deleted outright (F1a). This second pass
adds a PID-liveness check (`OpenProcess` / `GetExitCodeProcess`) before a sibling is ever swept; see
Context for the mechanism and why it also closes the create/load-DLL race, not just the two
findings' reproduction cases.

## Reported by

Owner, 2026-09-29, after finding the four directory families while clearing
`%LOCALAPPDATA%\Temp`. See prompt context; no separate evidence file — the counts below are this
packet's own verification.

## Scope

- [ ] In scope:
  - `crates/vt/src/pty/windows/conpty.rs` `#[cfg(test)] mod tests` — `scratch_directory` and its
    two callers (`conpty_api_prefers_the_bundled_host`, `conpty_api_falls_back_to_the_system_host`);
    the added `is_stale` (pure) and `process_is_alive` (real `OpenProcess`/`GetExitCodeProcess`)
    helpers and their tests, including the `#[ignore]`d two-process race regression.
  - `crates/app/src/crash_report.rs` `#[cfg(test)] mod tests` — `temporary_directory` and every
    caller (ten tests; one, `deleting_a_report_outside_the_current_store_is_refused`, is the one
    that was actually leaking — the other nine already cleaned up manually, but manual cleanup
    after assertions does not run on a panicking assertion, so all ten were latent leaks).
  - `crates/state/src/dock_persistence.rs` `mod persistence_tests` — the three tests
    (`explicit_path_updates_are_isolated_and_atomic`, `invalid_document_is_quarantined_and_updates_keep_working`,
    `legacy_fixture_migrates_during_shared_update`) that predated this file's existing `TempDirGuard`
    / `temp_directory` helper and were never migrated to it.
- [ ] Out of scope:
  - Any other `std::env::temp_dir()` user in the workspace not named in the owner's report or found
    to share this exact defect (grepped; see Documentation below). Recorded as BUG-0083 candidates
    where a leak was found but the guard pattern was not a small fix.
  - `crates/vt/src/pty/windows/conpty.rs` production code (`ConptyApi::resolve`, `load_bundled`,
    `executable_directory`). `load_bundled`'s "the module is intentionally never freed" behaviour is
    unchanged; the test fix works around it rather than through it (see Context).
  - `DEC-0013` itself (bundled ConPTY host preference) — not reopened, not touched.

## Acceptance

- [x] `oneterm-vt-pty-bundled-*`, `oneterm-vt-pty-system-*`, `oneterm-crash-report-test-*` and
      `oneterm-dock-*` entry counts in `%LOCALAPPDATA%\Temp` do not grow across three consecutive
      `cargo test` runs of the owning crate (`oneterm-vt`, `oneterm-app`, `oneterm-state`
      respectively), including under `--features vt-paranoid`, `--no-default-features`,
      `--test-threads=1` and `--test-threads=8` for `oneterm-vt`.
- [x] Every directory removal is best-effort and runs on a panicking assertion too (`Drop`), except
      where a still-loaded DLL makes immediate removal impossible on Windows — that case is
      documented and deferred to the next process instead of silently accepted as unfixable.
- [x] A sweep never removes a same-prefix sibling directory whose PID is a live process, checked
      (not assumed): a fake sibling named after a real live PID survives (regression test), and two
      genuinely concurrent processes running `conpty_api_prefers_the_bundled_host` never spuriously
      fail each other (10/10 iterations of the `#[ignore]`d two-process race regression, run by
      hand — this reproduced 13/15 failures before this pass; see Context).
- [x] No new dependency added (`tempfile` is not in `oneterm-vt`, `oneterm-app`, or `oneterm-state`'s
      dependency graph as a direct dependency; a small RAII guard is used instead, matching the
      pattern `crates/state/src/dock_persistence.rs` already used for its later tests; the liveness
      check uses `windows-sys`' `Win32_System_Threading`, already an enabled workspace feature).
- [x] Full CI-local gate passes.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/pty.md` — describes `ConptyApi` host
  resolution and the `DEC-0013` preference order; does not describe test scaffolding or temp-file
  lifecycle. Not stale.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/testing-and-bench.md` — describes the
  bench/property-test harness; does not mention `scratch_directory` or temp-directory cleanup as a
  contract. Not stale.
- `docs/decisions/DEC-0013-bundled-conpty-host-and-bump-script.md` — records the bundled-host
  preference order and the risk it guards against (Sixel passthrough silently dropping to
  `kernel32`). Says nothing about test temp-directory lifecycle. Not stale.
- `crates/vt/README.md`, `crates/vt/CHANGELOG.md` — the crate's external-facing promise is "what an
  embedder compiles against or what bytes the terminal replies with" (CHANGELOG.md, "The promise").
  This change touches only `#[cfg(test)]` code, nothing `pub`, so no entry is warranted.
- `scripts/check-ignored-tests.py` census — unaffected; no test gained or lost an `#[ignore]`.

### Documentation Action

- No contract change: reviewed docs describe production behaviour (ConPTY host resolution) or the
  bench/property-test harness, not test-fixture temp-directory lifecycle. This packet is the record
  of that lifecycle fix; no other doc claims a contract this changes.

Reason: temp-directory cleanup in `#[cfg(test)]` modules is test plumbing, not a documented
interface, schema, or behaviour an embedder or a future engineer needs to discover from
`docs/spec-intakes` or the crate's own docs. The code comments at each guard explain the mechanism
in place (this is where a future reader will actually look).

### Reconciliation

No docs changed; the no-change reason above still holds after implementation.

## Context

**Why `scratch_directory`'s `Drop` alone is not enough for the `bundled` case.**
`ConptyApi::load_bundled` calls `LoadLibraryW` on the staged `conpty.dll` and never calls
`FreeLibrary` — by design (see its own `SAFETY` comment: "the returned function pointers stay valid
for the life of the process"). Measured empirically on this machine: with only a `Drop`-based
`remove_dir_all`, running `cargo test -p oneterm-vt --lib -- conpty` three times grew
`oneterm-vt-pty-bundled-*` from 25 to 28 (one new, un-removable directory per run), while
`oneterm-vt-pty-system-*` (which never loads a DLL — `load_bundled` fails fast when
`conpty.dll` is absent) stayed at 24. Windows will not remove a directory containing a file that is
still mapped into a running process.

**The fix actually used (first pass — superseded by the liveness check below).**
`scratch_directory` sweeps stale same-`name` siblings (matched by the `oneterm-vt-pty-{name}-`
prefix, excluding its own current-PID directory) out of the OS temp directory *before* creating its
own. Combined with the existing best-effort `Drop`, this means: the `system` family is removed
immediately (no locked file, `Drop` succeeds); the `bundled` family always has at most one directory
standing (the current process's, removed by the very next process's sweep). Measured after the fix:
`oneterm-vt-pty-bundled-*` dropped from 28 (all stale) to 1 on the very next run, and stayed at 1
across three more full-suite runs, `vt-paranoid`, `--no-default-features`, and
`--test-threads=1`/`=8`. This also means the fix is self-healing for the owner's existing backlog —
it does not require a manual purge of `%LOCALAPPDATA%\Temp`.

**Why "a sibling is safe to remove because that process has exited" was wrong as written.** The first
pass asserted this but never checked it: the sweep matched on name alone (same prefix, not the
sweeper's own current-PID name), so *any* same-prefix directory was removed regardless of whether its
owning process was still running. It happened to be safe for the steady-state, single-process-at-a-
time case this packet's own before/after counts exercised, but independent verification
(`evidence/BUG-0082-verify.md`, findings F1/F1a/F1b) found it unsafe under real concurrency:

* **F1a** — a directory named after a real, running, unrelated process's PID was deleted outright.
* **F1b** — two genuinely concurrent OS processes each running `conpty_api_prefers_the_bundled_host`
  produced a spurious `assert_eq!` panic in 13 of 15 runs: process A creates its directory and starts
  staging `conpty.dll`; before A calls `LoadLibraryW` (the only thing that would have made the first
  pass's directory-locked accident protect it), B's sweep runs, sees A's directory as an unrecognized
  same-prefix sibling, and removes it. `resolve_in` then falls back to `System`, and A's own assertion
  (`assert_eq!(..., Bundled)`) fails. This repository's own present working setup — several
  `worktree-agent-*` branches, i.e. multiple concurrent Claude Code sessions building and testing the
  same crate on one physical Windows machine, each with `%LOCALAPPDATA%\Temp` in common — is exactly
  this shape.

**The fix now used: liveness, not name-matching alone.** Before a same-prefix sibling is removed, its
trailing PID is parsed out of the name (`is_stale`, a pure function taking an injected `alive: impl
Fn(u32) -> bool`, unit-tested directly with no real OS process involved) and checked against a real
liveness probe (`process_is_alive`): `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, ...)` fails with
`ERROR_INVALID_PARAMETER` (no such process), or it succeeds but `GetExitCodeProcess` reports anything
other than `STILL_ACTIVE` (the process object still exists but has already exited). Any other
`OpenProcess` failure (for example `ERROR_ACCESS_DENIED`) answers "alive" — the safe direction when
liveness genuinely cannot be determined. A name that does not parse as `<prefix><u32>` is left alone;
so is the sweeper's own current-PID name, unconditionally (it does not even ask `alive`, which
matters for the regression test that injects an always-"dead" liveness function). No new dependency:
`Win32_System_Threading` (`OpenProcess`, `GetExitCodeProcess`) is already an enabled workspace
`windows-sys` feature (added for `US-0137`'s `GetProcessMemoryInfo`), and `windows-sys` is already an
optional dependency of `oneterm-vt` under the `pty` feature.

**Why this also closes the create/load-DLL race, not just the two reproduction cases.** F1b's root
cause was never really about `LoadLibraryW` specifically — that was only ever an *accidental* protection
that happened to exist after the DLL load completed, and never before it. Once the sweep asks "is this
PID alive" instead of "is this name mine," the window between `create_dir_all` and `LoadLibraryW`
stops mattering: a live process's directory is skipped by any peer's sweep the instant it exists
(`create_dir_all` succeeding *is* the claim — a directory whose name embeds a live PID unconditionally
survives everyone else's sweep, at every stage of that PID's setup, DLL loaded or not). Verified: the
same two-process race that failed 13/15 times before this pass now passes 10/10 (regression test
`concurrent_bundled_host_race_does_not_spuriously_fail`, run by hand — see Verification Plan).

**Why the other two families needed no such workaround.** Neither `crash_report.rs`'s
`temporary_directory` nor `dock_persistence.rs`'s tests load anything that keeps a file handle open
past the test function returning, so a plain `Drop`-based guard removes them immediately and always.

**`crash_report.rs`: root cause, not the one symptom.** Only `outside-store` was reported as
leaking, but the other nine `temporary_directory` callers relied on a manual
`fs::remove_dir_all(directory).expect(...)` as the *last statement* of the test — which never runs
if an earlier assertion in the same test panics. That is a systemic latent leak, not specific to the
one test that happened to trip it, so `temporary_directory` itself was converted to return an
RAII guard and all ten call sites' manual cleanup was removed (dead code once the guard exists).
One call site (`reports_and_store_are_private_to_the_user`, Unix-only) chained `.join("crashes")`
directly off the temporary guard value, which would have dropped (and removed) the guard at the end
of that one `let` statement, before the rest of the test ran; it now binds the guard to `root`
separately and derives `directory` from it.

**`dock_persistence.rs`: reuse, not a new pattern.** This file already had a `TempDirGuard` +
`temp_directory(label)` helper (Ponytail rung 2 — already in this codebase) used by later tests; the
three earliest tests in the same file predated it and were simply never migrated. They now call the
existing helper instead of building their own `std::env::temp_dir().join(...)` path by hand.

**Other `std::env::temp_dir()` users checked and left alone (BUG-0083 candidates).** Grepped every
`std::env::temp_dir()` call in `crates/` (Grep tool; `rg` is not installed on this box) and checked
each file for an existing `Drop`-based guard; two lacked one
(`settings/src/ui_config.rs`, `settings/src/terminal_config/document_tests.rs`). Independent
verification (`evidence/BUG-0082-verify.md`, finding F5) cross-referenced the same grep against every
file containing `impl Drop for` and found this list incomplete — seven more unguarded, test-only,
same-shape users. The complete candidate list (nine files) is recorded once, in Handoff, rather than
duplicated here.

## Plan

- [x] Convert `crates/vt/src/pty/windows/conpty.rs`'s `scratch_directory` to return an RAII guard
      (`ScratchDirectory`, `Deref<Target = Path>`) with a best-effort `Drop`, plus a sweep of stale
      same-prefix siblings on creation.
- [x] Convert `crates/app/src/crash_report.rs`'s `temporary_directory` to return an RAII guard
      (`TemporaryDirectory`) with a best-effort `Drop`; remove the now-redundant manual
      `fs::remove_dir_all` calls at every call site; fix the one call site that chained `.join()`
      directly off the temporary.
- [x] Migrate `crates/state/src/dock_persistence.rs`'s three unconverted tests to the file's
      existing `temp_directory` helper; fix the one `.as_path()` call that no longer resolves
      through the guard's `Deref` (switched to `.as_ref()`, which the guard's existing `AsRef<Path>`
      impl provides).
- [x] Verify before/after directory counts per crate/run configuration (see Evidence).
- [x] Run the full local CI gate.

**Rework (second pass, after `evidence/BUG-0082-verify.md` FAIL):**

- [x] Add a pure `is_stale(name, prefix, current_pid, alive: impl Fn(u32) -> bool) -> bool` helper
      and a real `process_is_alive(pid) -> bool` (`OpenProcess` + `GetExitCodeProcess`); wire the
      sweep to use them instead of name-matching alone.
- [x] Unit-test `is_stale` directly (live PID kept, dead PID removed, own PID kept regardless of
      `alive`, unparsable name kept).
- [x] Add a non-ignored regression test that a sibling named after a real live PID (a spawned
      `cmd /c pause` child this test controls and later kills) survives the sweep.
- [x] Add an `#[ignore]`d regression test that spawns two concurrent processes of this same test
      binary running only `conpty_api_prefers_the_bundled_host`, ten times, and requires 0 failures;
      record it in `scripts/ignored-tests.txt` via `check-ignored-tests.py --write`.
- [x] Re-verify directory counts flat, re-run the full configuration matrix, run the new ignored
      race test by hand, and re-run the full CI gate.
- [x] Fold the verifier's complete BUG-0083 candidate list (F5) into Handoff.

## Decisions

None. No architectural or public-contract choice is made here; the mechanism and its trade-off are
documented in Context and in the code comments at each guard.

## Verification Plan

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

- `cargo test -p oneterm-vt --lib -- conpty` and the full `cargo test -p oneterm-vt` (default,
  `--features vt-paranoid`, `--no-default-features`, `--test-threads=1`, `--test-threads=8`), each
  x3, comparing `oneterm-vt-pty-{bundled,system}-*` counts in `%LOCALAPPDATA%\Temp` before/after.
- `cargo test -p oneterm-app --lib -- crash_report` x3, comparing `oneterm-crash-report-test-*`
  counts before/after.
- `cargo test -p oneterm-state --lib -- dock_persistence` x3, comparing `oneterm-dock-*` counts
  before/after.
- `cargo test -p oneterm-vt --lib -- --ignored concurrent_bundled_host_race`, run by hand once
  (10 internal iterations, 0 failures required — this is the F1b regression check).
- `python scripts/check-ignored-tests.py --write` to record the new ignored test, then
  `python scripts/check-ignored-tests.py` clean.
- `python scripts/vt-public-api.py --check --no-doc` / `--diff-platforms` after a fresh
  `cargo doc -p oneterm-vt --no-deps --all-features`.
- The rustdoc self-containment grep over `crates/vt/src` (no `US-`/`BUG-`/`DEC-`/`IN-` citation or
  bare `crates/`/`docs/` path in a `///`/`//!` line).
- Full `pwsh scripts/ci-local.ps1`.

## Evidence and Gaps

**Directory counts** (`%LOCALAPPDATA%\Temp`, this machine, this session):

| Family | Baseline (pre-fix, historical) | After `Drop`-only fix, x3 runs | After sweep fix | x3 more full-suite runs | vt-paranoid | no-default-features | threads=1 | threads=8 |
|---|---|---|---|---|---|---|---|---|
| `oneterm-vt-pty-bundled-*` | 25 | 28 (grew +3 — the bug this packet exists to fix) | 1 | 1 | 1 | 1 | 1 | 1 |
| `oneterm-vt-pty-system-*` | 24 | 24 (never regressed — no locked file) | 0 | 0 | 0 | 0 | 0 | 0 |
| `oneterm-crash-report-test-*` | 12 | — | — | 12 (flat across 3 runs) | n/a | n/a | n/a | n/a |
| `oneterm-dock-*` | 24 | — | — | 24 (flat across 3 runs) | n/a | n/a | n/a | n/a |

The 25/24/12/24 baselines are this developer machine's pre-existing historical backlog from before
this packet (not created by this packet's verification runs); the acceptance criterion is "does not
grow", which every run satisfies. `oneterm-vt-pty-bundled-*` additionally *shrank* (25→1) the first
time the sweep ran, because it cleaned up the historical backlog too.

**Test results:** every `cargo test` invocation above reported `0 failed`. Full counts are in this
session's tool transcript; not reproduced here.

**Independent verification, first pass: FAIL.** `docs/spec-intakes/IN-0029-vt-engine/evidence/BUG-0082-verify.md`
(commit `326bc389`) found the first pass's sweep unsafe under real concurrency (F1/F1a/F1b — see
Context for the mechanism and the fix). Everything else in that verification passed: `Drop`-on-panic
correct (F2), no other `.join()`-off-a-temporary chaining bug (F3), counts flat across the specified
configuration matrix (F4), sweep is best-effort and does not panic on a locked sibling (F6), no new
dependency (F7), public API / rustdoc / fmt / ignored-test census / `--no-default-features` all pass
(F8). F5 (BUG-0083 candidate list incomplete) is folded into Handoff below.

**Second pass, re-verification (this session):**

| Check | Result |
| --- | --- |
| `is_stale` unit tests (4 cases) | PASS |
| `sweep_keeps_a_sibling_named_after_a_live_pid` (real live child PID) | PASS |
| `concurrent_bundled_host_race_does_not_spuriously_fail`, run by hand (`--ignored`) | PASS — 10/10 iterations, 0 failures (was 13/15 *failures* before this pass) |
| `oneterm-vt-pty-bundled-*` / `-system-*` counts, x3 runs | flat at 1 / 0 |
| `cargo test -p oneterm-vt` default / `vt-paranoid` / `--no-default-features` | PASS, 0 failed each |
| `python scripts/vt-public-api.py --check --no-doc` / `--diff-platforms` (fresh `cargo doc`) | unchanged |
| Rustdoc self-containment grep (`crates/vt/src`) | clean |
| `python scripts/check-ignored-tests.py --write` then plain | recorded 19 ignored tests, then clean |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy -p oneterm-vt --all-targets -- -D warnings` | PASS |
| `python scripts/check-doc-paths.py` | PASS (212 paths, 11 documents) |
| `python scripts/check-english.py` | PASS (1083 files) |
| Full `pwsh scripts/ci-local.ps1`, `CARGO_BUILD_JOBS=4`, `target/debug/incremental` deleted first | PASS — final line: `ci-local: all checks passed.` |

**Gaps:**
- No CI runner run of this fix (Windows-only; verified on this developer machine only). The
  `oneterm-vt` pty code is `#[cfg(windows)]`-only, so CI's Windows job is the only place this can
  run in CI, and this packet was not pushed as part of this session.
- `harness.db` was not touched (see Status). Status/proof are hand-maintained here.
- The concurrency regression test is `#[ignore]`d (it forks two child processes ten times and takes a
  few seconds), so it does not run in the default gate; it must be run explicitly (see Verification
  Plan) whenever this sweep changes again.
- BUG-0083 candidates (see Handoff) are unverified; none were run in isolation to check whether they
  leak, since the owner's report did not include them and this packet keeps to the reported scope.

## Handoff

Implemented and re-verified in this session after a FAIL / rework cycle. Branch
`fix/vt-pty-test-tempdir`; first pass `a7854507`, verification `326bc389` (FAIL, evidence at
`docs/spec-intakes/IN-0029-vt-engine/evidence/BUG-0082-verify.md`), rework on top in one further
commit. Not pushed.

**BUG-0083 candidates (complete list — first pass named 2, independent verification's F5 found 7
more).** Every file below uses `std::env::temp_dir()` in test code with no `Drop`-based guard, the
same latent-panic-leak shape this packet fixed for `crash_report.rs` and `dock_persistence.rs`: none
were run in isolation to confirm an actual leak (same caveat both passes already record), so this is
a starting point for whoever opens `BUG-0083`, not a proven leak list.

- `crates/settings/src/ui_config.rs` (three tests)
- `crates/settings/src/terminal_config/document_tests.rs` (three tests)
- `crates/app/src/native_crash.rs:242` — `fs::remove_file(path).expect(...)` as the last statement
  (one file, not a directory tree, leaks on an earlier panic)
- `crates/session-ui/src/auth_form.rs:469` — same shape, one file
- `crates/core/src/config/shell.rs` — test module using `temp_dir()`, no `impl Drop` in the file
- `crates/ssh/src/agent_tests.rs`, `crates/ssh/src/route_tests.rs`, `crates/ssh/src/tunnel_tests.rs`
  — no `impl Drop` in any of the three, unlike sibling files in the same crate that already have one
  (`handler_tests.rs`, `keyfile_tests.rs`, `test_support.rs`, `sftp_task/handle_limit_tests.rs`,
  `sftp_task/transfer/in0037_verify_tests.rs`, `sftp_task/transfer/pipeline_budget_tests.rs`,
  `us0095_verify_tests.rs`) — the crate is inconsistent, not uniformly exposed
- `crates/update/src/config.rs` — no `impl Drop` in the file (its siblings `archive_tests.rs` and
  `manager_tests.rs` in the same crate do have one)

Next owner: whoever pushes/opens the PR should also decide whether to open `BUG-0083` for this list,
or drop it. If the sweep in `conpty.rs` changes again, re-run the `#[ignore]`d
`concurrent_bundled_host_race_does_not_spuriously_fail` by hand — it is not in the default gate.
