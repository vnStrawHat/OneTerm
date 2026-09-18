# Work: The docked SFTP table fits its panel and the dual pane has transfer controls

ID: US-0124
Intake: IN-0042
Created: 2026-09-17

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [x] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: existing-contract change
- Risk lane: **high-risk** — raised from normal during the verification rework. The packet
  migrates a persisted document (`docks.json`, `schema_version` 1 -> 2) and the migration is
  deliberately lossy, which is two of `docs/HARNESS.md`'s high-risk triggers ("data loss,
  migrations"). Detail design, required for this lane:
  [`low-level-design/sftp-table-state-migration.md`](low-level-design/sftp-table-state-migration.md).
  Nothing else in the packet is high-risk: no authorization, secret, external effect or
  credential path is touched.
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

- [x] At the default docked width, the table shows Name, Size and a date, with no horizontal
      scrollbar.
- [x] Columns not shown by default are reachable — a chooser, a menu, or an equivalent — and
      the choice persists across a restart.
- [x] Expanded to a wide window, the remote pane does not scroll horizontally, and the local
      pane has no always-empty column.
- [x] The dual pane offers a visible control that transfers the selected item between the two
      sides, in both directions, and it is usable without knowing a keyboard shortcut or a menu
      path.
- [x] The `⋮` menu and the right-click menu offer the same actions, in the same order, built
      from one list. Anything one has, the other has — including Edit and Refresh.
- [x] A saved `sftp_table_state` from before this packet does not leave a user stuck with the
      old widths. The packet states how that is handled and proves it with a seeded file.
- [x] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

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

Changed:

- `docs/sftp-browser-design.md` §4.5 — a new "Table shape as built" block: the six columns, the
  three shown by default, Name as the flexible column measured against the panel, and the
  `sftp_table_state` version rule.
- `docs/sftp-browser-design.md` §4.8 — retitled "Menus — one action list, two renderings": the
  shared `ENTRY_MENU`, the empty-area subset, and the two config rows the `⋮` menu adds.
- `docs/sftp-browser-design.md` §4.15 — each pane's toolbar carries its half of the transfer
  controls at the edge facing the other pane.
- `docs/agents/persistence.md` — the `docks.json` row documents `schema_version` 2 and the
  v1 -> v2 step with its fixture; the `sftp_table_state` row says the field carries no version of
  its own and points at the detail design; § Migration rules gains the two sentences this rework
  is the worked example of (a feature crate does not migrate its own field in a shared document,
  and a lossy migration must say what it drops).
- `docs/sftp-browser-design.md` §4.5 — the migration paragraph replaces the field-version one, and
  a resize now re-derives Name on the same gesture; §4.8 — availability is part of the list's
  data (Edit is dropped for a directory); §4.15 (line ~1310) — the Local pane's column order is
  `Name / Size / Date Modified`, which it has been since this packet and was left stale (`m2`).
- `docs/spec-intakes/IN-0042-ux-polish-round-1/low-level-design/sftp-table-state-migration.md` —
  new, the detail design the high-risk lane requires.

Unchanged, and why: `docs/sftp-follow-terminal-cwd/README.md` (the follow behaviour is untouched;
re-checked in the walk, frame `US-0124-49-sftp-after-tab-switch.png`),
`docs/spec-intakes/IN-0025-sftp-dual-pane/` (the pane model and its zoom are unchanged — the
packet adds buttons in front of the transfer actions that intake already defined),
`docs/PROJECT.md` (no invariant moves).

