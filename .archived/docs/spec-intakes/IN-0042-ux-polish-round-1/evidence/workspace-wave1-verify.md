# Independent verification — BUG-0067, US-0112, US-0113

Verifier: an independent session that did not write the code. Target:
`worktree-agent-a1857284c8b933f27` at `40420965` (7 commits on top of `main` @ `74d842e7`).
Date: 2026-09-17.

## Verdicts

| Packet | Verdict |
|---|---|
| `BUG-0067` — reselecting the right-dock mode reopens the dock | **PASS-WITH-NOTES** |
| `US-0112` — the status bar elides the path from the left | **FAIL** |
| `US-0113` — the right dock width follows the window | **PASS-WITH-NOTES** |
| **Overall** | **FAIL** (one ticked acceptance row is demonstrably unmet; nothing regressed against `main`, and the gate is green) |

`US-0112` fails on its own acceptance row *"The memory indicator shows its unit at every width
it is visible at … never `MEM 577.0`"*: at a 700 px window this build renders `MEM 174.1 M`
with the unit cut off and the dock button gone (`verify-US-0112-700-git-cwd.png`). The
elision function itself is correct and the 900 px cwd case the finding measured is genuinely
fixed — what is not delivered is the claim that bounding the path *"keeps the rest on
screen"*.

## Commands (final lines)

| Command | Final line |
|---|---|
| `cargo test -p oneterm-workspace` | `test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s` |
| `pwsh scripts/ci-local.ps1` | `ci-local: all checks passed.` |
| mutation run A (3 decision functions) | `test result: FAILED. 22 passed; 3 failed` |
| mutation run B (`elide_path_left`) | `test result: FAILED. 24 passed; 1 failed` |

The 25-test count matches the `US-0113` packet's claim exactly, and the four new pure
functions are all covered (see Mutation testing).

## What was verified, with evidence

### BUG-0067

- **The kit claim is accurate.** `reference/gpui-kit/crates/component/src/button/toggle.rs:311-397`
  — `ToggleGroup`'s outer handler builds `next = checks.clone(); next[ix] = !next[ix]`, where
  `checks` is the vector of the items' rendered `checked` values. A click on the selected
  segment of a single-select group therefore arrives as every toggle unchecked, and the old
  `checked && ix != current_ix` loop could not have seen it even with the inequality removed.
  Fix (a) alone would not have worked, as the packet says.
- **`clicked_mode_index` is right for every combination.**
  `crates/workspace/src/layout/title_bar.rs:162-164` diffs the post-click vector against the
  single-select state the group was rendered with. Because both `checks` and `current_ix`
  come from the same render (`title_bar.rs:121-143`), the recovery is correct even if the
  persisted mode changed between render and click. Exhaustive check over the nine
  (current, clicked) pairs for `SshClient`/`Agent`/`None`: each yields the clicked index; an
  unchanged vector yields `None`; an empty vector yields `None`.
- **Fix (b) works, verified live.** `sync_right_dock_mode`
  (`crates/workspace/src/layout/workspace/mod.rs:279-307`) hangs off the existing
  `cx.observe_in(&dock_area, …)` (`mod.rs:193-208`). Re-taken GUI walk on this HEAD, own
  build, own pid, `PrintWindow`, 1200x800 window, cwd in a git repo:
  - `verify-BUG-0067-24a-retake.png` — the tab bar's 4th trailing button collapsed the dock
    and the segmented control moved to **None** in the same frame.
  - `verify-BUG-0067-24b-retake.png` — clicking **SSH Client** reopened the dock at the same
    width (423 px both before and after, i.e. the 35 % ceiling of 1200 px), control shows
    SSH Client, `target/ui_config.json` reads `"right_dock_mode": "ssh_client"`.
- **No feedback loop, no per-render write.** `sync_right_dock_mode` returns before writing
  when `next == current` (`mod.rs:298-300`), and `UiConfig::persist`
  (`crates/settings/src/ui_config.rs:224-236`) snapshots and writes on the background
  executor. `SetRightDockMode` updates config and dock in one effect cycle
  (`actions.rs:113-139`), so the observer sees the settled state and writes nothing extra:
  one `ui_config.json` write per real mode change, none per frame. Nothing observes
  `UiConfig` and writes back to the dock, so there is no cycle.
