# Work: Write a full Windows update log

ID: US-0024
Intake: IN-0012
Created: 2026-08-20

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [ ] Implemented
- [x] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: maintenance
- Risk lane: normal
- Spec Intake, when required: IN-0012

## Outcome

A Windows update writes a single, timestamped log covering both the Rust
pre-quit phase and the detached helper's post-quit phase, so a failed or
rolled-back update can be diagnosed from one file without a console.

## Scope

- [x] In scope: `crates/update/src/install.rs` — a stable
  `<config>/updates/update.log`; Rust appends the scheduled version, dirs, and
  each terminated console host; the helper `.cmd` logs every step, redirects
  `xcopy` output to the log, and records errorlevel on each abort/rollback.
- [x] Out of scope: the app's `env_logger` console/stderr logging; terminal
  session logs (`~/.OneTerm/logs`); macOS/Linux install paths; log rotation.

## Acceptance

- [x] The log path is stable and discoverable: `<config>/updates/update.log`
  (not per-PID), and survives the helper's success cleanup (which removes only
  the staging and backup dirs).
- [x] Pre-quit (Rust) entries include the scheduled version + install/package
  dirs, each `OpenConsole.exe` terminated (with its path), and helper
  launch/spawn-failure.
- [x] Post-quit (helper) entries include a timestamped line per step and the
  full `xcopy` output (redirected to the log instead of `NUL`).
- [x] Each abort/rollback branch logs the observed `errorlevel`.
- [x] Logging never blocks or fails the update: append failures are ignored.

## Documentation

### Owning Docs Reviewed

- `crates/update/src/install.rs` — owning contract for the Windows install path
  and the generated helper script.
- `crates/core/src/terminal_logging.rs` — confirmed the update log is distinct
  from terminal session logs (`~/.OneTerm/logs`); reused `update_cache_dir()`
  under `oneterm_core::config_dir()` for co-location with update artifacts.

### Documentation Action

- No contract change. The updater has no external/product contract doc; the log
  is an internal debugging artifact. Behavior is captured by the code, its unit
  test, and the IN-0012 HLD data flow.

Reason: adding a debug log introduces no new user-facing or cross-crate contract.

### Reconciliation

- No owning doc required changes. IN-0012 HLD data flow already describes the
  log; the no-change reason above remains valid.

## Context

- The helper runs detached with `CREATE_NO_WINDOW`, and release builds have no
  console at all, so `log::info!`/`eprintln!` output is lost during an update.
  A file written by both phases is the only durable record.
- Batch specifics: `>>"%log%"` leads each `echo` so no trailing space is
  captured; a `:log` subroutine keeps step lines uniform; `xcopy` output is
  redirected with `>>"%log%" 2>&1`.

## Plan

- [x] Add `windows_update_log_path()` and `append_update_log()` (Rust).
- [x] Pass `ONETERM_UPDATE_LOG` to the helper environment.
- [x] Rewrite `windows_update_script_body` to log each step + capture xcopy.
- [x] Add a regression test asserting the script logs steps and captures xcopy.

## Decisions

- Depends on the console-host termination introduced with `DEC-0005` (the kills
  are among the events logged), but adds no new decision of its own.

## Verification Plan

Unit test on the generated script (log wiring + captured xcopy); a manual
end-to-end helper run confirming a complete `update.log`; workspace
fmt/clippy/build.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- `cargo test -p oneterm-update` — 59 passed (adds
  `windows_update_script_logs_every_step_to_the_update_log`).
- `cargo clippy -p oneterm-update --all-targets -- -D warnings` — clean;
  `cargo fmt` clean; `cargo build -p oneterm-app` — ok.
- Manual helper run produced a full `update.log`: `helper started` with dirs,
  `waiting for pid`, `OneTerm exited`, both `xcopy` listings with
  "3 File(s) copied", `launching the new build`, `update complete; cleaning
  up`. Verified on a read-only install dir with a real exe.
- Commit: `1290e97`.
- Gap: log path/content confirmed via the extracted helper script and the Rust
  unit test; not yet observed from a packaged release build in the field.

## Handoff

Complete.