**The `sftp_table_state` answer** (reworked after `M3`). `docks.json` carries the version, not
the field: `schema_version` goes **1 -> 2**, and `oneterm-state` — the document's owner — migrates
the layout inside `migrate_json_value`. The step drops `column_widths` entirely (Name's width is
derived and never stored; the others' v1 values were the v1 defaults) and keeps only the
`column_visibility` entries that are `false` and name a column that still exists, because under
the v1 defaults every column was visible, so a `true` records nothing the user did while a
`false` is a column they hid by hand. `expanded` and `local_dir` are not column layout and are
carried over untouched. The pre-migration document survives as the shared write path's `.bak`;
an unparseable document is quarantined by `update_dock_document` exactly as before. Fixture:
`crates/state/tests/fixtures/persistence/docks-v1.json`. Full design, including the failure
paths: [`low-level-design/sftp-table-state-migration.md`](low-level-design/sftp-table-state-migration.md).

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

Four, none of which sets a rule beyond this browser, so no `DEC`:

1. **Name is the flexible column.** The table widget has fixed-pixel columns and no flex mode
   (`reference/gpui-kit/crates/component/src/table/column.rs` — `width`, `min_width`, `fixed`,
   no grow), so the panel measures its list area with a `gpui::canvas` probe (the pattern
   `reference/gpui-kit/crates/webview/src/lib.rs:131` uses) and the delegate gives Name whatever
   the other visible columns leave over, down to a 100 px floor. One rule fixes both halves of
   `F29`: nothing overflows at the docked width, and nothing is left over as a blank strip when
   the pane is wide. Name is consequently **not resizable** and its width is **not persisted**;
   every other column is still both.
2. **The default set is Name + Size + Date Modified**, in that order (the high-level design's
   sketch). Permissions, Owner and Group keep their definitions and move behind the `⋮` menu's
   Columns section, which gained a label so it can be found.
3. **`docks.json`'s own `schema_version` goes 1 -> 2 and the document's owner migrates the
   SFTP column layout.** The first cut of this packet put a private `version` inside the
   feature-owned `sftp_table_state` and dropped the old layout at read time in the UI delegate,
   which bypassed `docs/agents/persistence.md` § Migration rules (verification finding `M3`).
   The field is gone; `oneterm_state::dock_persistence::migrate_sftp_table_state_to_v2` drops the
   stored widths and keeps only the columns the user hid by hand, inside `migrate_json_value`,
   with the shared write path's `.bak` and quarantine behaviour. See
   [`low-level-design/sftp-table-state-migration.md`](low-level-design/sftp-table-state-migration.md).
4. **The transfer controls live at the inner edge of each pane's toolbar**, not in the splitter
   gutter: the gutter belongs to `h_resizable`, and a third resizable child would have added two
   drag handles around a button strip. Local's `↑ Upload` sits at the right end of its toolbar
   and Remote's `↓ Download` at the left end of its own, so the pair meets at the split. Each
   acts on its own pane's selection and is disabled while that pane has none. The Local pane
   already had an unlabelled arrow button; it is now labelled and disabled-aware, and the Remote
   pane gained its counterpart, which it never had.

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
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
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

### Rework after independent verification (2026-09-17)

`evidence/sftp-agent-wave1-verify.md` returned **PASS with findings**. All three majors and every
minor are addressed here; the verdict's acceptance clauses were re-proved, not re-stated.

| Finding | What changed |
|---|---|
| `M1` — a column resize left the table overflowing, and the Local pane threw the drag away | `SftpPanel::on_table_event` now calls `table.refresh(cx)` after `apply_widths` — `refresh` is the only thing that re-reads `column()`, and therefore the only thing that re-derives Name. `LocalPane::on_table_event` handles `ColumnWidthsChanged` at all now, with its own `apply_widths` (which ignores a width offered for Name) and the same refresh. |
| `M2` — `Edit` offered on a directory, `do_edit` silently returned | Availability became part of the list's data: `SftpAction::available_for(MenuTarget)`, with `MenuTarget::{EmptyArea, File, Directory}`, and both menus render `menu_entries(target)`. A directory's menu drops `Edit` and keeps everything else in place (`edit_is_not_offered_on_a_directory`). `do_edit` reached by a key binding on a directory now says `"…" is a folder — open it instead of editing it.` instead of a debug log. |
| `M3` — the schema change bypassed the Migration rules | Reworked as described in Decisions 3 and Reconciliation: the document's own `schema_version` 1 -> 2, migrated by the document's owner, with the fixture, the round-trip test, the `.bak`, the quarantine path and a detail design. The ad-hoc field version is removed. The packet's lane is raised to high-risk. |
| `m1` — future-version gate `<` vs `!=` | Moot: the field-level gate is gone, and `migrate_json_value` already refuses a `schema_version` above the current one. Pinned by `future_schema_version_is_refused`. |
| `m2` — stale Local-pane column order in the design doc | Fixed in `docs/sftp-browser-design.md`. |
| `m3` — the "one list" invariant rests on both call sites naming the constant | Still true, and now with a second guard: both call sites go through `menu_entries(target)`, and the target is derived by one shared `MenuTarget::for_selection`. A renderer that invented its own list would still not be caught by a test; recorded below. |
| `m4` — the drop was proven in memory, not from a deserialized document | `version_less_document_migrates_and_round_trips` reads a version-less document from disk, migrates it, writes it back and asserts the `.bak`, the stored version and idempotence. |
| `m5` — one frame of overflow before the first measurement | Name's fallback width is its 100 px floor in both tables, which fits the narrowest dock the application can produce. |
| `m6` — the `⋮` menu reached the bottom of a 1000 px window | The menu is `scrollable` when its estimated height exceeds the kit's cap, the same estimate `TerminalPanel`'s `+` menu uses for the same reason. |
| `m7` — the Agent empty state over-promised | The body now names the condition: *"A coding agent that reports its status shows up here on its own while it works in one of your terminals."* |
| `m8` — asymmetric copy test | The headline and the body are now checked the same way, for `OSC` and for `20308`. |

### Re-taken and new frames (rework walk, loopback server on 127.0.0.1:2233)

| Frame | Shows |
|---|---|
| `evidence/US-0124-44b-seeded-old-state-after-connect.png` | **`M3` end to end.** A `docks.json` seeded as a genuine v1 document (`schema_version: 1`, all six columns, the old widths, `owner`/`group` hidden by hand) comes up as Name / Size / Date Modified with no horizontal scrollbar. The document the run wrote back: `"schema_version": 2`, `"column_widths": {}`, `"column_visibility": {"group": false, "owner": false}` — the widths gone, both hand-hides kept — with the pre-migration file next to it as `docks.bak`. |
| `evidence/US-0124-M2-directory-menu-has-no-edit.png` | **`M2`.** The row menu on `subdir`: Open / Download — Upload Files / Upload Folder / New Folder — Rename / Delete — Properties / Refresh. No Edit, no dangling separator. |
| `evidence/US-0124-45-sftp-context-menu.png` | The same menu on a file: Edit is back in second place, everything else unmoved. |
| `evidence/US-0124-48-sftp-overflow-menu.png` | **`m6` + `M2`.** The `⋮` menu with a directory selected: the same rows the row menu shows (no Edit), then Follow Terminal Cwd and the Columns chooser — now capped at the kit's height limit with a scrollbar, instead of running off the bottom of the window. |

An earlier run of this walk also re-proved, incidentally, that a **v2** document with every column
switched on is honoured as written (the table showed all six and scrolled): the migration only
touches a v1 document, and a v2 choice — including one that does not fit — is the user's.

### What changed

`crates/sftp-ui/src/{types,table_delegate,table_delegate_menu,render,panel,actions,local_pane,edit}.rs`,
`crates/core/src/{sftp,lib}.rs` and `crates/state/src/dock_persistence.rs` (+ its fixture). The
menu rewrite is the largest piece:
`table_delegate_menu.rs` now owns `SftpAction`, `MenuEntry`, the `ENTRY_MENU` list,
`empty_area_entries()` and one `build_menu` renderer, and `SftpPanel::do_open` was extracted in
`actions.rs` so the menu row and the `SftpOpen` binding share one behaviour.

### Focused (`cargo test -p oneterm-sftp-ui` — 55 passed)

New, all pure:

- `types::tests::the_default_columns_fit_the_docked_panel` — at 317, 420, 490 and 1900 px of
  panel the default set's total (Name + Size + Date Modified + the table's trailing gutter) is
  within the panel; Name never drops below its floor; Name is wider at 1900 than at 490; the
  default visibility is exactly `name, size, modified`.
