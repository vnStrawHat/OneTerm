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

- [x] On the Terminal page, clicking each sidebar sub-item lands on that group, with the
      group's title visible. Walked for the tenth item ("Completion") and the fifth
      ("Logging") specifically — the two `F14` measured. *(There is no longer a tenth or a
      fifth item to click: the fix is that Terminal's nine groups are five pages, so Completion
      and Logging are pages of their own and are reached without any scroll at all. Every
      sub-item of every page was walked instead — see the Evidence table.)*
- [x] On the Key Bindings page, clicking "Edit Menu" lands on the Edit Menu group.
- [ ] If the navigation cannot be fixed from the OneTerm side, the packet ships whatever is
      reachable, and Gaps records: the upstream item, the `reference/gpui-kit/` file and line
      that shows why, what the OneTerm-side attempt cost, and the follow-up raised upstream.
      "Upstream owns it" alone does not close this packet. *(not applicable: it was fixed from the OneTerm side. The upstream limit is recorded anyway, with the report text in Handoff.)*
- [ ] The sidebar fits the default 708 px window, or its chevrons actually collapse — the
      affordance and the behaviour agree either way. *(partial: fits collapsed and with one page open; overflows with several open — the row toggle is upstream. Frame: `evidence/US-0122-31-sidebar-all-expanded-overflows.png`.)*
- [ ] Appearance and About are reachable in the default window without the sidebar scrolling
      past the fold. *(partial: same condition as above.)*
- [x] Settings → General either has content worth a landing page or no longer exists as one.
      Whichever, no setting loses its effect and none becomes unreachable.
- [x] No page states the same name three times. Every page is checked, not just General and
      Terminal.
- [x] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

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
- [x] Platform proof
- [x] Verify command passed
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

> Reworked after independent verification (`evidence/settings-ui-wave1-verify.md`), which
> returned **FAIL**. Its MAJOR 2 was right: the cheap OneTerm-side fix — splitting a long page
> into several short **pages** — was named, priced at zero and never built, while the packet
> closed on the escape clause. It is built now, and the headline acceptance is met.

### Second rework, after the re-verification of `dab9cac4`

The re-verification (`evidence/settings-ui-wave1-verify.md` §"Re-verification") returned
**PASS with findings**: the split is real, the headline acceptance is met, every sub-item lands,
and the false build-guard claim is gone. Three of its findings were defects in the new work.

**`F-R2` (major) — `Reset All` reached across pages. Fixed.** The split created it: before it,
one page carried all thirty-seven rows and one `Reset All` meant what it looked like. After it,
the single registry-wide handler still sat on `BINDABLE_ACTIONS[0]`, which is an App Menu row
and therefore only ever on page 1 — so page 1 showed a destructive button while all of its own
rows were clean, one click silently reverted twenty-four rows the user could not see from
there, and pages 2 and 3 had no `Reset All` at all.

Each page now carries its own, scoped to the groups that page shows: `pages()` hands the first
group of each page its `reset_scope`, `binding_group` puts `on_reset` on that group's first row,
and `page_bindings_are_dirty` / `reset_page_key_bindings` work through `actions_reset_by`, the
pure decision function. Three tests:
`each_page_resets_exactly_the_actions_it_shows` (each page's set equals the ids it shows, no
page resets nothing, and the three pages **partition** the registry),
`a_pages_reset_all_does_not_reach_another_pages_bindings` (the regression by name: page 1 must
not contain `split_right`, `join_input_channel_a` or `sftp_open`, and the page that shows Split
Right is the one that resets it), and the existing coverage test. Checked by mutation: dropping
the `page_groups` filter — which is exactly the old behaviour — turns `52 passed` into
`50 passed; 2 failed`, and both failures are the new tests. Reverted.

Walked, on the reworked build:

