# Work: New Terminal menu rows show the session colour square

ID: US-0110
Intake: IN-0033
Created: 2026-09-16

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

- Change type: new capability (a new, independently acceptable outcome inside an already accepted intake; `US-0094` is shipped, so this is a new `US`, not acceptance rework of it)
- Risk lane: normal
- Spec Intake, when required: `IN-0033` — `docs/spec-intakes/IN-0033-new-terminal-button-lists-ssh-sessions/IN-0033.md`

## Outcome

Owner request, 2026-09-16 (Vietnamese, recorded in English): *the items in the SSH
Session section of the "+" (New Terminal) dropdown should also show the coloured square,
like in the SSH Sessions panel.*

Every saved-session row in the "+" dropdown draws the same 8 px coloured square the right
dock's session tree draws before the session name, filled with that session's saved
colour, and with the same default colour when the session has none saved. Nothing else in
the menu changes.

## Scope

- [x] In scope:
  - Carry the session colour to the menu through the existing `WorkspaceCommands` hop —
    the row tuple in `oneterm_state::commands::SavedSshSessionSections` gains a hex-colour
    `String`, produced with the default already applied.
  - `menu_entries` in `crates/session-ui/src/tree_builder.rs` resolves the colour (saved
    value, else `SshSession::DEFAULT_COLOR_HEX`), so the data default lives in one place
    next to the constant.
  - `TerminalPanel::title_suffix` renders each saved-session row as a square plus the
    title, matching `tree_render.rs`: `Hsla::parse_hex`, theme accent as the last resort,
    `div().w(px(8.)).h(px(8.))` and `gap_2`.
  - Documentation: the `docs/gui-layout.md` sentence describing the row, and the row-shape
    line in this intake's `high-level-design.md`.
- [x] Out of scope:
  - Local-shell rows, the "No saved sessions" hint, the two labelled separators, the
    plain separator and "New SSH Session" — all unchanged.
  - The right dock's session tree, the session dialog's colour picker, and the persisted
    `ssh_session.json` shape — unchanged.
  - Any new crate edge or dependency (R1/R5 forbid `terminal-view -> session-ui`).

## Acceptance

- [x] A saved session with a colour shows that colour's square in the "+" menu.
- [x] A saved session with no colour (or a blank one) shows the same default square the
      tree shows, `#56B6C2`.
- [x] The square is 8x8 px with a `gap_2` before the label, as in `tree_render.rs`.
- [x] Clicking a row still opens that session's connect dialog, and keyboard navigation
      still reaches the row.
- [x] No colour literal is added to `crates/terminal-view`: the square's colour is the
      session's, the fallback is `cx.theme().accent`.
- [x] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

## Documentation

### Owning Docs Reviewed

- `docs/gui-layout.md` line 65 — the sentence "Session rows show the session title only,
  so two saved sessions sharing a label are indistinguishable here by design …". This
  describes the row's content, so it must change.
- `docs/spec-intakes/IN-0033-new-terminal-button-lists-ssh-sessions/high-level-design.md`
  — the wireframe (line ~60-70), the "Grouped, storage order, title only" decision
  (line ~103), the "No colour or icon literals" decision (line ~122) and the data-flow
  step naming `Vec<(String, Vec<(u64, String)>)>` (line ~134). The row shape and the
  colour statement change.
- `docs/ssh-client-connect.md` lines 65 and 139 — both describe *which surfaces* enter the
  connect flow and that the `+` menu reuses `open_connect_dialog` by session id. Neither
  describes how a row is drawn, and this change does not alter the flow, the id, or the
  dialog. **No change.**
- `docs/agents/crate-dependency-rules.md` R1/R4/R5/R10 — reconfirmed: the colour crosses
  the same `WorkspaceCommands` fn-pointer hop as the title, as a primitive `String`, so
  `crates/state` still names no feature type and no new edge appears.
