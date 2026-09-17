# Independent verification — US-0124 and US-0125 (IN-0042)

Verifier: an independent session that did not write the change.
Base: `1be9c10d` (`main` @ `23d0fc15` already merged in).
Date: 2026-09-17.
Method: read `AGENTS.md`, `docs/HARNESS.md`, `docs/agents/persistence.md` before and after the
diff, both packets, findings `F28`–`F30` and before frames 22 / 44–48 in
`docs/spec-intakes/IN-0042-ux-polish-round-1/research/`, `docs/sftp-browser-design.md` §4,
`docs/agent-panel-display.md`, and `git diff main...HEAD` in full; then ran the packets' tests,
mutated two decision functions, and drove the built application with an own-pid GUI walk against
a loopback `sftp-dev-server` on port 2244.

---

## Verdicts

| Packet | Verdict |
|---|---|
| **US-0124** — the docked SFTP table fits its panel and the dual pane has transfer controls | **PASS with findings** — every acceptance clause holds as written and was reproduced independently, but two defects reachable from ordinary use are unrecorded, and the persisted-schema change does not follow the convention its own owning document states. |
| **US-0125** — the Agent panel has a header and plain-language empty state | **PASS** — every claim checked, including a pixel comparison of the two headers. Two minor copy/test notes only. |
| **Overall** | **PASS with findings.** Nothing here blocks the outcome; `M1` and `M2` are defects that should be opened as their own `BUG` packets, and `M3` is a documentation/classification reconciliation on `US-0124`. |

---

## Claims checked

### US-0124

| Claim | Where | Result |
|---|---|---|
| The kit's table has no flex column | `reference/gpui-kit/crates/component/src/table/column.rs:88-215` — `width`, `min_width`, `max_width`, `fixed`, `resizable`, `movable`; no grow/flex | **True** |
| The panel measures the list area with a `gpui::canvas` probe | `crates/sftp-ui/src/render.rs:384-394`, `crates/sftp-ui/src/local_pane.rs:1069-1079`; absolute, `size_full`, inside a `relative()` wrapper so it cannot affect layout | **True** |
| Name takes what the other visible columns leave, floor 100 px | `crates/sftp-ui/src/types.rs:219-234`, `crates/sftp-ui/src/table_delegate.rs:123-140` | **True for the default set**, but see `M1` |
| Name is not resizable and not persisted | `crates/sftp-ui/src/table_delegate.rs:298-305` (`resizable(!is_name)`), `:189-193` (no `name` width stored), `:207-210` (`apply_widths` skips Name) | **True** — the written `docks.json` contained `column_widths` for `size`/`modified`/`permissions`/`owner`/`group` and no `name` |
| Default visible set is Name + Size + Date Modified | `crates/sftp-ui/src/types.rs:276-295` | **True** (frames below) |
| The other three sit behind a labelled Columns section in the `⋮` menu | `crates/sftp-ui/src/render.rs:242` (`PopupMenuItem::new("Columns").disabled(true)`), `:245-263` | **True** (`US-0124-verify-48-overflow-menu.png`) |
| One `ENTRY_MENU` feeds the `⋮` and the right-click menus | `crates/sftp-ui/src/table_delegate_menu.rs:46-60`; call sites `crates/sftp-ui/src/render.rs:214` and `crates/sftp-ui/src/table_delegate.rs:498` | **True** — the two frames show the same ten items, same order, same icons |
| The empty-area menu is a no-selection subset | `crates/sftp-ui/src/table_delegate_menu.rs:143-161`, `crates/sftp-ui/src/table_delegate.rs:517` | **True** (test + mutation below) |
| Transfer controls at the inner toolbar edges, disabled without a selection | `crates/sftp-ui/src/local_pane.rs:1012-1026` (Upload, last child of the Local toolbar), `crates/sftp-ui/src/render.rs:283-295` (Download, first child of the Remote toolbar); pane order `crates/sftp-ui/src/render.rs:67-69` puts Local left, Remote right | **True** — `US-0124-verify-47-dual-pane-no-selection.png` (both dimmed) vs `…-both-enabled.png` (both lit) |
| The Download button confirms before replacing a local file | `crates/sftp-ui/src/transfer.rs:320-322` routes the expanded case to `download_entry_to_local`, which confirms | **True** (unchanged path, covered by `expanded_download_onto_an_existing_file_asks_first`) |
| Upload still overwrites a remote file without asking — recorded as a gap | `crates/sftp-ui/src/transfer.rs:145-200`: no existence check | **True and honestly recorded** in the packet's Gaps |
| NEW persisted `version: u32`, `#[serde(default)]`; a pre-packet document reads 0 and its column fields are dropped, `expanded`/`local_dir` kept | `crates/core/src/sftp.rs:175-196`, `crates/sftp-ui/src/table_delegate.rs:152-160` | **True** — reproduced with a real seeded `docks.json`; but see `M3` |
| Delete still confirms and is danger-styled | `crates/sftp-ui/src/actions.rs:248` (`.danger()`), untouched by the diff | **True** |
| `do_open` warns when nothing is selected | `crates/sftp-ui/src/actions.rs:101-118` | **True** |
| Permissions/Owner/Group still work when switched on | `crates/sftp-ui/src/panel_ops.rs:246-259` toggles and calls `t.refresh(cx)`, so Name is re-derived | **True** (implementer frame `US-0124-44d-permissions-on.png`; arithmetic checks out: 470 − 338 − 16 ≈ 116 px of Name) |

