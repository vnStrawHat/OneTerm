# US-0196 verification (advisor)

- Packet: `US-0196-vertical-tab-rail.md`; coder head `75cf9cc1` on base `c2b7792e`; diff `git diff c2b7792e..75cf9cc1`.
- Worktree: `.claude/worktrees/agent-a9322689c5c9f01fe`, branch `verify/US-0196`.
- Read: the full diff, `tab_rail.rs`, `tab_rail_tests.rs`, `dock_skin.rs`, `tab_row_layout.rs`, `ui_config.rs`, `tab_title.rs`, `panel/tests.rs`, `general.rs`, the intake, HLD and spec review; the kit's `base/src/dock/tab_group.rs` (`TabGroupContext`, the column layout), `base/src/dock/dock_area.rs:1127-1135` (one renderer per group, cached), `base/src/tabs.rs:149-234`, `component/src/dock/tab_panel.rs:433-687`; gpui 0.3.9 `app.rs:1308` / `:2131-2137`, `view.rs:530-557` (refresh and cached views), `elements/div.rs:4039-4065` (group hitbox stack).
- No code was changed. No pinned test was added: no defect in code was found (the findings below are documentation).

## Verdict

**PASS.** Every gate re-run green; the five weak points the coder named hold; the six deviations are sound and recorded. Two low documentation findings and two informational notes.

## Gate re-run (one crate per command, `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`)

| Command | Result |
| --- | --- |
| `cargo clippy -p oneterm-settings --all-targets -- -D warnings` | green |
| `cargo clippy -p oneterm-state --all-targets -- -D warnings` | green |
| `cargo clippy -p oneterm-workspace --all-targets -- -D warnings` | green |
| `cargo clippy -p oneterm-terminal-view --all-targets -- -D warnings` | green |
| `cargo clippy -p oneterm-settings-ui --all-targets -- -D warnings` | green |
| `cargo test -p oneterm-settings` | 55 passed, 0 failed |
| `cargo test -p oneterm-state` | 74 passed, 0 failed, 1 ignored (doc-test) |
| `cargo test -p oneterm-workspace` | 88 passed, 0 failed, 3 ignored (the `IN-0043` elevation tests) |
| `cargo test -p oneterm-terminal-view` | 406 passed, 0 failed, 3 ignored |
| `cargo test -p oneterm-settings-ui` | 58 passed, 0 failed, 1 ignored |
| `cargo fmt --all -- --check` | clean |
| `python scripts/check-theme-contrast.py` | 1638 pairings all >= 4.5:1; 624 hierarchy comparisons pass (matches the packet) |
| `python scripts/verify-dependency-graph.py` | passed, 20 packages; no manifest changed in the diff |
| `python scripts/check-english.py` | passed, 1135 files |
| `python scripts/check-doc-paths.py` | passed, 219 paths in 12 documents |

