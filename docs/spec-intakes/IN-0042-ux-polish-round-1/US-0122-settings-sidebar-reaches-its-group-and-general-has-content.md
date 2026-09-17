# Work: Settings sidebar items reach their group and General has content

ID: US-0122
Intake: IN-0042
Created: 2026-09-17

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

### The reference read, written down before any code

`F14` is **two** defects, not one, and the documented quirk is only the second of them.

**1. The deferred scroll under-shoots because the groups below the fold have no measured
height.** This is what the walkthrough hit.

- `reference/gpui-kit/crates/component/src/setting/settings.rs:222-230` — a sidebar sub-item
  click sets `state.selected_index` and `state.deferred_scroll_group_ix = Some(group_ix)`
  (the assignment is line 228).
- `reference/gpui-kit/crates/component/src/setting/page.rs:139-143` — the page renders its
  groups into a gpui `list` whose `ListState` is created with
  `ListState::new(groups_count, ListAlignment::Top, px(100.))` — 100 px of overdraw.
- `reference/gpui-kit/crates/component/src/setting/page.rs:152-158` — the deferred index is
  consumed **once** (`deferred_scroll_group_ix = None`, line 155) and handed to
  `ListState::scroll_to_reveal_item(ix)` (line 157). There is no second attempt on a later
  frame, so the scroll cannot converge as the list measures more items.
- `reference/zed/crates/gpui/src/elements/list.rs:664-694` — `scroll_to_reveal_item` has two
  branches. The backward one (`ix <= scroll_top.item_ix`) sets `item_ix = ix, offset = 0` and
  is **exact**, needing no measurement. The forward one seeks `state.items` for the summed
  height above `ix` and derives `goal_top` from it.
- `reference/zed/crates/gpui/src/elements/list.rs:245-297` — an item the list has not laid out
  is `ListItem::Unmeasured` and contributes **nothing** to `ListItemSummary::height`
  (the summary is declared at line 292).

So on a page opened at the top, every group below the viewport plus 100 px is height 0,
`bottom` collapses to roughly the height of the two or three measured groups, `goal_top`
clamps to ~0, and the list barely moves. That is exactly the measurement in `F14`:
*"clicking 'Completion' (10th) landed on Font (2nd); clicking 'Logging' (5th) landed
mid-Font."* It also explains why scrolling **up** the sidebar feels fine — that is the exact
branch.

**2. The index misalignment `docs/gui-layout.md` already records.**
`settings.rs:209-216` enumerates `page.groups.iter().filter(|g| g.title.is_some())`, so the
sidebar's `group_ix` counts only titled groups, while `page.rs:131-137` indexes every group
that matches the search query. `F14`'s own note is right: every Terminal group is titled, so
this is **not** what the walkthrough saw. The documented paragraph is not wrong, it is
incomplete — it describes the quirk that bites when a page mixes titled and untitled groups
and says nothing about the scroll itself.

### Why neither is reachable from the OneTerm side

- The `ListState` is private element state: `page.rs:139-145` creates it through
  `window.use_keyed_state("list-state:{page_ix}")`, which resolves against the kit's own
  `element_id_stack` (`reference/zed/crates/gpui/src/window.rs:3466-3487`). Reaching the same
  entity would mean reproducing the kit's entire element-id path from OneTerm's render, during
  paint, and re-deriving it on every kit upgrade.
- `SettingsState`, its `deferred_scroll_group_ix` field and `SettingPage::render` are all
  `pub(super)` (`settings.rs:248-253`, `page.rs:121-128`), so the scroll cannot be requested,
  repeated or replaced from outside the crate.
- The one public lever, `Settings::default_selected_index` (`settings.rs:101-104`), writes
  `selected_index` only; `deferred_scroll_group_ix` is initialised to `None`
  (`settings.rs:366-377`) and is never set from it, so it cannot pre-scroll either.
- `docs/PROJECT.md` forbids patching or vendoring `gpui-component`, and there is no `[patch]`
  section.

The OneTerm-side wrapper `P16` names — rendering the page body inside a container OneTerm
scrolls itself — was scoped and rejected. It requires collapsing a page into **one**
`SettingGroup` (otherwise the kit still owns the list), which switches off the kit's sidebar
sub-items for that page (`settings.rs:209`, gated on `page.groups.len() > 1`), its per-item
search filtering (`settings.rs:112-140`, which matches `SettingItem`s) and its page-level
`Reset All` (`page.rs:111-119`). That is the "stop and record the limit instead" line in this
packet's Risks, so it was not built.

