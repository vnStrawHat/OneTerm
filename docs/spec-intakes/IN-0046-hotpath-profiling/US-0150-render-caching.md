# Work: cache what an idle frame redraws for nothing (the rest of US-0145)

ID: US-0150
Intake: IN-0046
Created: 2026-09-29

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

- Change type: maintenance (performance; no behaviour or visual change)
- Risk lane: normal (medium risk: a cached view that misses a notify shows a stale UI, and
  a cached view laid out at the wrong size shifts pixels)
- Spec Intake, when required: [`IN-0046`](IN-0046.md); design:
  [`high-level-design.md`](high-level-design.md)

## Outcome

Owner ruling 2026-09-29: "handle the rest of US-0145", the candidates in
[`research/hotpath-evaluation.md`](research/hotpath-evaluation.md) § 8.5. Each candidate
is taken in order of gain over risk and kept only if it measurably cuts idle UI cycles or
frames with no visual or functional regression; otherwise the reason is recorded.

1. The title bar's app menu and SSH Client / Agent / None toggles are a cached view; the
   kit's drag region and window controls stay uncached and keep working.
2. The status bar is a cached view that re-renders only when one of its items notifies.
3. The tab bar and `+` dropdown chrome: what OneTerm controls, and an upstream note for
   what it does not.
4. The cursor drawn outside the dock, so a blink does not dirty it: only after 1-3 are
   measured; not shipped unless pixel-identical and correct with IME and popups.
5. Upstream gpui requests, with gpui-pre 0.3.7 file and line references. Not filed.

## Scope

- [x] In scope: `crates/workspace/src/layout/{title_bar.rs,statusbar.rs,workspace/*}`,
  what `TerminalPanel` returns for its tab bar suffix, measurements under
  `research/raw/us0150/`, `hotpath-measure.ps1` (`-NoBlink`, the UI thread id),
  `docs/gui-layout.md` § Frames and re-rendering, research § 9.
- [x] Out of scope: patching gpui-pre or GPUI Kit (`docs/PROJECT.md`), filing upstream
  issues, frame-rate caps, the terminal element's own paint.

## Acceptance

- [x] Per candidate: kept or dropped, with the measured gain or the reason (research § 9.2,
  § 9.4-9.6).
- [x] Before/after (interleaved, same build flags): idle focused and unfocused frames/s and
  UI Mcycles/s, hotpath thread %, sampler per frame (shares are not comparable, § 9.1),
  the two-tab TUI load.
- [x] Every kept cached view: 0 differing pixels in the title bar and tab bar rows, and in
  the status bar outside the clock's seconds and the per-process CPU/MEM value
  (PrintWindow pairs: dark, light, light in Agent mode, dark at a 20 px UI font); window
  drag, double-click maximise and restore, close-button hover, Win11 snap-layout hover and a
  toggle click work on the instance under test, as in `before`.
- [x] A `#[gpui::test]` per kept view: not re-rendered on an unrelated notify, re-rendered
  when its own state changes; the status bar's cached height pinned against the kit layout.
- [x] Accessibility (verification F1): while AccessKit is active both views are embedded
  uncached, and the title bar's six nodes (`MenuBar`, `Button "OneTerm"`, `ToolBar` and its
  three toggle `Button`s) stay in the UI Automation tree on every idle frame.
- [x] Gates: fmt, clippy `-D warnings`, `cargo test -p oneterm-workspace -p oneterm-app
  -p oneterm-terminal-view`, theme contrast, doc paths, English, full
  `pwsh scripts/ci-local.ps1` (see Evidence).

## Documentation

### Owning Docs Reviewed

- `docs/gui-layout.md` § Frames and re-rendering — "do not cache the title bar or the dock
  area" rule, the notify model.
