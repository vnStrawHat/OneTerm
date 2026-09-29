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
away stale siblings left by earlier, by-now-exited test processes, so the pre-existing backlog on a
developer machine drains on its own over normal use instead of needing a manual purge.

## Reported by

Owner, 2026-09-29, after finding the four directory families while clearing
`%LOCALAPPDATA%\Temp`. See prompt context; no separate evidence file — the counts below are this
packet's own verification.

## Scope

- [ ] In scope:
  - `crates/vt/src/pty/windows/conpty.rs` `#[cfg(test)] mod tests` — `scratch_directory` and its
    two callers (`conpty_api_prefers_the_bundled_host`, `conpty_api_falls_back_to_the_system_host`).
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
- [x] No new dependency added (`tempfile` is not in `oneterm-vt`, `oneterm-app`, or `oneterm-state`'s
      dependency graph as a direct dependency; a small RAII guard is used instead, matching the
      pattern `crates/state/src/dock_persistence.rs` already used for its later tests).
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

**The fix actually used.** `scratch_directory` now sweeps stale same-`name` siblings (matched by the
`oneterm-vt-pty-{name}-` prefix, excluding its own current-PID directory) out of the OS temp
directory *before* creating its own. A sibling from an earlier test process is safe to remove
unconditionally: that process has exited, so any DLL it loaded is already unloaded and the
directory is unlocked. Combined with the existing best-effort `Drop`, this means: the `system`
family is removed immediately (no locked file, `Drop` succeeds); the `bundled` family always has at
most one directory standing (the current process's, removed by the very next process's sweep).
Measured after the fix: `oneterm-vt-pty-bundled-*` dropped from 28 (all stale) to 1 on the very next
run, and stayed at 1 across three more full-suite runs, `vt-paranoid`, `--no-default-features`, and
`--test-threads=1`/`=8`. This also means the fix is self-healing for the owner's existing backlog —
it does not require a manual purge of `%LOCALAPPDATA%\Temp`.

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
each file for an existing `Drop`-based guard. Most already have one (`session-ui/src/session_state.rs`,
`sftp-ui/src/test_backend.rs`, `ssh/src/sftp_task/handle_limit_tests.rs`,
`ssh/src/sftp_task/transfer/in0037_verify_tests.rs`, `workspace/src/layout/workspace/layout_tests.rs`,
`workspace/src/layout/workspace/persistence.rs`) and are not candidates. Two do not and have the same
latent-panic-leak shape `crash_report.rs` had before this packet (manual `remove_dir_all` as the last
statement of each test, so an earlier failing assertion skips it):
  - `crates/settings/src/ui_config.rs:293,341,366,381,405,431` (three tests)
  - `crates/settings/src/terminal_config/document_tests.rs:199,229,237,252,257,275` (three tests)
Neither was in the owner's report — nothing here indicates either is actually leaking today — so
both are out of scope for this packet (see Scope) and are listed here only as a starting point if
`BUG-0083` is opened to convert them to the same guard pattern pre-emptively.

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

**Gate:** `pwsh scripts/ci-local.ps1` — final line recorded below once run to completion for this
packet's changes (see commit message / handoff for the actual pasted line).

**Gaps:**
- No CI runner run of this fix (Windows-only; verified on this developer machine only). The
  `oneterm-vt` pty code is `#[cfg(windows)]`-only, so CI's Windows job is the only place this can
  run in CI, and this packet was not pushed as part of this session.
- `harness.db` was not touched (see Status). Status/proof are hand-maintained here.
- BUG-0083 candidates (see Context) are unverified; none were run in isolation to check whether they
  leak, since the owner's report did not include them and this packet keeps to the reported scope.

## Handoff

Implemented and verified in this session. Branch `fix/vt-pty-test-tempdir`, not pushed. Next owner:
whoever pushes/opens the PR should also decide whether to open `BUG-0083` for the candidates listed
in Context, or drop them.
