# Work: The session context menu gains Duplicate and Move to Group

ID: US-0129
Intake: IN-0042
Created: 2026-09-18

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability
- Risk lane: normal
- Spec Intake, when required: `IN-0042` — `docs/spec-intakes/IN-0042-ux-polish-round-1/IN-0042.md`

## Outcome

A saved SSH session can be copied and re-grouped from the place the user right-clicks it. The
session leaf's context menu offers **Duplicate Saved Session** — which writes a new saved
session copying every field of the source and opens its Properties dialog so it can be renamed
— and **Move to Group ▸**, a submenu of the groups that already exist plus "No group" and
"New group…". Both are store writes; `ssh_session.json` keeps schema v2 unchanged.

## Findings and proposals covered

`F24`, the half `US-0119` declined:

> | F24 | session context menu | … No Duplicate, no Move to Group. |

`US-0119` fixed the order, the separator, the danger styling and the wording, and recorded in
its Gaps: *"Duplicate and Move to Group are still absent (`F24` notes them; `P10` did not
propose them). They are new capabilities, each independently acceptable, and belong in their
own packets if the owner wants them."* The owner asked for them on 2026-09-18. This is that
packet; `US-0119` is not reopened, because its own acceptance lines all held.

## Scope

- [ ] In scope:
  - `crates/session-ui/src/tree_render.rs` — `SESSION_MENU_ROWS` gains two rows, and the menu
    builder gains the Duplicate handler and the Move to Group submenu.
  - `crates/session-ui/src/session_state.rs` — `SshSessionStore::duplicate` and
    `SshSessionStore::set_group`, with the pure helpers behind them.
  - `crates/session-ui/src/session_dialog.rs` — `open_session_dialog` learns to put the initial
    focus in the Group field, for "New group…"; `existing_group_names` becomes crate-visible so
    the submenu and the dialog list the same groups.
  - `docs/ssh-client-connect.md` §6.5, `docs/gui-layout.md`, and the `F24` row of the round's
    before/after report.
- [ ] Out of scope:
  - The live tab's **Duplicate** (`SshDuplicateConfig`, `US-0116`'s tab menu). It reconnects a
    *running* session; this packet copies a *saved* one. Nothing is shared between them, and
    the row is named so they cannot be read as the same command — see Decisions.
  - Drag-and-drop between groups in the tree, and multi-select. Neither was asked for.
  - Renaming or deleting a group (`Rename Group…` already exists on the group folder).
  - Any `ssh_session.json` schema change — see Documentation.

## Acceptance

- [ ] The session leaf's context menu reads, top to bottom: `Open`, `Properties`,
      `Duplicate Saved Session`, `Move to Group ▸`, separator, `New Session`, separator,
      `Delete` (danger). `SESSION_MENU_ROWS` still describes it as data and its tests assert
      that order.
- [ ] Duplicate writes a new saved session that copies **every** field of the source — host,
      port, username, auth method, key path, colour, group, logging, jump host, port forwards,
      agent forwarding — under a new stable id that is not the source's and is never a reused
      one.
- [ ] The copy's label is the source's with `" (copy)"` appended, or `" (copy 2)"`,
      `" (copy 3)"`, … when the earlier candidate is already a label in the store.
- [ ] The copy sits immediately after its source in store order (`ssh_session.json`), not at the
      end of the list.
- [ ] Duplicate opens the **Properties** dialog on the copy and connects nothing.
- [ ] `Move to Group ▸` lists the groups that exist, plus `No group` and `New group…`, with the
      session's current group checked. Choosing one rewrites that session's `group` and the tree
      re-sorts under the new folder.
- [ ] `New group…` opens the Properties dialog for that session with the initial focus in the
      Group field.
- [ ] `ssh_session.json` stays at `schema_version: 2` and gains no field; a duplicate and a
      group change are ordinary store writes.
- [ ] `cargo test -p oneterm-session-ui` passes.
- [ ] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

## Documentation

### Owning Docs Reviewed

- `docs/ssh-client-connect.md` §6.5 ("Entry points and menus as they stand") — the table that
  lists what a right-click on a session offers. **Update required:** two rows are added to that
  menu.
- `docs/ssh-client-connect.md` §1.1 and §6.6 — which surfaces create a session, and the
  New/Edit dialog. **Update required:** Duplicate is a third way a saved session comes into
  existence, and the dialog gains an initial-focus rule.
- `docs/gui-layout.md` §Dock composition — describes the session panel's header and, in the
  centre-tab paragraph, the terminal tab's own `Duplicate` row. **Update required:** the two
  new rows, and the sentence that keeps the saved-session copy and the live-tab duplicate
  apart.