Not re-run: the full `ci-local.ps1` (the coder's run is recorded in the packet; the five touched crates and every script the diff affects were re-run here), the `oneterm-app` binary (left for the UI walk).

## The coder's weak points

1. **Live switch through `cx.refresh_windows()`.** Holds in the real tree, not only in the test host. `refresh_windows` queues `Effect::RefreshWindows` (gpui `app.rs:1308`), which sets `window.refreshing = true` and marks the window dirty (`app.rs:2131-2137`); a cached view (`AnyView::cached`, used by the workspace for the title bar and status bar and by the kit for the active panel) re-renders whenever `window.refreshing` is set (`view.rs:535`). Independently of that, the tab-group render reads the `UiConfig` entity (`dock_skin.rs:130`), which `detect_accessed_entities` records, so the `cx.notify()` in the setter would re-render it anyway. The test `the_settings_setter_flips_the_layout_on_the_next_frame` drives the real `OneTermTabGroupSkin` and the real `DockArea` inside a window; the only thing the test host lacks is the workspace's title and status bar, which do not sit above the dock area.
2. **Scoped global `TabRowLayout`.** Nothing else builds a title inside the scope: `TabRowLayout::Vertical.draw` (`tab_rail.rs:288-301`) wraps exactly one `handle.title(window, cx)` (or a `tab_name` label), `Tabs::children` is eager, and `render_tab_strip` builds elements only; the closures it registers (context menu, drags, clicks) run later, outside the scope, which is why the menu wording is captured into the closure (`tab_title.rs:357`, `:485`). The grip's own preview (`RailDragPreview::render`, `tab_rail.rs:407-412`) and the kit's `DragPanelPreview` therefore draw the horizontal row, as intended and as `the_rail_draws_its_rows_in_the_vertical_layout_and_nothing_else_does` proves. Early return: not possible (closure form). Panic: the previous value is not restored on unwind (`tab_row_layout.rs:30-36`) — immaterial at runtime because a panic inside a render is fatal to the window, but the packet calls it an "RAII guard", which it is not (finding 1).
3. **Drag arbitration.** The grip's `DragPanel` (`tab_rail.rs:334-344`) and the title row's `DragTerminalTab` (`tab_title.rs:285-296`) have disjoint hitboxes, so the arbitration no longer depends on bubble order at all, which is simpler than the strip. Base `Tab` stops only left mouse-down (`base/src/tabs.rs:172`) after the children have seen it. The split across crates is forced by R4 and is complete: the workspace test proves grip -> reorder and title row -> its own drag with no reorder, the terminal-view test proves the real row starts `DragTerminalTab` (accepted by a `DragTerminalTab` drop target) and the `x` starts nothing. `close_button_visible` is the kit's default `false` (`component/src/dock/mod.rs:158`) and OneTerm never sets it, so the row's own `x` is the only close control in both layouts, as before.
4. **`width` / `collapsed` inert.** Parsed, clamped, round-tripped and omitted while default (tests `the_vertical_tab_width_is_clamped_on_read`, `vertical_tabs_survive_a_load_and_a_save`); nothing reads them (`RAIL_WIDTH` is the constant, `tab_rail.rs:34`). `docs/agents/persistence.md` and `docs/gui-layout.md` both say so. Acceptable for this packet.
5. **Rail rule.** `dock_skin.rs:129-144` reads the switch, excludes a collapsed group, locates the group's node in `DockArea::layout(Center)` and applies `tab_rail::draws_rail` (non-empty, all `terminal`). The right dock keeps the kit strip (`other_groups_keep_what_they_draw_today`). The `DockArea` read from inside a tab group render is safe: the area's lease ends when its `render` returns, before child views are laid out, and the test exercises exactly this path.

## Persistence (`ui_config.json`)

- Absent -> default; `{"vertical_tabs":{"enabled":true}}` -> on; `"x"`, `5`, `true`, `[true]`, `null`, wrong-typed fields -> that field's default with a warning, the rest of the document intact, no quarantine (`a_garbage_vertical_tabs_value_loads_as_the_default`, which goes through `parse_document`, the function `load_from` quarantines on).
- Default omitted on write (field-level `skip_serializing_if` plus the object-level `is_default`); `{enabled:false, width:300}` writes `{"width":300}`.
- Width: `as_f64`, rounded, floored at 0, clamped to 140..=400 on read, never rewritten by the read.
- Older build: `UiConfig` has no `deny_unknown_fields`, so the field is ignored and dropped on that build's next write (the existing `a_config_from_the_agent_panel_era_loads_and_heals` proves unknown fields pass).
- Elevated window: the setter changes the entity then calls `persist`, which asks `write_refusal(is_restricted())` and returns before any write (`ui_config.rs:476-481`); the layout flips in memory. No setter-level test exists (finding 2).

## Crate rules, script, docs

- No `Cargo.toml` changed; `TabRowLayout` lives in `oneterm-state` (L2), the lowest crate both L3 consumers reach (R10); the shell names no feature type (R4). `verify-dependency-graph.py` passes.
- `check-theme-contrast.py`: `tab_bar.background` added to `tab.active.foreground`; the rail draws that pair on hover (`tab_rail.rs:372`) and the strip already did; numbers match the packet.
- `docs/gui-layout.md` "Vertical tabs" section matches the code (rule, seam, anatomy, grip, scroll, menu wording, colours, elevated window, inert fields); the dock-button sentence is qualified. `docs/agents/persistence.md` row and `docs/agents/structure.md` entries are accurate.
- Deviations 1-6 in the packet: each checked against the code; all sound. Deviation 4 (the `DragPanel` on the grip only) gives the HLD's visible behaviour with no overlapping hitbox.

## Findings

1. **Low — packet text: "RAII guard" is not what was built.** `US-0196-vertical-tab-rail.md:77` says the layout is set "(RAII guard)"; `crates/state/src/tab_row_layout.rs:30-36` is set / call / set-back with no `Drop` type, so the previous layout is not restored if the closure unwinds. No runtime effect (a render panic is fatal), but the record should say what the code does, or the code should match the record (a 6-line guard struct). Reproduction: read the two lines.
2. **Low — packet acceptance claims a test that does not exist.** `US-0196-vertical-tab-rail.md:46` ticks "an elevated window writes nothing (existing `write_refusal` test plus a setter-level assertion)"; the Evidence section (`:114`) says the setter has no write path of its own and names only the existing `write_refusal` test. There is no assertion on `set_vertical_tabs_enabled` under elevation (it would need the process-global flip of the `IN-0043` ignored tests). Drop the words "plus a setter-level assertion" or add the test to the ignored set.
3. **Info — HLD "Readers" paragraph is stale after deviation 1.** `high-level-design.md:265-267` still says `render_tab_strip` reads `UiConfig::global(cx)`; the packet records the deviation but the HLD was not amended. One sentence: the row reads `oneterm_state::TabRowLayout`, set by the rail.
4. **Info — `draws_rail` is evaluated three times per group per frame** (`dock_skin.rs:153`, `:167`, `:178`), each upgrading the area and walking the centre tree. Negligible today; worth a single evaluation per frame if `US-0197` adds per-frame work to it.

## For the owner to check by hand (the UI walk)

- The real window: switch on and off from Settings with two or more tabs open; the sessions keep running; the terminal is resized, not covered.
- Row behaviours in the rail: click, middle-click, `x`, double-click rename, right-click menu (reads "Close Tabs Below"), status icon, channel chips, recording dot; the grip appears on hover with the open-hand cursor; drag by grip reorders; drag by label into an empty Space works and dropped on another row does nothing.
- 40 tabs: wheel scroll in the rail; `+` scrolls the new tab into view.
- Split centre: two rails, one per group; a terminal tab dropped into the right dock keeps the strip there.
- Zoom and lock screen with the rail on; restart persistence; an elevated window (switch works, `ui_config.json` untouched).
- Light and dark themes: hovered and selected row text readable.