- `high-level-design.md` (this intake) — sites, leaf crates, measuring.
- `research/hotpath-evaluation.md` § 8 — US-0145's attribution and § 8.5 candidates.
- `evidence/US-0145-verify.md` — how the pairs were measured.
- gpui-pre 0.3.7 `src/view.rs` (`AnyView::cached`, `Entity::cached`, `prepaint_view`,
  `paint_view`), `src/window.rs` (`mark_view_dirty`, `reuse_prepaint`, `reuse_paint`,
  `insert_window_control_hitbox`, `Window::refresh`), `src/app.rs` (`App::notify`),
  `src/elements/div.rs` (hover notify, `window_control_area`); gpui-pre-windows 0.3.7
  `events.rs` (window-control hit test); GPUI Kit 0.7 `title_bar.rs`, `status_bar.rs`,
  `menu/app_menu_bar.rs`, `dock/tab_panel.rs`.

### Documentation Action

- Update required: `docs/gui-layout.md` § Frames and re-rendering (which views are cached,
  what dirties them, the refined "do not cache" rule), `research/hotpath-evaluation.md`
  (new § 9), `IN-0046.md` (US-0150 line).

Reason: a cached view is a rule later UI work must respect (whoever changes what the view
reads must notify it).

### Reconciliation

Changed: `docs/gui-layout.md` § Frames and re-rendering (what is cached, what must notify
it, the refined "do not cache" rule), `research/hotpath-evaluation.md` § 8.5 pointer and
new § 9, `research/hotpath-measure.ps1` (`-NoBlink`, the UI thread id in its summary line),
`high-level-design.md` (the `TitleBarContent::render` site), `IN-0046.md` (US-0150 line).

## Context

gpui 0.3.7 re-renders a child view embedded with `.cached(style)` only when it (or a
descendant) was notified, the window was refreshed, or its bounds, content mask or text
style changed. A cached view is laid out from `style`, not measured. Reused frames replay
hitboxes, mouse listeners, tooltips, deferred draws and the scene, but not
`window_control_hitboxes`; the kit puts those on the title bar's `bar` div and the
min/max/close buttons, outside any child OneTerm passes in.

## Plan

- [x] Baseline release build (thin LTO, line tables, `hotpath-profiling`) from main.
- [x] Candidate 1, test, measure; candidate 2, test, measure; candidate 3 built, measured,
  dropped.
- [x] Candidate 4 assessment; upstream section; docs; gates; commit.

## Decisions

None expected; the rules go into `docs/gui-layout.md`.

## Verification Plan

- `hotpath-measure.ps1` idle focused (`-Activate`) and unfocused (`-Inactive`) 90 s, TUI
  two tabs 120 s, before and after, same build flags; sampler (`raw/us0145-sampler/`) idle
  focused on both builds.
- PrintWindow captures of the instance under test, before and after, light and dark theme,
  compared pixel by pixel.
- GUI walk on the own pid: drag, double-click the title, hover close, hover maximise.
- `cargo test -p oneterm-workspace -p oneterm-app -p oneterm-terminal-view`, the gates above.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Commits on `perf/render-caching`: `89536cc6` (title bar content cached), `e4133b7e`
(status bar cached), then the records. Full tables in research § 9.

