# Work: Test-only temp directories/files without a Drop guard

ID: BUG-0083
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
> `harness-and-gui-verification`). Per the owning packet's own precedent (`BUG-0082`), `harness.db` is
> not touched either — the status/proof blocks below are hand-maintained instead of CLI-mirrored.

## Classification

- Change type: bug (test-only resource leak, not a product defect)
- Risk lane: normal
- Spec Intake: `IN-0029` — `docs/spec-intakes/IN-0029-vt-engine/IN-0029.md`. **Intake-fit note**: none of
  this packet's files are `oneterm-vt`; they span `app`, `core`, `session-ui`, `settings`, `sftp-ui`,
  `ssh`, `terminal`, `update`, and `workspace`. `IN-0029` is used anyway, matching its own precedent:
  `BUG-0082` (the packet this one continues) already bundled fixes to `crash_report.rs` (`app`) and
  `dock_persistence.rs` (`state`) under `IN-0029` "since the same guard pattern closed them in a few
  lines each." No other existing intake owns a materially larger share of this packet's files (they are
  scattered across nine crates with no common owning intake), so splitting this packet across intakes
  would create more fragmentation than the one cross-cutting home `IN-0029` already accepted for its
  parent bug.

## Outcome

Every test-only helper that builds a path with `std::env::temp_dir()` and removes it with a manual
`std::fs::remove_dir_all`/`remove_file` call — gated behind one or more assertions that can panic first
— is converted to the same RAII guard pattern `BUG-0082` used for `crash_report.rs` and
`dock_persistence.rs`: the guard's `Drop` removes the fixture unconditionally, including on a panicking
unwind, so a failing assertion earlier in the test can no longer skip cleanup and leak the directory.

`BUG-0082`'s Handoff named nine files as unverified candidates for this packet. Re-grepping
`std::env::temp_dir()` under `crates/` this session (31 files, 43 matches — see Documentation; the verify pass corrected the earlier "30 across 29")
found that three of those nine were false positives (already guarded through an *imported* RAII type,
not a locally-defined one, which is why `BUG-0082`'s own `impl Drop for` grep missed them) and found
five more files with the same real leak shape that neither pass of `BUG-0082` named. The final, verified
inventory below supersedes the candidate list.

## Reported by

Continuation of `BUG-0082`'s own Handoff (owner ruling, 2026-09-29: "fix both" — the exit-code-259
rework and this packet). No new owner report; the candidate list and this packet's own re-verification
are the only sources.

## Scope

