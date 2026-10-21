# US-0197 verification (advisor)

- Packet: `US-0197-resizable-rail-width.md`; commits `9498eeb3` (packet) and `96270e82` (code) of `git diff afb39845..96270e82`, branch `verify/IN-0055-r3` in worktree `agent-aa7d464a81f2a206b`. Part A of the same diff (US-0196 rework 2) is reviewed in `US-0196-verify.md`.
- Read: `tab_rail.rs` (`clamp_tab_rail_width`, `RailFrame`, `Resize`, `begin_frame`, `track_group`, `handle`, `tracker`, `finish_resize`), `dock_skin.rs:147-192`, `ui_config.rs:462-481`, the new workspace tests, the packet, the HLD § Width / Rail anatomy, `docs/gui-layout.md` "Resizing", `docs/agents/persistence.md`; kit `base/src/dock/tab_group.rs:691-728` (hook order); gpui 0.3.9 `window.rs:5432-5445` (`on_mouse_event`), `:4139-4145` (`set_window_cursor_style`), `:2710-2733` (`on_next_frame` / `request_animation_frame`), `:2376-2381` (`refresh`).
- No code changed, no pinned test added: no defect.

## Verdict

**PASS.** The clamp reproduces the HLD's worked examples, the drag is live in memory with one store on release, the group bound re-applies on the next frame without a loop, `draws_rail` is settled once per frame in the order base guarantees, the elevated window writes nothing, and the records match. Two low documentation nits.

## The coder's points

1. **`on_prepaint` + `request_animation_frame` re-clamp; hook ordering.** `track_group` requests a frame only when the group's painted *width* differs from the one recorded last frame (`tab_rail.rs`, `group.replace(bounds).size.width != bounds.size.width`), and `begin_frame` reads that recorded width to clamp the rail. The rail's width is inside the group's, and the group's width is set by its dock split, not by its content, so the rail changing width cannot change the group's width: after the one extra frame the comparison is equal and nothing more is requested. No feedback and no loop; `request_animation_frame` is the right call from inside a draw (it schedules a `notify` of the current view, `window.rs:2730-2733`), and the test `narrowing_the_window_clamps_the_rail_and_widening_it_restores_the_preference` pins that exactly one more frame is asked for. The once-per-frame assumption holds: base evaluates `renderer.frame(..)` first and then, as method-chain arguments in source order, `render_tab_bar(..)` and `content_frame(..)` within one `render` (`tab_group.rs:691-728`); `frame` is unconditional, so `draws()` / `width()` are never read before `begin_frame` in a frame. The one dependency is on base's call order, which the comment in `dock_skin.rs` records.
2. **Drag tracker.** `on_mouse_event` and `set_window_cursor_style` both `debug_assert_paint` (`window.rs:5436`, `:4140`), and the tracker registers them inside a `canvas` paint closure, which is a paint; the tracker exists only while `resize != Idle`, so no listener is installed in the idle state. Transitions: mouse-down on the handle stops propagation and sets `Pressed`, then `window.refresh()` from an event listener (outside the draw, so it is effective) paints the tracker next frame; a move with the left button down sets `Dragging(pointer.x - group.origin.x)`; a move with the button *not* down (released outside the window) or a mouse-up ends it through `finish_resize`, which stores only from `Dragging` and never from `Pressed` (a click stores nothing: tested). A mouse-up that lands between the mouse-down and the tracker's first paint leaves `Pressed`; the tracker then paints and the next pointer move (button not down) clears it; harmless and self-healing. Two rails in a split centre have separate state and trackers. The handle is `occlude()`d over the right 4 px of the overlay scrollbar; the kit thumb is wider than 4 px, so the scrollbar stays reachable; this is the packet's own walk item.
3. **Persistence of the width.** One write per drag: `finish_resize` -> `set_vertical_tabs_width(round(width))`, which clamps to 140..=400 and returns before touching the entity when the value is already stored (`ui_config.rs:468-473`; `the_width_setter_clamps_and_skips_an_unchanged_value`). The move frames change only the rail's `Cell`, never the entity (0 observer updates during the move, 1 at release: `a_drag_resizes_live_and_stores_the_width_once_on_release`). The stored value is the drag clamped to the absolute bounds, not the group ceiling, as the HLD's `preference = clamp(dragged, MIN, MAX)` says (tested: 400 stored while 280-odd is drawn). Elevated: the setter calls `persist`, which asks `write_refusal` and returns before any write; the entity is updated first so the rail keeps the width in memory.

## Everything else

- `clamp_tab_rail_width` reproduces the six HLD rows, the four window rows and the 65 % terminal share from 400 px up (unit tests), and the split-centre test proves each rail is bounded by its own group.
- `UiConfig::read_vertical_tabs` is read once per group per frame in `begin_frame`; `dock_skin.rs` no longer calls `draws_rail` three times (advisor F4 closed).
- The rows follow the live width (`the_rows_follow_the_live_width_of_the_rail`: row width = rail width - 1 px border), which is what Part A's start-truncated path is measured against.
- No new crate edge (`oneterm-workspace` already depends on `oneterm-settings`); `verify-dependency-graph` is in the coder's full gate.

## Findings