### US-0125

| Claim | Where | Result |
|---|---|---|
| The header is now drawn in the empty state too | `crates/agent-ui/src/view.rs:310` (`render_title_bar`), called from `:541` (empty state) and `:368` (populated, via `render_header`) | **True** |
| Styled like `SshClientPanel::render_header` — `h_8`, tab-bar background, one bottom border, plain `text_sm` title, framed trailing group | `crates/agent-ui/src/view.rs:310-356` vs `crates/app/src/ssh_client_panel.rs:160-191` | **True, pixel-for-pixel.** Scanning column x=1300 of my own frames: both headers have the top border at y=33, the band `rgb(30,34,39)` from y=34 to y=64, the bottom border `rgb(62,68,81)` at y=65, body from y=66 — identical in both. Title text starts at x=1120 (`Agents`) and x=1121 (`Session`), a one-pixel glyph-bearing difference. Same result on the implementer's own pair. |
| `dock_skin.rs` unchanged; `owns_header` already covers `AGENT` | `crates/workspace/src/layout/workspace/dock_skin.rs:111-118`, not in the diff | **True** — no outer tab bar in either mode |
| One `EMPTY_STATE` constant with a test holding `OSC 20308` to the footnote | `crates/agent-ui/src/view.rs:42-48`, test at `:686-698` | **True** (see `m8` for one asymmetry) |
| `docs/agent-panel-display.md` edits accurate | §1.1 (lines 25-32), §4 (lines 174-182), §8 (lines 353-356) | **True** — checked against the rendered panel and the code |
| The acceptance amendment (footnote keeps the protocol name) is recorded | `US-0125-…md` Acceptance clause 3 and Gaps | **True**, attributed to an owner instruction on 2026-09-17 |

---

## Findings

### Major

**M1 — Resizing a column leaves the table overflowing its panel; Name does not give the room back.**
`SftpPanel::on_table_event` handles `TableEvent::ColumnWidthsChanged`
(`crates/sftp-ui/src/panel.rs:590-598`) by calling `delegate_mut().apply_widths(&widths)` and
`cx.notify()`, but never `table.refresh(cx)`. Only `refresh` re-runs the kit's
`prepare_col_groups`, which is the only place `TableDelegate::column` — and therefore
`name_width()` (`crates/sftp-ui/src/table_delegate.rs:123`) — is re-read. The kit mutates its own
`col_groups[].width` during the drag and emits the new widths on mouse-up
(`reference/gpui-kit/crates/component/src/table/state.rs:1495-1496`), so after a drag the table is
wider than the panel and a horizontal scrollbar appears, with Name still at its old width.

