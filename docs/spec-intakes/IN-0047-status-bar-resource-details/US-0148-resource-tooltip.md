# Work: CPU/MEM item shows a resource table on hover

ID: US-0148
Intake: IN-0047
Created: 2026-09-28

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
- [ ] Changed
- [x] Reopened (acceptance rework)
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
  split. The item's text and cadence are unchanged. (A tooltip that refreshes while open
  was originally out of scope too — see the 2026-09-28 acceptance rework below, which
  brought it in.)

## Acceptance

- [x] Hovering the item shows, in this order: Memory: Private working set, Working set,
  Commit (private bytes), Peak working set (Windows) or Resident (RSS), Virtual size
  (Linux, macOS); CPU: Usage (with the logical core count), CPU time (user + kernel),
  Threads, Uptime.
- [x] No new timer; nothing is read per frame or on hover; the extra figures come from the
  existing 2 s sample.
- [x] Colours from the theme only; `python scripts/check-theme-contrast.py` passes.
- [x] The item's text is unchanged (`CPU 1.2%  MEM 49.3 MB`).
- [x] Unit tests with injected values: formatting, and the table lists every field in a
  fixed order.
- [x] Windows run: screenshot of the hovered tooltip; private WS, WS and commit agree with
  the OS counters for the same pid within a few MB; thread count exact.

### Acceptance rework — 2026-09-28

Owner ruling: "the information inside the tooltip does not update with the interval." The
first cut (above) baked `Label.details` into the tooltip's builder closure once, at hover
time; the kit only calls that closure once per hover (`gpui-base` `tooltip.rs`
`TooltipOverlay::request_show`), so the table froze there. Not accepted; reworked in place
(same packet, not a new BUG — the defect was found before this story was accepted).

- [x] The tooltip tracks the item's own 2 s cadence for as long as it stays open: hold the
  pointer still and the CPU time / Uptime (and memory) rows keep advancing, proven with
  `PrintWindow` captures at t=0, t=3 s, t=6 s.
- [x] No second timer: the live update rides the same `StatusText` tick that already drives
  the item's text (`cx.observe` + `cx.notify()`, not a new `Task`).
- [x] Verifier finding F1 (Major): the sample (the `sysinfo` refresh and the Windows thread
  snapshot) no longer runs on the UI thread; it moved to `cx.background_executor()`. The
  foreground tick cost is sub-millisecond (measured).
- [x] Verifier finding F2 (Minor): `refresh_kind()` documents and unit-tests its reliance on
  `ProcessRefreshKind::nothing()` defaulting `tasks` to `true`; the `thread_count()`
  ponytail comment no longer names `NtQueryInformationProcess` (it does not return a thread
  count).

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
- Acceptance rework (2026-09-28): `reference/gpui-kit/crates/base/src/tooltip.rs`
  (`TooltipOverlay::request_show`/`render` — the builder closure runs once per hover, not
  per repaint, which is why a snapshot froze); `crates/settings-ui/src/about.rs`
  (`AboutUpdateControls`) for the codebase's existing `cx.observe` + `.detach()` pattern for
  a small live entity nested in otherwise-static content.

### Documentation Action

- Update required: `docs/gui-layout.md` § Status bar; the module docs of `resource.rs`; the
  intake's `high-level-design.md` (acceptance rework: the diagram, data flow and the "does
  not update while open" claim all flip).

Reason: new visible behaviour of the item.

### Reconciliation

Changed: `docs/gui-layout.md` § Status bar (describes the hover table, its field order and
colours; live-refresh wording added 2026-09-28); the module docs of
`crates/workspace/src/widgets/resource.rs` (the "Hover table" section, plus the F1
background-executor note); `high-level-design.md` (diagram, data flow, idea section). No
other owning doc needed a change.

## Context

- `sysinfo` 0.37.2 on Windows: `memory()` = `WorkingSetSize`, `virtual_memory()` =
  `PrivateUsage`, `accumulated_cpu_time()` = user + kernel (ms), `run_time()` (s);
  `tasks()` is Linux-only and already refreshed (`ProcessRefreshKind::nothing()` keeps
  tasks on). It has no thread count on Windows.
- `sysinfo` already takes a `TH32CS_SNAPPROCESS` snapshot on every refresh; the thread
  count walks one more of the same kind.

## Plan

- [x] Records (intake, HLD, this packet).
- [x] `StatusText`: detail sections on `Label`, tooltip table.
- [x] `resource.rs`: sample struct, rows, Windows peak WS and thread count.
- [x] Tests, docs, manual run, gates, commit.
- [x] Acceptance rework (2026-09-28): `DetailsTooltip` live entity; move the sample to
  `cx.background_executor()` (F1); `refresh_kind` comment/test and the `thread_count`
  ponytail comment (F2); re-walk, re-gate, commit.

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
- Acceptance rework: hold the pointer on the item and capture at t=0/3s/6s to prove CPU
  time and Uptime advance while open; a throwaway timing probe (`--ignored --nocapture`,
  removed before commit) for the F1 before/after foreground-tick cost.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- Unit: `cargo test -p oneterm-workspace --lib`: 41 passed, 0 failed, 3 ignored (the
  unrelated elevation tests). New/changed: `widgets::resource::tests::the_table_lists_
  every_field_in_a_fixed_order`, `..._figures_the_os_does_not_give_are_left_out_or_marked`,
  `..._durations_scale_to_their_size`, plus the pre-existing `displayed_memory_*` and
  `format_memory_*` tests, all passing.
