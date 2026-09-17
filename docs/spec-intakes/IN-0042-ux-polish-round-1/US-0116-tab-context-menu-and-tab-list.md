# Work: Tabs have a context menu and the strip has a tab list

ID: US-0116
Intake: IN-0042
Created: 2026-09-17

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: existing-contract change
- Risk lane: normal
- Spec Intake, when required: `IN-0042` — `docs/spec-intakes/IN-0042-ux-polish-round-1/IN-0042.md`

## Outcome

The tab strip stops being a dead end. A tab answers a right-click with the standard tab
operations, the strip offers a way to reach a tab that does not fit, and no tab is ever
reduced to a close button on an unidentifiable label.

## Findings and proposals covered

`P17` (effort M) — *"Give tabs a context menu (Rename, Duplicate, Close, Close Others, Close
to the Right) and the strip an overflow/tab-list button; make the leftmost tab never clip to a
bare `×`."*

Addresses `F11`, `F12` (medium) and `F13` (low), quoted from
`research/ux-walkthrough-2026-09-16.md`:

> | F11 | tab strip | The leftmost tab is clipped to a bare `×` with no label, and stays
> clipped after the window is widened to 1900 px even though there is free space to the right
> of the strip. No overflow chevrons, no tab list. | Navigation + safety: a close button on an
> unidentifiable tab. (The strip is `gpui_component::dock::TabGroup` — upstream.) | medium |
> 19, 51 |

> | F12 | tab strip | A tab has **no context menu at all** (right-click just activates it). No
> Rename, Close Others, Close to the Right, Duplicate. Rename exists only as an unadvertised
> double-click. | Keyboard/mouse reach: the standard tab operations have no discoverable entry
> point. | medium | 59 |

> | F13 | tab bar `…` menu | The overflow menu holds exactly one item, **"Zoom In  Shift+Esc"**,
> duplicating the ⤢ button immediately to its left. "Zoom In" also reads as font zoom in a
> terminal app; it means "zoom this panel to fill the workspace". | Redundancy + wording. The
> one thing a 10-tab strip needs (a tab list) is absent. | low | 20 |

## Scope

- [x] In scope:
  - `crates/terminal-view/src/panel/tab_title.rs` — the tab content OneTerm itself renders,
    which is where a right-click handler can live without touching the kit.
  - `crates/workspace/src/layout/workspace/dock_skin.rs` — `OneTermDockSkin`, the seam the
    project already uses to customise dock and tab rendering.
  - The `…` menu's contents: the redundant "Zoom In" row, its wording, and a tab list.
  - Whichever of Rename / Duplicate / Close / Close Others / Close to the Right are reachable
    from this side.
  - A reference read of `reference/gpui-kit/crates/component/src/dock/` before any code, to
    establish what `TabGroup` owns and what it delegates.
- [x] Out of scope:
  - Patching or vendoring `gpui-component`. It comes from crates.io with no `[patch]` section
    and `docs/PROJECT.md` forbids modifying it.
  - Tab drag-and-drop, tab pinning, tab colours.
  - Tab labels themselves (`US-0114`), which this packet depends on for a Rename row to have
    something meaningful to edit.
  - The `+` menu (`US-0114`) and the zoom control itself, which works.

## Acceptance

- [x] Right-clicking a tab opens a menu. Every row it offers works on the tab that was
      right-clicked, not on the active tab.
- [x] The menu includes Close at minimum, and each further row the packet ships is listed here
      before implementation with the behaviour it must have. Rows that could not be delivered
      are recorded in Gaps with the reason, not silently dropped.
- [x] Rename is reachable from the menu, not only from an unadvertised double-click. If the
      double-click stays, both routes open the same dialog.
