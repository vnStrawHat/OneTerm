# Work: The session panel can add from the header, discloses with a chevron, and separates Delete

ID: US-0119
Intake: IN-0042
Created: 2026-09-17

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [ ] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: existing-contract change
- Risk lane: normal
- Spec Intake, when required: `IN-0042` — `docs/spec-intakes/IN-0042-ux-polish-round-1/IN-0042.md`

## Outcome

The SSH Sessions panel behaves like a list the user owns: there is always a way to add a
session, a group's disclosure looks like a disclosure, and the destructive row in the context
menu is separated and styled as destructive — the pattern the SFTP browser already uses.

## Findings and proposals covered

`P7` — *"Add a `+` button to the Session panel header and a context menu on the blank area
below the list, both dispatching the full new-session dialog."*

`P8` — *"Swap the group disclosure glyph from the maximise arrow to a chevron."*

`P10` (Delete half) — *"…put `Delete` behind a separator with danger styling in the session
context menu (SFTP already does this)."* The placeholder half of `P10` belongs to `US-0115`.

Addresses `F8`, `F24` (medium) and `F23` (low), quoted from
`research/ux-walkthrough-2026-09-16.md`:

> | F8 | Session panel | With sessions present there is **no way to add one from the right
> dock**: no `+` in the "Session" header, and right-clicking the blank area below the list
> produces no menu. (The empty-list state does offer "Right-click → New Session" —
> `crates/session-ui/src/render.rs:68`.) | Discoverability: the add affordance disappears
> exactly when the list stops being empty. | medium | 01, 52 |

> | F23 | Session tree | A group's expand/collapse glyph is the **diagonal maximise arrow**
> (↙↗) — the same icon the app uses for "zoom panel" on the SFTP Browser header and the tab
> bar. | Icon consistency: one glyph, two unrelated meanings; a tree disclosure conventionally
> reads as a chevron. | low | 57 |

> | F24 | session context menu | The menu is `New Session` / `Open` / `Delete` / `Property`.
> The global "New Session" sits in the top (most-misclicked) slot of an item-specific menu,
> `Delete` sits directly under `Open` with no separator and no destructive styling, and
> "Property" should read "Properties" (or "Edit…"). No Duplicate, no Move to Group. | Safety +
> wording. (SFTP's Delete *does* confirm and *is* styled danger — see 46 — so the app already
> has the better pattern.) | medium | 10 |

## Scope

- [ ] In scope:
  - `crates/session-ui/src/render.rs:60-76` — the "Session" header gains an add control, and
    the blank area below the list gains a context menu.
  - `crates/session-ui/src/panel.rs` — routing both to the full new-session dialog.
  - `crates/session-ui/src/tree_builder.rs` — the group disclosure glyph.
  - `crates/session-ui/src/tree_render.rs:185-215` — the session context menu: a separator
    before Delete, danger styling on it, and the row wording `F24` names.
  - Whether Delete confirms, matching the SFTP pattern the walkthrough praises.
- [ ] Out of scope:
  - Duplicate and Move to Group, which `F24` notes as absent but `P10` does not propose. They
    are new capabilities, each independently acceptable, and belong in their own packets if the
    owner wants them. Recorded in Gaps.
  - The dialogs themselves (`US-0118`, `US-0120`).
  - The `+` menu in the centre tab bar (`US-0114`), although both surfaces must end up opening
    the **same** full session dialog.
  - The tree's colour squares and subtitles, settled by `US-0110`.
  - The SFTP browser's own menus (`US-0124`).

## Acceptance

- [ ] With sessions present, a session can be added from the right dock: an add control in the
      "Session" header, and a context menu on the blank area below the list. Both open the full
      new-session dialog — the same dialog the tree's context menu opens and the same one
      `US-0114` puts in the `+` menu.
- [ ] The empty-list state's existing "Right-click → New Session" hint still works and now
      agrees with the header control.
- [ ] A group's expand/collapse glyph is a chevron, and the maximise arrow no longer appears in
      the tree. Expanding and collapsing still work by clicking it and by clicking the row.
- [ ] In the session context menu, Delete is separated from the rows above it and styled as
      destructive, matching the SFTP browser's delete.
- [ ] Delete confirms before removing, naming the session — or, if the packet decides not to
      confirm, that choice is recorded here with its reason before implementation.
- [ ] "Property" reads as a proper label ("Properties" or "Edit…"), and the same wording is used
      wherever else that action appears.
- [ ] The global "New Session" row no longer occupies the top slot of an item-specific menu.
- [ ] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

## Documentation

### Owning Docs Reviewed

- `docs/ssh-client-connect.md` §6 — the SessionPanel and its entry points. `P7` names it as the
  owning section. **Update required:** the header control and the blank-area menu are new
  entry points to the new-session dialog.
- `docs/ssh-client-connect.md` §1.1 — which surfaces open the connect dialog, and §1.3 for the
  dialogs themselves. Read for consistency with `US-0114`. **Update required** only if §1.1
  enumerates the surfaces.
- `docs/gui-layout.md` §Dock composition — describes `SshClientPanel` and its section headers,
  including that each header shows the hosted panel's `title_suffix` in a control group. The
  add control belongs in exactly that slot. **Update required** if the header's contents are
  described there.