- `types::tests::name_stops_shrinking_at_its_minimum` — too narrow for the set, Name stops at
  100 px and the table scrolls instead of squeezing names to nothing.
- `table_delegate::tests::name_fills_the_measured_panel_width` — the measurement is idempotent
  (a 0.2 px change is not a change, so it cannot drive a re-render loop), Name takes the
  leftover at 317 and at 1900, and hiding a column gives its width to Name.
- `table_delegate::tests::a_migrated_pre_version_state_gives_the_new_defaults` — what the
  browser makes of a migrated pre-US-0124 state: the new defaults minus the user's own hides.
- `table_delegate_menu::tests::edit_is_not_offered_on_a_directory` — the directory menu drops
  Edit and nothing else, and `MenuTarget::for_selection` maps both kinds of entry.
- `types::tests::the_persisted_column_keys_match_the_columns` — `SFTP_TABLE_COLUMNS` (what the
  migration filters by, in `oneterm-core`) and the browser's `SortColumn` name the same columns.
- In `oneterm-state`: `v1_document_migrates_the_sftp_table_layout`,
  `version_less_document_migrates_and_round_trips`, `future_schema_version_is_refused` — see the
  detail design's proof table.
- `table_delegate_menu::tests::the_shared_action_list_has_one_order` — the exact order both
  menus render, Edit and Refresh included.
