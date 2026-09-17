# Work: About gets Check for Updates, network settings get their page, the theme list opens on the current theme, default-only binding rows are single

ID: US-0121
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

Three Settings pages stop working against the user. The update action and the update settings
are on the same page; proxy and certificate settings are somewhere a person would look for
them; the theme dropdown opens showing the theme that is selected; and a key-binding row at
its default prints its binding once.

## Findings and proposals covered

`P12` — *"Give the Settings About page a "Check for Updates" button (same action as the
dialog), and move Proxy URL / Verify Certificates to their own "Network" page."*

`P13` — *"Open the theme dropdown scrolled to the current theme, and split it into
"Light"/"Dark" sections."*

`P11` (key-binding half) — *"…drop the redundant "Default: …" line on key-binding rows that are
at their default."* The descender half of `P11` belongs to `BUG-0069`.

Addresses `F17`, `F18` (medium) and the density half of `F32` (low), quoted from
`research/ux-walkthrough-2026-09-16.md`:

> | F17 | Settings → About | **Proxy URL** and **Verify Certificates** live under *About*, and
> the Updates group there has no "Check now" — the manual check exists only in the separate
> About **dialog**, where the update *settings* are not. | Categorisation: network settings are
> not "about", and the update action and its settings are split across two surfaces. | medium |
> 35, 36b, 37 |

