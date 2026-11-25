# Work: attribute and cut the UI thread's work outside the terminal element

ID: US-0145
Intake: IN-0046
Created: 2026-09-28

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

- Change type: maintenance (performance; no behaviour or visual change)
- Risk lane: normal (medium risk: skipping a notify or caching a view can leave a stale UI)
- Spec Intake, when required: [`IN-0046`](IN-0046.md); design:
  [`high-level-design.md`](high-level-design.md)

## Outcome

1. **Attribute.** `hotpath` sites on every OneTerm render the window runs (workspace root,
   title bar, status bar and its items, terminal panel and tab strip, Space, terminal
   view, the right-dock panels), the terminal element's `request_layout`, and the
   cursor-blink tick. A table from the idle and two-tab TUI loads says which renders run
   per frame, how often frames happen at idle, and what the ~2,500 allocations per frame
   outside the old sites are.
2. **Cut.** Idle frames no longer come one per timer: a blink, the status-bar ticks and
   the Agent panel's relative times share a frame, an empty Agent panel draws none, and
   output in a tab that is not shown draws no frame. No visual change. Under load the
   frames this frees are either saved (an uncapped window: less UI CPU) or spent on the
   shown terminal (a window at its frame-rate cap: more frequent refreshes, not less CPU).

## Scope

- [x] In scope:
  - `hotpath-profiling` feature on `oneterm-workspace`, `oneterm-sftp-ui`,
    `oneterm-session-ui` (dependency only, like the other leaves), fanned out from
    `oneterm-app`.
  - The sites listed in Outcome 1; raw reports under `research/raw/us0145-*`.
  - `hotpath-measure.ps1`: `-IdleSeconds`, `-Activate` / `-Inactive`, the UI thread's
    steady-state cycle count.
  - The smallest fix for the top items; the rest recorded as candidates.
- [x] Out of scope: text shaping (US-0146), the PTY loop (US-0147), gpui or GPUI Kit
  changes (`docs/PROJECT.md` forbids patching them), per-frame view caching (research
  § 8.3 / 8.5).

## Acceptance

- [x] Attribution table (top 10 by time and by allocations per frame, idle and TUI) in
  `research/hotpath-evaluation.md` § 8.2; raw reports in `research/raw/us0145-*`.
- [x] Before/after: idle UI-thread CPU, allocations per idle frame and per second, idle
  frames per second, TUI UI-thread CPU, `frame_time_under_output` (§ 8.4).
- [x] No visual change: a hidden tab re-renders when shown (`set_active(true)` notifies the
  panel); the unit test proves a hidden tab's view is not notified and the shown one is.
- [x] Clippy `-D warnings` with no feature, `oneterm-app/hotpath-profiling` and
  `oneterm-app/hotpath-profiling-alloc`; full `pwsh scripts/ci-local.ps1` passes.

## Documentation

### Owning Docs Reviewed

- `docs/gui-layout.md` — the workspace frame (title bar / dock area / status bar) and the
  status bar's items; the render/notify model was not described.
- `docs/terminal-backend.md` § 6.4 — the output -> `cx.notify()` -> element pipeline and the
  "notify only when something changed" rule.
- `docs/terminal-split.md` — Space tree rendering inside `TerminalPanel`; no change.
- `docs/agent-panel-display.md` § 9 — the Agent panel's refresh tick (verification F3).
- `high-level-design.md` (this intake) — how sites are wired; the leaf-crate list.
- gpui-pre 0.3.7 `src/window.rs` (`mark_view_dirty`, `draw_roots`, `reuse_prepaint`,
  `reuse_paint`, `Frame::window_control_hitboxes`), `src/view.rs` (`prepaint_view`,
  `ViewElement::cached`); GPUI Kit 0.7 `dock/tab_panel.rs` (the active panel is cached),
  `gpui-base` `dock/dock_area.rs` (`reconcile` notifies), `dock/panel.rs` (`set_active`
  contract).

### Documentation Action

- Update required: `docs/gui-layout.md` (new § Frames and re-rendering),
  `docs/terminal-backend.md` § 6.4, this intake's HLD and `IN-0046.md`,
  `research/hotpath-evaluation.md` § 8.

Reason: the fix changes when views notify and when timers wake, rules future UI work must
keep, and records why view caching is not the answer in gpui 0.3.7.

### Reconciliation

Changed: `docs/gui-layout.md` § Frames and re-rendering, `docs/terminal-backend.md` § 6.4,
`high-level-design.md` (leaf crates, how to add one), `IN-0046.md` (US-0145 done, surfaces),
`research/hotpath-evaluation.md` § 8, `research/hotpath-measure.ps1`.
`docs/terminal-split.md`: no change (the Space tree renders as before).
`docs/agent-panel-display.md` § 9: the refresh tick's grid phase, the silent empty
panel and the restart on a working card (cadences unchanged).

## Context

gpui re-renders from the window root on every frame. A `cx.notify()` marks the view and
every ancestor view dirty; a child view is skipped only when it is embedded with
`.cached(style)` and is not dirty. The kit's tab group caches its active panel, so the
right dock is reused on a terminal frame; the title bar, status bar and dock chrome are
not, and cannot usefully be (research § 8.3).

## Plan

- [x] Features and sites; release builds with `hotpath-profiling` / `-alloc`.
- [x] Idle and TUI, timing + alloc count; a DbgHelp sampler for the part hotpath cannot
  see; table.
- [x] Try caching the dock area (measured worse, reverted); fix: hidden tabs do not notify,
  timers on one grid; measure after; test; docs; gates; commit.
- [x] Verification findings (`evidence/US-0145-verify.md`): F1 TUI result stated as
  conditional; F3 Agent panel timer on the grid, no frame when empty; F4 idle figures;
  F5 records; F7 zero-interval guard.