1. **Low (record)** - `US-0197-resizable-rail-width.md:32` names the cursor `ResizeLeftRight`; the code and `docs/gui-layout.md` say column-resize (`CursorStyle::ResizeColumn`, `tab_rail.rs` `RESIZE_CURSOR`, asserted by `the_handle_has_the_resize_cursor`). One word in the packet.
2. **Low (record)** - the packet's Gaps say a release outside the window "is ended by the next pointer move without the button; not covered by a test"; true, and fine, but the same sentence belongs in the Context list of open choices rather than only in Gaps, since it is the designed behaviour. Optional.

## For the owner

- The walk items the packet lists: the drag in both themes, the cursor, the handle against the overlay scrollbar with 40 tabs, restart persistence, narrowing and widening the window, a split centre, the elevated window.
- Release the button outside the window during a drag, then move the pointer back in: the rail should keep the width where the pointer left and store it once.

## Gate (one crate per command, `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`, worktree target reused; app not built)

| Command | Result |
| --- | --- |
| `cargo clippy -p oneterm-terminal / oneterm-settings / oneterm-state / oneterm-terminal-view / oneterm-workspace --all-targets -- -D warnings` | all green |
| `cargo test -p oneterm-terminal` | 183 passed, 0 failed |
| `cargo test -p oneterm-settings` | 55 passed, 0 failed |
| `cargo test -p oneterm-state` | 76 passed, 0 failed |
| `cargo test -p oneterm-terminal-view` | 412 passed, 0 failed, 3 ignored |
| `cargo test -p oneterm-workspace` | 100 passed, 0 failed, 3 ignored |
| `python scripts/check-english.py` | passed |

Disk D: 10.7 GB free at start, 9.9 GB after the builds. Not re-run: the full `ci-local.ps1` (recorded in the packet), the app binary.

## UI walk (rework 2 + US-0197, ui-verifier, head 96270e82)

### UI verification IN-0055 r3 (two-line rail rows, resizable rail), head 96270e82

Build `in0055-bin4\oneterm.exe`, REAL USERPROFILE kept, scratch cwd `us0196-cwd-w3` (config under its `target\`). Shots `in0055r3-*.png`. Widths below are measured from pixel scans (rail border column), client px.

**Local shells still died** with the real profile: no cmd/pwsh child under the app, log shows "terminal session is closed". So no OSC 7501, no `cd`, no OSC 7 path: all Part A items that need status or cwd are NOT WALKED. Not caused by the profile; the agent environment blocks ConPTY children. Needs an owner run.

### Part A
| # | Verdict | Evidence |
|---|---|---|
| 1 | PARTLY | Row height 64 px in Zed One Dark (`01`) and Zed One Light (`06`). Chips and recording dot on line 1 NOT WALKED. |
| 2 | NOT WALKED | needs status/cwd. With no OSC 7 the row shows no path before the title only (`01`, `05`): no stray path. |
| 3, 4, 5 | NOT WALKED | need cwd/OSC 7 |
| 6 | PARTLY | Selected-row accent bar still shows (`01`, `05`, `06`). Grip level, reorder, Space drag, drop line NOT WALKED in this build. |
| 7 | PARTLY | 40 tabs scroll, scrollbar thumb drag works and stays clear of the resize handle (`02`, `03`); icon clearance NOT WALKED. |
| 8 | PARTLY | No × in any row (`01`, `05`, `06`). Middle-click and context menu NOT WALKED in this build. |
| 9 | NOT WALKED | switch-off not retried in this build |

### Part B
| # | Verdict | Evidence |
|---|---|---|
| 1 | PASS (cursor NOT WALKED) | Handle sits at the rail border (grab x = border-1). Dragging resizes live and the terminal follows (220 to 338 to 140 to 400 to 300). Hover and drag show a 3 px accent line (dark `04`, 82,139,255). Line-2 path re-truncation NOT WALKED (no path). Cursor shape not visible. |
| 2 | PASS | Drag far left: 140 px. Far right at 1280: 400 px (the 400 cap; 35 % would be 442). Window 1000 wide: 344 px; 800 wide: 274 px, i.e. 35 % of the group (window minus 16 px borders), slightly under the 350/280 quoted because the group is 984/784 px. |
| 3 | PASS | `ui_config.json` held `{"vertical_tabs":{"enabled":true,"width":300}}`; after restart the rail was 300 px. |
| 4 | PASS | Window 1000 to 344, 800 to 274, back to 1280 restored 400 (preference kept). |
| 5 | PASS | Right dock open: rail 288 px (35 % of the narrower group); closed: 400 again. |
| 6 | PASS | Split centre (grip drag to the right edge): both rails 221 px, each bounded by its own group (`05`) although the preference was 300. |
| 7 | PASS | Dragging the scrollbar thumb with 40 tabs scrolled the rows and did not resize the rail (`03`). |
| 8 | PASS | Hover line visible in dark (82,139,255 at x404..406, `04`) and light (82,111,255 at x225..227). |

### Defects
None found in what could be walked.

### Owner-only / needs a working shell
Part A 2-5, the status icon positions, path truncation at the start, `~`, split-tab cwd, chips on line 1, grip level with line 1, reorder and Space drag on 64 px rows, middle-click/menu on this build, switch-off strip, resize cursor shape.

Coordinator: merged on the advisor PASS for both parts and the Part B walk PASS. Part A items needing a live shell (status icon, cwd line, home abbreviation, split-tab cwd, chips on line 1, grip level) were not walkable because the agent environment blocks ConPTY children; they are pinned by terminal-view layout tests and left to the owner on screen.