- `docs/agents/persistence.md` §Schema owners (`ssh_session.json` row) and §Migration rules —
  read to confirm the claim in Acceptance. **No change:** the row already says schema v2 gives
  every session a stable `id` and records `next_session_id`, and that whole-document writes are
  coalesced through the single-flight queue. A duplicate is `next_id` handed out exactly as
  `add` hands it out, and a group change is the same field `rename_group` already rewrites, so
  no field, no version and no migration is involved. The rules say a version is added "when the
  first incompatible schema change is introduced"; there is none here.
- `docs/spec-intakes/IN-0042-ux-polish-round-1/US-0119-…md` — its Scope declines exactly this
  work and its Gaps says where it belongs. Read so this packet takes the declined half whole
  and does not re-litigate what `US-0119` settled (order, separator, danger, wording).
- `docs/spec-intakes/IN-0042-ux-polish-round-1/evidence/before-after-report.md` §4.6 and its
  `F24` row — the round's record that these two are absent. **Update required.**
- `crates/core/src/session_duplicate.rs` (`SshDuplicateConfig`) — read to be sure this packet
  shares nothing with it and to name the new row so the two are not confused. **No change.**
- `docs/PROJECT.md` — read for standing invariants. **No change.**

### Documentation Action

Update required: `docs/ssh-client-connect.md` §1.1, §6.5 and §6.6; `docs/gui-layout.md`;
`docs/spec-intakes/IN-0042-ux-polish-round-1/evidence/before-after-report.md` (`F24` row and
the `US-0119` line of §4.6); `IN-0042.md`'s packet list.

Reason: this packet adds two rows to a documented menu and a third way a saved session is
created, both described in the owning design.

### Reconciliation

Changed: `docs/ssh-client-connect.md` §1.1, §6.5, §6.6; `docs/gui-layout.md` (two paragraphs);
`evidence/before-after-report.md` (`F24` row, §4.6 `US-0119` row, plus a `US-0129` row);
`IN-0042.md` (packet list). `docs/agents/persistence.md` and `docs/PROJECT.md` needed no
change, for the reason recorded above — it still holds: nothing in `ssh_session.json` changed
shape.

## Context

- The store already hands out ids from one counter (`SshSessionStore::add`) and already
  rewrites the `group` field of many sessions at once (`rename_group_in`). Duplicate and
  Move to Group are the single-session versions of those two operations, so they are written
  next to them as the same kind of pure helper plus a thin `&mut self` method that notifies and
  saves.
- The tree sorts alphabetically within a group, so "right after the source" is a statement
  about `ssh_session.json` and the `+` menu (which follows store order), not about where the
  copy appears in the right dock.
- The kit's `PopupMenu::submenu` wants `&mut Context<PopupMenu>`, which a tree context-menu
  builder does not have — `Tree::context_menu` hands the builder a `&mut Context<TreeState>`.
  The kit supports exactly this case through `PopupMenuItem::submenu(label, entity)` plus
  `PopupMenu::build(window, cx, …)`, and wires the parent link on the parent's next render
  (`reference/gpui-kit/crates/component/src/menu/popup_menu.rs:1389-1405`, whose comment names
  "contexts that only have the menu value").
- `ComboboxState` is `Focusable`, so the Group field has a handle; `FormDialog::on_render` plus
  `common::defer_initial_focus_once` is the pattern the connect and quick-connect dialogs
  already use to put the caret somewhere after the dialog exists.

## Plan

- [ ] Store: `copy_label`, `duplicate_in`, `set_group_in` + the two `&mut self` methods.
- [ ] Dialog: optional initial focus on Group; `existing_group_names` crate-visible.
- [ ] Menu: two rows in `SESSION_MENU_ROWS`, the Duplicate handler, the submenu builder.
- [ ] Tests for the four pure pieces.
- [ ] Docs, then the GUI walk.

## Decisions

No decision record. The one naming choice is recorded here rather than in `docs/decisions/`
because it binds this menu and nothing future work must inherit:

**The row is "Duplicate Saved Session", not "Duplicate".** A terminal tab's own context menu
already has a row called **Duplicate** (`US-0116`), and it does something else entirely: it
reopens the *running* connection in a second tab through `oneterm_core::SshDuplicateConfig`,
carrying the live host, auth choice, jump chain and port forwards, and it connects. This row
writes a second row into `ssh_session.json` and connects nothing. The two live in different
menus, but a user who has seen both should not have to remember which is which, so the saved
one says what it copies. For the same reason the submenu reads **Move to Group**, matching the
`Rename Group…` wording already on the group folder.