- **Persisting `None` does not change the next launch.** Startup applies the persisted mode
  (`mod.rs:186-192`): `None` collapses a dock that `docks.json` already recorded as closed,
  which is what the previous build showed too. Re-opening with the dock button restores
  `SshClient`/`Agent` symmetrically, so the packet's *"the persisted mode is what the title
  bar shows on the next launch"* row holds. Nothing in `docs/gui-layout.md` §Persistence or
  `DEC-0016` (startup shell spawning) is touched by this.

### US-0112

- `elide_path_left` (`crates/workspace/src/widgets/status_text.rs:125-148`) was re-implemented
  standalone and exercised over the degenerate inputs the packet did not list. Results: empty
  path at any budget → `""`; `C:\` at budget 2 → `…\`; UNC `\\server\share\deep\path\here`
  cuts at a separator from the left (`…\deep\path\here` at 20, `…\here` at 10); forward-slash
  paths cut identically; a trailing separator is kept; exact fit is returned whole; a single
  over-long component keeps its end (`…oryname`); multibyte paths never split a character
  (`…ト管理`, `…\プロジェクト\本番`). One off-nominal case: **budget 0 returns `…`, one
  column wider than the budget**. Unreachable in the product because `path_budget` floors the
  available width at 120 px (≈20 characters), so it is a note, not a defect.
- **Click-to-copy still copies the full value.** `status_text.rs:245` takes
  `label.plain_text()` *before* `shorten.apply(...)`, and the click handler at
  `status_text.rs:264` writes that string. Confirmed by reading, not claimed.
- **`MB` really is on the after frame** at 900 px (`US-0112-50-narrow-900.png`) — and the
  path is elided from the left at a separator, as specified.
- **But the 440 px reservation is wrong whenever another indicator is visible.**
  `STATUS_BAR_RESERVED` (`status_text.rs:101`) is a fixed constant, while the git-status
  widget sits in the *same* left group, after the breadcrumb (`layout/statusbar.rs:37-47`),
  and the net-speed widget sits in the right group. Neither is accounted for, and both are
  `Shorten::Never`. Measured on independent captures (own build, own pid, shell cwd set to a
  deep path inside a git repo):
  - `verify-US-0112-900-git-cwd.png` (900 px): the path elides correctly, `MEM 177.6 MB`
    survives — but the branch renders as `worktree-ag`, a hard cut mid-token with no
    ellipsis. The real branch is `worktree-agent-a1857284c8b933f27`.
  - `verify-US-0112-700-git-cwd.png` (700 px): `MEM 174.1 M` — **the unit is cut** — the
    dock-toggle button is clipped off the bar entirely, and the branch again reads
    `worktree-ager`.
  - The same clip is visible at 1200 px (`verify-BUG-0067-24a-retake.png`: `worktree-agent-a185`).
  So `F10`'s defect class (hard clip, no ellipsis, the informative end lost) is not removed;
  it is relocated from the path onto the next unbounded indicator, and `MEM`'s unit is only
  safe above roughly 750 px.
- **The character model errs wide, not narrow.** `CHAR_ADVANCE_PER_REM = 0.375`
  (`status_text.rs:107`) models 6.0 px per character at a 16 px root font. Measured off
  `US-0112-50-narrow-900.png`: 58 characters occupy ≈357 px, i.e. ≈6.2 px per character. The
  budget is therefore ~3-6 % optimistic, so the rustdoc at `status_text.rs:124` and the packet
  Gaps line *"errs narrow"* are inaccurate in the direction that matters.

### US-0113

- **Clamp arithmetic.** `clamp_right_dock_width` (`mod.rs:52-61`) re-implemented and swept:
  window 0 or negative → unchanged; 500 → 240 (floor, 48 % of the window); 600 → 240 (40 %);
  685 → 240 (the floor/ceiling crossing, exact); 686 → 240.1; 900 → 315; 1900 → 490
  unchanged; 100000 → unchanged. A requested width below the ceiling is never widened. The
  floor means the terminal keeps the majority only down to a ~480 px window; below that the
  dock wins. That is the packet's stated choice and it is documented.
- **Startup clamp, live.** First launch at 900 px with no `docks.json`: dock 317 px, terminal
  583 px (independent capture). The seam is `apply_center_reset` (`layout.rs:40-47`), which
  clamps the loaded width before applying it.
- **Resize round trip, live, no false drag.** 1200 → 900 → 1600 in one session with a clean
  `docks.json`: dock 420 → 317 → 480. `docks.json` after the session holds `"size": 480.0` —
  the user's preference, not the clamped 315. So `apply_right_dock_width` (`mod.rs:339-353`)
  and `track_preferred_right_dock_width` (`mod.rs:314-333`) do not mistake their own
  programmatic clamp for a drag: every OneTerm-owned seam sets exactly
  `clamp(preferred, window)`, which is the value the tracker compares against.
- **The resize seam exists and is the right one.** `cx.observe_window_bounds`
  (`reference/zed/crates/gpui/src/app/context.rs:425-438`, *"invoked when the window is
  resized"*). `set_dock_size` only notifies when the value actually changes
  (`reference/gpui-kit/crates/base/src/dock/dock_area.rs:384-392`), and `save_layout` is
  debounced by 2 s (`mod.rs:367-396`), so a live window drag does not thrash `docks.json`.
- **The kit's own floor is below OneTerm's.** `PANEL_MIN_SIZE = px(100.)`
  (`reference/gpui-kit/crates/base/src/resizable/mod.rs:14`) and `Dock::set_size` clamps to it
  (`dock_placement.rs:156-158`), so the 240 px floor is never silently rewritten by the kit —
  one way the preference could have been clobbered, and it is not.
- **The persistence gap is real, and worse than the packet states.** See finding **M3**.

## Findings

### Major

- **M1 (`US-0112`) — a ticked acceptance row is not met: the memory unit is still cut, and the
  git branch is cut at every width tested.** `STATUS_BAR_RESERVED` is a fixed 440 px that
  ignores the git-status and net-speed indicators, both of which are `Shorten::Never`.
  Captures: 700 px → `MEM 174.1 M` (unit cut, dock button clipped away); 900 px and 1200 px →
  branch clipped mid-token with no ellipsis. The code comment at `status_text.rs:85-88`
  (*"Bounding the one unbounded indicator keeps the rest on screen"*) and the paragraph added
  to `docs/gui-layout.md` §Status bar assert the opposite of what the frames show. A fix that
  would clear this: derive the reservation from the indicators that are actually visible (the
  sampler already knows whether git status and net speed have labels), or give the right-hand
  group layout priority so the breadcrumb is the element that shrinks.

- **M2 (`BUG-0067`) — the packet's only gpui test cannot fail on the bug, and one ticked
  acceptance row is unsatisfiable under the fix that was chosen.**
  `reopening_a_collapsed_right_dock_keeps_its_width` (`layout_tests.rs:225-246`) drives only
  `DockArea::toggle_dock` and `dock_util::set_right_dock_open` — neither of which the diff
  touches — so it passes unchanged against every mutation of the three functions this packet
  and `US-0113` added (verified: it stayed green while all three decision-function mutations
  failed their own tests). The Plan row *"write the regression test first … and watch it
  fail"* is therefore not supported by what was delivered; the packet's Gaps section admits
  the click is unreachable headlessly but does not say this test is bug-independent.
  Separately, acceptance row 1 — *"then click the title bar's already-highlighted mode: the
  dock reopens"* — is ticked, but under fix (b) the already-highlighted mode after a
  dock-button collapse is **None**, and clicking **None** correctly leaves the dock closed
  (`actions.rs:124-128`). The sequence that actually reopens the dock clicks *a different*
  mode and therefore takes the `switch_right_dock_mode` branch, which **rebuilds** the panel
  (`actions.rs:160` → `build_named_panel`) — the exact path the packet's own Risks
  section says to keep a reopen away from. The sentence added at `docs/gui-layout.md:32`
  (*"Selecting the mode that is already selected … reopens a collapsed dock without
  rebuilding the panel"*) now describes a path fix (b) has made unreachable for the dock
  buttons; it still holds for a click on an already-open dock. Nothing here is a functional
  regression against `main` — the dead end is genuinely gone — but the record overstates what
  was verified.

- **M3 (`US-0113`) — the stored width ratchets down and never recovers.** `save_layout` and
  the startup reset persist the *applied* width, so a single session at a narrow window
  rewrites the user's preference permanently. Measured: after one 700 px session,
  `target/docks.json` held `"size": 240.0`, and the next 1200 px launch started the dock at
  240 px instead of 480 px — it does not return when the window is wide again, because the
  in-memory preference is seeded from the file. The packet's Gaps section states the
  narrow-restart half of this, and the HLD accepts *"the kit rewrites the clamped value on
  the next save"*, so it is a disclosed design consequence rather than an undisclosed defect —
  but the packet's own Risk line (*"a user who narrows the window once loses their wide dock
  forever"*) is precisely what now happens, and the wording *"the preference does not survive
  a restart"* understates it: the loss is monotonic across sessions and silent. Verdict for
  the packet stays PASS-WITH-NOTES because the behaviour is documented and reversible by one
  drag, but this is the item to reopen if the width matters.

### Minor

- **m1 (`BUG-0067`) — `shows_agent` is not scoped to the right dock.** `mod.rs:286-292` calls
  `find_tab_node_by_panel_name`, which walks **Center first**, then Left/Right/Bottom
  (`crates/state/src/dock_util.rs:76-105`). If the Agent panel is ever reachable outside the
  right dock, reopening an SSH Client dock would select **Agent** in the title bar. Not
  reachable today — `OneTermDockSkin` suppresses the tab bar for those leaves
  (`dock_skin.rs:116`) and only the right dock ever builds `panel_names::AGENT` — so this is
  latent. A one-line scope to `dock_area.layout(DockPlacement::Right)` would remove it.
- **m2 (`BUG-0067`) — a legacy `ui_config.json` can still show the stale mode briefly.**
  `sync_right_dock_mode` runs only on a dock notification, and it early-returns when the
  layout has no right dock at all (`mod.rs:282-284`). A file saved by an older build (mode
  `SshClient`, dock closed) shows `SSH Client` selected over a collapsed dock until the first
  dock notification. Self-correcting and one click recovers it.
- **m3 (`US-0112`) — `elide_path_left(path, 0)` returns `…`, one column over budget**, and
  the test named `the_cut_never_lands_inside_a_component` only asserts that the result is a
  suffix of the input, which the over-long-component branch satisfies while cutting inside a
  component. Both are cosmetic: the product never passes a budget under ~20.
- **m4 (`US-0112`) — the tooltip still reads only "Click to copy"** (`status_text.rs:261`).
  An elided path has no way to show its full value on hover. Not in the packet's acceptance.
- **m5 (`US-0113`) — a drag above the ceiling is honoured until the next window resize**, then
  snapped back to the ceiling without warning. This is the documented design (*"a dragged
  width is honoured as it is, and becomes the remembered preference"*), but it means the
  35 % ceiling is not an invariant of the UI, only of the widths OneTerm itself applies.
- **m6 (docs) — `docs/gui-layout.md` §Persistence is now slightly stale.** It still says *"A
  loaded document feeds the right-dock layout, width, and open state"* with no mention that
  the width is clamped on apply and the clamped value is what gets written back. §Dock
  composition and §Status bar carry the new rules and are otherwise accurate; §Dock
  composition's first new sentence is the one qualified by **M2**.
- **m7 (evidence) — the `BUG-0067` frames were captured against the BUG-0067 commit, not the
  final tree.** `BUG-0067-24b-after-clicking-sshclient-again.png` shows a 487 px dock in a
  1200 px window (40.6 %), which `US-0113` later made impossible. The re-takes in this
  directory show the same scene at 423 px on `40420965`.

## Mutation testing

Each mutation was applied alone or in one batch, `cargo test -p oneterm-workspace` was run,
and the file was restored with `git checkout --` (working tree verified clean afterwards).

| Mutation | Result |
|---|---|
| `clamp_right_dock_width`: return `requested` instead of `requested.min(ceiling)` | `the_right_dock_width_is_clamped_to_a_share_of_the_window` FAILED at `layout_tests.rs:175` (the first assertion) |
| `right_dock_mode_for`: `(false, _) => None` → `(false, mode) => mode` | `right_dock_mode_follows_the_dock_state` FAILED at `layout_tests.rs:198` |
| `clicked_mode_index`: `checks[ix] != (ix == current_ix)` → `checks[ix] && ix != current_ix` (the pre-fix predicate) | `clicking_the_selected_segment_reports_it_too` FAILED at `title_bar.rs:180` |
| `elide_path_left`: `<= max_chars` → `< max_chars` | `a_path_that_fits_is_shown_whole` FAILED at `status_text.rs:279` |

In every one of these runs `reopening_a_collapsed_right_dock_keeps_its_width` and
`switch_right_dock_mode_swaps_panel_and_keeps_width` stayed green — the evidence behind **M2**.

## Gaps in this verification

- **A real splitter drag was not exercised.** The same limitation the implementer recorded:
  posted `WM_*` messages do not reach the kit's resize handle. The drag path is covered here
  by code reading plus the resize round trip, which exercises the same preference tracker.
- **The "clicking None twice leaves the dock closed" and "clicking the selected mode over an
  open dock does not rebuild" rows** were confirmed by reading `actions.rs:113-139`, not by a
  frame; the re-taken walk covered the collapse and the reopen only.
- **`sync_right_dock_mode` still has no headless test**, as the packet says. The GUI re-take
  is the proof that it runs.
- **Only the Windows path was walked.** No macOS or Linux behaviour was checked.
- The GUI walk used its own binary, its own pid, and a shell cwd inside this worktree; the
  debug build keeps its configuration in `<worktree>/target/`, so no user-level state was
  touched.



---

# Re-verification of `aa783f25` — 2026-09-17

Verifier: an independent session that did not write the code and did not write the first
report above. Target: `worktree-agent-a26570dbee8c60926` at `aa783f25` (the rework commit
`595965a6` plus a merge of `main` @ `ffc02c19`). Diff read: `git diff ebf33ad7...HEAD`.

## Verdicts

| Packet | First verdict | Now |
|---|---|---|
| `BUG-0067` — reselecting the right-dock mode reopens the dock | PASS-WITH-NOTES | **PASS** |
| `US-0112` — the status bar elides the path from the left | **FAIL** | **PASS** |
| `US-0113` — the right dock width follows the window | PASS-WITH-NOTES | **PASS** |
| **Overall** | FAIL | **PASS** |

All three majors are fixed, and each was re-proved here rather than read off the packet: `M1`
from my own 700 px frame, `M2` by mutating the new decision function and watching the new test
fail, `M3` by re-running the seeded-preference sequence end to end. Six of the seven minors are
fixed; `m2` is consciously left and its reason now holds more strongly than before. The seven
new findings below are all minor or notes — none blocks acceptance.

## Commands (final lines)

| Command | Final line |
|---|---|
| `cargo test -p oneterm-workspace` | `test result: ok. 34 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s` |
| `pwsh scripts/ci-local.ps1` | `ci-local: all checks passed.` |
| mutation — `reopen_or_rebuild` always `Rebuild` | `test result: FAILED. 32 passed; 2 failed` |
| mutation — the `Show` arm secretly rebuilds the panel | `test result: FAILED. 33 passed; 1 failed` (`reopening must not rebuild the panel`) |
| mutation — `state_with_preferred_width` is a no-op | `test result: FAILED. 33 passed; 1 failed` |
| mutation — `right_dock_panel_mode` reads `Center` | `test result: FAILED. 33 passed; 1 failed` |
| mutation — `divide_centre` drops the path floor | `test result: FAILED. 33 passed; 1 failed` |
| mutation — `divide_centre` ignores the icon chrome | `test result: FAILED. 31 passed; 3 failed` |
| mutation — `elide_path_left(_, 0)` returns `…` again | `test result: FAILED. 32 passed; 2 failed` |
| mutation — `status_font_size` measures at `1.0 rem` | `test result: ok. 34 passed` — **no test catches it** (`n2`) |
| mutation — the drag-cap re-apply branch disabled | `test result: ok. 34 passed` — **no test catches it** (`n3`) |

Every mutation was applied alone and reverted immediately; `git status --short` was empty after
each one.

## Per-finding status

| Finding | Status | How it was checked here |
|---|---|---|
| **M1** — memory unit cut, git branch hard-cut, dock button clipped | **Fixed** | Own 700 px frame (`US-0112-reverify-700-git-cwd.png`): `MEM 175.7 MB` with its unit, the dock-toggle button on the bar, branch `worktree-agent-a26570dbee8c…` elided *with* an ellipsis, path `…\workspace` elided from the left. Same at 1000 px (`US-0113-reverify-1000-legacy-600.png`) and at 1184 px. |
| **M2** — the reopen rebuilt the panel; the test could not fail | **Fixed** | Mutating `reopen_or_rebuild` to always `Rebuild` fails two tests; a subtler mutation that keeps the returned `DockModeAction::Show` but rebuilds inside the `Show` arm still fails `reopening_a_collapsed_right_dock_keeps_its_panel_instance` at the `PanelId` assertion. The test is no longer bug-independent. |
| **M3** — the stored width ratcheted down | **Fixed** | Seeded `docks.json` at `400.0`, launched at 1200 px (dock ≈ 400), narrowed to 700 px and waited past the 2 s debounce (dock ≈ 240, the floor), then `Stop-Process -Force` — no exit hook. `docks.json` still read `400.0`, and the relaunch at 1200 px came back at ≈ 400 px (`US-0113-reverify-1200-after-a-700-kill.png`). The first report measured `240.0` here. |
| **m1** — `shows_agent` not scoped to the right dock | **Fixed, and covered** | `right_dock_panel_mode` reads `dock_area.layout(DockPlacement::Right)` only; pointing it at `Center` fails a test. |
| **m2** — a legacy `ui_config.json` shows a stale mode briefly | **Left, reason holds** | Still reachable: `sync_right_dock_mode` runs only on a dock notification, and startup skips `switch_right_dock_mode` when the saved mode is `SshClient`. It is now strictly better than before, because clicking the segment that is (wrongly) highlighted takes the `Show` path and opens the dock — the display and the click finally agree. |
| **m3** — `elide_path_left(_, 0)` returned `…` | **Fixed, and covered** | Both rules return `""` at budget 0; `nothing_ever_comes_back_wider_than_its_budget` sweeps every budget for both, and restoring the old behaviour fails it. `the_cut_lands_on_a_separator_while_one_fits` now asserts the boundary, not just a suffix relation. |
| **m4** — the tooltip only said "Click to copy" | **Fixed** | `StatusText::render` builds the tooltip from the full sampled text whenever shortening changed it, falling back to "Click to copy". Untested, as the packet's Gaps say. |
| **m5** — a drag above the ceiling stood until the next resize | **Fixed in code, untested** | `track_preferred_right_dock_width` re-applies the clamp when the reported size is over the ceiling. See `n3`. |
| **m6** — `docs/gui-layout.md` §Persistence stale | **Fixed** | §Persistence now states that the stored width is the preference and that the clamp applies on load; §Dock composition and §Status bar were rewritten to match the code. |
| **m7** — the BUG-0067 frames predated the final tree | **Fixed** | Both `BUG-0067-24*.png` are re-captured in the rework commit and show a 420 px dock in a 1200 px window, which is the 35 % ceiling. |

## What was checked against the implementer's claims

- **The kit's zero-basis claim is accurate.**
  `reference/gpui-kit/crates/component/src/status_bar.rs:97-103` — the centre region is
  `region().flex_1()`, and `flex_1` (`reference/zed/crates/gpui/src/styled.rs:181-186`) sets
  `flex_grow = 1`, `flex_shrink = 1`, `flex_basis = relative(0.)`. A zero base size gives the
  centre a scaled shrink factor of zero, so it is never the item that absorbs shrinkage; while
  the pinned ends fit, the centre simply grows into what they leave. `n4` records the part of
  the guarantee the packet does not state.
- **The measurement font matches the rendered font.** `status_font_size` is
  `window.rem_size() * 0.75` and `text_xs()` is literally `rems(0.75)`
  (`reference/zed/crates/gpui/src/styled.rs:545-548`) — the same value, not an approximation.
  Family and weight come from the same `window.text_style()` stack in both places: `Root` pushes
  `cx.theme().font_family` (`reference/gpui-kit/crates/component/src/root.rs:591`) above both
  `OneTermWorkspace::render` (where `build_status_bar` measures) and `StatusText::render` (where
  the glyphs are shaped), and nothing on the status-bar path overrides the weight. No drift.
  `style.to_run(text.len())` is correct — `TextRun::len` is in bytes.
- **`divide_centre` at the extremes.** Swept with a temporary probe test (removed afterwards;
  the tree was verified clean), four visible indicators, 300 px of fixed labels, a 200 px
  branch. `shrinkable` is `window − BAR_CHROME − ICON_CHROME·icons − fixed`:

  | `shrinkable` | path | git | sum |
  |---|---|---|---|
  | 0 | 0 | 40 | **40 (over)** |
  | 10 | 0 | 40 | **40 (over)** |
  | 39 | 0 | 40 | **40 (over)** |
  | 40 | 0 | 40 | 40 |
  | 50 | 10 | 40 | 50 |
  | 119 | 79 | 40 | 119 |
  | 120 | 80 | 40 | 120 |
  | 121 | 80 | 41 | 121 |
  | 280 | 80 | 200 | 280 |
  | 1 000 000 | 999 800 | 200 | 1 000 000 |

  Monotone, never negative, and exact from 40 px up; below 40 px it over-allocates by at most
  40 px — see `n1`. With no branch at all the whole centre goes to the path, at every width.
- **The remaining constants are right at the default.** `ICON_CHROME = 16` is exactly
  `Icon::xsmall` (`Size::XSmall => size_3()` = 0.75 rem = 12 px,
  `reference/gpui-kit/crates/component/src/icon.rs:161`) plus `gap_1` (0.25 rem = 4 px).
  `BAR_CHROME = 116` accounts for the bar's `px_2` (2 × 8), the two outer `gap_2`, the right
  region's four `gap_2`, the centre's three `gap_2` and four 1 px separators — 92 px — leaving
  ~24 px for the ghost xsmall dock button. The gap counts are **constant whether or not an
  indicator is visible**, because a hidden `StatusText` still renders its (empty) root div, so a
  fixed constant is the right model rather than a lucky one. The larger-UI-font half of the
  question is not reachable today: nothing in OneTerm calls `set_rem_size`; see `n6`.
- **Every `docks.json` write is routed through `state_with_preferred_width`.** A grep over
  `crates/` for `dump(cx)` / `save_state_logged` / `DockAreaState` finds exactly four production
  call sites — `save_layout_on_exit` (`mod.rs:395-400`, shared by both exit hooks), the
  debounced save (`mod.rs:420-431`), `reset_center_only` (`layout.rs:22-25`) and
  `reset_default_layout` (`layout.rs:92-96`) — and all four rewrite the width first. Every write
  then funnels through the one `save_state_to`. `crates/sftp-ui/src/persistence.rs` also writes
  `docks.json`, but only the `sftp_table_state` field through `update_dock_document_at`, and
  never touches `dock_state`.
- **The startup preference is the raw stored value.** `preferred_right_dock_width` is read
  (`mod.rs:193-197`) *after* `load_layout` and *before* `reset_center_only`, so it is the
  document's own number; the clamp is applied later, by `apply_center_reset`. Confirmed live: a
  legacy `docks.json` at `600.0` opened in a 984 px viewport applies ≈ 344 px (35 %) and leaves
  `600.0` in the file (`US-0113-reverify-1000-legacy-600.png`). A first launch with no file
  stores the `480.0` default, not the 414 px it applied.
- **A preference below the floor is applied as it is.** `clamp_right_dock_width` is a ceiling
  only — `MIN_RIGHT_DOCK_WIDTH` floors the *ceiling*, never the requested width — so a 150 px
  preference stays 150 px at any window size, with the kit's own `PANEL_MIN_SIZE = px(100.)` as
  the only lower bound. That matches the rustdoc and is not a defect.
- **The sync path does not fight the contains-based decision.**
  `on_action_set_right_dock_mode` applies the mode first and reads `UiConfig` afterwards, so the
  dock observer (which runs in the following effect flush) always sees the settled dock and
  early-returns when `next == current`. Walked by hand through all six (contained, requested)
  pairs plus collapse-then-reopen and None-twice. The "collapse by key binding, then again"
  attack has no target: `SetRightDockMode` is dispatched only from the title bar's segmented
  control (`title_bar.rs:149`) and appears in no keymap.
- **Click-to-copy still copies the sampled value**, taken before `shorten.apply` in
  `Render for StatusText`.

## New findings

### Minor

- **`n1` (`US-0112`) — `divide_centre` can hand out 40 px more than the centre has.** When
  `shrinkable` falls below `MIN_BRANCH_WIDTH`, the branch budget is floored at 40 px without
  being capped by what is left
  (`(shrinkable - MIN_PATH_WIDTH).max(MIN_BRANCH_WIDTH).min(git)`), so the two budgets sum to
  40 px while the region is narrower — see the table above. Harmless in practice: both the kit's
  `region()` and OneTerm's inner div are `overflow_hidden`, so the excess clips *inside* the
  centre and cannot reach the pinned ends, and reaching it needs a window under roughly 560 px
  with every indicator live. The existing test
  `a_window_too_narrow_for_anything_never_returns_a_negative_budget` asserts non-negativity but
  not that the budgets fit. One `.min(shrinkable)` on the branch would close it.
- **`n2` (`US-0112`) — nothing pins the measurement font size.** Changing `status_font_size`
  from `rem_size * 0.75` to `rem_size * 1.0` leaves all 34 tests green. The value is correct
  today and the failure mode errs narrow (over-elision, never overflow), but the whole packet
  rests on measure and render agreeing, and only a comment says they do.
- **`n3` (`US-0113`) — the new drag-cap branch is uncovered.** Disabling
  `if clamp_right_dock_width(size, window_width) != size { self.apply_right_dock_width(…) }` in
  `track_preferred_right_dock_width` leaves all 34 tests green, and a real splitter drag still
  cannot be driven headlessly or by posted `WM_*` messages. `m5`'s fix therefore rests on code
  reading alone — the same gap the first report recorded for the drag path, now attached to a
  behaviour change rather than to unchanged code.

### Notes (no action implied)

- **`n4` (`US-0112`) — the pinned-ends guarantee is load-bearing on `overflow_hidden`, not only
  on `flex_1`.** `region()` (`status_bar.rs:83`) sets `.overflow_hidden()`, which is what
  suppresses a flex item's automatic (min-content) minimum size; with `overflow: visible` the
  zero basis alone would not stop a long centre label pushing the ends. Separately, the left and
  right regions keep the default `flex-shrink: 1` with an `auto` basis, so once the *pinned*
  content alone exceeds the window — roughly below 400 px with every indicator live — they do
  shrink and clip. What is delivered is "the centre can never push the ends", not "the ends
  never clip"; the code comment and `docs/gui-layout.md` state the stronger form.
- **`n5` (`BUG-0067`) — two notions of "the mode showing" now coexist.**
  `right_dock_panel_mode` answers from the panel the dock contains (the click path), while
  `right_dock_mode_for` prefers the persisted mode whenever the dock is open (the sync path).
  They agree on every reachable sequence today because `apply_right_dock_mode` makes them agree
  before persisting. If they diverged, the title bar would highlight one mode while a click
  rebuilt to another — self-healing in one click. No test pins the agreement.
- **`n6` — the px constants are exact at the default rem size only.** gpui's spacing tokens are
  `rems` (`gap_2` = `rems(0.5)`), so `BAR_CHROME` and `ICON_CHROME` drift if a UI zoom is ever
  added. Nothing calls `set_rem_size` today, so there is no reachable case; the arithmetic above
  is the record of what the constants stand for.
- **`n7` — the `f32::MAX` budget seed (`mod.rs:265-278`) is on a dead path.**
  `build_status_bar` writes both budgets earlier in the same render pass than the indicators
  read them, so the seeded value is never the one a frame uses. The comment ("they start wide
  enough that the first frame shows the labels whole") describes a case that cannot occur.

## Gaps in this re-verification

- **No GUI click walk for `BUG-0067`.** The reopen was not re-driven through the title bar in a
  live window; posted messages do not reach the kit's toggle group. It is covered here by the
  `PanelId` test (proved to fail under a rebuild, by mutation), by reading the handler, and by
  the implementer's re-captured frames.
- **No real splitter drag** (`n3`) — the same limitation both earlier sessions recorded.
- **The net-speed indicator was hidden in every frame I captured** (an idle terminal samples no
  traffic), so the `M1` scene with *all five* indicators live was not photographed. The layout
  argument does not depend on it — `divide_centre` treats net speed as fixed width, and the
  pinned ends are protected by the flex layout either way — but the widest-content case is
  reasoned, not seen.
- **Only Windows, at 100 % DPI.** A 1200 × 800 window gives a 1184 px viewport, so the clamp
  numbers above are 35 % of 1184 / 984 / 684, not of 1200 / 1000 / 700.
- **`docks.json` seeding, not dragging**, is how a non-default preference was created — the same
  substitution the implementer used.
- The walk used its own build, launched its own pid, addressed only that pid's window and closed
  only that pid; configuration stayed in `<worktree>/target/`.