| Step | Frame | Result |
| --- | --- | --- |
| Split Right rebound to `F7` on **Key Bindings: Terminal** | `evidence/US-0122-fr2-01-page2-dirty.png` | That page now shows its own `Reset All` (it had none before) and the row reads `F7` with "Default: ctrl-shift-right". |
| **Key Bindings** opened while page 2 is dirty | `evidence/US-0122-fr2-02-page1-clean-no-undo.png` | **No undo icon.** Page 1's rows are all clean and it no longer answers for another page's state. |
| Page 1's `Reset All` — pressed anyway, with page 2 still dirty | `evidence/US-0122-fr2-03-page1-reset-leaves-f7.png` | Split Right is **still `F7`**. |
| Page 2's `Reset All` | `evidence/US-0122-fr2-04-page2-reset-reverts-f7.png` | Split Right back to `Ctrl+Shift+Right`, and `ui_config.json` back to no `key_bindings` key. |

**`F-R3` (minor) — the coverage claim was true of one table, not both. Corrected.**
`KEY_BINDING_PAGES` really is checked against `BINDABLE_ACTIONS` itself. `TERMINAL_PAGES` has no
enumerable source to check against — each group is a function in its own module — so the claim
"both are asserted against their own group source" was wrong. The test is now
`no_terminal_group_is_placed_on_two_pages`, which asserts what it can (no duplicate placement,
and the walked composition of nine), and its comment states the real guard for coverage: a
builder left off the table has no caller and `cargo clippy -- -D warnings` fails it as dead
code, which is how the verifier's mutation C was caught. The `>1 group` property is enforced
only for `KEY_BINDING_PAGES`, where it is true; `no_terminal_page_carries_more_groups_than_have_been_walked`
now says only what holds for terminal pages — at least one group, at most three.

**`F-R4` (minor) — "Completion" was stated three times. Fixed.** Sidebar row, page header and
group heading all said it; Terminal Logging was one step from the same. Both are single-group
pages created by the split, after the naming sweep's reasoning was written. The group is now
untitled on both and the page carries the description instead, so the name is printed twice and
the prose survives: `evidence/US-0122-28d-settings-completion.png`,
`evidence/US-0122-28c-settings-terminal-logging.png`.
`a_single_group_page_describes_itself_instead_of_repeating_its_name` holds the rule for any
single-group terminal page added later.

**`F-R7` (minor, upstream) — search blanks the content pane.** Not caused here and not fixable
here: `Settings::render_active_page` indexes `selected_index.page_ix` into the **filtered** page
list (`settings.rs:145-163`), so a query that filters out the selected page renders the fallback
empty `div`, and clearing the query re-points the same integer at a different page. The split
raised the exposure from six pages to twelve, so it travels with the scroll defect: it is now
**upstream report 2 of 2** in Handoff, written out the same way.

**`F-R8` (trivial, process) — a code change shipped inside a docs commit.** True.
`SettingsPanel::render`'s notification layer (`panel.rs`, +13) landed in `dab9cac4`, whose
subject is `docs(harness): …`. No history is rewritten; the commit list below records it so the
change is findable by someone reading the log rather than the packet.

### Commits

| Commit | Contents |
| --- | --- |
| `8653b76c` | records: `US-0121` opened, decisions taken before code |
| `4796dcfa` | `US-0121` implementation + docs |
| `b85f9267` | records: `US-0122` opened, the reference read |
| `9673e0cf` | `US-0122` first implementation (General, the fold, the naming sweep) + docs |
| `433a9fbd` | records: `US-0123` opened, the old→new table |
| `62562385` | `US-0123` implementation + `DEC-0018` + docs |
| `9b1b2ca9` | records: all three closed, first evidence |
| `c19018a0` | first rework: the page split, and `US-0121`/`US-0123` verification fixes |
| `dab9cac4` | first rework records and evidence — **and one code change**: `panel.rs`'s notification layer (`F-R8`) |
| *(this one)* | second rework: per-page `Reset All` (`F-R2`), the notification layer's paint order (`F-R6`), and the four minors |


### Commands