## Verification Plan

1. **Focused:** `cargo test -p oneterm-session-ui` over the four pure pieces — the copy-label
   rule including the `(copy 2)` step, the duplicate's field-for-field copy, its placement and
   its id, the group rewrite (including blank → ungrouped and a no-op for an unknown id), and
   `SESSION_MENU_ROWS` as data. This extends the tables `US-0119` and `TEST-19` already test.
2. **Unit:** `cargo test -p oneterm-session-ui`.
3. **Integration:** `cargo test --workspace`.
4. **Platform:** `pwsh scripts/ci-local.ps1`.
5. **E2E (GUI walk):** against a seeded `target/ssh_session.json` in the worktree's own config
   directory, driving only the walk's own process id.
   - the session context menu with both new rows;
   - the `Move to Group` submenu open, showing `No group`, the existing groups with the current
     one checked, and `New group…`;
   - a duplicate in the tree under its source with the Properties dialog open on it;
   - a session that has moved into another group's folder.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Risks

- **Two "Duplicate"s.** Mitigated by the row name; see Decisions.
- **A copy that is not a copy.** `SshSession` gains fields over time (`jump_host`,
  `port_forwards`, `agent_forwarding` all arrived after v2). A hand-written field list would
  silently stop copying the next one, so the duplicate is `session.clone()` with the label
  replaced — the compiler keeps it complete, and a test asserts the clone equals the source
  apart from the label.
- **An id that is not new.** Reusing an id inside one document breaks the guarantee schema v2
  exists for. The duplicate takes `next_id` and advances it, exactly as `add` does.
- **A group typo splitting a group.** `Move to Group` only offers names that already exist, so
  the submenu cannot create `Infra` beside `infra`; the free-text route stays in the dialog,
  which is where it was.
- **Focus that does not land.** Deferred focus into a dialog is timing-dependent. It reuses the
  helper two other dialogs already use and is proved by a frame, not by assertion.

## Evidence and Gaps

### What was built

- **`SshSessionStore::duplicate(id, cx) -> Option<SshSessionId>`** — clones the source's
  `SshSession` whole, replaces only the label, inserts at `source_index + 1`, takes `next_id`
  and advances it, then notifies and saves. `None` when the id is gone (the same no-op shape
  `update` and `remove` already have).
- **`session_state::copy_label`** — `"{label} (copy)"`, then `"{label} (copy 2)"`, `3`, … until
  no session in the store carries that exact label. Duplicating a copy therefore gives
  `prod (copy) (copy)`: the rule appends to whatever label it was handed, which is what the
  owner asked for and is one rule instead of two.
- **`SshSessionStore::set_group(id, group, cx)`** — trims, treats blank as ungrouped (the
  convention `rename_group_in` and `build_tree_items` already share), and is a no-op when the
  value would not change or the id is gone.
- **The menu.** `SESSION_MENU_ROWS` is `[Open, Properties, Duplicate, MoveToGroup, Separator,
  NewSession, Separator, Delete]` — still the only description of the order, still data, and
  its tests now pin the two new rows between Properties and the first separator.
- **The submenu** is built when the parent menu is built, so it reads the store at right-click
  time and can never list a stale group. It is attached with `PopupMenuItem::submenu` over a
  `PopupMenu::build`, the path the kit documents for a context that holds only the menu value.
- **"New group…"** opens the Properties dialog for that session with the caret in the Group
  combobox. `open_session_dialog` gained a `focus_group: bool`; every other caller passes
  `false`, so nothing else changed its behaviour.
- **Duplicate saves the copy before the dialog opens, and that dialog's Cancel keeps it.**
  Cancel discards the edits, not the duplicate — which is what makes the row "Duplicate"
  rather than "New from…"; undoing one means deleting the copy. Recorded here and in §6.5
  after the verification found it undocumented (F3).

### After the independent verification (`evidence/US-0129-verify.md`, PASS with findings)

