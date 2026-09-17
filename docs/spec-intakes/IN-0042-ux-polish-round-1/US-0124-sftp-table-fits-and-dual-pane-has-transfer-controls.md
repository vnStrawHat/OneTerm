# Work: The docked SFTP table fits its panel and the dual pane has transfer controls

ID: US-0124
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

The SFTP browser fits the panel it ships in and makes its central action visible. At the
default dock width the table shows the columns that matter without a horizontal scrollbar, and
the dual pane offers an explicit way to move a file between the two sides.

## Findings and proposals covered

`P19` (effort M) — *"Give the SFTP table sensible default column widths for the docked
(~490 px) case — Name + Size + Date, the rest behind a column chooser — and add explicit
transfer controls between the dual panes, plus one shared action list for the `⋮` and
right-click menus."*

Addresses `F29` and `F30`, both medium, quoted from
`research/ux-walkthrough-2026-09-16.md`:

> | F29 | SFTP browser (docked) | At the default dock width the table shows Name / Date
> Modified / Pe… and needs a horizontal scrollbar; **Size is not visible at all**. Even
> expanded to 1900 px the remote pane still scrolls horizontally and the local pane carries an
> always-empty 4th column. | Density: default column widths never fit the panel they ship in. |
> medium | 44, 47 |

> | F30 | SFTP browser | The dual pane has **no transfer affordance between the panes** — no
> arrows, no toolbar. The `⋮` menu and the right-click menu offer the same actions in different
> order, one with icons and one without, and only the right-click menu has Edit/Refresh. |
> Discoverability of the central action + menu consistency. | medium | 45, 47, 48 |

## Scope

- [ ] In scope:
  - `crates/sftp-ui/src/` — the table's default column set and widths for the docked case, a
    column chooser for the rest, and the always-empty fourth column in the local pane.
  - Transfer controls between the dual panes.
  - One shared action list feeding both the `⋮` menu and the right-click menu, so they cannot
    drift in contents or order again.
  - `crates/state/src/dock_persistence.rs` — `sftp_table_state`, read to confirm whether a
    saved column state from before this packet overrides the new defaults (see Context).
- [ ] Out of scope:
  - The transfer *mechanism*: upload, download, progress, conflict handling, resume. This
    packet surfaces an action the browser already performs; it does not change how a file
    moves.
  - Remote file editing (`IN-0011`) and the SFTP-follows-terminal-CWD behaviour, both of which
    the walkthrough found working.
  - The delete confirmation, which the walkthrough praised and `US-0119` copies.
  - The dual-pane zoom (`IN-0025`), which works as documented.
  - The dock width itself (`US-0113`), which this packet depends on.

## Acceptance

- [ ] At the default docked width, the table shows Name, Size and a date, with no horizontal
      scrollbar.
- [ ] Columns not shown by default are reachable — a chooser, a menu, or an equivalent — and
      the choice persists across a restart.
- [ ] Expanded to a wide window, the remote pane does not scroll horizontally, and the local
      pane has no always-empty column.
- [ ] The dual pane offers a visible control that transfers the selected item between the two
      sides, in both directions, and it is usable without knowing a keyboard shortcut or a menu
      path.
- [ ] The `⋮` menu and the right-click menu offer the same actions, in the same order, built
      from one list. Anything one has, the other has — including Edit and Refresh.
- [ ] A saved `sftp_table_state` from before this packet does not leave a user stuck with the
      old widths. The packet states how that is handled and proves it with a seeded file.
- [ ] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

## Documentation

### Owning Docs Reviewed

- `docs/sftp-browser-design.md` §4 — the table, its columns and the menus. `P19` names it as
  the owning section. **Update required:** the default column set, the chooser, the transfer
  controls, and the single shared action list.
- `docs/sftp-browser-design.md` §1 — the browser's overall shape, read for the docked versus
  dual-pane distinction. **Update required** only if it enumerates columns.
- `docs/agents/persistence.md` and `crates/state/src/dock_persistence.rs:42-44` —
  `sftp_table_state` is an optional field on `DockDocument`, owned by `crates/state` but
  written by the SFTP feature. Read before changing defaults: a persisted state from an earlier
  version will win over a new default unless something handles it. **Record the answer.**
- `docs/sftp-follow-terminal-cwd/README.md` — read to confirm the pane's follow behaviour is
  not disturbed by the column changes. **No change expected.**
- `docs/spec-intakes/IN-0025-sftp-dual-pane/` — the dual pane's own intake, for what was
  already decided about the two panes and their zoom. Read it before adding controls between
  them. **No change.**
- `docs/PROJECT.md` — read for standing invariants. **No change.**

### Documentation Action

Update required: `docs/sftp-browser-design.md` §4 (and §1 if it enumerates columns).

Reason: the packet changes the documented default table shape and adds a documented control to
a documented surface.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.
Include the `sftp_table_state` answer.

## Context