Reproduced in the walk: widening Size by ~96 px in the docked browser at a 1600 px window
produced a clipped `Date Modified` column and a horizontal scrollbar
(`US-0124-verify-column-resize-overflows.png`); clicking Refresh re-derived Name and the scrollbar
went away (`US-0124-verify-column-resize-heals-on-refresh.png`).

This contradicts `docs/sftp-browser-design.md` §4.5 as written — *"Name takes whatever the other
visible columns leave over"* — and the packet's Decision 1, for a gesture the same decision keeps
supported (*"every other column is still both [resizable and persisted]"*). It self-heals on the
next listing, toggle, tab switch or window resize, which is why it survived the implementer's walk.
The same root cause hits the Local pane harder: `LocalPane::on_table_event`
(`crates/sftp-ui/src/local_pane.rs:675-696`) ignores `ColumnWidthsChanged` entirely, so a dragged
width there both overflows until a refresh *and* is then thrown away.
Fix is one line — refresh after `apply_widths` — plus the same in the Local pane.
**Not recorded in the packet's Gaps.**

**M2 — `Edit` is now offered on directories in both menus and does nothing at all.**
`ENTRY_MENU` is unconditional, so every row menu offers all ten actions
(`crates/sftp-ui/src/table_delegate.rs:490-498`). `SftpPanel::do_edit`
(`crates/sftp-ui/src/edit.rs:209-214`) answers a directory selection with
`log::debug!(… "— ignored"); return;` — no notification, no dialog, no state change. Before this
diff the row menu offered `Open`/`Download` for a directory and `Edit`/`Download` for a file
(removed code in `table_delegate_menu.rs`), and the `⋮` menu had no `Edit` at all, so this dead end
is newly reachable from two menus.

Reproduced: right-clicking `subdir` offers Edit (`US-0124-verify-edit-offered-on-a-directory.png`);
clicking it changes nothing, five seconds later
(`US-0124-verify-edit-on-a-directory-does-nothing.png`). The packet's Gaps cover only the
*no-selection* case (*"one invoked with nothing selected reports 'Select a file or folder to …'"*)
— the with-a-directory case is silent and unrecorded. The cheapest fix is the one the packet
already applied to `do_open`: a warning notification.

**M3 — The persisted-schema change does not follow `docs/agents/persistence.md` § Migration rules,
and that section was not reconciled.**
`docks.json` already carries a top-level `schema_version` with a migration hook
(`crates/state/src/dock_persistence.rs:27`, `:62-71`). It is still `1` — confirmed in the file the
walk's application wrote. Instead of the documented route (bump the document version, migrate
`1 -> 2` through `migrate_json_value`, which preserves the `.bak`), a second private `version` was
added inside the feature-owned field and the drop happens at read time in the UI delegate. The
rules section says, in order:

- *"Add an explicit top-level schema version when the first incompatible schema change is
  introduced"* — the top-level version exists and was not bumped for an admittedly incompatible
  change.
- *"Migrate a parsed value in memory, validate it … then persist through the shared atomic-write
  path"* — nothing is migrated, and the stale v0 column data stays on disk until the next debounced
  save.