| Candidate | Result | Evidence |
| --- | --- | --- |
| 1. Title bar toggles + app menu cached, window controls uncached | **Kept** | Content renders 1.00 -> 0.04-0.07 per idle frame; `AppTitleBar` render 12-15 -> 1.7 us/frame; sampler title-bar subtree 30.8 / 20.8 -> 9.1-14.9 samples per frame, toggle group -> 0; `before` -> `v1` idle focused -4.8 % / -4.0 %, TUI -7 % per frame. Pixels 0 in the title bar rows (4 pairs); hit test, drag, double-click, close hover, snap layouts, toggle click as `before`. |
| 2. Status bar cached | **Kept** | Bar renders 1.00 -> 0.51-0.61 per idle focused frame, 0.05-0.09 under TUI; its sites 112 -> 65 us per idle focused frame, 55 -> 4.6 under TUI. Whole-thread effect (`v1` -> `v2`): -0.7 % to +6 % idle (below noise), -1.5 % / -4.1 % / -28 % TUI per frame. Height pinned at rem 12-20; pixels 0 outside the clock's seconds and CPU/MEM value, also at a 20 px UI font. Tooltip (US-0148): hovered 8 s over the cached bar, table and label advanced together every 2 s (`raw/us0150/pixels/resource-tooltip-live-v2.png`). |
| 3. Tab bar `+` trigger cached | **Dropped** | `v2` -> `after`: idle focused -3.0 % / +4.7 %, unfocused +15 %, TUI +2.1 % per frame; sampler dropdown triggers 7.0 / 10.6 -> 6.8 per frame (mostly the kit's `...` menus). Patch kept in `raw/us0150/tools/dropped-add-tab-button.patch`. Kit notes: research § 9.4. |
| 4. Cursor overlay outside the dock | **Not built** | Ceiling: the blink is ~0.95 frames/s of 2.09 (`-NoBlink` runs). Needs a cached dock area, which gpui's refresh cascade makes worse on every output frame (US-0145 § 8.3); a block cursor re-paints the covered glyph from the element's glyph cache and the IME composition is anchored there, so an overlay is not pixel-identical without duplicating both. Research § 9.5. |
| 5. Upstream gpui | Written, not filed | Window-control hitbox replay (`window.rs` 987 / 1014 / 1952 / 3879 / 5128, `div.rs` 2577); no refresh cascade for a dirty cached view (`view.rs` 489 / 501 / 564); replay AccessKit nodes for a reused view (`window/a11y.rs` 275 / 429, `window.rs` 1004 / 3814 / 6794, `view.rs` 484-489; verification F1). Research § 9.6. |

Before -> final (`v2`), interleaved pairs: idle focused 17.8-31.5 -> 16.8-26.5 Mcycles/s,
5 of 6 pairs lower, median -9 %; frames/s unchanged (about 2.1); TUI -8 % and -4 %
Mcycles per frame (two pairs at the same frame rate); hotpath UI thread % follows the
frame rate and is too coarse at idle (0.3-3.3 %). **Idle unfocused: no expected change**
(verification F4): the clock drives those frames and the status bar re-renders on 94 % of
them, so only the title bar's ~16 us per frame can be saved; the pairs read -15 % here and
+22 % in the verification, both noise. The first pair of the day (`after`, all three
candidates) read 31.6 -> 25.2 (-20 %) idle focused.

Gates: `cargo fmt --all -- --check` clean; `cargo clippy --workspace --all-targets -- -D
warnings` clean; `cargo test -p oneterm-workspace -p oneterm-app -p oneterm-terminal-view`
26 + 398 (3 ignored) + 45 (3 ignored) passed; `check-theme-contrast.py`,
`check-doc-paths.py` (212 paths), `check-english.py` (1074 files) pass; full
`CARGO_BUILD_JOBS=3 pwsh scripts/ci-local.ps1` (no `target/release`): 141 `test result: ok`
lines, none failed, final line `ci-local: all checks passed.`

Tests: `title_bar::tests::caching::another_views_frame_reuses_the_content_and_a_mode_change_does_not`,
`statusbar::tests::caching::another_views_frame_reuses_the_bar_and_an_item_notify_does_not`,
`statusbar::tests::caching::status_bar_height_matches_the_kit_layout` (since the rework: the
workspace's embed and its accessibility branch against the kit layout),
`title_bar::tests::caching::the_content_fills_the_kit_row_cached_or_not` (rework); mutations
(content embedded uncached, no `UiConfig` observer, height without the border, and the five
of the rework) each fail one.

### Rework after the verification (`evidence/US-0150-verify.md`, FAIL on F1)

- **F1 (accessibility).** gpui-pre 0.3.7 does not replay a reused view's AccessKit nodes,
  so the cached title-bar content took `MenuBar`, `Button "OneTerm"`, `ToolBar` and the three
  toggle `Button`s out of the UI Automation tree on every reused frame. Both embeds now go
  through `crate::layout::cached_unless_a11y` (`crates/workspace/src/layout/mod.rs`):
  cached while `window.is_a11y_active()` is false, otherwise the view laid out uncached in a
  box of the same style (AccessKit activation refreshes the window, `window.rs` 1649-1661).
  UI Automation client against the own window (`raw/us0150/tools/uia.ps1`, 5 samples 3 s
  apart after activation, idle): **6/6 title-bar nodes in every sample**, focused (18 nodes)
  and unfocused (17), plus the status bar's dock `Button` (`raw/us0150/a11y/rework-*.txt`).
  With a11y off, caching is unchanged: 0.08 content and 0.53 status-bar renders per idle
  focused frame (`runs/us0150-rework-idleF2`). One launch had AccessKit activate by itself
  1 s after start, with no client of ours (`a11y/us0150-rework-idleF-stderr.log`): that
  window rendered both views on every frame (1.00 per frame), the fallback working as
  designed. Unit test: not possible for the active branch, because the test platform's
  window ignores `a11y_init` (gpui-pre 0.3.7 `platform.rs` line 1070, default no-op), so
  `is_a11y_active` never turns true; the uncached branch is driven directly through
  `embed_view(.., cached: false)` and must lay out exactly like the cached one
  (`the_content_fills_the_kit_row_cached_or_not`, `status_bar_height_matches_the_kit_layout`).
  Pre-existing, upstream, out of scope: the kit's cached right-dock panel (`Pane "SSH
  client"`, `gpui-component` 0.7 `dock/tab_panel.rs` line 782) drops out of the tree the same
  way on main.
- **F2 (production embeds under test).** `statusbar::embed` owns the bar's box and the
  switch and is what the workspace renders and the tests host; the title bar's box is
  `content_style()`, used by `AppTitleBar::render` and the tests. Mutations, each killed:
  embed always uncached, the a11y switch inverted, the bar at a fixed 28 px, the title
  content at a fixed 100 px, the uncached box without its style.
- **F3 (snap layouts).** The Windows 11 snap-layout flyout is proven only by this packet's
  real-mouse capture (`raw/us0150/pixels/window-controls-before-after.png`); the
  verification's synthetic hover did not register. The hit test answers `HTMAXBUTTON` there,
  as on main.
- **F4.** Idle unfocused reworded above and in research § 9.2.
- Rework gates: fmt clean; clippy `-D warnings` clean with no feature and with
  `oneterm-app/hotpath-profiling`; `cargo test -p oneterm-workspace -p oneterm-app
  -p oneterm-terminal-view` 26 + 398 (3 ignored) + 46 (3 ignored) passed;
  `check-doc-paths.py` (212 paths), `check-english.py` (1076 files) pass; full
  `CARGO_BUILD_JOBS=3 pwsh scripts/ci-local.ps1`: 141 `test result: ok` lines, none failed,
  final line `ci-local: all checks passed.`

Gaps:

- AccessKit on Windows stays active for the window's life once any UI Automation client
  queries it (a screen reader, but also, as seen once here, an unidentified system client
  1 s after launch). Such a window gets none of this story's gain (both views render on
  every frame, as on main). How often that happens on users' machines is not measured.
- The whole-thread cycle counter moves ±20 % between runs of the same build on this
  machine (another agent building, desktop state), so the status bar's share (about 1-2 %
  idle) is proven by its hotpath sites and render counts, not by the counter; the owner
  may prefer to drop `e4133b7e` on that ground (it reverts cleanly).
- The sampler could not resolve the total per-frame change (per-frame `Window::draw`
  samples overlap between builds; the dock-area subtree read higher in `v2` with the same
  render counts there): it is intrusive (about 0.8 frames/s while sampling) and its
  subtree shares move with UI Automation and IME activity. The sampled TUI pair was not
  used (the `v2` run drew a third of the frames of `before`), nor two hotpath TUI runs
  whose load did not start (3.6 and 5.3 frames/s).
- The pixel pairs compared `before` with `after` (the final code plus the dropped `+`
  view), so they cover the final code's regions too.
- 96 DPI only; 150 % could not be set for a private instance without touching system
  settings. A DPI change refreshes the window (`Window::bounds_changed`), so no cached view
  survives it.
- The real-mouse walk made the instance topmost for a few seconds and took the foreground.
- Platform proof (macOS, Linux) waits for CI.

## Handoff

- State: reworked after the verification (F1-F4) on `perf/render-caching`; not merged,
  not pushed.
- Next owner / action: coordinator: re-check F1/F2, then merge; `harness.db`
  row by whoever owns the database.
- Blockers: none.