- `crates/session-ui/src/tree_render.rs` (the tree's square) and
  `crates/session-ui/src/session_state.rs` (`SshSession::DEFAULT_COLOR_HEX`) — the
  behaviour being mirrored.

### Documentation Action

Update required:

- `docs/gui-layout.md` — the "Session rows show the session title only" sentence gains the
  coloured square and its default.
- `high-level-design.md` — wireframe rows, the title-only decision, the "no colour
  literals" decision and the row tuple in the data flow.
- `IN-0033.md` — `US-0110` added to Candidate Work Packets.

Reason: the change alters what a documented UI row contains and the shape of a documented
data type. `docs/ssh-client-connect.md` stays as it is, for the reason recorded above.

### Reconciliation

Docs changed: `docs/gui-layout.md`, this intake's `high-level-design.md`, `IN-0033.md`,
and this packet. The `docs/ssh-client-connect.md` no-change reason still holds after
implementation — the connect flow, its entry point and its id are untouched.

## Context

- The row travels `menu_entries` -> `SavedSshSessionSections` -> `title_suffix`. Row shape
  chosen: `(u64, String, String)` = `(stable session id, title, hex colour)`. A third
  tuple element rather than a struct because `crates/state` sits below the feature crates
  and must stay primitive-only (R10), and because the two existing elements already
  travel this way.
- The default is applied by the producer (`menu_entries`), not the renderer, so
  `SshSession::DEFAULT_COLOR_HEX` is read in exactly one place, next to the constant. The
  renderer's only fallback is `cx.theme().accent`, for a hex that will not parse.
- Kit check (`reference/gpui-kit/crates/component/src/menu/popup_menu.rs`) before choosing
  `PopupMenuItem::element` over `.icon()`:
  - `render_item` builds the same `MenuItemElement::new(ix, &group_name)` base for
    `Item` and `ElementItem` — same `px(8.)` inner padding, same `rounded(radius)`, same
    `.selected(selected)` and the same `on_hover` listener (lines ~1193-1210).
  - Height: `Item` sets `.h(item_height)` on the outer element; `ElementItem` sets
    `.min_h(item_height)` on its inner `h_flex` (lines ~1240-1250 vs ~1274). Both resolve
    to the kit's 26 px for content shorter than that, which an 8 px square beside a
    `text_sm` label is — so `ROW_HEIGHT = 28.` still holds and was left alone.
  - Click: `ElementItem` gets the same `this.on_click(ix, …)` listener when not disabled
    (line ~1234).
  - Keyboard: `is_clickable()` matches `ElementItem { disabled: false, .. }` (line ~236),
    so `select_up` / `select_down` land on it, and `confirm()` has an explicit
    `ElementItem { handler, action, .. }` arm that calls the handler and dismisses
    (line ~849), identical to the `Item` arm.
  - The one difference: `a11y_label()` returns `None` for `ElementItem` (line ~278), so
    the row carries no `aria_label`. Recorded under Gaps. The same already applies to the
    two labelled separators, which are `ElementItem`s too.
- `labelled_separator` in the same file is the in-repo precedent for
  `PopupMenuItem::element`.

## Plan

- [x] Records first: this packet, the `IN-0033.md` entry, the `high-level-design.md`
      updates — committed before any source edit.
- [x] `crates/state/src/commands.rs`: row tuple and its doc comment.
- [x] `crates/session-ui/src/tree_builder.rs`: `menu_entries` resolves the colour; doc
      comment updated; existing tests moved onto a `row()` helper and two new tests added.
- [x] `crates/terminal-view/src/panel/terminal_panel.rs`: render the square.
- [x] `docs/gui-layout.md` sentence.
- [x] Focused tests, then `pwsh scripts/ci-local.ps1`, then the GUI walk.

## Decisions

None. The row shape and the default's home are implementation detail inside choices
`IN-0033` already recorded; nothing here is a constraint future work must inherit beyond
what the HLD now states.

## Verification Plan

1. `cargo test -p oneterm-session-ui menu_entries` — the colour passes through, the
   default is applied for `None` and for a blank value, and the existing ordering and
   fallback tests still hold with the wider row.
2. `cargo test -p oneterm-terminal-view` — the panel's test double builds the new tuple.
3. `pwsh scripts/ci-local.ps1` with `CARGO_BUILD_JOBS=6` — the full gate.
4. E2E on the Windows desktop: launch `cargo run -p oneterm-app`, open the "+" dropdown,
   capture the saved-session rows with their squares, including one session whose colour
   is unset (default square).

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### Focused

```
cargo test -p oneterm-session-ui menu_entries
    running 9 tests
    ... menu_entries_carries_the_saved_colour ... ok
    ... menu_entries_applies_the_default_colour_when_none_is_saved ... ok
    test result: ok. 9 passed; 0 failed; 0 ignored; 53 filtered out

cargo test -p oneterm-terminal-view
    test result: ok. 341 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out
```

The two new tests are the ones that bite: `menu_entries_carries_the_saved_colour` fails if
the producer ever emits a constant instead of the session's own hex, and
`menu_entries_applies_the_default_colour_when_none_is_saved` fails if the `None` or blank
arm is dropped. The seven pre-existing `menu_entries` tests were moved onto a `row()`
helper so the widened tuple did not force a literal default into every expectation.

### Gate

```
pwsh scripts/ci-local.ps1   ($env:CARGO_BUILD_JOBS = 6)
...
ci-local: third-party notices OK
ci-local: all checks passed
```

### E2E (Windows desktop)

Built and launched `target/debug/oneterm.exe` from this worktree (pid 4848) and drove only
that pid; it was stopped by pid afterwards. `target/ssh_session.json` (the debug config
dir is `target/`, relative to the process cwd, so this worktree's store is isolated from
the owner's) was seeded with six sessions: `prod-web` `#E06C75`, `staging` `#C678DD`,
`no-colour-saved` with no `color` field, group `infra` holding `db-01` `#98C379` and
`db-02` `#E5C07B`, and group `lab` holding `sandbox`, also with no `color` field.

- `evidence/US-0110-menu-rows-with-colour-squares.png` — the "+" dropdown open: each saved
  session row carries its own square (red `prod-web`, purple `staging`, green `db-01`,
  amber `db-02`), the local shells, both labelled separators and "New SSH Session" are
  unchanged, and the right dock's tree is visible in the same frame for comparison.
- `evidence/US-0110-menu-rows-zoomed.png` — the same menu at 3x, where the two sessions
  that saved no colour (`no-colour-saved`, `sandbox`) clearly show the default `#56B6C2`
  teal, the same teal the tree gives them.
- `evidence/US-0110-session-tree-for-comparison.png` — the right dock's SSH Sessions tree
  alone, the surface being matched.
- `evidence/US-0110-connect-dialog-from-coloured-row.png` — clicking the `prod-web` row
  opens "Connect to prod-web (deploy@10.20.0.11:22)", so `on_click` still fires on the
  element item and the session id still routes correctly.

Capture method: the desktop was locked during the walk, so `CopyFromScreen` returned the
lock screen. `PrintWindow(hwnd, dc, PW_RENDERFULLCONTENT)` against the launched pid's own
window returned the real frames, and the clicks were delivered as `WM_MOUSEMOVE` /
`WM_LBUTTONDOWN` / `WM_LBUTTONUP` posted to that window. Both target the single pid this
session launched; no window was enumerated by name or title.

### Gaps

- `PopupMenuItem::ElementItem` carries no `aria_label` (the kit's `a11y_label()` returns
  `None` for it, `popup_menu.rs` line ~278), so a saved-session row is now unlabelled to a
  screen reader where a plain `Item` row was labelled. Hover, selection, click and
  keyboard navigation are unaffected. Fixing it needs a kit change (an `a11y_label` on
  `ElementItem`), which is out of this packet's scope; the menu's two labelled separators
  already have the same property.
- The square's colour is not asserted by an automated test at the render layer — the
  gpui element tree is not queryable in the panel tests. The data half (which hex reaches
  the row) is unit-tested; the drawing half is covered by the GUI evidence above.
- Keyboard navigation over the new rows was **not** exercised in the GUI walk: the locked
  desktop made synthetic key input unreliable, and the click evidence was captured by
  window message instead. It rests on the kit source read recorded under Context —
  `is_clickable()` matches `ElementItem` and `confirm()` has an explicit `ElementItem` arm
  — plus the fact that the menu's existing element items already navigate correctly.

## Handoff

The coordinator inserts the `harness.db` row in the main checkout. Nothing else pending.

### harness.db rows

```python
import sqlite3, datetime
db = sqlite3.connect("harness.db")
intake_id = db.execute("select rowid from intake where document_number=33").fetchone()[0]
now = datetime.datetime.now().isoformat(timespec="seconds")
db.execute(
    "insert into story (id,title,created_at,risk_lane,contract_doc,packet_doc,status,"
    "unit_proof,integration_proof,e2e_proof,platform_proof,evidence,verify_command,"
    "last_verified_at,last_verified_result,notes,intake_id) "
    "values (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    (
        "US-0110",
        "New Terminal menu rows show the session colour square",
        now,
        "normal",
        "docs/gui-layout.md",
        "docs/spec-intakes/IN-0033-new-terminal-button-lists-ssh-sessions/"
        "US-0110-new-terminal-menu-rows-show-session-colour.md",
        "implemented",
        "cargo test -p oneterm-session-ui menu_entries (9 passed)",
        "cargo test -p oneterm-terminal-view (341 passed)",
        "GUI walk: evidence/US-0110-menu-rows-with-colour-squares.png, "
        "evidence/US-0110-menu-rows-zoomed.png, "
        "evidence/US-0110-connect-dialog-from-coloured-row.png",
        "pwsh scripts/ci-local.ps1 -- all checks passed",
        "docs/spec-intakes/IN-0033-new-terminal-button-lists-ssh-sessions/evidence/",
        "pwsh scripts/ci-local.ps1",
        now,
        "pass",
        "Row tuple widened to (id, title, hex colour); default applied in menu_entries. "
        "Gap: ElementItem carries no aria_label (kit limitation).",
        intake_id,
    ),
)
db.commit()
```
