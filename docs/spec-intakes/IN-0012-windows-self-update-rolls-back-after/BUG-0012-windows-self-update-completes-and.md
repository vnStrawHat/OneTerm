# Work: Windows self-update completes and relaunches

ID: BUG-0012
Intake: IN-0012
Created: 2026-08-20

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

- Change type: bug
- Risk lane: normal
- Spec Intake, when required: IN-0012

## Outcome

On Windows, an in-app update replaces the installed binaries and relaunches
OneTerm on the new version, instead of quitting, silently rolling back, and
reopening the old version.

## Scope

- [x] In scope: Windows install path in `crates/update/src/install.rs` —
  terminate stale `OpenConsole.exe` holding install-dir file locks before quit;
  harden the generated helper `.cmd` (`/R` on every `xcopy`, reset errorlevel
  before `start`).
- [x] Out of scope: the check/download/stage flow; macOS and Linux install
  paths; the release archive format; logging content (owned by US-0024).

## Acceptance

- [x] A lingering `OpenConsole.exe` whose image is inside the install directory
  is terminated before the app quits, so the helper can overwrite the binaries.
- [x] Termination never matches by bare process name — only by resolved full
  image path under the install directory (see DEC-0005), so other apps'
  `OpenConsole.exe` (e.g. Windows Terminal) is left running.
- [x] `xcopy` overwrites read-only installed files (uses `/R`); a read-only
  `conpty.dll`/`oneterm.exe` no longer forces a rollback.
- [x] A non-fatal errorlevel left by `xcopy` cannot make the post-`start`
  `if errorlevel 1` roll back a launch that actually succeeded.
- [x] On success the new binary launches and the backup + staging dirs are
  removed; on failure the previous install is restored.

## Documentation

### Owning Docs Reviewed

- `crates/update/src/install.rs` — `install_staged_update`,
  `schedule_windows_update`, and the `windows_update_script_body` helper are the
  owning contract for Windows install/relaunch behavior.
- `docs/decisions/DEC-0005-*` — console-host termination policy (created with
  this work).

### Documentation Action

- No separate product contract doc exists for the updater; behavior is owned by
  the code and its unit tests. The consequential choice (how to scope process
  termination) is recorded in DEC-0005, and the mechanics are captured in the
  IN-0012 HLD.

Reason: the updater has no external/product contract surface to update beyond
the code; the durable rationale belongs in a decision record, which was created.

### Reconciliation

- Added: `docs/decisions/DEC-0005-*`, IN-0012 HLD data flow. Code comments in
  `install.rs` (CORR-60 / CORR-61) document the two fixes inline. No stale doc
  remained.

## Context

- The terminal backend (`vendor/alacritty_terminal/src/tty/windows/conpty.rs`)
  loads `conpty.dll` and launches `OpenConsole.exe` from the executable's
  directory. A console host that outlives OneTerm keeps handles on exactly the
  files the updater must overwrite.
- Windows cannot overwrite a file that another process holds open, which is why
  the helper's `xcopy` failed and rolled back until the host was killed.

## Plan

- [x] Enumerate processes via ToolHelp; for each `OpenConsole.exe`, resolve the
  full image path and `TerminateProcess` when it is under the install directory.
- [x] Add `Win32_System_Diagnostics_ToolHelp` to the workspace `windows-sys`
  features and a `cfg(windows)` dependency in `crates/update/Cargo.toml`.
- [x] Add `/R` to all three `xcopy` calls; insert `(call )` before `start`.
- [x] Regression tests on the generated script and the process-name decode.

## Decisions

- `DEC-0005` — Terminate only OneTerm's own `OpenConsole.exe` before a Windows
  update (full-path match, never process name).

## Verification Plan

Focused unit tests on the generated helper script and the process-name decode;
a manual end-to-end run of the helper against a read-only install directory with
a real executable; workspace fmt/clippy/build.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- `cargo test -p oneterm-update` — 58 passed (script asserts `/R` present,
  errorlevel reset ordering; `process_entry_name` decode).
- `cargo clippy -p oneterm-update --all-targets -- -D warnings` — clean.
- `cargo fmt -p oneterm-update` — clean; `cargo build -p oneterm-app` — ok.
- Manual helper run (git-bash driving `cmd`) against a read-only install dir
  with a real exe as the new build: `oneterm.exe` and `conpty.dll` replaced,
  `x64/OpenConsole.exe` replaced, backup + staging removed. The trailing
  "batch file cannot be found / exit=1" is the script deleting itself and is
  benign (the file work completed first).
- Commits: `b9e7d06` (script hardening), `214b333` (console-host termination —
  root cause).
- Gap: full in-app Windows E2E with a naturally lingering `OpenConsole.exe` was
  reproduced/confirmed by the reporting user in the field, not re-run in CI.
  The fix only takes effect from a build that ships it; the on-disk 0.4.0 helper
  is unchanged, so the first hop to a fixed build may still need a manual install.

## Handoff

Complete. Next update from any build carrying this fix should self-heal.