| Command | Result |
| --- | --- |
| `cargo test -p oneterm-settings-ui` | `test result: ok. 52 passed; 0 failed` |
| `cargo clippy -p oneterm-settings-ui --all-targets -- -D warnings` | clean |
| `cargo test --workspace` | 2109 passed, 12 ignored |
| `pwsh scripts/ci-local.ps1` | **`ci-local: all checks passed.`** |

### What the split is

Two pages were longer than the window; both are now several pages that each fit it. Nothing the
kit gives a page is lost: sub-items appear on any page with more than one group
(`settings.rs:209`), search is per `SettingItem` and page-independent (`settings.rs:112-140`),
`Reset All` is per page (`page.rs:111-119`), and the Key Bindings "reset every binding" handler
stays on the first row of the registry so one `Reset All` still restores the whole table.

| Was | Is now | Groups |
| --- | --- | --- |
| Terminal (9 groups) | Terminal | Font, Cursor |
| | Terminal Display | Layout, Scroll, Bell |
| | Mouse & Clipboard | Mouse, Security |
| | Terminal Logging | Logging |
| | Completion | Completion |
| Key Bindings (6 groups) | Key Bindings | App Menu, Edit Menu |
| | Key Bindings: Terminal | Terminal Context Menu, Input Channel |
| | Key Bindings: Sessions | Session Tabs Context Menu, SFTP Context Menu |

Twelve pages in total. Both splits are tables — `terminal/mod.rs`'s `TERMINAL_PAGES` and
`key_bindings_ui.rs`'s `KEY_BINDING_PAGES` — and both are asserted against their own group
source, which is what the first pass's deleted helper could not do (see Gaps).

### Acceptance, walked