- `table_delegate_menu::tests::the_empty_area_menu_is_a_subset_of_the_same_list` — the
  empty-area menu is the same list filtered to the actions that need no selection, with no
  leading, trailing or doubled separator.

Updated: `persisted_state_round_trips_and_ignores_invalid_values` (now asserts `version`, and
that no `name` width is stored), `toggling_visibility_never_hides_name`,
`widths_apply_in_visible_order_and_are_clamped` (Name's width is ignored, not stored).

`cargo test --workspace` — green.

### E2E (GUI walk)

`cargo build -p oneterm-app --profile fast-dev`, the walkthrough's `gui.ps1` driver (PrintWindow
+ posted `WM_*`, own pid only), against the repository's loopback server
`cargo run -p oneterm-tools --bin sftp-dev-server -- --port 2233 --root <scratch>` (2233, not
2222: another session was walking concurrently). The server was stopped afterwards.

| Frame | Shows |
|---|---|
| `evidence/US-0124-44-sftp-connected-1600.png` | Docked, connected, 1600 px window: **Name / Size / Date Modified**, Size visible, no horizontal scrollbar. |
| `evidence/US-0124-44a-sftp-connected-900.png` | The same at a 900 px window (~317 px dock): Name shrinks and truncates, still no horizontal scrollbar. |
| `evidence/US-0124-44d-permissions-on.png` | Permissions switched on from the `⋮` menu's labelled Columns section: four columns, Name gives up the room, still no scrollbar. |
| `evidence/US-0124-44e-columns-persisted-after-connect.png` | After a restart: Permissions is still on. The written document was `{"version":1, …, "permissions":true}` with no `name` width. |
| `evidence/US-0124-44b-seeded-old-state-after-connect.png` | A `docks.json` seeded with a pre-US-0124 `sftp_table_state` (no version, all six visible, Name 320): it is ignored and the new defaults are used. |
| `evidence/US-0124-45-sftp-context-menu.png` | Right-click: Open / Edit / Download — Upload Files / Upload Folder / New Folder — Rename / Delete — Properties / Refresh. |
| `evidence/US-0124-48-sftp-overflow-menu.png` | The `⋮` menu: the same ten items, same order, same icons, then Follow Terminal Cwd and the Columns chooser. |
| `evidence/US-0124-47-sftp-expanded.png` | Dual pane, wide: both panes Name / Size / Date Modified, no blank fourth column, no horizontal scroll, `↑ Upload` (disabled, nothing selected locally) and `↓ Download` (enabled) meeting at the split. |
| `evidence/US-0124-47b-transfer-controls-both-enabled.png` | A selection in each pane: both controls enabled. |
| `evidence/US-0124-47c-after-upload.png` | `↑ Upload` clicked: `upload-me.txt` is on the remote side, the queue says Done. |
| `evidence/US-0124-47d-after-download.png` | `↓ Download` clicked on a remote row: `data.csv` is in the Local pane, queue Done. Both directions, buttons only. |
| `evidence/US-0124-46-sftp-delete-confirm.png` | Regression: Delete still confirms, still danger-styled. |
| `evidence/US-0124-49-sftp-after-tab-switch.png` | Regression: the browser still follows the active tab. |