## Decisions

None. The gpui constraints are recorded in `docs/gui-layout.md` § Frames and re-rendering.

## Verification Plan

- `hotpath-measure.ps1` idle (focused and unfocused) and TUI, before and after, timing and
  alloc-count builds.
- `cargo test -p oneterm-terminal-view --profile fast-dev frame_time_under_output --
  --ignored --nocapture`.
- `cargo test -p oneterm-state -p oneterm-workspace -p oneterm-terminal-view`; clippy with
  and without the features; `check-doc-paths.py`, `check-english.py`; full
  `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- Attribution (research § 8.2): at idle every frame re-runs the workspace, status bar
  (5 items), title bar and tab strip; the terminal renders on the blink frames only; the
  right dock almost never. Frames: 3.2-3.7/s focused, 1.65/s unfocused, from unaligned
  timers. Outside every site: about 4.2 ms and 2,062 allocations per idle frame, 2,135 per
  TUI frame. Sampler: `Window::draw` 82 % of the UI thread's busy time (taffy 21.6 %,
  heap 15 %, paint 9 %), title bar 14.9 %, dock area 18 %, status bar 2 %.
- Dock-area caching: built, measured worse (31.9 -> 33.9 Mcycles/s), reverted. Title-bar
  caching: not built (gpui does not replay window-control hitboxes).
- Fix, back-to-back pairs (UI thread cycle counter, startup excluded): idle focused
  3.18 -> 2.11 frames/s and 35.1 -> 30.2 Mcycles/s here, 3.47 -> 2.04 and 34.95 -> 33.73
  in the verification (so -3.5 % to -14 % cycles for -34 % to -41 % frames); idle
  unfocused 1.65 -> 1.25 frames/s and 23.3 -> 19.3 Mcycles/s (verification 1.76 -> 1.23,
  29.85 -> 18.51). Allocations per idle second 7,577 -> 4,971; per frame unchanged
  (~2,100-2,200).
- TUI, two tabs: **the direction of the CPU change depends on whether the frame rate is
  capped.** Here the window was not capped: 44.4 -> 31.3 frames/s with the same terminal
  renders (7,071 / 7,136) and 413 -> 202 Mcycles/s (-51 %; hotpath thread 19.4 % ->
  9.0 %). In the verification both builds drew ~32 frames/s: the frames that used to draw
  nothing became terminal frames (terminal renders 50.4 % -> 99.8 % of frames, the shown
  tab refreshed 16 -> 32 times a second) and UI cycles rose 289.6 -> 403.4 Mcycles/s
  (+39 %). Both confirm the mechanism (no frame draws nothing); a capped window gets
  smoother output from the fix, not a CPU cut.
- Agent panel (F3): the relative-time tick sleeps on the 1 s grid and an empty panel draws
  no frame; the spinner keeps 120 ms and restarts the tick when a card starts working.
  Not measured: three release builds for the idle-with-Agent-panel run failed in rustc
  (`STATUS_STACK_BUFFER_OVERRUN` compiling `windows` 0.62.2 at opt-level 3, the same
  crash fat LTO with debug info gave earlier), so the expected drop from the verifier's
  2.12 frames/s towards the SSH Client mode's 1.23 is unproven. `hotpath-measure.ps1
  -AgentPanel` starts a run in Agent mode for whoever measures it.
- `frame_time_under_output` (`fast-dev`, 3 runs): flood 1557-1649 us, idle 227-282 us,
  within US-0142's range; the harness does not go through either change.
- Unit: `panel::tests::output_in_a_hidden_tab_does_not_notify_its_view` (fails without the
  change: `[hidden, shown]` notified), `tick_tests::a_tick_lands_on_the_shared_grid`,
  `tick_tests::a_zero_interval_wakes_now`.
  `cargo test -p oneterm-state -p oneterm-workspace -p oneterm-terminal-view`: 42 + 390
  (3 ignored) + 42 (3 ignored) passed; after the verification fixes
  `cargo test -p oneterm-state -p oneterm-agent-ui -p oneterm-workspace`: 43 + 6 + 42
  (3 ignored) passed.
- Independent verification: PASS, `evidence/US-0145-verify.md` (findings F1-F7; F1, F3,
  F4, F5, F7 addressed in the follow-up commit; F2 folded into the ranges above; F6 is
  informational, a one-tick window after a tab switch that the switch frame covers).
- Clippy `-D warnings`: clean with no feature, `oneterm-app/hotpath-profiling` and
  `oneterm-app/hotpath-profiling-alloc`.
- Full gate: `CARGO_BUILD_JOBS=3 pwsh scripts/ci-local.ps1` (no `target/release`), 141
  `test result: ok` lines, none failed. Final line: `ci-local: all checks passed.`
- Gaps: no `#[gpui::test]` for "a blink re-renders nothing else" (it does re-render the
  whole window; that is gpui's model, § 8.3), so the idle cut is proven by measurement
  only. Idle focused still draws ~2.1 frames/s where 2.0 is the aim (a tick that lands on
  the other side of a vsync from the blink draws its own frame). TUI frame rates vary
  with the window's visibility on the desktop, so only back-to-back pairs compare. The
  sampler slows the thread it samples and folds inlined frames into their caller. No
  manual GUI check beyond the measurement runs and the verifier's walk. Platform proof
  waits for CI (macOS and Linux not run).

## Handoff

- State: implemented and independently verified (PASS); verification findings addressed on
  `perf/idle-render`; not merged, not pushed.
- Next owner / action: coordinator re-checks the follow-up commit, then merge; CI run for
  platform proof; `harness.db` row (proposed in the evidence file) written by whoever owns
  the database.
- Blockers: none.