- **Depends on `US-0113`.** `F29` measures the table against "the default dock width
  (~490 px)". `US-0113` changes what that width is at a given window size. Choosing column
  defaults first would mean choosing them twice. Land it, then measure.
- **The persisted state is the trap.** `sftp_table_state` stores column widths and visibility.
  A user who has ever used the browser has one saved, and a new default that is only applied
  when no state exists fixes nothing for them. The options are: leave it (and accept that only
  new users benefit — which fails `F29`), bump something so the old state is discarded, or
  merge the new defaults into an old state. Decide, record, and prove it with a seeded file.
- **The two menus.** `F30`'s second half is the cheap, high-value half: one list, two
  renderers. It also prevents the next divergence, which is worth more than this round's fix.
  Do it first — it is small and it makes the transfer controls easy to add in both places.
- **The transfer control.** The walkthrough says there is no affordance "between the panes";
  arrows in the gutter are the conventional answer and are what `P19` proposes. Keep it to the
  action that already exists — transfer the selection — rather than inventing a queue, a
  progress panel or a drag-and-drop model. Those are separate outcomes.
- **The always-empty fourth column** in the local pane is a bug hiding inside a density
  finding: a column that never has content should not be in the default set, and it may not
  belong at all. Find out which before deciding.
- Ladder: no new table widget, no new menu framework. The table, the menus and the transfer
  action all exist; this packet chooses defaults, unifies two lists and adds a button.
- `research/before/44-sftp-connected.png` (docked, Size invisible), `47-sftp-expanded.png`
  (wide, still scrolling, empty 4th column), `45-sftp-context-menu.png` and
  `48-sftp-overflow-menu.png` (the two divergent menus) are the before pictures.

## Plan

- [ ] Land `US-0113`; then measure the table against the new default dock width.
- [ ] Unify the two menus into one action list. Smallest, highest value, do it first.
- [ ] Choose the default column set and widths; find out what the empty fourth column is.
- [ ] Decide and implement the `sftp_table_state` question.
- [ ] Add the transfer controls.
- [ ] Update `docs/sftp-browser-design.md` §4.
- [ ] Re-capture the scenes against the repository's loopback `sftp-dev-server`.

## Decisions

Possibly one: if the packet discards or migrates a persisted `sftp_table_state`, that is a
persisted-state change future work inherits and should be recorded — as a `DEC` if it sets a
rule, or in `docs/agents/persistence.md` if it is a one-off.

## Verification Plan

1. **Focused:** `cargo test -p oneterm-sftp-ui` over the pure halves — the default column set
   and the widths it derives for a given panel width; the single action list (both menus render
   the same actions in the same order, which is an assertion over one `Vec`); and the
   `sftp_table_state` reconciliation (an old saved state plus the new defaults yields the
   stated result). Pure data, no gpui, and the only automated proof.
2. **Unit:** `cargo test -p oneterm-sftp-ui`, `cargo test -p oneterm-state`.
3. **Integration:** `cargo test --workspace`.
4. **Platform:** `pwsh scripts/ci-local.ps1`.
5. **E2E (GUI walk, re-capture these scenes):** run the repository's loopback
   `sftp-dev-server` (`cargo run -p oneterm-tools --bin sftp-dev-server -- --port 2222 --root
   <scratch>`) as the walkthrough did, and stop it afterwards.
   - `44-sftp-connected.png` — the docked browser. The after frame must show Size and no
     horizontal scrollbar.
   - `47-sftp-expanded.png` — the dual pane at width, with the transfer controls visible and no
     empty fourth column.
   - `45-sftp-context-menu.png` and `48-sftp-overflow-menu.png` — the two menus, now identical
     in contents and order.
   - `46-sftp-delete-confirm.png` — regression: the delete confirmation still works.
   - `49-sftp-after-tab-switch.png` — regression: the browser still follows the active tab.
   Plus, not in the walkthrough: a launch with a pre-existing `sftp_table_state` from before
   the change, to show the reconciliation. Capture as `44b`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Risks

- **Fixing it only for new users.** The persisted column state is the difference between "the
  defaults are better" and "the browser fits". Skipping it means the finding stands for
  everyone who has used the application.
- **A transfer control that transfers the wrong thing.** Two panes, two selections. The control
  must act on the pane the user last touched, and the direction must be unambiguous. Walk both
  directions with a selection in each pane.
- **Destructive overwrite.** Transferring onto an existing file is a data-loss path. If the
  existing transfer action already handles it, leave it alone; if adding a visible button makes
  it much easier to hit, check what it does on a name collision and record it. Do not ship a
  one-click overwrite with no confirmation.
- **Column chooser creep.** A chooser can become a table-configuration dialog. Keep it to
  showing and hiding the columns that exist.
- **Measuring against the wrong width.** If `US-0113` has not landed, the "default dock width"
  is the old absolute 480 px and the defaults chosen will be wrong. Check the dependency.

## Evidence and Gaps

After implementation, record commands, results, and anything skipped, unavailable, partial, or failing.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