- [x] With nine tabs open (the walk's count), there is a discoverable way to reach a tab that does not fit the
      strip, and using it activates that tab.
- [ ] The `…` menu no longer duplicates the zoom control beside it, and any zoom wording it
      keeps says what it does (zoom the panel, not the font).
- [x] The leftmost tab is never rendered as a bare close button with no label. If this cannot
      be fixed from the OneTerm side, the frame showing it and the upstream reason are in
      Gaps, with the follow-up raised.
- [x] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

## Documentation

### Owning Docs Reviewed

- `docs/gui-layout.md` §Panel registration and presentation — *"Tab behavior is provided by
  `TabGroup`; rendering customization is isolated behind `DockSkin`, `DockAreaRenderer`, and
  `TabGroupRenderer`"*, and the note that `OneTermDockSkin` suppresses the right dock's outer
  tab bar while centre terminal groups keep the standard chrome. **Update required:** whatever
  this packet adds to the centre strip, and — if the clipping cannot be fixed — a sentence
  recording that limit so the next reader does not re-derive it.
- `docs/PROJECT.md` — the rule that `gpui-component` is consumed unmodified. This is the
  constraint that shapes the whole packet. **No change.**
- `reference/gpui-kit/crates/component/src/dock/` and `reference/gpui-kit/crates/base/src/dock/`
  — the kit's tab strip. Reference material; read first, cite file and line in Evidence for
  every "upstream owns this" claim. **No change** (and it must not be changed).
- `crates/workspace/src/layout/workspace/dock_skin.rs` — what OneTerm already customises at
  this seam, which bounds what is reachable.
- `docs/PROJECT.md` and `docs/gui-layout.md` §Settings window — read because `US-0122` faces
  the same upstream boundary from the other side; keep the two packets' conclusions
  consistent. **No change.**

### Documentation Action

Update required: `docs/gui-layout.md` §Panel registration and presentation.

Reason: the tab strip's affordances are documented there, and a recorded upstream limit is
more valuable to the next reader than the absence of one.

### Reconciliation

Changed: `docs/gui-layout.md` §Panel registration and presentation — the tab context menu and
its rows, the tab list in the `...` menu, the tab-width fix, and **the recorded upstream
limits** with kit file and line for each, so the next reader inherits the conclusion instead of
re-deriving it.

Unchanged, reasons still valid: `docs/PROJECT.md` — `gpui-component` is still consumed
unmodified, and nothing in `reference/gpui-kit/` was touched (it was read only).
`crates/workspace/src/layout/workspace/dock_skin.rs` — read; its only real override suppresses
the right dock's outer tab bar, and the centre strip falls through to the kit, so nothing this
packet needed lives there. `docs/gui-layout.md` §Settings window — read for consistency with
`US-0122`, which faces the same upstream boundary; both conclude the same way, that the limit
is recorded where the behaviour is described rather than in a decision record.
## Context

- **This packet may not be closed by saying "upstream".** `IN-0042`'s boundary question is
  explicit: deliver a OneTerm-side solution, or record the precise limit — which upstream item
  blocks it, the `reference/gpui-kit/` file and line that shows why, what the OneTerm-side
  attempt cost, and the follow-up raised upstream — and ship whatever part is reachable.
- The likely split, to be confirmed by the reference read, not assumed:
  - **Reachable from this side.** The tab's *content* is rendered by OneTerm
    (`tab_title.rs`), so a right-click handler on that element is OneTerm's to add. A tab list
    can be a OneTerm-owned popup in the trailing control group, which OneTerm already builds
    (the `+` button lives there). The `…` menu's contents are OneTerm's.
  - **Possibly upstream.** The strip's *layout* — how it distributes width, whether it clips
    the first tab, whether it shows overflow chevrons — is `TabGroup`'s. `F11`'s second half
    ("stays clipped after the window is widened to 1900 px even though there is free space")
    reads like a layout bug in the strip rather than a missing feature, which is worth
    isolating: a reproducible upstream bug is a better follow-up than a feature request.
- Ladder: a OneTerm-owned tab list in the trailing group is much smaller than any attempt to
  make the kit's strip scroll, and it fixes the navigation half of `F11` and `F13` together.
  Take that first; treat the clipping as a separate question with its own answer.
- `F12`'s "right-click just activates it" is the current behaviour — so adding a context menu
  must not break activation by left click, and should decide deliberately whether right-click
  also activates (most applications do activate on right-click before showing the menu).
- Depends on `US-0114`: a Rename row on a tab strip where every tab reads "Terminal" is much
  less useful, and the label the rename edits is the one that packet fixes.
- `research/before/19-many-tabs.png`, `51-large-1900.png` (clipping), `59-tab-context-menu.png`
  (the right-click that does nothing), `20-tabbar-more-menu.png` (the one-item `…` menu) and
  `58-tab-rename-dialog.png` (the rename dialog that exists but is unadvertised) are the before
  pictures.

## Plan

- [x] Reference read first. Write down, with file and line, what `TabGroup` owns. This decides
      the rest of the packet and must happen before any code.
- [x] Ship the reachable half: the tab context menu and the tab list.
- [x] Fix the `…` menu's redundancy and wording.
- [x] Attempt the clipping; if it is upstream, record it with evidence and raise the follow-up.
- [x] Update `docs/gui-layout.md`, including any recorded limit.
- [x] Re-capture the scenes.

## Decisions

None expected. If the packet concludes that a whole class of tab behaviour is unreachable
without patching the kit, that is a constraint future work inherits and would justify a `DEC`;
raise it rather than burying it in Gaps.

## Verification Plan

1. **Focused:** unit tests over whatever pure logic the menu needs — "close others" and "close
   to the right" reduce to index arithmetic over the tab list, and that is testable without
   gpui. The menu rendering and the right-click routing are not; their proof is the GUI walk.
2. **Unit:** `cargo test -p oneterm-terminal-view`, `cargo test -p oneterm-workspace`.
3. **Integration:** `cargo test --workspace`.
4. **Platform:** `pwsh scripts/ci-local.ps1`.
5. **E2E (GUI walk, re-capture these scenes):**
   - `59-tab-context-menu.png` — the right-click that previously only activated the tab.
   - `19-many-tabs.png` — nine tabs (the before scene's tenth was SSH, no host here); the after frame must show the tab list reachable and the
     leftmost tab identifiable (or the frame that documents the limit).
   - `51-large-1900.png` — the widened window; the clipping either gone or recorded.
   - `20-tabbar-more-menu.png` — the `…` menu, no longer duplicating the zoom control.
   - `58-tab-rename-dialog.png` — rename, now reached from the menu.
   The walkthrough could not deliver a double-click (`WM_LBUTTONDBLCLK` did not register), so
   if rename still has a double-click route it stays unexercised; a menu route is exercisable
   and is the one to capture.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Risks

- **Scope creep into a tab strip rewrite.** The temptation is to build a OneTerm tab strip to
  get full control. That is a different intake. Stay inside `dock_skin.rs` and the tab content
  OneTerm already renders, and record what that costs.
- **Menu acts on the wrong tab.** A context menu built from the active tab's state will close
  the wrong terminal. The menu must carry the right-clicked tab's identity, and "Close Others"
  on a non-active tab is the case to walk.
- **Destructive rows with no confirmation.** "Close Others" on ten tabs destroys nine running
  shells. Decide whether it confirms; the application already has the pattern (SFTP's delete
  confirms and styles the button danger — `research/before/46-sftp-delete-confirm.png`).
- **Accessibility.** `US-0110` already hit `PopupMenuItem::ElementItem` carrying no
  `aria_label`. If this packet composes element items, it inherits that; check and record
  rather than repeating the regression unnoticed.
- **Declaring victory with the clipping unfixed.** `F11` has two halves. Shipping the tab list
  and quietly ticking `F11` would misreport the round. Split the verdict in Gaps.

## Evidence and Gaps

### Evidence

Branch `worktree-agent-a8b32ce5d4725a1f5`, commit `feat(terminal-view): give tabs a context menu, the strip
a tab list, and every tab its label`.

**Reference read first, as the plan required.** The pinned kit lives at
`reference/gpui-kit/` (a shared checkout beside the worktree). What it decides, with lines:

| question | answer | where |
| --- | --- | --- |
| Does the kit handle right-click on a tab? | No. The only handler on a `Tab` is `on_click`; the base element stops **left** mouse-down only. So a right-click handler inside the panel-supplied `title()` element fires. | `crates/component/src/tab/tab.rs:871-873`, `crates/base/src/tabs.rs:172` |
| Can a panel add rows to the `...` menu? | Yes, and only there — its rows are prepended above the kit's own separator. | `crates/component/src/dock/tab_panel.rs:333-337` |
| Can a panel remove or rename the `...` menu's zoom row? | **No.** The row is appended by the kit after the panel's rows; `zoom_control()` only enables or disables it. | `crates/component/src/dock/tab_panel.rs:338-345` |
| Can the app override the row's wording? | **No.** It is `t!("Dock.Zoom In")` from a locale compiled into the kit crate; `rust_i18n` stores translations per crate, so a consumer's own `i18n!` is a different store. | `crates/component/locales/ui.yml:163-164`, `crates/component/src/lib.rs:123` |
| Is the leftmost-tab clipping upstream? | **No — it was ours.** The kit's `Tab` is content-sized and `flex_shrink_0` with no `min_w`, so OneTerm's `w_full()` on the title row was a percentage against an indefinite parent: it contributed nothing to intrinsic sizing and every tab collapsed to its own `min_w(px(100.))`, label length irrelevant. | `crates/component/src/tab/tab.rs:715-718,794-802`; `crates/terminal-view/src/panel/tab_title.rs` |
| Does the strip scroll or show chevrons? | It scrolls, but only when the active tab changes; the kit's own `TabBar::menu` overflow list exists but the dock never enables it, and it would label every dock tab `Dock.Unnamed`. | `crates/component/src/dock/tab_panel.rs:462-466`, `crates/component/src/tab/tab_bar.rs:521-552` |
| Can a OneTerm popup switch tabs? | Yes — `TabGroup::select_tab(ix, ...)`, reached through the `WeakEntity<TabGroup>` `TerminalPanel` already stores. | `crates/base/src/dock/tab_group.rs:232` |

`crates/workspace/src/layout/workspace/dock_skin.rs` was read and **not changed**: its only
real override is suppressing the right dock's outer tab bar, and the centre terminal strip
falls straight through to the kit. Nothing this packet needed was reachable there, so the seam
the packet expected to use turned out to be the wrong one — the tab content and
`Panel::dropdown_menu` were the right ones.

Changed:

- `crates/terminal-view/src/panel/tab_title.rs` — the width fix, the context menu and its rows,
  `CloseScope` / `tabs_to_close`, `sibling_tabs` / `tabs_of`, `close_tabs` with its
  confirmation, `tab_list_menu`, a per-panel element id for the title row, and four tests.
- `crates/terminal-view/src/panel/terminal_panel.rs` — `Panel::dropdown_menu`.
- `crates/terminal-view/src/panel/mod.rs` — `trim_path_title` re-export (shared with `US-0117`).
- `docs/gui-layout.md` §Panel registration and presentation, including the recorded limits.

Checks:

- `cargo test -p oneterm-terminal-view --lib` — 351 passed, 0 failed, including
  `close_others_keeps_the_right_clicked_tab_and_closes_right_to_left`,
  `close_to_the_right_stops_at_the_right_clicked_tab` and
  `a_target_outside_the_list_closes_nothing`. They pin the one thing that is pure here: the
  order is **descending**, so a removal never shifts a position still to come, and a target
  outside the list closes nothing. **The menu rendering and the right-click routing are not
  unit-testable; the captures below are their proof.**
- `pwsh scripts/ci-local.ps1` — see below.

GUI walk:

- `evidence/US-0116-59-tab-context-menu.png` — right-clicking a tab opens Rename... / Duplicate
  / (separator) / Close / Close Others / Close to the Right. The right-clicked tab was
  "Command Prompt"; the active tab was "PowerShell 7".
- `evidence/US-0116-58-tab-rename-dialog.png` — Rename from that menu opens the dialog
  prefilled with **"Command Prompt"**, the tab that was right-clicked, not the active one. This
  is the frame that proves the rows carry the right-clicked tab's identity.
- `evidence/US-0116-20-tabbar-more-menu.png` — the `...` menu: "Terminal Tabs" and **nine**
  rows, the active one checked, then the kit's separator and its "Zoom In Shift+Esc".
- `evidence/US-0116-20b-tab-list-activates-a-hidden-tab.png` — clicking the first row activates
  tab 1, which had scrolled off the left; the strip scrolls back to it.
- `evidence/US-0116-19-many-tabs.png` — **nine** tabs at 1400 px, more than the strip holds:
  the leftmost visible tab reads **"PowerShell"** in full. **No bare `x`.**
- `evidence/US-0116-51-large-1900.png` — widened to 1900 px: all nine tabs visible, every one
  fully labelled, free space to the right of the strip and nothing clipped.

Tab count, corrected in the rework round (`F-116.1`): the walk opened **nine** tabs, not ten.
The before scene had ten because one of them was SSH (`research/before/51-large-1900.png`:
nine "Terminal" tabs plus `dev@127.0.0.1:22...`), and no host is reachable here. Nine still
overflows the 1400 px strip, so every claim the frames support is unaffected.

**Acceptance rows, with the behaviour each shipped with** (the packet asked for these to be
listed rather than left implicit):

| row | behaviour |
| --- | --- |
| Rename... | Opens the existing rename dialog for the right-clicked tab. The double-click route stays and opens the same dialog. |
| Duplicate | `duplicate_session` on that tab's active Space — the same path the terminal's own Duplicate Session menu uses, so an SSH tab still goes through its auth dialog. |
| Close | `close_tab` on the right-clicked tab. No confirmation, exactly like the `x` and middle-click. |
| Close Others | Every other terminal tab, closed right to left. Disabled when there is no other tab. Confirms first. |
| Close to the Right | Every tab after it, right to left. Disabled on the last tab. Confirms first. |

Destructive rows: the two bulk rows confirm when they would close **more than one** tab
("Close N terminal tabs? Their sessions end.", danger-styled OK), reusing the alert-dialog
pattern the SFTP delete already uses. One tab closes without a prompt, because the `x`,
middle-click and Close are all unconfirmed and a prompt for a single tab only there would be
inconsistent.

A double-lease panic was found and fixed during the walk: `Panel::dropdown_menu` hands over a
borrowed `&mut self`, so the tab list must not read the active panel back through its entity.
`tabs_of` takes the already-borrowed panel; `sibling_tabs` is the wrapper for callers that hold
nothing. The crash frame is not kept as evidence — the fixed build is what the captures show.

### Rework round (after independent verification)

- **`F-116.1`** — the two sentences that said "ten" now say nine, and the leftmost tab in
  `19-many-tabs.png` is named correctly ("PowerShell", not "PowerShell 7"). This was the only
  place a frame contradicted the text.
- **`F-116.2`** — the in-code citation at `tab_title.rs` is now `tab_panel.rs:333-337`, the
  range that carries the full sense (rows *above* the kit's separator), matching the table above.
- **`F-116.4`** — `close_tabs`' doc comment said the confirmation fires for more than one
  *running shell*; the code counts **tabs**, and so does the dialog's own wording. The comment
  now says tabs and notes that a tab whose shell already exited counts the same.
- `F-116.3` (the in-code `tab.rs:715-718,794-800` range against the table's `794-802`) is left
  as it is: both contain the load-bearing `:800 .flex_shrink_0()` and the in-code range is the
  tighter one, which the verifier also concluded.

### Gaps

**`F11` splits in two, and the verdict is split with it:**

- *Leftmost tab clipped to a bare `x`* — **fixed, OneTerm-side.** It was never upstream; see the
  reference table. The title row is now sized by its label between a 100 px floor and a 220 px
  ceiling.
- *No overflow chevrons, no tab list* — **the tab list is delivered**, in the `...` menu.
  **Chevrons are not**: the strip's scroll behaviour belongs to `TabBar`
  (`crates/component/src/tab/tab_bar.rs:499-519`) and it scrolls into view only when the active
  tab changes (`crates/component/src/dock/tab_panel.rs:462-466`). Nothing on the OneTerm side
  can add a chevron to it. Not raised upstream from here — **this session opened no upstream
  issue**, and that follow-up is still owed.

**`F13` also splits:**

- *The `...` menu held exactly one item* — **fixed.** It now opens with the tab list.
- *That item duplicates the zoom button beside it* — **not fixable from this side.** The kit
  appends its own zoom row after the panel's rows and `zoom_control()` only enables or disables
  it (`crates/component/src/dock/tab_panel.rs:338-345`). `PanelControl::Toolbar` would grey the
  row out rather than remove it, which trades a redundant control for a confusing one, so the
  row was left working.
- *"Zoom In" reads as font zoom* — **not fixable from this side.** The string is
  `t!("Dock.Zoom In")` from `crates/component/locales/ui.yml:163-164`, compiled into the kit
  crate; `rust_i18n` keys its store per crate, so the application cannot override it.
  `docs/PROJECT.md` forbids patching `gpui-component`. Recorded in `docs/gui-layout.md` so the
  next reader does not re-derive it.

Other gaps:

- **No `DEC` was raised.** The packet said a whole class of unreachable tab behaviour would
  justify one. What is unreachable is narrow — two properties of one upstream menu row and the
  strip's scroll affordance — and it is now recorded in `docs/gui-layout.md` beside the
  behaviour it constrains, where the next reader will be. A `DEC` would scatter it.
- **`PopupMenuItem::ElementItem` accessibility (`US-0110`'s finding) was not re-hit**: every row
  this packet adds is a plain `PopupMenuItem::new`, not an element item, so it inherits whatever
  `aria_label` the kit gives plain rows. Not separately checked.
- **"Close Others" was not walked**, only unit-tested and shipped with a confirmation; closing
  nine live shells inside the walk would have ended the session being captured. The
  confirmation dialog itself is therefore **uncaptured**.
- **Right-click does not activate the tab** before showing the menu, which is a deliberate
  change from the old behaviour (`F12`: "right-click just activates it"). Every row carries the
  right-clicked tab's identity, so activation buys nothing but a re-render racing the popup.
  Left-click activation is untouched.
- The 220 px ceiling is a judgement, not a measurement: it holds "Command Prompt" and a long
  `user@host: ~/path` comfortably and elides beyond that. No frame exercises a title long
  enough to elide.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
