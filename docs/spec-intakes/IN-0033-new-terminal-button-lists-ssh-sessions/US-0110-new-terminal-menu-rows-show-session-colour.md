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
  - `session_color_hex` in `crates/session-ui/src/tree_builder.rs` resolves the colour
    (saved value when `parse_hex` accepts it, else `SshSession::DEFAULT_COLOR_HEX`), and
    both `menu_entries` and the tree's own `tree_render.rs` go through it, so the two
    surfaces cannot draw one session two ways.
  - `TerminalPanel::title_suffix` renders each saved-session row as a square plus the
    title, matching `tree_render.rs`: `Hsla::parse_hex`, theme accent as the last resort,
    `div().w(px(8.)).h(px(8.))` and `gap_2`.
  - Documentation: the `docs/gui-layout.md` sentence describing the row, and the row-shape
    line in this intake's `high-level-design.md`.
- [x] Out of scope:
  - Local-shell rows, the "No saved sessions" hint, the two labelled separators, the
    plain separator and "New SSH Session" — all unchanged.
  - The session dialog's colour picker and the persisted `ssh_session.json` shape —
    unchanged. The right dock's tree keeps its appearance for every colour the app can
    save; the rework only routes it through the shared resolver so a hand-edited value
    resolves the same way on both surfaces.
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
- The colour is resolved by the producer, not the renderer, in one function
  (`session_color_hex`) that both the tree leaf and the menu row call — so
  `SshSession::DEFAULT_COLOR_HEX` is read once, next to the constant, and the two surfaces
  cannot disagree about any input. Each renderer's `cx.theme().accent` arm is unreachable
  while the constant is a valid hex.
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
   resolver agrees with the tree for every input `parse_hex` rejects, each row keeps its
   own colour within one section, and the existing ordering and fallback tests still hold
   with the wider row. This is the only automated proof of the behaviour.
2. `cargo test -p oneterm-terminal-view` — a **compile check on the widened tuple**, not
   coverage of the menu. The crate's test double (`src/panel/tests.rs:454`) returns
   `Vec::new()` and no test in the crate exercises `title_suffix`, so the 341 passing
   tests prove only that the new type still builds everywhere it is named. The rendering
   itself is covered by the GUI evidence, and by nothing else — see Gaps.
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

## Owner decision pending: accessible name

Raised by the independent verification as **F1** (major); see
`evidence/US-0110-verify.md` §6. **No code was changed for it — it needs a call, not a
patch.**

What changed: a saved-session row used to be `PopupMenuItem::new(name)`, an `Item`, whose
`a11y_label()` returns `Some(label)` (`popup_menu.rs:274-276`) and reaches the element as
`.aria_label(label)` (`popup_menu.rs:1210`). It is now a `PopupMenuItem::element`, an
`ElementItem`, whose `a11y_label()` is `None` (`popup_menu.rs:278`). So each row still
renders as `Role::MenuItem` but carries no accessible name: a screen reader announces the
row without the session it opens. That is a regression in a row that previously had one,
on the surface this packet set out to make more legible. (The packet originally noted "the
two labelled separators already have the same property" — that does not carry: those are
`disabled`, deliberately outside hover and keyboard navigation, so they were never
announced as items in the first place.)

Why it cannot be fixed here: `gpui-component` comes from crates.io (`Cargo.toml:44`, no
`[patch]` section), and the `a11y_label()` match is private to the kit. The only in-repo
alternative, `PopupMenuItem::new(name).icon(square)`, keeps the accessible name but forces
`Icon::xsmall()` = 12 px (`popup_menu.rs:1138`), and it flips `has_left_icon` for the whole
menu, indenting the local-shell labels too.

The two options, for the owner:

- **A — accept, with a follow-up upstream.** Keep the 8 px square that matches the tree
  exactly, record the regression in a `DEC`, and open a follow-up to carry an
  `aria_label` on `ElementItem` in `gpui-component`. Costs: the rows stay unnamed to a
  screen reader until that lands.
- **B — use `.icon()` at 12 px.** The rows keep their accessible name today, at the cost
  of a square half again the size of the tree's, and the local-shell rows gaining an icon
  gutter they do not use — so the two surfaces stop matching, which is the outcome this
  packet was asked for.

Recommendation: **A**, because the request was explicitly "like in the SSH Sessions panel"
and B breaks exactly that; but this is the owner's call and the packet should not be
accepted as if a Gaps bullet had settled it.

## Evidence and Gaps

### Rework after independent verification (2026-09-16)

`evidence/US-0110-verify.md` returned PASS-WITH-NOTES. F2, F4 and F3 are addressed below;
F1 is recorded above as an owner decision and no code changed for it. F5 is moot: the
`chore(release): v0.6.0` bot commit (`fef4b866`) that rode along on the branch is now on
`main`, so this packet's diff is clean.

**F2 — one resolver, both surfaces.** `session_color_hex(&SshSession) -> &str` now lives in
`crates/session-ui/src/tree_builder.rs`, beside `session_subtitle`, and is the only place
that decides which hex a session is drawn with: the saved value when
`Colorize::parse_hex` accepts it, `SshSession::DEFAULT_COLOR_HEX` otherwise. Both
`tree_render.rs` (the tree leaf) and `menu_entries` (the "+" row) call it, so the
verifier's three divergent inputs — `"#abc"`, `"#GGGGGG"`, `" #E06C75 "` — now draw the
same teal on both surfaces. `terminal_panel.rs`'s `cx.theme().accent` arm is kept but is
now unreachable, exactly as the tree's already was, and its comment says so instead of
claiming a fallback policy of its own.