> | F18 | Settings → Appearance | The theme dropdown opens at the top of ~40 entries with the
> first row highlighted, not scrolled to the current theme (the ✓ was off-screen for "Zed One
> Dark"). Light and dark themes are interleaved in one list while "Mode" is a separate
> Light/Dark setting. | Orientation: the user cannot see what is selected without scrolling,
> and the two controls can contradict each other. | medium | 32 |

> | F32 | Key Bindings page | Every row prints the binding twice — the chip (`Ctrl+T`) and
> "Default: ctrl-t" — even when they are equal… | Density + F6 + orientation. | low | 27, 39 |

`F32`'s contrast half is `US-0111`; its "entering capture replaces the whole row" half is not
proposed by `P11` and is recorded in Gaps.

## Scope

- [ ] In scope:
  - `crates/settings-ui/src/about.rs` — a "Check for Updates" control, running the same action
    the About dialog runs; and removing the network fields from this page.
  - `crates/settings-ui/src/updates/` — the update check entry point both surfaces call.
  - `crates/settings-ui/src/panel.rs:85-93` — the page list, if a Network page is added.
  - `crates/settings-ui/src/appearance.rs` — the theme dropdown: open on the current theme, and
    grouped by light/dark.
  - `crates/settings-ui/src/key_bindings/key_bindings_ui.rs` — the `Default:` line, shown only
    when the row differs from its default.
- [ ] Out of scope:
  - The update *mechanism*: checking, downloading, applying, rollback. This packet adds a
    second entry point to an existing action and moves two settings.
  - The sidebar's navigation behaviour (`US-0122`), which this packet must not depend on —
    a new page must be reachable by scrolling even while the sidebar jump is broken.
  - The default key bindings themselves (`US-0123`).
  - The capture UI replacing the whole row (`F32`, third symptom) — not proposed; Gaps.
  - Settings → General's emptiness and the triple-naming (`US-0122`).

## Acceptance

- [x] The Settings About page offers a manual update check that performs the same check the
      About dialog performs, and its result is visible on the page.
- [x] Proxy URL and Verify Certificates are no longer under About. They are under a heading a
      user would look for when thinking "network", and both still read and write the same
      configuration values they do today.
- [x] Changing the proxy or the certificate setting on its new page still affects the update
      check — the move must not orphan the settings from the code that reads them.
- [x] Opening the theme dropdown shows the currently selected theme, with its tick visible,
      without scrolling.
- [x] Light and dark themes are visually separated in the list.
- [x] Selecting a theme still works, still persists, and still agrees with the separate Mode
      setting — or, if the two can still contradict each other, that is recorded in Gaps as a
      finding this packet leaves standing.
- [x] A key-binding row whose binding equals its default shows the binding once. A row that
      differs still shows what the default was, so Reset is meaningful.
- [x] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

## Documentation

### Owning Docs Reviewed

- `docs/auto-update.md` — the update flow, its settings and where they live. `P12` names it as
  the owning doc. **Update required:** a second entry point for the manual check, and the new
  home of the proxy and certificate settings.
- `docs/gui-layout.md` §Settings window — describes the page composition, the `GroupBoxVariant`,
  the reset behaviour, the accessibility roles, and the untitled-group ordering rule: *"The
  upstream sidebar numbers only titled groups, while page scrolling indexes every group.
  OneTerm therefore keeps untitled groups after all titled groups on a page; the About-page
  ordering regression protects that alignment."* **This constrains the About page directly** —
  adding or removing a group there can break that alignment and the test that guards it.
  **Update required** if the page list changes.
- `crates/settings-ui/src/panel.rs:85-93` — the page list
  (`general`, `key_bindings`, `terminal`, `ssh`, `appearance`, `about`). A Network page is a
  seventh entry, and its position changes every later page's index.
- `docs/ssh-authentication.md` and `docs/ssh-client-connect.md` — read only to confirm the
  proxy and certificate settings this packet moves are the **update** client's, not the SSH
  client's, and that there is no second "network" concept already owned elsewhere. Record the
  answer. **No change expected.**
- `docs/PROJECT.md` — read for standing invariants. **No change.**

### Documentation Action

Update required: `docs/auto-update.md` (the manual check's second entry point, and the
settings' new home), and `docs/gui-layout.md` §Settings window if the page list or the
About page's group ordering changes.

Reason: the packet moves documented settings between documented pages and adds a documented
action to a second surface.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.
Include whether the About-page group-ordering rule still holds and whether its guard test
needed updating.

## Context

- **The About page has a guard test.** `docs/gui-layout.md` records an "About-page ordering
  regression" that protects the titled-group alignment between the sidebar's numbering and the
  page's scroll index. Removing two fields from About and possibly adding a page will move
  those indexes. Read the test before editing the page, not after it goes red.
- **Where the network settings go.** `P12` proposes a new "Network" page. A seventh page for
  two fields is a lot of chrome; an alternative is a titled "Network" group on an existing page
  that already owns connection behaviour. The acceptance is deliberately "no longer under
  About" rather than "a new page", so either is allowed — decide against `docs/auto-update.md`
  and record the reason here. Whichever is chosen, the fields must stay wired to the code that
  reads them.
- **Do not depend on `US-0122`.** The sidebar's sub-item navigation is broken (`F14`) and
  `US-0122` fixes it. A new page or group added here must be reachable by scrolling in the
  meantime, so this packet is acceptable on its own. Ordering the two so this one lands first
  also gives `US-0122` the finished page list to navigate.
- **The theme dropdown.** ~40 entries, opening at the top with the first row highlighted. Two
  separate fixes: scroll-to-selected on open, and grouping. The kit's dropdown owns the
  scrolling; check `reference/gpui-kit/crates/component/src/` for a scroll-to-item or
  selected-index API before building anything. The `+` menu's code already notes that
  `scroll_to_item` is a no-op outside a scrolling container
  (`crates/terminal-view/src/panel/terminal_panel.rs:720-724`) — useful prior knowledge.
- **The key-binding row.** `overrides_from_effective`
  (`crates/settings-ui/src/key_bindings/state.rs:125-138`) already computes exactly "does this
  differ from its default". The row just has to ask. No new state.
- `research/before/35-settings-about.png`, `36-settings-about-updates.png`,
  `36b-settings-about-end.png`, `37-about-dialog.png` (`F17`), `32-theme-dropdown.png` (`F18`)
  and `27-settings-keybindings.png` (`F32`) are the before pictures.

### Decisions taken before writing code

- **Network becomes its own page, not a group elsewhere.** `docs/auto-update.md` §"Data
  ownership" line 221 owns `proxy_url` and `verify_certificates` as *update* preferences
  (`UpdateConfig`, persisted to `update_config.json`). `docs/ssh-authentication.md` and
  `docs/ssh-client-connect.md` were read and neither names a proxy or a certificate setting:
  the SSH client has no "network" concept of its own, so folding the updater's two fields into
  the SSH page would file them under the wrong client. They keep the group builder they already
  have (`updates::network_group`), now hosted by a two-field `Network` page between SSH and
  About. No change to what reads them.
- **The About page keeps every group titled.** Removing `Network` leaves
  `[Identity, Links, Updates]`, all titled, so the untitled-group ordering rule in
  `docs/gui-layout.md` §Settings window still holds and
  `about::tests::identity_group_leads_the_about_page` still guards it.
- **The theme dropdown opens on the current theme by ordering, not by scrolling.** The kit
  cannot open a popup scrolled to its checked row — see Gaps for the file and line. The list is
  therefore built so the current theme is the first selectable row: the current theme's mode
  section comes first, and inside it the current theme leads. Sections are header rows in the
  option list itself (the dropdown field only accepts `(value, label)` pairs), carrying a
  sentinel value the setter ignores.

## Plan

- [ ] Read the About-page ordering guard test first.
- [ ] Decide new page versus new group; record it.
- [ ] Move the network settings; confirm they still drive the update check.
- [ ] Add the manual check to the About page, reusing the dialog's action.
- [ ] Theme dropdown: check the kit's API, then scroll-to-selected and grouping.
- [ ] Key-binding row: hide `Default:` when it equals the chip.
- [ ] Update `docs/auto-update.md` and `docs/gui-layout.md`.
- [ ] Re-capture the scenes.

## Decisions

None expected. If the packet adds a seventh Settings page, that is a page-list change worth a
line in `docs/gui-layout.md`, not a rule future work inherits.

## Verification Plan

1. **Focused:** `cargo test -p oneterm-settings-ui` over the pure halves — the row's
   "show the default line?" predicate (it must agree with `overrides_from_effective`), the
   theme list's grouping and the index it opens at given a current theme. Plus the existing
   About-page ordering test, reviewed rather than merely made green.
2. **Unit:** `cargo test -p oneterm-settings-ui`, `cargo test -p oneterm-settings`.
3. **Integration:** `cargo test --workspace`.
4. **Platform:** `pwsh scripts/ci-local.ps1`.
5. **E2E (GUI walk, re-capture these scenes):**
   - `35-settings-about.png`, `36-settings-about-updates.png`, `36b-settings-about-end.png` —
     About without the network fields and with the manual check.
   - `37-about-dialog.png` — the dialog, unchanged, for comparison.
   - `32-theme-dropdown.png` — the dropdown opening on the current theme, with the tick
     visible and light/dark separated. This is the acceptance frame for `F18`.
   - `31-settings-appearance.png` — the Appearance page around it.
   - `27-settings-keybindings.png` — rows at their default printing once.
   - `39-keybinding-capture.png` — a row that differs, still showing its default.
   Plus, not in the walkthrough: the network settings on their new page, captured as `35b`, and
   the update check's result state.
   Back up `target/{ui_config,update_config}.json` before the walk and restore afterwards, as
   the walkthrough did.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Risks

- **Orphaning the network settings.** Moving two fields between pages is trivial; leaving them
  writing to a value nothing reads is the failure mode. The acceptance requires proving the
  moved settings still affect the update check.
- **Breaking the About-page group ordering.** There is a documented alignment and a regression
  test guarding it. Editing the page without reading them first will produce a red test that is
  tempting to "fix" by changing the expectation.
- **Fighting the kit's dropdown.** If the kit offers no scroll-to-selected, building one may
  mean replacing the dropdown — which is far more than `F18` is worth. If it is not reachable,
  ship the grouping (which shortens the distance to the selected item) and record the scroll
  half in Gaps with the kit file and line.
- **Mode versus theme still contradicting.** `F18`'s second half is a design problem, not a
  scroll problem: a single interleaved list plus a separate Light/Dark mode can disagree.
  Grouping makes it visible rather than solving it. Say so.
- **Reset losing its meaning.** Hiding the `Default:` line on rows at their default is right,
  but the Reset control must still be understandable on those rows — it is a no-op there. Check
  how it reads.

## Evidence and Gaps

> Reworked after independent verification (`evidence/settings-ui-wave1-verify.md`), which
> returned **PASS with findings**. No acceptance clause failed; two of this packet's own Gaps
> were wrong and one reachable improvement was written off as unreachable. Both are fixed
> below, and the frames are re-captured on the reworked build.

### Second rework, after the re-verification of `dab9cac4`

The re-verification returned **PASS** for this packet: both withdrawn Gaps were withdrawn
correctly, the theme picker's headings are real `PopupMenuItem::label`s with the dead click
gone, the persisted shape is unchanged, and the rewritten agreement test fails under the
mutation it names. One finding, a prose defect.

**`F-R1` (minor) — three prose sites claimed more than the kit does. Fixed.** `appearance.rs`'s
module doc, the `ThemeRow::Section` doc comment and `docs/gui-layout.md` §Settings window all
said `PopupMenuItem::label` is excluded "from clicking **and from keyboard navigation**". The
first half is true; the second is not, and this packet's own Gap already said so — three places
asserting what a fourth refuted. All three now say what actually holds: excluded from clicking,
stepped over by `select_up`/`select_down` once a row is selected, but the very first Down-arrow
still lands on row 0 because the kit sets index 0 without checking
(`popup_menu.rs:906-911`); Enter there does nothing.


### Commands

| Command | Result |
| --- | --- |
| `cargo test -p oneterm-settings-ui` | `test result: ok. 52 passed; 0 failed` |
| `cargo clippy -p oneterm-settings-ui --all-targets -- -D warnings` | clean |
| `cargo test --workspace` | 2109 passed, 12 ignored |
| `pwsh scripts/ci-local.ps1` | **`ci-local: all checks passed.`** |

Focused tests:

- `appearance::tests` — the picker's ordering and sectioning, the index it opens at for a dark
  and a light selection, the fallback when the selected theme is not registered, that a
  heading is a different row variant from a theme and so can never reach `apply_theme_named`,
  and that an empty section contributes no heading.
- `key_bindings::key_bindings_ui::tests::the_default_line_agrees_with_what_is_persisted_as_an_override`
  — rewritten. It used to compare the row against `is_at_default` while
  `overrides_from_effective` also calls `is_at_default`, so both sides moved together and the
  test could not fail (verifier MINOR 5). It now holds **both** the row and the persistence
  layer to a written-out truth table. Checked by mutation: making `is_at_default` treat an
  unbound action as "at default" turns `48 passed` into `45 passed; 3 failed`, and this test
  is one of the three. Reverted.
- `about::tests::identity_group_leads_the_about_page` — the existing About-page ordering guard,
  still green and still meaning what it says.

### Acceptance, walked

1016x708, `gui.ps1`, own pid only, on the reworked build.

| Acceptance | Frame | Result |
| --- | --- | --- |
| About offers a manual check that performs the About dialog's check | `evidence/US-0121-36-settings-about-updates.png` | **MET.** Both call `updates::check_now` (`about.rs:103`, `updates/groups.rs:253`), which the verifier confirmed by reading. |
| ...and its result is visible on the page | `evidence/US-0121-36b-settings-about-check-result.png` | **MET.** "OneTerm 0.6.0 is up to date." after a live GitHub round trip. |
| Proxy / Verify Certificates are no longer under About | `evidence/US-0121-35-settings-about.png` | **MET.** Application, Links, Updates. |
| ...and are under a heading a user would look for | `evidence/US-0121-35b-settings-network.png` | **MET.** Network ▸ GitHub Connection. |
| ...and still drive the update check | `evidence/US-0121-36b-...` | **MET.** The verifier confirmed `proxy_item`/`certificate_item` are byte-identical and still write `UpdateConfig` → `update_config.json`; the successful round trip is the behavioural half. |
| The theme dropdown shows the current theme, tick visible, without scrolling | `evidence/US-0121-32-theme-dropdown.png` | **MET.** "Dark themes" heading, then `✓ Zed One Dark` as the first selectable row. |
| Light and dark are visually separated | same frame | **MET**, and better than before: the headings now render muted and disabled rather than looking like themes (see below). |
| A row at its default shows its binding once | `evidence/US-0121-27-settings-keybindings.png` | **MET.** |
| A row that differs still shows its default | `evidence/US-0121-39b-keybinding-differs.png` | **MET.** New Terminal Tab on `F5` keeps "Default: ctrl-t". |
| The capture row | `evidence/US-0121-39-keybinding-capture.png` | Unchanged, as scoped. |

### What the rework changed

- **The section headings are no longer dead clickable rows** (verifier MINOR 3 and MINOR 4).
  The picker is now a `SettingField::element` that builds its own `PopupMenu`, so the headings
  are `PopupMenuItem::label` — which the kit renders `disabled(true).cursor_default()`
  (`popup_menu.rs:1221-1228`) and excludes from `is_clickable` (`:229-241`), and therefore from
  clicking and from `select_up`/`select_down`. The Gap that said this was "supported by
  `PopupMenu` but not exposed by `DropdownField`" was wrong to stop there: `SettingField::element`
  is public and this crate already uses it five times for exactly this reason
  (`terminal/font.rs`, `about.rs`, `terminal/logging.rs`, `updates/groups.rs` ×3).
- **The "Mode and Color Theme can still contradict each other" Gap is withdrawn** (verifier
  MAJOR 1). They cannot. `Theme::apply_config` ends with `self.mode = config.mode`
  (`reference/gpui-kit/crates/component/src/theme/schema.rs:1060`, `:1104`), and `Theme::change`
  sets the mode and then re-applies that mode's stored config
  (`.../theme/mod.rs:237-255`); both dropdowns read the one `Theme` global. The real
  consequence — switching Mode swaps the colour theme to the last one used in that mode — is
  now the Mode row's own description, visible in `evidence/US-0122-26-settings-general.png`.

### Docs reconciled

- `docs/auto-update.md` — the manual check's second entry point, which Settings page owns which
  update setting, and why the two network fields have their own page. Unchanged by the rework.
- `docs/gui-layout.md` §Settings window — the theme-list paragraph now names `theme_rows`,
  explains why the picker is an element field, and records the reordering consequence.
- `docs/ssh-authentication.md`, `docs/ssh-client-connect.md` — read, **no change**: neither
  names a proxy or a certificate setting.
- `docs/agents/persistence.md` — the verifier checked it independently: no schema owner, field
  or version changed, so its Schema owners table still describes `update_config.json`
  correctly. **No change.**
- `docs/PROJECT.md` — read for invariants. **No change.**

### Gaps

- **The kit still cannot open a dropdown scrolled to its checked row.** `PopupMenu::selected_index`
  is private and starts `None` (`popup_menu.rs:292`, `:332`); the only code that scrolls is the
  private `set_selected_index` (`:879-886`), reached only from keyboard navigation. So the
  ordering is the mechanism, not a stopgap for a scroll that could be requested.
- **The first Down-arrow still highlights the heading.** `select_down` with no selection sets
  index 0 unconditionally (`popup_menu.rs:906-910`) without asking whether row 0 is clickable.
  `Confirm` there is now inert (`confirm` matches only `Item`/`ElementItem`, `:833-861`) instead
  of dismissing the menu with no change, and a second Down reaches the first theme — so the
  dead *click* and the dead *Enter* are gone, but the first keypress is still absorbed. That
  last step needs the upstream `select_down` to skip non-clickable rows.
- **The list reorders between opens** (verifier MINOR 6). `theme_rows` is recomputed from the
  current theme on every render, so picking a theme in the other mode moves that whole section
  to the top next time. It is the deliberate trade that puts the tick on screen; it is now
  recorded in `docs/gui-layout.md` rather than left as a surprise.
- **Network is a one-group page** (verifier MINOR 7), which is the shape `F16` filed against
  General, while `US-0122` folded Appearance away saying two controls did not earn a page. The
  two are decided on different grounds — ownership for Network (the settings are the updater's,
  not the SSH client's; `docs/auto-update.md`), findability for Appearance — and after
  `US-0122`'s split the round ships several one-group pages by design. Recorded so the
  before/after report does not read it as an inconsistency left unnoticed.
- **`F32`'s third symptom is untouched**, as scoped: entering capture still replaces the whole
  row. Not proposed by `P11`.


## Handoff

Complete. Reworked once after independent verification; no open action for another actor.
The one upstream item this packet leans on (a popup cannot be opened scrolled to its checked
row) is not filed and does not need to be: the ordering makes it moot, and `US-0122`'s Handoff
carries the kit report that is worth filing.