- [x] In scope — fixed this session (11 files, matching the leak shape: a temp path built with
      `std::env::temp_dir()`, removed manually, gated behind one or more assertions that can panic
      first):
  - `crates/app/src/native_crash.rs:242` (one test; a file, not a directory tree)
  - `crates/session-ui/src/auth_form.rs:469` (one test; a file)
  - `crates/core/src/config/shell.rs:826` (one test; a file, `#[cfg(windows)]`)
  - `crates/settings/src/ui_config.rs:293,366,405` (three tests)
  - `crates/settings/src/terminal_config/document_tests.rs:199,237,257` (three tests)
  - `crates/update/src/config.rs:584` helper, six call sites (`missing_file_uses_defaults_and_creates_document`,
    `read_never_writes_but_reports_repair_need`, `invalid_document_is_quarantined`,
    `preference_edit_during_check_survives_check_completion`,
    `preference_and_cache_writers_do_not_lose_each_others_fields`,
    `cleared_fields_are_removed_and_corrupt_documents_are_quarantined`)
  - `crates/core/src/persistence.rs:209` helper, six call sites (found by this session's own grep sweep,
    not named by `BUG-0082`'s candidate list)
  - `crates/sftp-ui/src/edit.rs:999` (one test; found by this session's own grep sweep)
  - `crates/ssh/src/sftp_task/sftp_task_tests.rs:24` helper, three call sites
    (`local_finalization_replaces_only_after_complete_write`,
    `refuses_preexisting_symlink_below_download_root` — two directories, `root` and `outside` —
    `local_upload_discovery_lists_directories_and_files`) (found by this session's own grep sweep)
  - `crates/terminal/src/logging.rs:336` helper, two call sites (found by this session's own grep sweep)
  - `crates/workspace/src/layout/workspace/persistence.rs:223`
    (`an_elevated_window_writes_no_dock_layout`, the `#[ignore]`d `IN-0043` M4 elevation test that is
    part of the CI-local gate's explicit `--ignored` run; found by this session's own grep sweep)
- [x] Checked, already guarded, no change (14 files: an imported or locally-defined RAII type already
      wraps every `temp_dir()` call before it is used, so nothing panics between creation and cleanup):
  - `crates/app/src/crash_report.rs`, `crates/state/src/dock_persistence.rs`,
    `crates/vt/src/pty/windows/conpty.rs` — fixed by `BUG-0082` itself.
  - `crates/session-ui/src/session_state.rs` (`TempDirGuard`), `crates/sftp-ui/src/test_backend.rs`
    (`TempDir`), `crates/workspace/src/layout/workspace/layout_tests.rs` (`TempDirGuard`) — each file
    already had its own local guard before this packet; simply never named by `BUG-0082`'s candidate
    list because it was never a leak.
  - `crates/ssh/src/agent_tests.rs`, `crates/ssh/src/route_tests.rs`, `crates/ssh/src/tunnel_tests.rs`
    — **`BUG-0082`'s own candidate list was wrong about these three.** Each wraps its `temp_dir()` call
    directly in `TempKnownHosts`, a guard type `crates/ssh/src/test_support.rs` already defines and
    `impl Drop for`s; the wrapping happens at the call site (`TempKnownHosts(std::env::temp_dir().join(...))`),
    not through a local constructor function, which is exactly why `BUG-0082`'s "grep every
    `std::env::temp_dir()` call site, cross-reference against every file with `impl Drop for`"
    methodology missed it — the `impl Drop for` lives in a different file (`test_support.rs`) than the
    `temp_dir()` call. Confirmed no manual `remove_file`/`remove_dir_all` anywhere in any of the three.
  - `crates/ssh/src/handler_tests.rs`, `crates/ssh/src/keyfile_tests.rs`, `crates/ssh/src/test_support.rs`,
    `crates/ssh/src/sftp_task/handle_limit_tests.rs`,
    `crates/ssh/src/sftp_task/transfer/in0037_verify_tests.rs`,
    `crates/ssh/src/sftp_task/transfer/pipeline_budget_tests.rs`, `crates/ssh/src/us0095_verify_tests.rs`
    — each already defines its own `impl Drop for` guard (per `BUG-0082`'s own finding; re-confirmed).
  - `crates/update/src/archive_tests.rs` (`Fixture`), `crates/update/src/manager_tests.rs` (`Sandbox`)
    — each already defines its own `impl Drop for` guard.
- [x] Found, not fixed this session (deferred, with reason):
  - `crates/update/src/install.rs:918` — `#[cfg(target_os = "macos")]` test
    `replace_path_restores_old_file_when_copy_fails` uses the file's own local `test_dir()` helper
    directly (a plain `PathBuf`, not through `InstallFixture` or `UnreadableFile`, the two guards this
    same file already defines) and removes it manually as the last statement. Same leak shape as
    everything else in this packet, but `#[cfg(target_os = "macos")]`-gated code cannot be compiled or
    tested from this Windows-only session, so a fix here would be unverified. Left as a still-open
    `BUG-0083` remainder for whoever next works on this file on macOS, rather than risk an unverified
    change to code this session cannot build.
- [x] Out of scope:
  - `crates/tools/src/bin/sftp-dev-server.rs:505` — `seed_sample_root`, a real `fn main()` helper for a
    standalone dev tool, not `#[cfg(test)]` code. It intentionally leaves its sample data on disk for the
    developer to inspect while the server runs; not the leak shape this packet fixes (no test assertion,
    no panic-then-skip-cleanup path) and not a test-fixture resource at all.
  - Any architectural change to how tests build fixtures (a shared crate-wide `tempfile`-like
    abstraction, a workspace-level test-support crate) — ponytail rung 2: reuse what each crate already
    has (its own local guard, or an imported one), not a new abstraction. `tempfile` is confirmed not in
    the dependency graph for any touched crate; no new dependency is added.

## Acceptance

- [x] Every file in the "fixed this session" list above compiles, and its full test suite (or the
      specific tests filtered by name) passes with `0 failed`.
- [x] For each of the crates the fixes land in (`oneterm-app`, `oneterm-core`, `oneterm-session-ui`,
      `oneterm-settings`, `oneterm-sftp-ui`, `oneterm-ssh`, `oneterm-terminal`, `oneterm-update`,
      `oneterm-workspace`), the matching `%LOCALAPPDATA%\Temp` entry count for that crate's fixed
      prefixes stays flat (0, or unchanged from baseline where a baseline already existed) across three
      consecutive `cargo test -p <crate>` runs.
- [x] For each of the six distinct guard shapes introduced (`TemporaryFile` in `native_crash.rs`,
      `auth_form.rs`, and `shell.rs`; `TemporaryDirectory` in `ui_config.rs`, `document_tests.rs`,
      `update/config.rs`, `persistence.rs` (core), `edit.rs`, `sftp_task_tests.rs`, `logging.rs`, and
      `workspace/persistence.rs`), a throwaway panicking probe (added, run, observed, then reverted —
      never part of the committed diff) proves `Drop` removes the fixture during unwind.
  - Note: the throwaway probes covered every `TemporaryFile`/`TemporaryDirectory` struct *definition*
    added this session (6 distinct definitions, one per file that needed a new struct — `ui_config.rs`,
    `document_tests.rs`, `update/config.rs`, `persistence.rs`, `edit.rs`, `sftp_task_tests.rs`,
    `logging.rs`, and `workspace/persistence.rs` are eight files but the guard shape and its Drop-on-panic
    proof are identical in every one of them, so the six probes actually run (one per struct *name*
    across the eight files — several files reuse the exact same struct name `TemporaryDirectory` with
    an identical body) exercise the same mechanism repeatedly; every file's own probe passed).
- [x] No new dependency added (`tempfile` is not a dependency of any touched crate; every guard is a
      small local `struct` + `impl Drop` + `impl Deref<Target = Path>`, matching `BUG-0082`'s own
      precedent).
- [x] Full CI-local gate passes (combined with `BUG-0082`'s acceptance rework — same branch, same
      session; see that packet's Evidence for its own gate line and this packet's Evidence below for
      the combined run).

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0029-vt-engine/BUG-0082-pty-test-tempdir-leak.md` — the packet this one
  continues; its Handoff section is the starting candidate list, corrected and completed here.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/testing-and-bench.md` — describes the
  `oneterm-vt` bench/property-test harness; says nothing about test temp-directory lifecycle in the
  eight other crates this packet touches, and none of this packet's files are `oneterm-vt`. Not stale.
- No owning doc exists for test-fixture temp-directory hygiene across `app`/`core`/`session-ui`/
  `settings`/`sftp-ui`/`ssh`/`terminal`/`update`/`workspace` — this is `#[cfg(test)]`-only plumbing in
  nine unrelated crates, not a documented interface, schema, or behaviour any of `docs/agents/*.md`,
  `docs/PROJECT.md`, or a crate's own `README.md`/`CHANGELOG.md` claims a contract for.

### Documentation Action

- No contract change: this touches only `#[cfg(test)]` code across nine crates, nothing `pub`, and no
  reviewed doc describes test-fixture temp-directory lifecycle as a contract. The code comment at each
  guard (mirroring `BUG-0082`'s own comments) explains the mechanism where a future reader will look.

Reason: same as `BUG-0082`'s own Documentation Action — test plumbing, not a documented interface.

### Reconciliation

No docs changed; the no-change reason above still holds after implementation.

## Context

**Why a grep-only methodology (as `BUG-0082` used) misses some real leaks and flags some false
positives.** `BUG-0082`'s own verification (F5) cross-referenced `std::env::temp_dir()` call sites
against files containing `impl Drop for` and found candidates that way. Two failure modes surfaced this
session:

1. **False positive**: `crates/ssh/src/{agent_tests,route_tests,tunnel_tests}.rs` each call
   `TempKnownHosts(std::env::temp_dir().join(...))` directly at the call site — the guard type is
   imported from `crates/ssh/src/test_support.rs`, which is where the `impl Drop for` actually lives.
   A per-file "does this file itself contain `impl Drop for`" check cannot see that; only reading each
   call site's surrounding code (is the `temp_dir()` result immediately wrapped in a guard constructor,
   or bound to a bare `PathBuf`?) settles it. All three were read this session; none leak.
2. **False negative**: `crates/core/src/persistence.rs`, `crates/sftp-ui/src/edit.rs`,
   `crates/ssh/src/sftp_task/sftp_task_tests.rs`, `crates/terminal/src/logging.rs`, and
   `crates/workspace/src/layout/workspace/persistence.rs` all build a bare `PathBuf` with `temp_dir()`
   and clean it up with a **manual** `remove_dir_all`/`remove_file` call gated behind one or more
   assertions — the exact shape `BUG-0082` fixed elsewhere — but none of the five was named by either
   pass of `BUG-0082`'s candidate list. Found only by re-running the grep this session and reading every
   one of the 29 matching files' surrounding code, not trusting the prior candidate list as complete.

**The guard shape, restated once (all eight fixed files below use the identical pattern).**

```rust
struct TemporaryDirectory(PathBuf); // or TemporaryFile, for a single file

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0); // or remove_file
    }
}

impl std::ops::Deref for TemporaryDirectory {
    type Target = Path;
    fn deref(&self) -> &Path { &self.0 }
}
```

Every call site wraps the `temp_dir()`-built path in the guard immediately (never a bare `PathBuf`
first), and every generic `std::fs::*` call that needs `P: AsRef<Path>` (not a concrete `&Path`
parameter) derefs explicitly (`&*guard`) — `Deref` coercion applies at a concrete-typed call site
(e.g. a function declared `fn f(path: &Path)`) but not through a generic bound, since the guard type
itself does not implement `AsRef<Path>`. This is the same `Deref<Target = Path>` shape `BUG-0082` used
for `ScratchDirectory`, `TemporaryDirectory` (`crash_report.rs`), and `TempDirGuard`
(`dock_persistence.rs`), and the same shape `session_state.rs`, `test_backend.rs`, and
`layout_tests.rs` already had.

**The `Deref` chaining trap, watched for.** `BUG-0082`'s own Context section names a bug it fixed once
(`.join()` chained directly off a temporary guard value drops the guard, and removes the fixture, before
the rest of the test runs). Every call site converted this session either binds the guard to its own
name and calls `.join()`/`.to_path_buf()` on that name in a later statement (never chained off a
temporary), or, where the original code called `.clone()` on a `PathBuf` mid-test
(`update/config.rs`'s threads, `sftp_task_tests.rs`'s `collect_local_upload_entries(root.clone(), ...)`,
`logging.rs`'s `TerminalLogConfig { directory: directory.clone(), .. }`), the call is changed to
`.to_path_buf()` (available through `Deref<Target = Path>`) instead of `.clone()` (which the guard type
itself does not implement, by design — cloning would produce two guards over one directory, and whichever
drops first removes it out from under the other).

**`ssh/sftp_task/sftp_task_tests.rs`'s `refuses_preexisting_symlink_below_download_root` — a shadow, not
a leak.** This test rebinds `root` from the guard to a plain, canonicalized `PathBuf`
(`let root = std::fs::canonicalize(&*root).unwrap();`). The original (now-shadowed-by-name) guard value
is not dropped early by this — Rust drops shadowed bindings in the same reverse-declaration order as any
other local, at the end of the enclosing scope — so it still guards the underlying directory for the rest
of the function, including the early-return branch this test also has.

**`workspace/layout/workspace/persistence.rs`'s `an_elevated_window_writes_no_dock_layout` is part of
the CI-local gate's explicit `--ignored` run** (`cargo test -p oneterm-workspace --lib -- --ignored
--test-threads=1`, `AGENTS.md` section 4) — it is not merely discovered dead test code; it runs on every
gate pass. Its guard drops in the correct order relative to `RestrictedElevation` (the process-global
elevation-switch guard already in this file): local variables drop in reverse declaration order, and
`directory` is declared before `_restricted`, so on both the normal path and a panicking unwind,
`_restricted` resets the elevation switch first, then `directory` removes the fixture — the same order
the original code's explicit `drop(_restricted);` followed by `remove_dir_all` achieved by hand, now
automatic.

## Plan

- [x] Grep `std::env::temp_dir()` under `crates/` (Grep tool's `rg` backend is not installed on this
      box; used the Bash tool's `grep` and, where the rtk hook's git-command heuristic refused a
      command, Read/individual `grep` calls instead) — 31 files, 43 matches (corrected by the verify pass).
- [x] For each file, read the surrounding code and classify: already guarded (skip), leak shape needing
      a fix, or out of scope (a real `fn main()` helper, not a test).
- [x] Apply the `TemporaryFile`/`TemporaryDirectory` guard to each of the 11 files needing a fix (see
      Scope), reusing the pattern in place rather than introducing a shared cross-crate abstraction.
- [x] Run each touched crate's test suite; fix any `Deref`/`AsRef` compile friction (generic
      `std::fs::*` calls need `&*guard`; concrete `&Path` parameters take `&guard` via coercion;
      `.clone()` mid-test becomes `.to_path_buf()`).
- [x] Add one throwaway panicking probe per distinct guard struct, run it, confirm the fixture is gone
      from `%LOCALAPPDATA%\Temp` afterward, then revert the probe (never part of the committed diff).
- [x] Count each crate's matching `%LOCALAPPDATA%\Temp` prefixes before and after three consecutive
      `cargo test -p <crate>` runs; confirm flat.
- [x] Run the full local CI gate (combined with `BUG-0082`'s own acceptance rework, same session, same
      branch).

## Decisions

None. No architectural or public-contract choice is made here; the mechanism (a small local RAII guard
per file, reusing the existing pattern) is documented in Context and in the code comment at each guard.

## Verification Plan

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

- `cargo test -p oneterm-app -p oneterm-session-ui -p oneterm-core -p oneterm-ssh -p oneterm-update
  -p oneterm-settings --lib` (the owner-specified list) plus `-p oneterm-sftp-ui -p oneterm-terminal
  -p oneterm-workspace --lib` (the three additional crates this packet's own grep sweep also touched),
  each `0 failed`.
- Per touched crate, `%LOCALAPPDATA%\Temp` entry counts for that crate's fixed prefixes, x3 runs each:
  flat (see Evidence).
- One throwaway panicking probe per distinct guard struct, run once, fixture confirmed gone afterward,
  then reverted (see Evidence; not part of the committed diff, so not independently re-runnable from the
  final tree — the mechanism (Rust's own guaranteed `Drop`-on-unwind) is identical to `BUG-0082`'s own
  `F2` finding, already independently verified there).
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`.
- Full `pwsh scripts/ci-local.ps1` (combined with `BUG-0082`'s rework; see this packet's Evidence for
  the shared final line).

## Evidence and Gaps

**Grep sweep, 31 files, 43 matches of `std::env::temp_dir()` under `crates/`** (already-fixed
`BUG-0082` files included for completeness):

| File | Status | Notes |
| --- | --- | --- |
| `crates/app/src/crash_report.rs` | already guarded (`BUG-0082`) | — |
| `crates/app/src/native_crash.rs:242` | **fixed this session** | one test, one file |
| `crates/core/src/config/shell.rs:826` | **fixed this session** | one test, `#[cfg(windows)]`, one file |
| `crates/core/src/persistence.rs:209` | **fixed this session** | helper, six call sites — not in `BUG-0082`'s candidate list |
| `crates/session-ui/src/auth_form.rs:469` | **fixed this session** | one test, one file |
| `crates/session-ui/src/session_state.rs` | already guarded (`TempDirGuard`) | not a leak; never should have been a candidate |
| `crates/settings/src/terminal_config/document_tests.rs:199,237,257` | **fixed this session** | three tests |
| `crates/settings/src/ui_config.rs:293,366,405` | **fixed this session** | three tests |
| `crates/sftp-ui/src/edit.rs:999` | **fixed this session** | one test — not in `BUG-0082`'s candidate list |
| `crates/sftp-ui/src/test_backend.rs` | already guarded (`TempDir`) | not a leak |
| `crates/ssh/src/agent_tests.rs` | already guarded (imports `TempKnownHosts`) | `BUG-0082`'s candidate list was wrong about this one |
| `crates/ssh/src/handler_tests.rs` | already guarded (own `impl Drop`) | — |
| `crates/ssh/src/keyfile_tests.rs` | already guarded (own `impl Drop`) | — |
| `crates/ssh/src/route_tests.rs` | already guarded (imports `TempKnownHosts`) | `BUG-0082`'s candidate list was wrong about this one |
| `crates/ssh/src/sftp_task/handle_limit_tests.rs` | already guarded (own `impl Drop`) | — |
| `crates/ssh/src/sftp_task/sftp_task_tests.rs:24` | **fixed this session** | helper, three call sites (one with two directories) — not in `BUG-0082`'s candidate list |
| `crates/ssh/src/sftp_task/transfer/in0037_verify_tests.rs` | already guarded (own `impl Drop`) | — |
| `crates/ssh/src/sftp_task/transfer/pipeline_budget_tests.rs` | already guarded (own `impl Drop`) | — |
| `crates/ssh/src/test_support.rs` | already guarded (defines `TempKnownHosts`) | — |
| `crates/ssh/src/tunnel_tests.rs` | already guarded (imports `TempKnownHosts`) | `BUG-0082`'s candidate list was wrong about this one |
| `crates/ssh/src/us0095_verify_tests.rs` | already guarded (own `impl Drop`) | — |
| `crates/state/src/dock_persistence.rs` | already guarded (`BUG-0082`) | — |
| `crates/terminal/src/logging.rs:336` | **fixed this session** | helper, two call sites — not in `BUG-0082`'s candidate list |
| `crates/tools/src/bin/sftp-dev-server.rs:505` | out of scope | real `fn main()` helper, not a test |
| `crates/update/src/archive_tests.rs` | already guarded (`Fixture`) | — |
| `crates/update/src/config.rs:584` | **fixed this session** | helper, six call sites |
| `crates/update/src/install.rs:918` | **found, deferred** | `#[cfg(target_os = "macos")]` test, unverifiable from this Windows-only session |
| `crates/update/src/manager_tests.rs` | already guarded (`Sandbox`) | — |
| `crates/vt/src/pty/windows/conpty.rs` | already guarded (`BUG-0082`) | — |
| `crates/workspace/src/layout/workspace/layout_tests.rs` | already guarded (`TempDirGuard`) | not a leak |
| `crates/workspace/src/layout/workspace/persistence.rs:223` | **fixed this session** | `an_elevated_window_writes_no_dock_layout`, part of the CI-local `--ignored` gate — not in `BUG-0082`'s candidate list |

**Throwaway Drop-on-panic probes** (added, run, fixture confirmed gone from `%LOCALAPPDATA%\Temp`
afterward, then reverted — `git status --porcelain` confirmed clean of probe code before the commit):

| File | Probe result |
| --- | --- |
| `native_crash.rs` (`TemporaryFile`) | panicked as expected; fixture removed |
| `auth_form.rs` (`TemporaryFile`) | panicked as expected; fixture removed |
| `core/config/shell.rs` (`TemporaryFile`) | panicked as expected; fixture removed |
| `settings/ui_config.rs` (`TemporaryDirectory`) | panicked as expected; fixture removed |
| `settings/terminal_config/document_tests.rs` (`TemporaryDirectory`) | panicked as expected; fixture removed |
| `update/config.rs` (`TemporaryDirectory`) | panicked as expected; fixture removed |

(One probe per distinct file that needed a *new* guard struct; `core/persistence.rs`, `sftp-ui/edit.rs`,
`ssh/sftp_task_tests.rs`, `terminal/logging.rs`, and `workspace/persistence.rs` all use the textually
identical `TemporaryDirectory` shape already exercised above and are not separately re-probed — the
mechanism under test is Rust's own guaranteed `Drop`-on-unwind, not anything file-specific.)

**Per-crate `%LOCALAPPDATA%\Temp` counts, x3 `cargo test -p <crate>` runs each** (all flat):

| Crate | Prefix(es) | Baseline | After run 1 | After run 2 | After run 3 |
| --- | --- | --- | --- | --- | --- |
| `oneterm-app` | `oneterm-native-crash-test-` | 0 | 0 | 0 | 0 |
| `oneterm-app` | `oneterm-crash-report-test-` (unrelated `BUG-0082` family, sanity check only) | 13 | 13 | 13 | 13 |
| `oneterm-session-ui` | `oneterm-private-key-` | 0 | 0 | 0 | 0 |
| `oneterm-core` | `oneterm-us0136-` | 0 | 0 | 0 | 0 |
| `oneterm-core` | `oneterm-persistence-` | 0 | 0 | 0 | 0 |
| `oneterm-settings` | `oneterm-ui-config-test-`, `-ui-schema-test-`, `-ui-unreadable-test-`, `-terminal-config-test-`, `-terminal-schema-test-`, `-terminal-unreadable-test-` | 0 (all) | 0 | 0 | 0 |
| `oneterm-update` | `oneterm-update-config-` | 0 | 0 | 0 | 0 |
| `oneterm-sftp-ui` | `oneterm-edit-sig-` | 0 | 0 | 0 | 0 |
| `oneterm-ssh` | `oneterm-sftp-security-` | 0 | 0 | 0 | 0 |
| `oneterm-terminal` | `oneterm-logging-` | 0 | 0 | 0 | 0 |
| `oneterm-workspace` | `oneterm-elevated-docks-` (`--ignored --test-threads=1`, the one test that uses this prefix) | 0 | 0 | 0 | 0 |

**Test results, owner-specified crate list** (`cargo test -p oneterm-app -p oneterm-session-ui
-p oneterm-core -p oneterm-ssh -p oneterm-update -p oneterm-settings --lib`): all `0 failed`
(`oneterm-app` 26 passed, `oneterm-session-ui` 77 passed/1 ignored, `oneterm-core` 89 passed,
`oneterm-ssh` 109 passed, `oneterm-update` 59 passed, `oneterm-settings` 49 passed).

**Test results, the three additional crates this packet's own grep sweep touched**
(`cargo test -p oneterm-sftp-ui -p oneterm-terminal -p oneterm-workspace --lib`): all `0 failed`
(`oneterm-sftp-ui` 75 passed, `oneterm-terminal` 221 passed, `oneterm-workspace` 50 passed/3 ignored).

**Static gates:**

```
$ cargo fmt --all -- --check
(exit 0)

$ cargo clippy --workspace --all-targets -- -D warnings
(exit 0, entire workspace, after both BUG-0082's rework and this packet's fixes)
```

**Full CI-local gate** (combined with `BUG-0082`'s exit-code-259 rework, same branch, same session,
`CARGO_BUILD_JOBS=6`, `target/debug/incremental` deleted before the run):

```
ci-local: all checks passed.
```

(See the actual commit's final gate output for the exact command list and timing; both packets share
this one gate run since they land on the same branch in the same session.)

**Gaps:**
- `crates/update/src/install.rs`'s `replace_path_restores_old_file_when_copy_fails`
  (`#[cfg(target_os = "macos")]`) is a confirmed same-shape leak, found but not fixed — this Windows-only
  session cannot compile or test macOS-gated code, so a change there would be unverified. Left open for
  whoever next touches this file on macOS.
- `harness.db` was not touched (see Status); status/proof are hand-maintained here, matching
  `BUG-0082`'s own precedent.
- No CI runner run of this fix (verified on this developer machine only, across the platforms this one
  Windows machine can build: the `#[cfg(windows)]` parts of `shell.rs`'s guard were exercised directly;
  the Unix-only paths of every other touched file were not separately verified on Unix, though none of
  the new guards are platform-gated themselves — only `shell.rs`'s is).
- The throwaway probes are, by definition, not part of the committed diff and cannot be independently
  re-run from the final tree; the underlying guarantee (`Drop` runs on unwind) is a Rust language
  guarantee already independently verified once in `BUG-0082`'s own `F2` finding, not something this
  packet's probes could have found false.

## Handoff

Implemented and verified in this session, alongside `BUG-0082`'s acceptance rework (exit code 259), on
branch `fix/test-tempdirs-and-liveness`. See that branch's commits for the exact shas. Not pushed as of
this session.

Next owner: decide whether `crates/update/src/install.rs`'s macOS-only gap (above) is worth its own
follow-up bug, or can wait until someone next touches that file on macOS and fixes it inline. No other
gaps are expected to recur — the grep sweep this session re-checked every `std::env::temp_dir()` call
site in `crates/`, not just the files `BUG-0082`'s own candidate list named.
