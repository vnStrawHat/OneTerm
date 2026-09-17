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


