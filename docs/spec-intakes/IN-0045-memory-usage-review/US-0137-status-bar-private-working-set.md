# Work: Status bar MEM shows the private working set

ID: US-0137
Intake: IN-0045
Created: 2026-09-25

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: existing-contract change
- Risk lane: normal (one status bar widget; no persisted data, no public contract)
- Spec Intake, when required: [`IN-0045`](IN-0045.md)

## Outcome

Owner ruling, 2026-09-25: the status bar `MEM` value is the number Task Manager's
"Memory" column shows, the private working set. Today it shows commit (`PrivateUsage`):
182 MB for one idle tab where Task Manager shows 49 MB
([`research/memory-attribution.md`](research/memory-attribution.md) section 1).

## Scope

- [x] In scope:
  - `crates/workspace/src/widgets/resource.rs`: on Windows, read
    `PROCESS_MEMORY_COUNTERS_EX2::PrivateWorkingSetSize` through `GetProcessMemoryInfo`.
    If the call fails (an older Windows may reject the larger `cb`) or the field is 0
    (it is not filled before Windows 10 22H2 or Windows 11 22H2 with the September 2023 cumulative update), use the working set (`sysinfo`
    `Process::memory()` = `WorkingSetSize`). Consequence: such a Windows shows the full
    working set, about 136 MB idle instead of 51.
  - On Linux and macOS, show `sysinfo` `Process::memory()`, the resident set size (RSS).
    That is the closest "resident" figure the OS gives. It includes shared pages, so it
    reads higher than a private figure would.
  - The owning doc: `docs/gui-layout.md` § Status bar names the figure.
- [x] Out of scope:
  - The item's format (`CPU 1.2%  MEM 49.3 MB`) and its 2 s cadence: unchanged.
  - A tooltip that also shows commit. Decided against (see Decisions).
  - Any change to what the process actually uses (`BUG-0078`, `DEC-0020`).

## Acceptance

- [x] On Windows the status bar `MEM` agrees with the private working set of the same pid
  (`\Process(...)\Working Set - Private`, Task Manager "Memory") within a few MB.
- [x] If the private working set is unavailable, the item shows the working set instead of
  nothing.
- [x] Format and cadence unchanged; the value keeps its unit (`US-0112`).
- [x] `cfg(windows)` sits on the FFI call only; the selection is platform-independent and
  unit-tested with injected values on every OS.
- [x] `docs/gui-layout.md` says which figure `MEM` is.

## Documentation

### Owning Docs Reviewed

- `docs/gui-layout.md` § Status bar: lists the "CPU/memory indicator" without saying which
  memory figure. Update required.
- `docs/spec-intakes/IN-0045-memory-usage-review/IN-0045.md` and
  `research/memory-attribution.md` section 1: which counter the owner read, and the three
  Win32 counters.
- `docs/agents/code-style.md`: `#[cfg(windows)]` on the FFI call, `SAFETY` comments on
  `unsafe`.
- `docs/agents/crate-dependency-rules.md`: nothing restricts `windows-sys` in
  `oneterm-workspace` (R7 is about the engines).
- The packet that introduced the item: commit `d6270811` (no packet; it chose
  `virtual_memory()` as "closer to Task Manager").
- `US-0112` (IN-0042): the value and its unit are one token, `Shorten::Never`. Unchanged.

### Documentation Action

- Update required: `docs/gui-layout.md` § Status bar; the module docs of `resource.rs`;
  the `US-0137` line and the open question in `IN-0045.md`.

Reason: the figure changes meaning, and nothing said which figure it was.

### Reconciliation

Changed: `docs/gui-layout.md` § Status bar (names the figure per platform), the module
docs of `crates/workspace/src/widgets/resource.rs`, `IN-0045.md` (the `US-0137` line and
the owner question are closed).

## Context

- `sysinfo` 0.37.2 reads `PROCESS_MEMORY_COUNTERS_EX` on Windows: `memory()` =
  `WorkingSetSize`, `virtual_memory()` = `PrivateUsage`. It does not expose
  `PrivateWorkingSetSize`, so a small FFI call is needed.
- `windows-sys` 0.59 is already a workspace dependency; this adds its
  `Win32_System_ProcessStatus` feature (no new crate; `Cargo.lock` gains one line: `oneterm-workspace` depends on
  `windows-sys` 0.59.0).

## Plan

- [x] Packet (this file).
- [x] `resource.rs`: `private_working_set()` (FFI, `None` off Windows) and a pure
  `displayed_memory(private_ws, resident)`.
- [x] Unit tests for `displayed_memory`.
- [x] Docs.
- [x] Release run with a private profile; compare with the OS counters.
- [x] Gates, commit.

## Decisions

- No tooltip with commit. The ruling asks for one number; commit stays one click away in
  Task Manager ("Commit size") and in `research/measure.ps1`. `StatusText` has no custom
  tooltip today, so a tooltip would widen its API for a figure nobody asked to keep. Add
  it if the owner asks.
- No decision record: this is one widget's display choice, recorded here.

## Verification Plan

- `cargo test -p oneterm-workspace` (the new selection tests and the existing
  `format_memory` test).
- Release build, run with a private `USERPROFILE`, read the status bar and compare with
  `Get-Process -Id <own pid>` `WorkingSet64` / `PrivateMemorySize64` and the
  `Working Set - Private` counter for the same pid.
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `python scripts/check-doc-paths.py`, `python scripts/check-english.py`, then the full
  `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- Unit: `cargo test -p oneterm-workspace --lib resource`: 3 passed
  (`displayed_memory_prefers_the_private_working_set`,
  `displayed_memory_falls_back_to_resident_when_unavailable`, the existing
  `format_memory` test).
- Platform (Windows 11 26200, release build of this branch, private `USERPROFILE`,
  1280x800, one cmd tab, own pid 31740 only). The status bar was read from a
  `PrintWindow` capture; the counters were read for the same pid at the same moment:

  | Scene | Status bar `MEM` | `Working Set - Private` (perf counter) | EX2 `PrivateWorkingSetSize` | `Get-Process` `WorkingSet64` | `Get-Process` `PrivateMemorySize64` (commit, the old figure) |
  | --- | --- | --- | --- | --- | --- |
  | idle 30 s | 51.4 MB | 51.6 MB | 51.6 MB | 137.7 MB | 184.2 MB |
  | after 300,000 lines of output | 85.5 MB | 85.4 MB | 85.0 MB | 171.7 MB | 200.3 MB |

  The status bar agrees with the private working set within 0.5 MB (it samples every
  2 s). Before this change it showed commit, the last column.
- The Windows fallback (before Windows 10 22H2 or Windows 11 22H2 with the September 2023 cumulative update: the call fails on the larger `cb`, or the EX2
  field is left at 0; the item then shows the full working set, about 136 MB idle instead
  of 51) is proven by the unit test only; no such Windows was available.
- Linux and macOS: not run. They show `sysinfo` `memory()` (RSS), which is what the
  widget's resident fallback is; the selection test covers that path on every CI runner.
- Integration proof: not applicable (one widget, no cross-crate flow).
- Gates: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `python scripts/check-doc-paths.py`, `python scripts/check-english.py`, then
  the full `pwsh scripts/ci-local.ps1` with `CARGO_BUILD_JOBS=4`:
  `ci-local: all checks passed.` The run used a shared `CARGO_TARGET_DIR`;
  `vt-public-api.py` reads `<repo>/target/doc` directly, so a temporary junction from the
  worktree's `target` to that directory was needed and then removed.
