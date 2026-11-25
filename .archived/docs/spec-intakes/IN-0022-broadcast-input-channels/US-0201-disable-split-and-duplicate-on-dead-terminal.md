# Work: The terminal's right-click menu disables Split and Duplicate Session once its session has ended

ID: US-0201
Intake: IN-0022
Created: 2026-10-22

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

- Change type: existing-contract change (the terminal-area context menu)
- Risk lane: normal (menu enablement only; no persisted schema, no secret handling, no external effect)
- Spec Intake, when required: IN-0022 (owner ruling 2026-10-22, translated: "on a terminal whose session has ended, the terminal area's right-click menu shows Split and Duplicate Session disabled instead of enabled; everything else in that menu is unchanged"). IN-0022 owns it because `US-0199` and `US-0200` already define what a dead Space may do in the tab and terminal menus (a dead Space cannot join an Input Channel, its row is disabled), and this packet reuses their liveness test and their disabled-row style.

## Outcome

Right-clicking a terminal whose session has ended (local shell exited, SSH closed or dropped; `session.alive()` is false, the liveness `TerminalView::session_alive` has used since `US-0200`) shows, in the terminal area's menu: the four "Split Right / Left / Up / Down" rows disabled, and the "Duplicate Session" row disabled (a plain disabled row with no submenu, the shape `US-0199` gives the dead tab's "Input Channel" row). A running terminal's menu is exactly what it was. Every other row (New Terminal, Copy, Paste, Select All, Clear, Log, Close Terminal Tab, Close Space) is unchanged for both.

## Scope

- [x] In scope: `MenuContext` carries `session_alive`; `build_menu` renders the Duplicate Session row and the four split rows disabled when it is false; `split_items` gets a `disabled` argument; tests; the docs that describe the terminal-area menu.
- [x] Out of scope: the empty-Space placeholder menu (it has no terminal, so its splits stay enabled); the tab menu's "Duplicate" row; the `DuplicateSession` / `Split*` key-bound action handlers (decision in the Plan); a new feature for the Edit rows and Log (rework 1 only strips the click path from their disabled states, see the Plan rule).

## Acceptance

