# Work: Tabs have a context menu and the strip has a tab list

ID: US-0116
Intake: IN-0042
Created: 2026-09-17

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

- [ ] In scope:
  - `crates/terminal-view/src/panel/tab_title.rs` — the tab content OneTerm itself renders,
    which is where a right-click handler can live without touching the kit.
  - `crates/workspace/src/layout/workspace/dock_skin.rs` — `OneTermDockSkin`, the seam the
    project already uses to customise dock and tab rendering.
  - The `…` menu's contents: the redundant "Zoom In" row, its wording, and a tab list.
  - Whichever of Rename / Duplicate / Close / Close Others / Close to the Right are reachable
    from this side.
  - A reference read of `reference/gpui-kit/crates/component/src/dock/` before any code, to
    establish what `TabGroup` owns and what it delegates.
- [ ] Out of scope:
  - Patching or vendoring `gpui-component`. It comes from crates.io with no `[patch]` section
    and `docs/PROJECT.md` forbids modifying it.
  - Tab drag-and-drop, tab pinning, tab colours.
  - Tab labels themselves (`US-0114`), which this packet depends on for a Rename row to have
    something meaningful to edit.
  - The `+` menu (`US-0114`) and the zoom control itself, which works.

## Acceptance

- [ ] Right-clicking a tab opens a menu. Every row it offers works on the tab that was
      right-clicked, not on the active tab.
- [ ] The menu includes Close at minimum, and each further row the packet ships is listed here
      before implementation with the behaviour it must have. Rows that could not be delivered
      are recorded in Gaps with the reason, not silently dropped.
- [ ] Rename is reachable from the menu, not only from an unadvertised double-click. If the
      double-click stays, both routes open the same dialog.
- [ ] With ten tabs open, there is a discoverable way to reach a tab that does not fit the
      strip, and using it activates that tab.
- [ ] The `…` menu no longer duplicates the zoom control beside it, and any zoom wording it
      keeps says what it does (zoom the panel, not the font).
- [ ] The leftmost tab is never rendered as a bare close button with no label. If this cannot
      be fixed from the OneTerm side, the frame showing it and the upstream reason are in
      Gaps, with the follow-up raised.
- [ ] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

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

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

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

- [ ] Reference read first. Write down, with file and line, what `TabGroup` owns. This decides
      the rest of the packet and must happen before any code.
- [ ] Ship the reachable half: the tab context menu and the tab list.
- [ ] Fix the `…` menu's redundancy and wording.
- [ ] Attempt the clipping; if it is upstream, record it with evidence and raise the follow-up.
- [ ] Update `docs/gui-layout.md`, including any recorded limit.
- [ ] Re-capture the scenes.

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
   - `19-many-tabs.png` — ten tabs; the after frame must show the tab list reachable and the
     leftmost tab identifiable (or the frame that documents the limit).
   - `51-large-1900.png` — the widened window; the clipping either gone or recorded.
   - `20-tabbar-more-menu.png` — the `…` menu, no longer duplicating the zoom control.
   - `58-tab-rename-dialog.png` — rename, now reached from the menu.
   The walkthrough could not deliver a double-click (`WM_LBUTTONDBLCLK` did not register), so
   if rename still has a double-click route it stays unexercised; a menu route is exercisable
   and is the one to capture.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
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

After implementation, record commands, results, and anything skipped, unavailable, partial, or failing.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