- Platform (Windows 11 Enterprise 10.0.26200, `fast-dev` build of this branch, private
  `USERPROFILE`/`HOME`, 1280x800, one `Command Prompt` tab, own pid 9860 only). Walk and
  screenshots: [`evidence/US-0148-gui-walk.md`](evidence/US-0148-gui-walk.md),
  [`evidence/US-0148-no-hover-dark.png`](evidence/US-0148-no-hover-dark.png) (baseline, no
  tooltip), [`evidence/US-0148-tooltip-dark.png`](evidence/US-0148-tooltip-dark.png) (hovered).
  Comparison with `GetProcessMemoryInfo` (the same `PROCESS_MEMORY_COUNTERS_EX2` struct the
  widget itself reads) and `Get-Process -Id 9860`, read moments after the screenshot:

  | Figure | Tooltip | OS counter | Agreement |
  | --- | --- | --- | --- |
  | Private working set | 53.3 MB | 53.4 MB | within 0.1 MB |
  | Working set | 167.7 MB | 167.9 MB | within 0.2 MB |
  | Commit (private bytes) | 112.9 MB | 113.1 MB | within 0.2 MB |
  | Peak working set | 171.6 MB | 171.6 MB | exact |
  | Threads | 14 | 14 (`Get-Process`) | exact |
  | CPU time (user + kernel) | 7.8 s | 7.97 s (`TotalProcessorTime`) | within the 2 s sample lag |
  | Uptime | 2m 53s | 176 s (`Now - StartTime`) | within the 2 s sample lag |

  Also confirmed: a baseline screenshot with the cursor moved away from the window shows no
  tooltip, and (**this cut only, superseded by the 2026-09-28 rework below**) the tooltip's
  figures stayed frozen across 27 s of continued hovering while the item's own `MEM` text
  ticked.
- Linux and macOS: not run. The `RESIDENT_NAME`/`VIRTUAL_NAME` labelling and the "figures
  the OS does not give are left out or marked" path are covered by the unit test only (as
  planned in the intake's Validation Shape).
- Integration proof: not applicable (one widget, no cross-crate flow).
- Gates: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `python scripts/check-theme-contrast.py`, `python scripts/check-doc-paths.py`,
  `python scripts/check-english.py` all passed individually before the full gate. Full
  `pwsh scripts/ci-local.ps1` with `CARGO_BUILD_JOBS=6` in this worktree's own `target/`:
  `ci-local: all checks passed.`
- No gaps against this cut's acceptance criteria. Follow-up candidates (shells line,
  user/kernel CPU split) are recorded in `IN-0047.md` and left out, as scoped.

### Acceptance rework — 2026-09-28

- Live refresh: `cargo test -p oneterm-workspace --lib`: 42 passed, 0 failed, 3 unrelated
  ignored (elevation tests) — includes the new
  `widgets::resource::tests::refresh_kind_keeps_tasks_on_for_the_linux_thread_count` (F2).
  No unit test for the entity-observes-entity refresh path itself (a `#[gpui::test]` would
  need a real window and hover simulation to add anything a GUI walk does not already
  prove); proven instead by the GUI walk below.
- GUI walk: `fast-dev` build of this branch after the fix, private `USERPROFILE`/`HOME`,
  1280x800, own pid 22316 only. Pointer moved onto the CPU/MEM item once and held there;
  `PrintWindow` captures at t=0, t=3 s, t=6 s —
  [`evidence/US-0148-gui-walk.md`](evidence/US-0148-gui-walk.md) § "Acceptance rework: the
  tooltip stays live while open".

  | Capture | CPU time | Uptime |
  | --- | --- | --- |
  | t=0 | 1.8 s | 29.0 s |
  | t=3s | 1.9 s | 33.0 s |
  | t=6s | 2.0 s | 35.0 s |

  Both figures advance at every capture with the pointer motionless — the acceptance
  criterion holds.
- F1 (verifier, Major): measured on this machine, the pre-fix synchronous sample cost
  10.0-14.1 ms (median ~11.2 ms) on the UI thread, matching the verifier's ~10-16 ms
  finding; the post-fix foreground tick cost 0.2-41.6 µs (all 8 samples sub-millisecond).
  Full numbers: `evidence/US-0148-gui-walk.md` § "F1: the sample moved off the UI thread".
- F2 (verifier, Minor): `refresh_kind_keeps_tasks_on_for_the_linux_thread_count` pins
  `ProcessRefreshKind::nothing().tasks() == true`; the `thread_count()` ponytail comment no
  longer names `NtQueryInformationProcess`.
- Gates re-run after the rework: `cargo fmt --all -- --check`, `cargo clippy --workspace
  --all-targets -- -D warnings`, `cargo test -p oneterm-workspace` (42 passed, 3 unrelated
  ignored), `python scripts/check-theme-contrast.py`, `python scripts/check-doc-paths.py`,
  `python scripts/check-english.py` all passed individually, then the full
  `pwsh scripts/ci-local.ps1` with `CARGO_BUILD_JOBS=4` in this worktree's own `target/`:
  `ci-local: all checks passed.`
- No gaps against the rework's acceptance criteria.
