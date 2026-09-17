# Work: Settings sidebar items reach their group and General has content

ID: US-0122
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

The Settings window can be navigated by its own sidebar. Clicking a sub-item lands on that
group; the sidebar does not grow past the window; and the landing page is not one field
restating its own name three times.

## Findings and proposals covered

`P16` (effort M–L) — *"Fix Settings sidebar sub-item navigation so a sub-item scrolls to its
group. Upstream `gpui_component::setting::Settings` owns the scroll and `PROJECT.md` forbids
patching it, so this is either an upstream fix or a OneTerm-side page wrapper."*

`P21` (effort M) — *"Give Settings→General real content — pull Appearance's Mode/Theme and the
shell default up to it, or fold General away — and cut the header/title/description
triple-naming to two levels across every page."*

Addresses `F14` (**high**), `F16` (medium) and `F15` (low), quoted from
`research/ux-walkthrough-2026-09-16.md`:

> | F14 | Settings sidebar | Clicking a sub-item does **not** scroll to its group. On Terminal,
> clicking "Completion" (10th) landed on Font (2nd); clicking "Logging" (5th) landed mid-Font.
> On Key Bindings, "Edit Menu" left the page on "App Menu". | Navigation: a 10-group page is
> effectively unnavigable by its own sidebar. `gui-layout.md` documents a related upstream
> index quirk, but every Terminal group *is* titled, so the documented workaround does not
> explain it. | **high** | 30, 30b, 38 |

> | F15 | Settings sidebar | Section headers show a chevron but clicking one navigates instead
> of collapsing, so the sidebar grows to ~25 rows and About/Appearance fall below the fold in
> the default 708 px window. | Density / affordance mismatch. | low | 26, 38, 31 |