- *"Preserve the pre-migration `.bak`; quarantine input that cannot be parsed or migrated without
  data loss"* — this drop **is** data loss by design, and neither a quarantine nor a migration
  `.bak` happens. (A feature crate may not quarantine the shared document, which is exactly why
  this belongs at the document owner's level.)
- Fixture convention: *"Migration fixtures live under the owning crate's
  `tests/fixtures/persistence/`"* — `crates/sftp-ui` has no `tests/` directory at all; compare
  `crates/state/tests/fixtures/persistence/docks-v0.json`.

Only the schema-owner **table row** was updated (`docs/agents/persistence.md:56`); the Migration
rules section below it still states the opposite policy, so the document now contradicts itself.
Relatedly, the packet classifies the risk lane as **normal** while `docs/HARNESS.md` § High-Risk
Triggers lists *"data loss, migrations, retention, ownership, or integrity"* — the packet should
either name the trigger and say why losing presentation state is not material, or take the
high-risk lane. The *behaviour* is correct and I reproduced it end to end; this finding is about
the rule the change bypassed and the document it left inconsistent.

### Minor

**m1** — `apply_persisted_state` gates on `state.version < SFTP_TABLE_STATE_VERSION`
(`crates/sftp-ui/src/table_delegate.rs:153`). A document written by a *future* version reads as
`2 < 1 == false` and its unknown-schema column fields are applied. `!=` would be the safe test.

**m2** — `docs/sftp-browser-design.md:1310` still describes the Local pane's table as
*"(Name / Date Modified / Size)"*. `LocalColumn::ALL` is now `[Name, Size, Modified]`
(`crates/sftp-ui/src/local_pane.rs:70`), and the packet's own point is that the two panes line up.
The stale phrase is four lines above a sentence this diff edited.

**m3** — The "one list, two renderings" invariant is enforced by both call sites literally naming
`ENTRY_MENU`, not by a test. `the_shared_action_list_has_one_order`
(`crates/sftp-ui/src/table_delegate_menu.rs:215`) pins the constant's order only; a call site
switched to a different slice would not be caught. Adequate, but the packet's Acceptance wording
("built from one list") is stronger than the proof.

**m4** — The pre-version drop is proven against an in-memory `SftpTableState::default()`
(`crates/sftp-ui/src/table_delegate.rs:596`), not against a deserialized version-less document. The
`#[serde(default)]` path is only exercised incidentally by
`crates/core/src/sftp.rs:392-398`, which parses `{"column_widths":{"name":300.0}}` but never
asserts `version == 0`. One extra assertion there would close it. (I closed it by hand instead:
the walk seeded a real pre-packet `docks.json` — see the E2E table.)

**m5** — Before the first `canvas` measurement, Name falls back to its configured 200 px
(`crates/sftp-ui/src/table_delegate.rs:126-130`), which is wider than a ~307 px dock can take with
Size + Date Modified. That is one frame of overflow on every cold open of the panel. Harmless in
practice, unmentioned.

**m6** — The `⋮` menu is now sixteen rows (ten actions, two separators' worth of structure, Follow
Terminal Cwd, the Columns label and six checkboxes). In both the implementer's frame and mine it
reaches y≈971 of a 1000 px window; a shorter window will clip the Group row. The unified list made
it four rows taller than before.

**m7 (US-0125)** — *"A coding agent working in one of your terminals shows up here on its own"*
(`crates/agent-ui/src/view.rs:45-46`) over-promises. Per `docs/osc-agent-status.md` §1 an agent
appears only if it implements and emits the sequence; most do not. The footnote carries the
condition but reads as trivia rather than as the precondition. The packet's own Risks section
predicted exactly this (*"Vaguer, not clearer … The copy must still tell the user what causes an
entry"*). A phrasing such as "…that reports its status shows up here on its own" would keep the
plain language and the truth.

**m8 (US-0125)** — the copy test asserts `!headline.contains("OSC")` but, unlike the body, not
`!headline.contains("20308")` (`crates/agent-ui/src/view.rs:692-694`). One-line asymmetry.

---

## Commands run

All from the verifier's own worktree, `CARGO_BUILD_JOBS=3`.

```
cargo test -p oneterm-sftp-ui -p oneterm-agent-ui -p oneterm-core
  oneterm-sftp-ui  → test result: ok. 55 passed; 0 failed; 0 ignored
  oneterm-agent-ui → test result: ok. 6 passed; 0 failed; 0 ignored
  oneterm-core     → test result: ok. 51 passed; 0 failed; 0 ignored
```

Every new and changed test named in the two packets ran and passed:
`types::tests::the_default_columns_fit_the_docked_panel`,
`types::tests::name_stops_shrinking_at_its_minimum`,
`table_delegate::tests::name_fills_the_measured_panel_width`,
`table_delegate::tests::a_pre_version_state_falls_back_to_the_new_defaults`,
`table_delegate::tests::persisted_state_round_trips_and_ignores_invalid_values`,
`table_delegate::tests::toggling_visibility_never_hides_name`,
`table_delegate::tests::widths_apply_in_visible_order_and_are_clamped`,
`table_delegate_menu::tests::the_shared_action_list_has_one_order`,
`table_delegate_menu::tests::the_empty_area_menu_is_a_subset_of_the_same_list`,
`view::tests::empty_state_copy_keeps_the_protocol_in_the_footnote`.

### Mutation testing (both reverted; `git status` clean afterwards)

| Mutation | Caught by |
|---|---|
| `crates/sftp-ui/src/types.rs:233` — drop `- TABLE_TRAILING_GUTTER` from `name_column_width`, so the table would overflow by the trailing gutter | `test result: FAILED. 53 passed; 2 failed` — `types::tests::the_default_columns_fit_the_docked_panel` (`types.rs:367`) and `table_delegate::tests::name_fills_the_measured_panel_width` (`table_delegate.rs:620`) |
| `crates/sftp-ui/src/table_delegate_menu.rs:110` — make `SftpAction::NewFolder` need a selection, so the empty-area menu loses a row | `test result: FAILED. 54 passed; 1 failed` — `table_delegate_menu::tests::the_empty_area_menu_is_a_subset_of_the_same_list` (`table_delegate_menu.rs:239`) |

Both decision functions are genuinely covered.

### Gate

```
pwsh scripts/ci-local.ps1
...
==> python scripts/check-theme-contrast.py
check-theme-contrast: 702 foreground/surface pairings across 117 token/variant rows, all >= 4.5:1

==> python scripts/third-party-notices.py --check
THIRD-PARTY-NOTICES.md is up to date.

ci-local: all checks passed.
```

The gate started before this report was written, so its two documentation checks were re-run
afterwards over the finished file:

```
python scripts/check-english.py    → English contributor-text check passed for 956 files.
python scripts/check-doc-paths.py  → Doc path check passed for 200 current paths in 11 documents.
```

---

## E2E — the verifier's own GUI walk

Own `cargo build -p oneterm-app --profile fast-dev` binary, launched by this session with its
working directory set to a scratch directory so `config_dir()` (`target/`, debug) is isolated from
the machine's real configuration. The driver posts `WM_*` to the window of that pid only and
captures with `PrintWindow`; the application was closed with `WM_CLOSE` to the same pid. Backing
store: the repository's own loopback `sftp-dev-server` on **127.0.0.1:2244** (nobody else's port),
stopped afterwards. `target/fast-dev` was deleted after the walk.

| Frame | What it shows |
|---|---|
| `US-0124-verify-44-docked-1600.png` | Docked browser at a 1600 px window, connected: **Name / Size / Date Modified**, Size visible, no horizontal scrollbar. Re-take of before-frame 44, where Size was absent entirely. |
| `US-0124-verify-44-docked-900.png` | The same at a 900 px window (~307 px of list area) **and** the persistence proof: this run started from a `docks.json` seeded with a pre-US-0124 `sftp_table_state` (no `version`, all six columns visible, `name: 320.0`). The seeded layout is ignored and the new defaults are used. Measured against the frame: the list area spans x 584–891 (~307 px) and the Name/Size divider sits at x 687, so Name is 103 px — exactly `307 − 72 − 116 − 16`, the formula in `name_column_width`. No horizontal scrollbar. |
| `US-0124-verify-48-overflow-menu.png` | The `⋮` menu: Open / Edit / Download — Upload Files / Upload Folder / New Folder — Rename / Delete — Properties / Refresh, then Follow Terminal Cwd and the dimmed **Columns** label with the six checkboxes. |
| `US-0124-verify-45-context-menu.png` | The right-click menu: the same ten items, same order, same icons. Reproduces `F30`'s fix independently. |
| `US-0124-verify-47-dual-pane-no-selection.png` | Dual pane wide: both panes Name / Size / Date Modified, no blank fourth column, no horizontal scroll; `↑ Upload` at the right end of the Local toolbar and `↓ Download` at the left end of the Remote toolbar, meeting at the split, **both dimmed** with nothing selected. |
| `US-0124-verify-47-dual-pane-both-enabled.png` | A selection in each pane: **both controls enabled**. |
| `US-0124-verify-column-resize-overflows.png` | **`M1`.** After dragging the Size divider ~96 px right: `Date Modified` is clipped and a horizontal scrollbar has appeared; Name did not shrink. |
| `US-0124-verify-column-resize-heals-on-refresh.png` | **`M1`.** One click on Refresh: Name is re-derived (names now truncate), the scrollbar is gone, the wider Size persists. |
| `US-0124-verify-edit-offered-on-a-directory.png` | **`M2`.** The row menu on `subdir` offers `Edit`. |
| `US-0124-verify-edit-on-a-directory-does-nothing.png` | **`M2`.** Five seconds after clicking it: no dialog, no notification, no change. |
| `US-0125-verify-22-agent-panel.png` | Agent mode, empty: the `Agents` header, the bot icon, `No agents are running`, the plain sentence and the one dimmed footnote. Re-take of before-frame 22, which had no header and read *"Agents that emit OSC 20308 appear here."* |
| `US-0125-verify-01-ssh-client-headers.png` | SSH Client mode in the same session and the same window: the `Session` and `SFTP Browser` headers the new one matches. Pixel comparison in the claims table above. |

Written `docks.json` after the first run (proving the format claims):

```json
"sftp_table_state": { "version": 1, "column_widths": { "permissions": 150.0, "size": 60.0,
  "owner": 90.0, "group": 90.0, "modified": 116.0 }, "column_visibility": { "name": true,
  "size": true, "owner": false, "group": false, "permissions": false, "modified": true },
  "expanded": false, "local_dir": "C:\\Users\\trunglt" }
```

`version: 1` is written, no `name` width is stored, and the document's own `schema_version` is
still `1` (`M3`). The `size: 60.0` is the residue of the `M1` resize probe, not a default.

---

## Gaps in this verification

- **No real remote host.** Everything here is the loopback `sftp-dev-server`, the same gap the
  packet records.
- **The populated Agent panel was not re-taken.** Producing one needs a process emitting OSC 20308;
  I accepted the implementer's `US-0125-22c-agent-panel-populated.png` at face value. The claim
  that the header steals no row from the list is therefore *not* independently verified.
- **"The choice persists across a restart" was verified one way only.** I confirmed that the panel
  *writes* `version: 1` plus the full visibility map, and that a version-less document is dropped on
  read; I did not switch a column on, restart and confirm it comes back. That direction rests on
  `persisted_state_round_trips_and_ignores_invalid_values` and the implementer's
  `US-0124-44e-columns-persisted-after-connect.png`.
- **The transfers themselves were not re-run** (`47c`/`47d`). I verified that both buttons enable
  and that `do_download` routes the expanded case to the confirming
  `download_entry_to_local`; I did not re-execute an upload and a download.
- `46-sftp-delete-confirm.png` and `49-sftp-after-tab-switch.png` were not re-taken. Both paths are
  untouched by the diff (`do_delete` is byte-identical; the follow behaviour is not in the diff at
  all), so the risk is low, but they rest on the implementer's frames.
- The walk's "Trust and Connect" wrote a `127.0.0.1:2244` entry into the machine's OpenSSH
  `known_hosts`; the entry was removed afterwards.
- `research/ux-walkthrough-2026-09-16.md` and `research/before/` are inside the intake folder
  (`docs/spec-intakes/IN-0042-ux-polish-round-1/research/`), not at the repository root. Before
  frames 22, 44, 45, 46, 47 and 48 were read there.

## Suggested follow-up

- One `BUG` for `M1` (refresh the table after `ColumnWidthsChanged`, in both panes).
- One `BUG` for `M2` (`do_edit` on a directory should say so, the way `do_open` now does).
- Reopen `US-0124` for `M3`: reconcile `docs/agents/persistence.md` § Migration rules with what was
  actually done, or do what it says; and state why the "migrations / data loss" high-risk trigger
  does not apply.
- `m2` (stale Local-pane column order in `docs/sftp-browser-design.md:1310`) can ride along with
  either.