### What this packet ships instead

Navigation is improved by making the pages navigable rather than by fixing the scroll:

- **General gets the content `P21` names** — Appearance's Mode and Color Theme, and the
  default shell — so the window opens on a page worth opening on.
- **The Appearance page is folded away**, its two groups now living on General. One fewer page
  row, and one fewer place to look for the theme.
- **Terminal drops from ten groups to nine** (Shell moves to General), so its sub-item list is
  shorter and its content is closer to fitting the fold. A page whose groups fit the viewport
  is measured in full, and then `scroll_to_reveal_item` is exact — so shortening pages makes
  the upstream defect bite less often even though it does not remove it.
- **The triple-naming is cut to two levels on every page**, not only on the two the
  walkthrough named.
- **The index-alignment rule becomes a test.** `panel::sidebar_group_to_scroll_index`
  reproduces the kit's two indexings side by side and the unit tests assert that OneTerm's
  "untitled groups last" ordering keeps them equal, and that any other ordering breaks them.
  The rule was prose in `docs/gui-layout.md`; now it fails a build.

### The chevron (`F15`)

Also upstream, and the affordance is half-honest rather than wrong:
`sidebar/menu.rs:312-333` renders the caret as its own `Button` whose handler calls
`cx.stop_propagation()` (line 327) and toggles the open state, so **clicking the chevron does
collapse the group**. What never closes is the row: `settings.rs:193` passes
`click_to_open(true)`, and `menu.rs:344-350` reads that as "force open", never toggle. OneTerm
cannot change that argument, so the row count is reduced instead (one page and one group
fewer), which is what keeps About and the theme settings above the fold in the default window.

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
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
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

### Commands

| Command | Result |
| --- | --- |
| `cargo test -p oneterm-settings-ui` | 46 passed |
| `cargo clippy -p oneterm-settings-ui --all-targets -- -D warnings` | clean |
| `cargo test --workspace` | 2069 passed, 12 ignored |
| `pwsh scripts/ci-local.ps1` | see the closing note below |