It returns the hex **text**, not an `Hsla`, which is a deliberate deviation from the
rework brief's "`menu_entries` then emits `color.to_hex()`". `Colorize::to_hex` is lossy:
it truncates each channel (`color.rs:269-287`) after a round trip through `Hsla`. Probed
over ten hex values, three came back changed — `#C678DD -> #C677DD`, `#010203 -> #010202`,
`#123456 -> #113456`. Converting in the producer would therefore have re-introduced the
very divergence F2 is about, and made it *more* reachable: it would hit every colour the
session dialog itself saves, not just a hand-edited file. Relaying the resolved text keeps
the two surfaces byte-identical. The string is still always parseable, which was the point
of the instruction, because it is either a value `parse_hex` just accepted or the crate's
own constant.

**F4 — the surviving mutation is caught.** `menu_entries_carries_the_saved_colour` gained
a third session, `db-02` `#E5C07B`, in the *same* `infra` group as `db-01` `#98C379`, so a
section now contains two different colours. Running the verifier's M3 mutation (every row
after the first in a section takes the first row's colour):

```
test tree_builder::tests::menu_entries_carries_the_saved_colour ... FAILED
assertion `left == right` failed: each row keeps its own colour, including within one section
  left: [("", [(1, "prod", "#E06C75")]), ("infra", [(2, "db-01", "#98C379"), (3, "db-02", "#98C379")])]
 right: [("", [(1, "prod", "#E06C75")]), ("infra", [(2, "db-01", "#98C379"), (3, "db-02", "#E5C07B")])]
test result: FAILED. 9 passed; 1 failed; 0 ignored; 53 filtered out
```

The mutation was reverted immediately; the suite is green below.

**F3 — the Verification Plan no longer implies `-p oneterm-terminal-view` covers the
menu.** Step 2 now says it is a compile check on the widened tuple, matching what Gaps
already said.

### Focused

```
cargo test -p oneterm-session-ui menu_entries
    running 10 tests
    ... menu_entries_carries_the_saved_colour ... ok
    ... menu_entries_applies_the_default_colour_when_none_is_saved ... ok
    ... menu_entries_and_the_tree_resolve_every_colour_alike ... ok
    test result: ok. 10 passed; 0 failed; 0 ignored; 53 filtered out

cargo test -p oneterm-session-ui
    test result: ok. 63 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

cargo test -p oneterm-terminal-view
    test result: ok. 341 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out
```

The three colour tests are the ones that bite. `menu_entries_carries_the_saved_colour`
fails if the producer emits a constant instead of the session's own hex, or pairs a row
with a neighbour's colour (F4 above).
`menu_entries_applies_the_default_colour_when_none_is_saved` fails if the `None` or blank
arm is dropped. `menu_entries_and_the_tree_resolve_every_colour_alike` walks the verifier's
probe inputs — blank, whitespace, no hash, lowercase, 3-digit, 8-digit, garbage, padded —
and asserts for each that the menu row and the tree's square resolve to the same string,
so the two surfaces cannot drift apart again. The seven pre-existing `menu_entries` tests
were moved onto a `row()` helper so the widened tuple did not force a literal default into
every expectation.

### Gate

Re-run after the verification rework, from this worktree root with
`$env:CARGO_BUILD_JOBS = 6`:

```
pwsh scripts/ci-local.ps1
...
==> python scripts/check-english.py
English contributor-text check passed for 928 files.
==> python scripts/completion-catalog.py validate
[completion-catalog] all catalogs valid
==> python scripts/third-party-notices.py --check
THIRD-PARTY-NOTICES.md is up to date.

ci-local: all checks passed.
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

The PNGs were captured before the verification rework and were **not** re-taken: the
desktop is still locked, and the rework cannot change what they show. Every colour in the
walk (`#E06C75`, `#C678DD`, `#98C379`, `#E5C07B`, and the two sessions with none) is a
value `parse_hex` already accepted, so `session_color_hex` returns it unchanged and each
square is the same pixel it was.

Capture method: the desktop was locked during the walk, so `CopyFromScreen` returned the
lock screen. `PrintWindow(hwnd, dc, PW_RENDERFULLCONTENT)` against the launched pid's own
window returned the real frames, and the clicks were delivered as `WM_MOUSEMOVE` /
`WM_LBUTTONDOWN` / `WM_LBUTTONUP` posted to that window. Both target the single pid this
session launched; no window was enumerated by name or title.

### Gaps

- `PopupMenuItem::ElementItem` carries no `aria_label`, so a saved-session row is now
  unlabelled to a screen reader where a plain `Item` row was labelled. Hover, selection,
  click and keyboard navigation are unaffected. This is **not** settled here: see "Owner
  decision pending: accessible name" above, which is where it must be resolved before the
  packet is accepted.
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
        "cargo test -p oneterm-session-ui (63 passed; menu_entries 10 passed)",
        "cargo test -p oneterm-terminal-view (341 passed)",
        "GUI walk: evidence/US-0110-menu-rows-with-colour-squares.png, "
        "evidence/US-0110-menu-rows-zoomed.png, "
        "evidence/US-0110-connect-dialog-from-coloured-row.png",
        "pwsh scripts/ci-local.ps1 -- all checks passed",
        "docs/spec-intakes/IN-0033-new-terminal-button-lists-ssh-sessions/evidence/",
        "pwsh scripts/ci-local.ps1",
        now,
        "pass",
        "Row tuple widened to (id, title, hex colour); one session_color_hex resolver "
        "shared by the tree and the menu (verifier F2). Owner decision pending: "
        "ElementItem carries no aria_label (kit limitation) -- accept with an upstream "
        "follow-up, or use .icon() at 12px and diverge from the tree's 8px.",
        intake_id,
    ),
)
db.commit()
```