`target/fast-dev`, 1016x708 (the walkthrough's window size), `gui.ps1`, own pid only.
**Every sidebar sub-item on every page was clicked from a freshly opened page**, and the
deepest one on each page is framed.

| Page | Deepest sub-item clicked | Frame | Result |
| --- | --- | --- | --- |
| Terminal | Cursor (2 of 2) | `evidence/US-0122-30-terminal-cursor-subitem.png` | **MET.** Scrolled 113 px; the whole Cursor group is on screen, heading included. |
| Terminal Display | Bell (3 of 3) | `evidence/US-0122-30b-terminal-display-bell-subitem.png` | **MET.** Whole Bell group revealed. |
| Key Bindings | Edit Menu (2 of 2) | `evidence/US-0122-38-keybindings-edit-menu.png` | **MET.** Edit Menu heading and all five rows on screen. |
| Key Bindings: Terminal | Input Channel (2 of 2) | `evidence/US-0122-30c-kb-terminal-input-channel.png` | **MET.** All seven Input Channel rows on screen. |
| Key Bindings: Sessions | SFTP Context Menu (2 of 2) | `evidence/US-0122-30d-kb-sessions-sftp-subitem.png` | **MET.** All nine SFTP rows on screen. |
| Mouse & Clipboard | Security (2 of 2) | `evidence/US-0122-30e-mouse-clipboard-security.png` | **MET.** Page fits entirely; the click needs no scroll at all. |
| SSH | SFTP Edit Limit (3 of 3) | `evidence/US-0122-30f-ssh-edit-limit-subitem.png` | **MET.** Whole group revealed. |
| About | Updates (3 of 3) | `evidence/US-0122-30g-about-updates-subitem.png` | **MET.** Whole Updates group revealed, Check Now and status included. |
| General | Shell (3 of 3) | `evidence/US-0122-26-settings-general.png` | **MET.** All three groups fit; no scroll needed. |
| Terminal Logging, Completion, Network | — | `evidence/US-0122-28c-...`, `-28d-...`, `US-0121-35b-...` | One group each, so no sub-items; the page row itself is the navigation and page selection scrolls nothing, so it is exact by construction. |

The two scenes `F14` measured are the first two rows: the deepest item on the page that used to
be Terminal's tenth, and the one that used to be its fifth. Both land now. The before pictures
of the failure are `research/before/30-settings-completion.png` and
`30b-settings-sidebar-logging.png`; the first pass's own frames of the same failure on the
unsplit page have been removed from `evidence/` so the round does not carry two contradictory
answers to the same scene.

| Other acceptance | Frame | Result |
| --- | --- | --- |
| The sidebar fits 708 px | `evidence/US-0122-26-settings-general.png` | **MET** with everything collapsed: twelve page rows, General at y=104 to About at y=500, no scrollbar. |
| ...or its chevrons collapse | `evidence/US-0122-31-sidebar-all-expanded-overflows.png` | **PARTIAL, framed.** With all nine expandable pages open the sidebar is 34 rows, overflows and scrolls — "Scroll" is cut off at the bottom edge. The caret collapses a group (upstream `menu.rs:312-333`); the row never closes (upstream `click_to_open(true)`, `settings.rs:193`). This is the trade the split buys, shown rather than argued. |
| Appearance and About reachable without the sidebar scrolling | `evidence/US-0122-26-settings-general.png` | **MET** in the default state and in every single-page-open state walked. Not met with several pages open — same frame as above. Graded PARTIAL in the round's terms, per the verifier's MINOR 5. |
| General has content worth a landing page | `evidence/US-0122-26-settings-general.png` | **MET.** Theme, Interface, Shell. |
| No setting lost its effect or became unreachable | `-26-`, `-28-`, `-28b-`, `-28c-`, `-28d-`, `-30e-` | **MET.** Nine terminal groups across five pages, asserted by `the_terminal_groups_are_spread_over_pages_none_of_which_is_long`; every key-binding group placed exactly once, asserted by `key_binding_pages_cover_every_group_exactly_once` against `BINDABLE_ACTIONS` itself. The verifier counted 58 items on `main` against 59 here and attributed the one addition to `US-0121`'s Check for Updates. |
| No page states the same name three times | `-28-`, `-28b-`, `-30e-`, `US-0121-35b-` | **MET**, swept across every page. |

### Docs reconciled

- `docs/gui-layout.md` §Settings window — the page list is now a twelve-row table, with a
  paragraph stating that pages are short **because the sidebar depends on it**, what the budget
  is, and that adding a group to a full page breaks navigation below it.
- `docs/gui-layout.md` §"Sidebar navigation" — **the false claim is gone.** The paragraph used
  to say `panel.rs`'s tests "pin the rule so it fails a build". They did not. It now says
  plainly that no test can guard the titled/untitled ordering rule from this side, and why
  (`SettingPage::groups` and `SettingGroup::title` are `pub(super)`), and the scroll paragraph
  says the fix from this side is page composition.
- `crates/settings-ui/src/panel.rs`, `terminal/mod.rs`, `key_bindings/key_bindings_ui.rs`
  module docs — each says why its pages are short.
- `docs/PROJECT.md` — read for the "no patching `gpui-component`" rule. **No change.**
- `reference/gpui-kit/`, `reference/zed/` — read only, cited by line. **Not changed.**

### Gaps

- **The upstream defect is untouched; the split routes around it.** `SettingPage::render` asks
  a gpui `list` to reveal a group it has not measured, once, and `scroll_to_reveal_item`'s
  forward branch treats an unmeasured item as zero-height. Nothing in this packet changes that:
  a page long enough to put a group outside the measured window would fail exactly as before.
  That is why the two page tables carry comments saying so and why both are asserted. The
  report to file upstream is written out in full in **Handoff**.
- **The claim that a test guards the group-ordering rule was false and is withdrawn.**
  `sidebar_group_to_scroll_index` was `#[cfg(test)]` over a hand-written `&[bool]`; the
  verifier removed `.title("Interface")` from a real page and 46/46 stayed green. The helper,
  its three tests and the two sentences that trusted them are deleted. The rule survives as
  prose plus the fact that every OneTerm group on every page is titled today; it cannot be
  guarded from this side and `docs/gui-layout.md` now says so instead of claiming otherwise.
- **The page budget is walked, not asserted.** The tests cap the number of *groups* per page
  (one to three) because that is the proxy a test can see; the real constraint is rendered
  height, which only a walk can measure. A page of three unusually tall groups would pass the
  test and fail the window. The cap is deliberately tight for that reason, and the walk above
  is the measurement.
- **`Completion` needs a short scroll to read its last three items**
  (`evidence/US-0122-28d-settings-completion.png`), and `Terminal` its last two
  (`evidence/US-0122-28-settings-terminal.png`). Neither has a navigation consequence —
  `Completion` is one group, so the page row is its only entry and page selection is exact;
  `Terminal`'s second group is revealed in full by its sub-item. Splitting further would buy a
  scrollbar's worth of comfort for two more sidebar rows, and was not worth it.
- **Twelve pages is more sidebar than six.** Collapsed it fits with room to spare; expanded it
  does not, and the frame shows it. The row that would fix that — a chevron that toggles rather
  than forces open — is upstream (`settings.rs:193`).
- **The `P16` page wrapper was still not built**, and that is now a choice rather than a gap:
  the split achieves the acceptance without losing sub-items, search or Reset All, which the
  wrapper would have cost. See Handoff.


## Handoff

Next owner: the repository owner, to file the report below with GPUI Kit. **Nothing has been
filed from here** — this session has no issue tracker access and does not open issues on the
project's behalf. Everything else in this packet is complete; this is the "follow-up raised
upstream" half of `IN-0042.md:201-203`, written out so filing it is a copy and a paste.

### Upstream report 1 of 2, ready to file

**Title:** `setting::Settings` — a sidebar sub-item cannot scroll to a group below the fold of
a freshly opened page

**Body:**

> **What happens**
>
> On a `setting::SettingPage` taller than its viewport, clicking a sidebar sub-item for a group
> below the fold scrolls only a little way and lands on an earlier group. The further down the
> page the group is, the shorter the jump falls. On a ten-group page, clicking the tenth
> sub-item moves the page by about 95 px and lands on the second group. Clicking the same
> sub-item again gets closer, and repeating it eventually arrives.
>
> **Why**
>
> `SettingPage::render` consumes `deferred_scroll_group_ix` once and calls
> `ListState::scroll_to_reveal_item`:
>
> ```rust
> // crates/component/src/setting/page.rs:152-158
> let deferred_scroll_group_ix = state.read(cx).deferred_scroll_group_ix;
> if let Some(ix) = deferred_scroll_group_ix {
>     state.update(cx, |state, _| { state.deferred_scroll_group_ix = None; });
>     list_state.scroll_to_reveal_item(ix);
> }
> ```
>
> `scroll_to_reveal_item`'s forward branch derives its target from the summed heights in
> `state.items`:
>
> ```rust
> // gpui/src/elements/list.rs:664-694
> } else {
>     let mut cursor = state.items.cursor::<ListItemSummary>(());
>     cursor.seek(&Count(ix + 1), Bias::Right);
>     let bottom = cursor.start().height + padding.top;
>     let goal_top = px(0.).max(bottom - height + padding.bottom);
>     ...
> }
> ```
>
> An item the list has not laid out is `ListItem::Unmeasured` (`list.rs:245-249`) and
> `ListItem::summary` gives it `height: px(0.)` when its `size_hint` is `None`
> (`list.rs:1622-1637`); `ListState::new` (`list.rs:314-331`) populates itself through
> `splice`, which creates every item as `ListItem::Unmeasured { size_hint: None, .. }`
> (`list.rs:520-532`). A freshly opened page has laid out only the viewport plus the
> `px(100.)` overdraw the page passes to `ListState::new` (`page.rs:139-143`), so every group
> below that weighs nothing, `bottom` collapses to roughly the measured height, and `goal_top`
> clamps to about `overdraw`. The backward branch (`ix <= scroll_top.item_ix`) sets
> `item_ix = ix, offset = 0` and is exact, which is why scrolling *up* the sidebar always
> works.
>
> **Three possible fixes, cheapest first**
>
> 1. Seed the heights. `ListState::reset_with_uniform_height` is public (`list.rs:372-376`) and
>    hands every item a `size_hint` through `apply_uniform_item_height` (`:378-394`), which
>    `ListItem::summary` then counts. Using it at `page.rs:143` with a rough per-group estimate
>    makes the forward branch approximately right from the first frame, with no retry.
> 2. Scroll to the item's top instead of revealing it: `ListState::scroll_to(ListOffset { item_ix: ix, offset_in_item: px(0.) })`
>    needs no measurement at all and is arguably what a sidebar jump means anyway.
> 3. Keep `deferred_scroll_group_ix` set until the target is actually visible
>    (`ListState::bounds_for_item`, `list.rs:698`, returns `None` for an item that has not been
>    rendered, which is exactly the "not there yet" signal), so the scroll converges over a
>    frame or two.
>
> **Workaround in use**
>
> Splitting long pages into pages that each fit the window. Page *selection* swaps
> `selected_index.page_ix` (`settings.rs:197-207`) and scrolls nothing, so it is exact; a page
> whose groups all fall inside the measured window is measured in full, and then
> `scroll_to_reveal_item` is exact too. It is not a fix — it is a constraint on page
> composition that consumers have to keep obeying.

### Upstream report 2 of 2, ready to file

Found by the second independent verification (`F-R7`). Nothing in this round caused it and
nothing in this round can fix it, but the split raised the exposure from six pages to twelve,
so it travels with the first report.

**Title:** `setting::Settings` — searching blanks the content pane, and clearing the query
lands on the wrong page

**Body:**

> **What happens**
>
> With a page selected, type a query that the current page does not match. The sidebar
> correctly narrows to the pages that *do* match — search does reach across pages — but the
> content pane goes **empty**, including for the page still shown as selected. Clicking a page
> in the filtered sidebar and then clearing the query lands on a different page than the one
> that was clicked.
>
> Walked on a twelve-page `Settings`: from page 7 of 12, typing `gutter` narrowed the sidebar
> to two pages and blanked the pane; clicking the second of them and clearing the query landed
> on the unfiltered list's page 1.
>
> **Why**
>
> `selected_index.page_ix` is an index into the **filtered** list, but it survives a change of
> filter:
>
> ```rust
> // crates/component/src/setting/settings.rs:145-163
> fn render_active_page(&self, state, pages: &Vec<SettingPage>, ...) {
>     let selected_index = state.read(cx).selected_index;
>     for (ix, page) in pages.into_iter().enumerate() {
>         if selected_index.page_ix == ix { return page.render(...); }
>     }
>     return div().into_any_element();   // <- the blank pane
> }
> ```
>
> `filtered_pages` (`settings.rs:112-140`) drops every page with no matching item, so the list
> `render_active_page` walks is shorter than the one the index was chosen from. When the
> selected page is filtered out, no `ix` matches and the fallback empty `div` is rendered; when
> the query is cleared, the same integer now addresses a different page.
>
> **Possible fix**
>
> Select by identity rather than by position — keep the page's title (or a stable id) in
> `SelectIndex` and resolve it against whichever list is being rendered — or, more cheaply,
> render the selected page unfiltered when it is not in the filtered list, and remap
> `page_ix` through the filter whenever the query changes.
>
> **Severity**
>
> Cosmetic on a small `Settings`; on a twelve-page one it reads as the window having broken.

### If the owner wants the wrapper instead

The `P16` page wrapper stays unbuilt and this packet records why (see §"Why neither is
reachable"). If a future owner decides the three capabilities it costs — sidebar sub-items,
per-item search, page-level `Reset All` — are worth losing on one page, that is a `DEC`, not a
reopening of this packet.