> | F16 | Settings → General | The whole page is one field, "UI Font Size", with the section
> header, item title and description each restating it ("Interface / UI font size." → "UI Font
> Size" → "Interface font size in px."). ~85 % of the page is empty. | First impression: the
> settings landing page reads as unfinished. The same triple-naming runs through Terminal
> ("Shell / Shell for new local terminals." → "Shell / Choose shell kind."). | medium | 26, 28 |

## Scope

- [ ] In scope:
  - `crates/settings-ui/src/panel.rs` — the page list and whatever wrapper the navigation fix
    needs.
  - `crates/settings-ui/src/terminal/mod.rs` — the ten-group page `F14` measured.
  - `crates/settings-ui/src/general.rs` and `crates/settings-ui/src/appearance.rs` — General's
    contents, or its removal.
  - Every page's header/title/description text, cut to two levels.
  - The sidebar's chevron affordance: either it collapses or it does not look like it should.
  - A reference read of `reference/gpui-kit/crates/component/src/` (the `Settings` widget)
    before any code, to establish what the kit owns.
- [ ] Out of scope:
  - Patching or vendoring `gpui-component`. It comes from crates.io with no `[patch]` section
    and `docs/PROJECT.md` forbids modifying it.
  - Which settings exist, what they do, and how they persist. This packet moves and renames
    what is shown; it does not add or remove a setting's effect.
  - The About page's contents and the theme dropdown (`US-0121`).
  - The key-binding rows (`US-0121`) and the default bindings (`US-0123`).

## Acceptance

- [ ] On the Terminal page, clicking each sidebar sub-item lands on that group, with the
      group's title visible. Walked for the tenth item ("Completion") and the fifth
      ("Logging") specifically — the two `F14` measured.
- [ ] On the Key Bindings page, clicking "Edit Menu" lands on the Edit Menu group.
- [ ] If the navigation cannot be fixed from the OneTerm side, the packet ships whatever is
      reachable, and Gaps records: the upstream item, the `reference/gpui-kit/` file and line
      that shows why, what the OneTerm-side attempt cost, and the follow-up raised upstream.
      "Upstream owns it" alone does not close this packet.
- [ ] The sidebar fits the default 708 px window, or its chevrons actually collapse — the
      affordance and the behaviour agree either way.
- [ ] Appearance and About are reachable in the default window without the sidebar scrolling
      past the fold.
- [ ] Settings → General either has content worth a landing page or no longer exists as one.
      Whichever, no setting loses its effect and none becomes unreachable.
- [ ] No page states the same name three times. Every page is checked, not just General and
      Terminal.
- [ ] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

## Documentation

### Owning Docs Reviewed

- `docs/gui-layout.md` §Settings window — the whole section, and in particular: *"The upstream
  sidebar numbers only titled groups, while page scrolling indexes every group. OneTerm
  therefore keeps untitled groups after all titled groups on a page; the About-page ordering
  regression protects that alignment without adding a heading to its identity block."*
  `P16` says this replaces that paragraph. **Update required** — but read it first: it
  describes a *known index misalignment* between the sidebar's numbering and the scroll's
  indexing, which is very likely the same mechanism `F14` is seeing. `F14`'s own note that
  "every Terminal group *is* titled, so the documented workaround does not explain it" is the
  clue that the paragraph is incomplete rather than wrong.
- `docs/PROJECT.md` — the rule that `gpui-component` is consumed unmodified. This bounds the
  packet. **No change.**
- `reference/gpui-kit/crates/component/src/setting/` (or wherever the `Settings` widget lives)
  — what owns the sidebar index and the page scroll. Reference material; cite file and line in
  Evidence for every "upstream owns this" claim. **No change** (and it must not be changed).
- `crates/settings-ui/src/panel.rs:85-93` — the page list, which `US-0121` may have changed.
  Land that packet first so this one navigates the finished list.
- `docs/PROJECT.md` and `docs/gui-layout.md` §Panel registration — read because `US-0116`
  faces the same upstream boundary from the other side; keep the two packets' conclusions
  consistent. **No change.**

### Documentation Action

Update required: `docs/gui-layout.md` §Settings window — the untitled-group paragraph is
replaced by what is actually true after this packet, including any recorded upstream limit.

Reason: the current paragraph documents a workaround for an index quirk that does not explain
the behaviour a user hits, which is worse than documenting the limit plainly.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

## Context

- **This packet may not be closed by saying "upstream".** `IN-0042`'s boundary question is
  explicit. Ship the reachable part and record the limit precisely.
- **Start from the documented quirk, not from scratch.** `gui-layout.md` already says the
  sidebar numbers only titled groups while the page indexes every group, and that OneTerm
  orders untitled groups last to keep the two aligned. `F14` shows the alignment failing on a
  page where that ordering rule should hold. Either the rule is not being followed on the
  Terminal page, or the mechanism is different from what the paragraph describes. Establishing
  which is the first hour of this packet and may turn a "M–L upstream" item into a small fix.
- **The OneTerm-side option.** `P16` names it: a page wrapper that owns its own scroll. That
  means OneTerm renders the page content inside a container it controls and maps sidebar
  selection to a scroll offset itself, instead of handing the whole page to the kit. It is a
  real option and it is where the reachable half probably lives — but it is also where "M–L"
  comes from, so scope it honestly before starting.
- **General.** Two answers, both acceptable: give it content (the Mode/Theme and shell
  defaults that new users look for first), or remove it so the window opens on a page with
  something on it. Removing a page is the smaller change and the lazier one; giving it content
  is better for a first-time user, who is exactly who this round is about. Decide and record.
  Either way no setting may lose its effect.
- **The triple-naming** is a text edit across every page and is the cheapest part of the
  packet. Do it, and check every page rather than the two the walkthrough named.
- **The chevron.** `F15` is an affordance mismatch: a control that looks collapsible and is
  not. Collapsing is the better fix, but a chevron that is simply removed is honest too, and
  smaller. Decide against the ~25-row sidebar — if collapsing is what keeps About and
  Appearance above the fold, collapse.
- `research/before/26-settings-general.png`, `28-settings-terminal.png` (`F16`),
  `30-settings-completion.png`, `30b-settings-sidebar-logging.png`, `38-keybindings-edit-menu.png`
  (`F14`) and `31-settings-appearance.png` (`F15`) are the before pictures.

## Plan

- [ ] Land `US-0121` first so the page list is final.
- [ ] Reference read; establish whether `F14` is the documented index quirk or something else.
      Write the finding down before touching code.
- [ ] Fix or wrap; record the limit if it is not reachable.
- [ ] Decide General's fate; implement.
- [ ] Chevron decision; implement.
- [ ] Triple-naming sweep across every page.
- [ ] Rewrite the `docs/gui-layout.md` §Settings window paragraph.
- [ ] Re-capture the scenes.

## Decisions

Possibly one: if the conclusion is that OneTerm must own the Settings page scroll rather than
delegating it to the kit, that is an architectural constraint future work inherits and
deserves a `DEC`. Raise it rather than leaving it in a packet.

## Verification Plan

1. **Focused:** `cargo test -p oneterm-settings-ui` over whatever index mapping the fix
   introduces — sidebar item *n* maps to group *n* on a page with a known group list, including
   a page mixing titled and untitled groups. That mapping is pure data and is where a test can
   bite; the scroll itself is not queryable.
2. **Unit:** `cargo test -p oneterm-settings-ui`, including the existing About-page ordering
   regression test, reviewed rather than merely made green.
3. **Integration:** `cargo test --workspace`.
4. **Platform:** `pwsh scripts/ci-local.ps1`.
5. **E2E (GUI walk, re-capture these scenes):**
   - `30-settings-completion.png` — clicking "Completion", the tenth Terminal sub-item. The
     after frame must show the Completion group.
   - `30b-settings-sidebar-logging.png` — clicking "Logging", the fifth.
   - `38-keybindings-edit-menu.png` — clicking "Edit Menu".
   - `26-settings-general.png` — General, with content or gone.
   - `28-settings-terminal.png` — the triple-naming cut to two levels.
   - `31-settings-appearance.png` — Appearance reachable without the sidebar scrolling.
   The window is 708 px tall in the walkthrough's frames; use the same height so the fold is
   comparable.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Risks

- **Rebuilding the Settings widget.** The slope from "own the scroll" to "own the page" to
  "own the Settings window" is short and expensive. If the wrapper starts reimplementing the
  kit's widget, stop and record the limit instead.
- **Losing a setting while reorganising General.** Moving or removing a page must not drop a
  field. Enumerate the settings before and after and compare; a setting that exists but is
  unreachable is worse than an empty page.
- **Breaking the About-page ordering guard.** The documented alignment has a regression test.
  Changing the page list or the group ordering will touch it.
- **Text edits changing meaning.** Cutting three names to two is easy to do badly: the level
  that survives must be the one that says what the setting *does*, not the one that repeats the
  section title.
- **Declaring `F14` fixed on one page.** It was measured on two pages with different group
  counts. Walk both.

## Evidence and Gaps

After implementation, record commands, results, and anything skipped, unavailable, partial, or failing.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