- **F1 — the submenu now scrolls.** `PopupMenu::item()` never turns scrolling on (only the
  kit's `with_menu_items` builder does), and an unscrollable menu has no height cap at all, so
  a user with enough groups would have lost the rows past the window bottom — `New group…`
  among them, since it is last. `move_to_group_submenu` now calls `.scrollable(...)` with the
  same row estimate and the same cap (half the window, at most 450px) the centre `+` menu and
  the SFTP overflow menu already use. Frame `US-0129-06`.
- **F2 — the checked row is computed from the normalized group.** `set_group_in`'s trim rule
  is now the shared `session_state::normalized_group`, and the submenu checks through it, so a
  hand-edited `"group": " infra "` checks `infra` instead of checking nothing. Test:
  `a_padded_stored_group_normalizes_to_the_row_it_should_check`.
- **F3, F4** — documentation, above and in the frame list.
- **The `(copy) (copy)` rule stays.** The verifier recommended against stripping a trailing
  ` (copy N)`: a label is free text, so `prod (copy)` may be a name the user chose, and
  turning a copy of it into `prod (copy 2)` would state the wrong provenance silently. The
  Properties dialog opens with the Label field at the top, which is where that is fixed.

### Commands

- `cargo test -p oneterm-session-ui` — 76 passed, 0 failed (72 before this packet, plus
  `copy_label_numbers_only_the_candidates_already_in_use`,
  `duplicate_copies_every_field_next_to_its_source`, `set_group_moves_exactly_one_session`;
  `tree_render::tests` gained `duplicate_and_move_to_group_join_the_session_block` and its two
  existing tests were widened to the new row table).
- `pwsh scripts/ci-local.ps1` — "ci-local: all checks passed".

### Evidence frames

Walked against a seeded `target/ssh_session.json` (two ungrouped sessions, an `infra` group of
two), in the worktree's own `target/` config directory, driving only the walk's own pid.

- `evidence/US-0129-01-session-context-menu.png` — Open / Properties / Duplicate Saved Session /
  Move to Group ▸ / — / New Session / — / Delete, Delete still in the danger colour.
- `evidence/US-0129-02-move-to-group-submenu.png` — the submenu open beside its row: `No group`
  **checked** (the right-clicked `DevServer` has none), `infra`, a separator, `New group…`.
- `evidence/US-0129-04-moved-into-group.png` — after clicking `infra` in that submenu,
  `DevServer` is inside the `infra` folder and gone from the root. The frame shows the tree
  only; that the write landed as `"group": "infra"` on id 1, with `schema_version` still 2 and
  no new field, was read from `target/ssh_session.json` during the walk and is not visible in
  the capture (verification F4).
- `evidence/US-0129-06-thirty-group-submenu.png` — the same submenu against a store seeded with
  30 groups (verification F1). It stops at the cap — about 445px, ending well inside the 918px
  window at `group-17` — instead of the ~900px of rows it holds. That cap is the proof the fix
  is live: the kit applies `max_height` only `when(self.scrollable, …)`, so before this change
  the popup had **no** height limit at all and the rows past the window bottom, `New group…`
  among them, were unreachable. The scroll gesture itself is not in the frame; see Gaps.
- `evidence/US-0129-03-duplicate-and-properties.png` — `Duplicate Saved Session` on `Staging`:
  `Staging (copy)` in the tree directly under `Staging`, with **Edit SSH Session** open on the
  copy carrying host `10.0.0.12`, port `2222`, username `deploy` and the source's colour. No
  connect dialog, no tab.
- `evidence/US-0129-05-new-group-focuses-the-field.png` — `Move to Group ▸ New group…` on
  `Staging`: the dialog opens with the focus ring on the **Group** combobox and on nothing
  else.

### Gaps

- **The deferred focus into the Group field is proved by a frame, not by an assertion.** Focus
  is not queryable from a unit test here; the frame shows the dialog open with the focus ring
  on the combobox and on nothing else, which is as far as a `PrintWindow` capture goes.
- **No frame of the `(copy 2)` step.** The rule is unit-tested including that step; the walk
  duplicated once, because a second duplicate proves nothing the test does not.
- **The copy's position in `ssh_session.json` is proved by the unit test, not by the walk.**
  The tree sorts alphabetically, so the right dock cannot show store order; the `+` menu
  follows store order and would, but it was not captured.
- **The 30-group submenu's scroll gesture was not driven**, only its height cap captured.
  Neither a posted `WM_MOUSEWHEEL` (its `lParam` is in screen coordinates, so a posted client
  point lands outside the popup) nor the arrow keys (they went to the parent menu and closed
  the submenu) moved it — the method limits `IN-0042` §4.7 already records for this walk. The
  cap in frame `US-0129-06` is what the fix changes, and the kit source
  (`popup_menu.rs:1452-1456`) is what ties the cap to `scrollable`.
- **Move to Group offers no way to create a group in the submenu itself** — `New group…` hands
  off to the dialog. A free-text field inside a popup submenu is a second group-creation
  surface to keep in step with the dialog's, which `US-0118` had just finished making
  consistent.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