- `crates/sftp-ui/` delete confirmation — the pattern being copied
  (`research/before/46-sftp-delete-confirm.png`). Read it rather than reinventing the
  confirmation; reusing the existing shape is the point of `F24`'s parenthesis.
- `crates/theme/src/icon.rs` and `reference/gpui-kit/crates/component/src/icon.rs` — for the
  chevron. The kit's Lucide set is available and almost certainly already has it; do not add
  an SVG if it does. **No change expected.**
- `docs/PROJECT.md` — read for standing invariants. **No change.**

### Documentation Action

Update required: `docs/ssh-client-connect.md` §6 (and §1.1 if it enumerates entry points), and
`docs/gui-layout.md` if the section header's contents are described there.

Reason: this packet adds documented entry points to a documented dialog and changes a
documented menu.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

## Context

- `F8` is a state-dependent affordance: the hint exists for the empty list
  (`render.rs:68`) and vanishes when the list fills. The fix is to make the affordance
  unconditional, which means the header — a place that is always there — rather than a second
  conditional hint.
- Depends on `US-0114`: that packet is where the full session dialog becomes reachable from a
  workspace-level `WorkspaceCommands` entry. Inside `crates/session-ui` the dialog is already
  reachable directly (`panel.rs:146`), so this packet does not strictly need the command — but
  both surfaces must open the same dialog, and `US-0114` is where that is settled. Land it
  first and reuse its entry point rather than adding a second route.
- `F23` is one icon. Check `crates/theme/src/icon.rs` and the kit's `IconName` set before
  adding an SVG; a chevron almost certainly exists in both.
- `F24` has four symptoms and `P10` proposes one fix (the Delete separator and styling). The
  wording and the misplaced "New Session" row are in the same menu, in the same file, and cost
  nothing extra — so they are in scope. Duplicate and Move to Group are not: they are new
  behaviour, not polish, and folding them in here would make this packet unacceptable on its
  own.
- The confirmation question: SFTP's delete confirms, names the file and styles the button
  danger. A session is cheaper to lose than a file, but it is still the user's configuration
  and there is no undo. Reuse the SFTP pattern unless there is a reason not to, and record the
  call.
- `research/before/01-first-launch.png` and `52-session-empty-area-menu.png` (`F8`),
  `57-session-tree-with-group.png` (`F23`), `10-session-context-menu.png` (`F24`) and
  `46-sftp-delete-confirm.png` (the pattern to copy) are the before pictures.

## Plan

- [ ] Confirm `US-0114`'s dialog entry point exists; reuse it.
- [ ] Header add control and blank-area menu.
- [ ] Chevron glyph (check the existing icon sets first).
- [ ] Context menu: separator, danger styling, wording, row order; decide the confirmation.
- [ ] Update `docs/ssh-client-connect.md` §6.
- [ ] Re-capture the scenes.

## Decisions

None expected. If Delete gains a confirmation, that matches an existing in-repo pattern rather
than setting a new rule.

## Verification Plan

1. **Focused:** `cargo test -p oneterm-session-ui` over the menu's row list as data — the rows
   present, their order, and which row is marked destructive. The tree builder's tests already
   cover row construction (`tree_builder.rs`), so this extends an existing pattern rather than
   inventing one. The rendering, the styling and the confirmation dialog are not queryable;
   their proof is the GUI walk, and Evidence must say so.
2. **Unit:** `cargo test -p oneterm-session-ui`.
3. **Integration:** `cargo test --workspace`.
4. **Platform:** `pwsh scripts/ci-local.ps1`.
5. **E2E (GUI walk, re-capture these scenes):**
   - `01-first-launch.png` — the Session header with its add control, sessions present.
   - `52-session-empty-area-menu.png` — right-clicking the blank area now produces a menu.
   - `57-session-tree-with-group.png` — the chevron disclosure, expanded and collapsed.
   - `10-session-context-menu.png` — the reordered, separated, danger-styled menu.
   - `53-new-session-from-tree.png` — the dialog both new entry points open.
   Plus, not in the walkthrough: the Delete confirmation, captured as `10b`, beside
   `46-sftp-delete-confirm.png` for comparison.
   Seed `ssh_session.json` with sessions in and out of groups before the walk, as the
   `US-0110` walk did, and use the worktree's own `target/` config directory so the owner's
   store is untouched.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Risks

- **Two dialogs again.** If the header control opens a dialog different from the one the tree's
  menu opens, this packet recreates `F7` inside the panel. One entry point, used by every
  surface.
- **Deleting the wrong session.** A blank-area context menu and an item context menu are easy
  to confuse in code. The blank-area menu must offer only global actions, and the item menu
  must act on the right-clicked item.
- **Data loss with no undo.** Delete removes a saved session from `ssh_session.json`. The
  confirmation question is the mitigation, and skipping it needs a stated reason.
- **Icon drift.** Adding a new SVG when the kit already ships a chevron grows the icon set for
  nothing. Check both sets first.
- **Header crowding.** The section header already carries a control group (the SFTP Browser's
  expand/collapse toggle lives in the sibling header). Adding a control must not push the title
  or collide at the dock widths `US-0113` allows — check at the narrow width, not just the
  default.

## Evidence and Gaps

After implementation, record commands, results, and anything skipped, unavailable, partial, or failing.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
