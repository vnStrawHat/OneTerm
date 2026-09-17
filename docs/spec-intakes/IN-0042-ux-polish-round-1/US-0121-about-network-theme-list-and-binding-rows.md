# Work: About gets Check for Updates, network settings get their page, the theme list opens on the current theme, default-only binding rows are single

ID: US-0121
Intake: IN-0042
Created: 2026-09-17

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
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

- [ ] The Settings About page offers a manual update check that performs the same check the
      About dialog performs, and its result is visible on the page.
- [ ] Proxy URL and Verify Certificates are no longer under About. They are under a heading a
      user would look for when thinking "network", and both still read and write the same
      configuration values they do today.
- [ ] Changing the proxy or the certificate setting on its new page still affects the update
      check — the move must not orphan the settings from the code that reads them.
- [ ] Opening the theme dropdown shows the currently selected theme, with its tick visible,
      without scrolling.
- [ ] Light and dark themes are visually separated in the list.
- [ ] Selecting a theme still works, still persists, and still agrees with the separate Mode
      setting — or, if the two can still contradict each other, that is recorded in Gaps as a
      finding this packet leaves standing.
- [ ] A key-binding row whose binding equals its default shows the binding once. A row that
      differs still shows what the default was, so Reset is meaningful.
- [ ] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

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

- **Network becomes its own page, not a group elsewhere.** `docs/auto-update.md` Â§"Data
  ownership" line 221 owns `proxy_url` and `verify_certificates` as *update* preferences
  (`UpdateConfig`, persisted to `update_config.json`). `docs/ssh-authentication.md` and
  `docs/ssh-client-connect.md` were read and neither names a proxy or a certificate setting:
  the SSH client has no "network" concept of its own, so folding the updater's two fields into
  the SSH page would file them under the wrong client. They keep the group builder they already
  have (`updates::network_group`), now hosted by a two-field `Network` page between SSH and
  About. No change to what reads them.
- **The About page keeps every group titled.** Removing `Network` leaves
  `[Identity, Links, Updates]`, all titled, so the untitled-group ordering rule in
  `docs/gui-layout.md` Â§Settings window still holds and
  `about::tests::identity_group_leads_the_about_page` still guards it.
- **The theme dropdown opens on the current theme by ordering, not by scrolling.** The kit
  cannot open a popup scrolled to its checked row â€” see Gaps for the file and line. The list is
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
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
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

After implementation, record commands, results, and anything skipped, unavailable, partial, or failing.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