Focused tests added: `panel::tests` —
`an_all_titled_page_scrolls_to_the_group_the_sidebar_names` (a nine-group page, every index),
`untitled_groups_kept_last_leave_every_sidebar_item_aligned` (OneTerm's ordering rule) and
`an_untitled_group_before_a_titled_one_desynchronises_the_two_indexes` (the failure mode the
rule exists to prevent). `about::tests::identity_group_leads_the_about_page` was reviewed, not
merely kept green: after `US-0121` the About page is `[Identity, Links, Updates]`, all titled,
so the alignment the test guards still holds for the same reason it did before.

### Acceptance, walked

1016x708, `gui.ps1`, own pid only.

| Acceptance | Frame | Result |
| --- | --- | --- |
| Terminal: clicking each sub-item lands on that group — the 10th | `evidence/US-0122-30-settings-completion.png` | **NOT MET.** Clicking "Completion" (now 9th of 9) left the page on Font/Cursor. The upstream under-shoot, reproduced on the fixed build. |
| ...and the 5th | `evidence/US-0122-30b-settings-sidebar-logging.png` | **NOT MET.** "Logging" (now 4th) landed on Cursor/Layout — two groups short. |
| Key Bindings: "Edit Menu" lands on the Edit Menu group | `evidence/US-0122-38-keybindings-edit-menu.png` | **MET.** The Edit Menu heading and its first five rows are on screen. It works here because `US-0121` halved the App Menu group's height, so both groups now fall inside the measured window. |
| Gaps record the upstream item with file and line | §"The reference read" above, and Gaps below | done, with a reproduction |
| The sidebar fits 708 px, or its chevrons collapse | `evidence/US-0122-31-sidebar-collapsed-chevron.png` | **MET, with a caveat.** The caret does collapse a group: the frame is taken after clicking Key Bindings' chevron, which folded its six sub-items away. The *row* still never closes (upstream `click_to_open(true)`). |
| Appearance and About reachable without the sidebar scrolling past the fold | same frame, and `evidence/US-0122-26-settings-general.png` | **MET.** Six page rows instead of seven; with Terminal fully expanded (nine sub-items) About sits at y≈572 of 708. Opening a *second* group at the same time still pushes About below the fold — see Gaps. |
| General has content worth a landing page | `evidence/US-0122-26-settings-general.png` | **MET.** Theme (Mode, Color Theme), Interface (UI Font Size), Shell (Shell, Custom Program). |
| No setting lost its effect or became unreachable | same frame + `evidence/US-0122-28-settings-terminal.png` | **MET.** The groups moved, the builders did not: `general::page` calls `appearance::theme_group` and `terminal::shell_group`, which are the same functions the Appearance page and the Terminal page called. Terminal shows its remaining nine groups. |
| No page states the same name three times | `evidence/US-0122-28-settings-terminal.png`, `-26-`, `evidence/US-0121-35b-settings-network.png` | **MET.** Swept across every page, not only General and Terminal: Font, Cursor, Layout, Scroll, Bell, Security, Completion, Shell, Interface, Theme, the SSH edit-limit group and the update status row. |

### Docs reconciled

- `docs/gui-layout.md` §Settings window — rewritten. The page list is six pages with General as
  the landing page and no Appearance page; the two-level naming rule is stated; and the
  untitled-group paragraph is replaced by §"Sidebar navigation, and the two things upstream
  owns", which keeps the index rule (now with a test behind it) and adds the scroll defect with
  the `reference/` files and lines that show it, plus the chevron's true behaviour.
- `docs/PROJECT.md` — read for the "no patching `gpui-component`" rule, which bounded the
  packet. **No change.**
- `reference/gpui-kit/crates/component/src/setting/`, `.../sidebar/menu.rs` and
  `reference/zed/crates/gpui/src/elements/list.rs` — read only, cited by line. **Not changed,
  and must not be.**
- `docs/gui-layout.md` §Panel registration — read for consistency with `US-0116`, which meets
  the same upstream boundary from the tab strip's side. **No change:** both conclusions are the
  same shape (ship what is reachable, cite the private state).

### Gaps

- **`F14`'s scroll is not fixed and cannot be fixed from this side.** The mechanism, the
  citations and the rejected OneTerm-side option are in §"The reference read" above; the
  reproduction is `evidence/US-0122-30-settings-completion.png`. The one-line statement of the
  upstream item: *`gpui_component::setting::SettingPage::render` asks a gpui `list` to reveal a
  group that the list has not measured, once, and `ListState::scroll_to_reveal_item`'s forward
  branch treats an unmeasured item as zero-height, so the jump lands short in proportion to how
  far down the page the group is.* A fix upstream is small — retry the deferred scroll while
  the target is still not visible, or scroll to the item's top with `ListState::scroll_to`
  instead of revealing it, which needs no measurement — but it is a change to a published
  crate, and `docs/PROJECT.md` forbids patching it here. **Follow-up: raise it with GPUI Kit,
  quoting `setting/page.rs:152-158`.**
- **The under-shoot converges with repeated clicks**, which is the diagnosis showing itself:
  `evidence/US-0122-30c-completion-second-click.png` is a second click on "Completion" after
  the first had caused more groups to be measured, and it lands further down. A user can reach
  the group by clicking the same sub-item repeatedly. That is not a fix and is not documented
  as a workaround; it is recorded because it confirms the cause.
- **The chevron is only half honest, and that half is upstream.** The caret collapses
  (`sidebar/menu.rs:312-333`); clicking the row forces the group open and never closes it,
  because `Settings` hard-codes `click_to_open(true)` (`settings.rs:193`) and exposes no way to
  pass `click_to_toggle` instead. `F15`'s complaint is exactly that mismatch. This packet
  reduced the row count (one page and one group fewer) rather than fixing the affordance.
- **Two groups open at once still overflows the sidebar.** Key Bindings (6 sub-items) plus
  Terminal (9) plus six page rows is 21 rows, and Network and About fall below 708 px until one
  is collapsed with its caret. Fully fixing this needs the row toggle above.
- **`US-0122` did not attempt the page wrapper.** It was scoped and rejected for the three
  capabilities it would cost (sub-items, per-item search, page-level Reset All). If a future
  packet decides those are worth losing on one page, that is a `DEC`, not a retry of this one.
- **Not re-captured: `31-settings-appearance.png`.** The Appearance page no longer exists; its
  contents are the first group of `evidence/US-0122-26-settings-general.png`. The before/after
  report should pair scene 31 with that frame rather than with a missing page.


## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