### Platform

`pwsh scripts/ci-local.ps1` — `ci-local: all checks passed`.

### Gaps

- **`US-0113` landed while this packet was in flight**, and `main` was merged in before the walk,
  so the frames are against the real dock width. The flexible-Name rule makes the defaults
  independent of that width anyway, which is why no column default names a dock size.
- ~~**Uploading onto an existing remote file still overwrites it without asking**, exactly as the
  drag-and-drop and menu uploads did before this packet; only the download direction confirms
  (IN-0025). The button makes the existing action visible, it does not change it. Worth its own
  packet; not opened here because it is a change to the transfer mechanism, which this packet
  puts out of scope.~~ **Closed 2026-09-18 by `US-0128`**
  (`US-0128-upload-asks-before-overwriting.md`), which gave the packet this gap asked for:
  every upload path now confirms through the same helper the download direction uses.
- **Menu items are never greyed out.** A row menu drops what does not apply to that row (Edit on
  a directory), but an action that needs a selection is still offered with none and reports
  "Select a file or folder to …" when used, rather than being disabled. That is the behaviour the
  `⋮` menu already had.
- **The "one list" invariant is structural, not tested.** Both menus call `menu_entries(target)`;
  a renderer that built its own list would compile. `m3` from the verification, left standing.
- **The `⋮` menu's height is estimated, not measured** (rows × a constant, like the `+` menu's):
  a menu within one row of the kit's cap can guess wrong by one row. Marked `ponytail:` in place.
- The measurement probe repaints the table when the panel width changes; it is guarded by a
  0.5 px threshold and was exercised by resizing the window between 900 and 1600 px in the walk,
  but there is no automated test that a resize cannot loop — the guard itself is unit-tested.
- **`M1` has no after-frame: the column-resize drag could not be driven from this session.** The
  kit's resize handle is a gpui drag (`on_drag_move` with a `ResizeColumn` payload,
  `reference/gpui-kit/crates/component/src/table/state.rs:1436`), and posted `WM_MOUSEBUTTON`/
  `WM_MOUSEMOVE` sequences did not start one — twelve attempts across four x offsets and three y
  offsets, fast and slow, with and without a hover, all left the columns untouched. The
  verifier's own session did manage it, so this is a limit of this walk, not of the widget. The
  fix is proven by two tests instead — `panel::tests::a_column_resize_reaches_the_delegate` (the
  event reaches the delegate through the subscription this packet changed) and
  `table_delegate::tests::a_widened_column_comes_out_of_name` (the width Name is then derived at)
  — plus the one-line `table.refresh(cx)` that makes the kit re-read them, visible in the diff.
  Whoever re-verifies should re-take `US-0124-verify-column-resize-overflows.png` and expect no
  scrollbar.
- No real remote host was used; everything is the loopback `sftp-dev-server`.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
