# Work: The terminal's right-click menu disables Split and Duplicate Session once its session has ended

ID: US-0201
Intake: IN-0022
Created: 2026-10-22

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

- Change type: existing-contract change (the terminal-area context menu)
- Risk lane: normal (menu enablement only; no persisted schema, no secret handling, no external effect)
- Spec Intake, when required: IN-0022 (owner ruling 2026-10-22, translated: "on a terminal whose session has ended, the terminal area's right-click menu shows Split and Duplicate Session disabled instead of enabled; everything else in that menu is unchanged"). IN-0022 owns it because `US-0199` and `US-0200` already define what a dead Space may do in the tab and terminal menus (a dead Space cannot join an Input Channel, its row is disabled), and this packet reuses their liveness test and their disabled-row style.

## Outcome

Right-clicking a terminal whose session has ended (local shell exited, SSH closed or dropped; `session.alive()` is false, the liveness `TerminalView::session_alive` has used since `US-0200`) shows, in the terminal area's menu: the four "Split Right / Left / Up / Down" rows disabled, and the "Duplicate Session" row disabled (a plain disabled row with no submenu, the shape `US-0199` gives the dead tab's "Input Channel" row). A running terminal's menu is exactly what it was. Every other row (New Terminal, Copy, Paste, Select All, Clear, Log, Close Terminal Tab, Close Space) is unchanged for both.

## Scope

- [x] In scope: `MenuContext` carries `session_alive`; `build_menu` renders the Duplicate Session row and the four split rows disabled when it is false; `split_items` gets a `disabled` argument; tests; the docs that describe the terminal-area menu.
- [x] Out of scope: the empty-Space placeholder menu (it has no terminal, so its splits stay enabled); the tab menu's "Duplicate" row; the `DuplicateSession` / `Split*` key-bound action handlers (decision in the Plan); the Edit rows and Log (already shaped by their own state).

## Acceptance

- [ ] Dead session: the four split rows and the Duplicate Session row are disabled; Duplicate Session has no submenu.
- [ ] Live session: the four split rows are enabled and Duplicate Session is the submenu it was.
- [ ] Other rows are unaffected by liveness (Copy still follows the selection; New Terminal, Paste, Select All, Clear, Close Terminal Tab, Close Space are built the same).
- [ ] The empty-Space placeholder menu still has its four splits enabled.
- [ ] Full `pwsh scripts/ci-local.ps1` passes; `cargo build -p oneterm-app` builds.

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

To be filled at completion.

## Context

- **Menu builder.** `crates/terminal-view/src/input/menu.rs` `build_menu(menu, &MenuContext, ...)`, called from the wrapper's `context_menu` closure in `terminal_view/render.rs`; the closure runs on every right-click, so reading `session.read(cx).alive()` there is always current. `PopupMenu` keeps its item list `pub(crate)`, so tests inspect the `PopupMenuItem` values the builder helpers return (the `Item { disabled, .. }` and `Submenu { .. }` variants are public).
- **Shared sinks.** `split_items` is also used by the empty-Space placeholder menu (`space/render.rs`), whose Space has no terminal: it passes `disabled = false`.
- **Other entry points to the same commands.** `DuplicateSession` and `SplitRight|Left|Up|Down` actions handled by `TerminalPanel` on the active Space (key bindings, none bound by default, `DEC-0018`); the tab menu's "Duplicate" (`panel/tab_title.rs`).

## Plan

- [ ] `MenuContext.session_alive: bool`, set in the `context_menu` closure from `session.read(cx).alive()` (the closure holds the session, not the view; it is the same `alive()` that `TerminalView::session_alive` reads).
- [ ] `build_menu`: if `!ctx.session_alive` the Duplicate Session entry is `PopupMenuItem::new("Duplicate Session").action(DuplicateSession).disabled(true)` (no submenu, same shape as `US-0199`'s dead "Input Channel" row; the action stays so the shortcut hint column does not change) and the splits pass `disabled = true`.
- [ ] `split_items(..., disabled)`; the item construction factored into `split_item` so a test can read the built item.
- [ ] Choice, key-bound and tab-menu paths: **menu only.** The owner's words are the terminal area's menu. Refusing in `duplicate_session_to` would be one line, but it also changes the tab menu's "Duplicate", which the owner did not mention and which is the only way to reconnect a dropped SSH tab (it reopens the connection); refusing the split actions needs a look-up of the Space's view in the shared `split_active_at`, which the placeholder menu also uses (an empty Space has no view and must still split). Neither is a one-line guard on the action handler alone, so neither is done; the recommendation goes in the report for an owner ruling.

## Decisions

No decision record: it is a menu-enablement rule inside IN-0022's liveness model (`US-0199`, `US-0200`), and the Plan records the choices.

## Verification Plan

- `cargo test -p oneterm-terminal-view`: unit tests on the `PopupMenuItem`s the builder helpers return (`split_item` enabled and disabled; the disabled Duplicate Session row is `disabled` and not a submenu; the live one is the submenu), and the existing `menu_tests.rs`.
- Full gate `pwsh scripts/ci-local.ps1`; `cargo build -p oneterm-app`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

To be filled after implementation.

## Handoff

None.