- [x] Dead session: the four split rows and the Duplicate Session row are disabled; Duplicate Session has no submenu.
- [x] Live session: the four split rows are enabled and Duplicate Session is the submenu it was.
- [x] Other rows are unaffected by liveness (Copy still follows the selection; New Terminal, Paste, Select All, Clear, Close Terminal Tab, Close Space are built the same).
- [x] The empty-Space placeholder menu still has its four splits enabled.
- [x] A disabled row does nothing: on a dead terminal, hover + Enter and a click on each of the four splits and Duplicate Session leave the tab with one Space and dispatch no `DuplicateSession` (window test over the real `build_menu`); the same rule holds for every other disabled row we build.
- [x] Full `pwsh scripts/ci-local.ps1` passes; `cargo build -p oneterm-app` builds.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0022-broadcast-input-channels/IN-0022.md`, `US-0199`, `US-0200` (liveness rule and the disabled-row precedent).
- `docs/terminal-split/04-context-menu.md` (the terminal Space menu layout, Split R/L/U/D, Duplicate Session submenu).
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/low-level-design/input.md` "Context menu" (the builder's row order).
- `docs/gui-layout.md` (terminal tab menu and Broadcast input channels; where the terminal area's menu is mentioned).
- `docs/spec-intakes/IN-0003-duplicate-terminal-sessions/IN-0003.md`, `IN-0006`: the Duplicate Session contract (unchanged: a live session still duplicates exactly as before).

### Documentation Action

- Update required: `docs/terminal-split/04-context-menu.md` (a dead terminal's Split and Duplicate Session rows are disabled), `IN-0018` LLD `input.md` (menu builder), `docs/gui-layout.md` (one sentence beside the dead-terminal Input Channel note), `IN-0022.md` (packet link).

Reason: the docs describe the menu as unconditionally enabled.

### Reconciliation

Changed: `docs/terminal-split/04-context-menu.md` (the dead-terminal bullet), `IN-0018` LLD `input.md` (menu builder), `docs/gui-layout.md` (Broadcast input channels), `IN-0022.md` (packet link, ticked). Rework 1 added the "a disabled menu row carries no click path" rule to `docs/terminal-split/04-context-menu.md` and the IN-0018 `input.md` LLD. `IN-0003` / `IN-0006` unchanged: a live session duplicates exactly as before.

## Context

- **Menu builder.** `crates/terminal-view/src/input/menu.rs` `build_menu(menu, &MenuContext, ...)`, called from the wrapper's `context_menu` closure in `terminal_view/render.rs`; the closure runs on every right-click, so reading `session.read(cx).alive()` there is always current. `PopupMenu` keeps its item list `pub(crate)`, so tests inspect the `PopupMenuItem` values the builder helpers return (the `Item { disabled, .. }` and `Submenu { .. }` variants are public).
- **Shared sinks.** `split_items` is also used by the empty-Space placeholder menu (`space/render.rs`), whose Space has no terminal: it passes `disabled = false`.
- **Other entry points to the same commands.** `DuplicateSession` and `SplitRight|Left|Up|Down` actions handled by `TerminalPanel` on the active Space (key bindings, none bound by default, `DEC-0018`); the tab menu's "Duplicate" (`panel/tab_title.rs`).

## Plan

- [x] `MenuContext.session_alive: bool`, set in the `context_menu` closure from `session.read(cx).alive()` (the closure holds the session, not the view; it is the same `alive()` that `TerminalView::session_alive` reads).
- [x] `build_menu`: if `!ctx.session_alive` the Duplicate Session entry is `PopupMenuItem::new("Duplicate Session").action(DuplicateSession).disabled(true)` (first version; reworked, see below) a plain disabled row with no submenu, the shape of `US-0199`'s dead "Input Channel" row, and the splits pass `disabled = true`.
- [x] Rule (rework 1, advisor FAIL finding 1): **a disabled menu row carries no click path** (no `.action(..)`, no `.on_click(..)`). The kit's `confirm` (`popup_menu.rs:861-892`, Enter) runs a row's handler or action without checking `disabled`, and hovering a greyed row selects it (`menu_item.rs:116-118`), so hover + Enter would still split the tab or dispatch `DuplicateSession` through a row the menu shows greyed. This overturns the first version's reason for keeping the action ("so the shortcut hint column does not change"): a greyed row now shows no shortcut hint, which is what a disabled row should show. Applied with `PopupMenuItem::disabled(d).when(!d, |item| item.action(..).on_click(..))`:
  `split_item`, `dead_duplicate_item` (no action at all), and, same hole, every other disabled row we build: Copy without a selection (`edit_item` takes a `disabled` argument), Log Start while running and Stop while not, and the tab menu's Close Others / Close to the Right when they would close nothing (`close_scope_row`). Rows that already carried nothing (the tab menu's disabled "Input Channel", "No saved sessions") are unchanged.
- [x] Test seam (rework 1, finding 4): `gpui-component`'s `test-support` feature in this crate's dev-dependencies (it turns on `gpui-base/test-support`, and only it also implements the scroll traits for the observed wrapper `snapshots` needs: `gpui-base`'s feature alone does not compile). It adds `gpui-fast`, `gpui-fast-macros` and `taffy 0.14` to `Cargo.lock`, dev-only.
- [x] `split_items(..., disabled)`; the item construction factored into `split_item` so a test can read the built item.
- [x] Choice, key-bound and tab-menu paths: **menu only.** The owner's words are the terminal area's menu. Refusing in `duplicate_session_to` would be one line, but it also changes the tab menu's "Duplicate", which the owner did not mention and which is the only way to reconnect a dropped SSH tab (it reopens the connection); refusing the split actions needs a look-up of the Space's view in the shared `split_active_at`, which the placeholder menu also uses (an empty Space has no view and must still split). Neither is a one-line guard on the action handler alone, so neither is done. **Owner ruling 2026-10-22: keep** — key-bound split and duplicate and the tab menu's Duplicate stay enabled on an ended terminal.

## Decisions

No decision record: it is a menu-enablement rule inside IN-0022's liveness model (`US-0199`, `US-0200`), and the Plan records the choices.

## Verification Plan

- `cargo test -p oneterm-terminal-view`: unit tests on the `PopupMenuItem`s the builder helpers return (`split_item` enabled and disabled; the disabled Duplicate Session row is `disabled` and not a submenu; the live one is the submenu), and the existing `menu_tests.rs`.
- Full gate `pwsh scripts/ci-local.ps1`; `cargo build -p oneterm-app`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- Code: `MenuContext::session_alive` (read from `session.alive()` in the `context_menu` closure of `terminal_view/render.rs`, i.e. on every right-click); `build_menu` builds `dead_duplicate_item()` instead of the Duplicate Session submenu and passes `disabled = !session_alive` to `split_items`; `split_items` now folds over `split_item`; the placeholder menu (`space/render.rs`) passes `false` (`crates/terminal-view/src/input/menu.rs`).
- Tests (`input/menu_tests.rs`): `split_items_follow_the_session_liveness` (each direction: enabled live, disabled dead, same label), `the_dead_duplicate_row_is_a_plain_disabled_row`, `disabled_rows_carry_no_click_path` (row model: the four disabled splits and the dead Duplicate row carry no handler and no action; the enabled splits do), `a_dead_terminals_menu_greys_split_and_duplicate_and_they_do_nothing` (window test over the real `build_menu` and `gpui_base::test_support::snapshots`: the eleven expected labels are drawn; for Duplicate Session and each split, hover + Enter and a click leave `leaf_count()` at 1 and dispatch no `DuplicateSession`) and its control `a_live_terminals_split_row_splits_on_hover_and_enter` (the same drive on a running terminal splits, so the harness does reach the rows). Mutation check: restoring the action and click handler on a disabled split row fails both the row-model test and the window test.
- Gap closed: the earlier gap (`PopupMenu` keeps its item list `pub(crate)`) is covered by the window test through the drawn rows. Still not unit-tested: the live Duplicate Session row being the submenu (covered by the ui-verifier walk, `evidence/US-0201-verify.md`).
- Rework 1 (advisor FAIL, then ui-verifier PASS for what is walkable): finding 1 (hover + Enter acted on disabled rows) fixed at the root in `split_item` and `dead_duplicate_item`; finding 2 (same hole: Copy, Log Start/Stop, Close Others / to the Right) fixed by the same rule; finding 3 (records) this file and `menu.rs` rustdoc; finding 4 (test gap) the window test. The advisor's two `should_panic` pins became the two plain tests above.
- Gate: `pwsh scripts/ci-local.ps1` (`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`): after rework 1 every step passed (5120 tests passed summed over every test run in the script; `verify-dependency-graph.py` passes with the new dev-dependency feature) except the last, `third-party-notices.py --check`, which failed because `Cargo.lock` gained `gpui-fast`, `gpui-fast-macros` and `taffy 0.14` (dev-only, from the kit's `test-support` feature); `THIRD-PARTY-NOTICES.md` was regenerated (+3 packages, 866 total) and the check then passed. `cargo build -p oneterm-app` built.
- Key bindings and tab menu: not refused (Plan). Recommendation: refuse nothing further for the splits (a split of a dead tab yields an empty Space the user can fill with New Terminal Here, which stays useful) and keep the tab menu's Duplicate enabled (it is the reconnect path of a dropped SSH tab); if the owner wants consistency, a guard in `duplicate_session_to` is one line but also disables that reconnect path. The menu now disagrees with those two paths on a dead terminal, by decision. Owner ruled 2026-10-22: keep them enabled.
- Not walked on screen: the change is visible, so `ui-verifier` is dispatched.

## Handoff

None.
