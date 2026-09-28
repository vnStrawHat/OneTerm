# Work: CPU/MEM item shows a resource table on hover

ID: US-0148
Intake: IN-0047
Created: 2026-09-28

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [ ] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability
- Risk lane: normal (one status bar widget; no persisted data, no public contract)
- Spec Intake, when required: [`IN-0047`](IN-0047.md)

## Outcome

Hovering the status bar's CPU/MEM item shows the kit tooltip with a two-column table of
what the OneTerm process uses, from the same 2 s sample as the item
([`high-level-design.md`](high-level-design.md)).

## Scope

- [x] In scope:
  - `crates/workspace/src/widgets/status_text.rs`: a `Label` may carry detail sections;
    when it does, hovering shows them as a table in the kit tooltip.
  - `crates/workspace/src/widgets/resource.rs`: the sample keeps the extra figures; a pure
    function builds the rows; Windows thread count from a process snapshot.
  - `docs/gui-layout.md` § Status bar.
- [x] Out of scope (follow-up candidates in the intake): the shells line, user vs kernel
  split, a tooltip that refreshes while open. The item's text and cadence are unchanged.

## Acceptance

- [ ] Hovering the item shows, in this order: Memory: Private working set, Working set,
  Commit (private bytes), Peak working set (Windows) or Resident (RSS), Virtual size
  (Linux, macOS); CPU: Usage (with the logical core count), CPU time (user + kernel),
  Threads, Uptime.
- [ ] No new timer; nothing is read per frame or on hover; the extra figures come from the
  existing 2 s sample.
- [ ] Colours from the theme only; `python scripts/check-theme-contrast.py` passes.
- [ ] The item's text is unchanged (`CPU 1.2%  MEM 49.3 MB`).
- [ ] Unit tests with injected values: formatting, and the table lists every field in a
  fixed order.
- [ ] Windows run: screenshot of the hovered tooltip; private WS, WS and commit agree with
  the OS counters for the same pid within a few MB; thread count exact.

## Documentation

### Owning Docs Reviewed

- `docs/gui-layout.md` § Status bar: describes the item and `MEM`; no tooltip. Update
  required.
- `docs/spec-intakes/IN-0045-memory-usage-review/US-0137-status-bar-private-working-set.md`:
  decided "no tooltip" because `StatusText` had none; superseded by the owner's request.
- `docs/spec-intakes/IN-0045-memory-usage-review/research/memory-attribution.md` § 1: which
  Win32 field is which Task Manager column.
- `scripts/check-theme-contrast.py` `SURFACES`: `popover.foreground` and
  `muted.foreground` on `popover.background` are listed (the kit tooltip's fill and text).
- `docs/agents/code-style.md`: `#[cfg(windows)]` on the FFI call only; `SAFETY` comments.
- Kit tooltip: `reference/gpui-kit/crates/component/src/tooltip.rs` (`Tooltip::element`).

### Documentation Action

- Update required: `docs/gui-layout.md` § Status bar; the module docs of `resource.rs`.

Reason: new visible behaviour of the item.

### Reconciliation

To fill at completion.

## Context

- `sysinfo` 0.37.2 on Windows: `memory()` = `WorkingSetSize`, `virtual_memory()` =
  `PrivateUsage`, `accumulated_cpu_time()` = user + kernel (ms), `run_time()` (s);
  `tasks()` is Linux-only and already refreshed (`ProcessRefreshKind::nothing()` keeps
  tasks on). It has no thread count on Windows.
- `sysinfo` already takes a `TH32CS_SNAPPROCESS` snapshot on every refresh; the thread
  count walks one more of the same kind.

## Plan

- [x] Records (intake, HLD, this packet).
- [ ] `StatusText`: detail sections on `Label`, tooltip table.
- [ ] `resource.rs`: sample struct, rows, Windows peak WS and thread count.
- [ ] Tests, docs, manual run, gates, commit.

## Decisions

- No decision record: one widget's display, recorded here.

## Verification Plan

- `cargo test -p oneterm-workspace` (new row tests).
- Fast-dev or release run with a private `USERPROFILE`; hover the item; `PrintWindow`
  capture of the own hwnd into `evidence/`; compare with `Get-Process -Id <own pid>` and
  the `Working Set - Private` counter.
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `python scripts/check-theme-contrast.py`, `python scripts/check-doc-paths.py`,
  `python scripts/check-english.py`, then the full `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

To fill at completion.
