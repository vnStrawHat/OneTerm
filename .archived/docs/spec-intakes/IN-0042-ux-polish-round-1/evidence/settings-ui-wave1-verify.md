# Independent verification — US-0121, US-0122, US-0123 (`crates/settings-ui`)

Verifier: an independent session that did not write this code.
Branch verified: `worktree-agent-a2f7d43e6af96f087` @ `9939b4a4` (main `@0395135e` already merged in).
Verified from a separate worktree reset to that sha; nothing outside it was touched.
Date: 2026-09-17.

## Verdicts

| Packet | Verdict |
| --- | --- |
| `US-0121` — About check, Network page, theme list, binding rows | **PASS with findings.** Every acceptance clause is met and reproduced. Two of the packet's own Gaps are wrong: one records a defect the code already prevents, one understates a reachable option. |
| `US-0122` — sidebar reaches its group, General has content | **FAIL.** The headline acceptance is not met (correctly reported), but the packet closes on `IN-0042`'s escape clause without satisfying either half of it, and its central compensating claim — "the index-alignment rule … now fails a build" — is false and is disproved below. |
| `US-0123` — app defaults leave the single-Ctrl keys to terminals | **PASS on code, FAIL on the record.** The table, the migration and the collision rule are correct and reproduced. `DEC-0018`'s Status section now contradicts itself on this branch and the packet's Evidence/Gaps still describe the pre-merge world. |
| **Overall** | **CHANGES REQUIRED** before the round closes. No code defect found in `US-0121`/`US-0123`; the blocking items are `US-0122`'s unmet closing condition and a false test claim, plus a stale decision record. |

Everything below cites file and line. Kit and gpui citations were read in
`reference/gpui-kit/` and `reference/zed/` (read-only) and each line number in the
packets was checked, not assumed.

---

## 1. Commands run

| Command | Final line |
| --- | --- |
| `cargo test -p oneterm-settings-ui` | `test result: ok. 46 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` (lib) + `0 passed` (doc-tests) |
| Mutation run 1 — `theme_entries` ordering | `test result: FAILED. 44 passed; 2 failed` |
| Mutation run 2 — `is_at_default` | `test result: FAILED. 44 passed; 2 failed` |
| Mutation run 3 — `collisions_with_overrides` | `test result: FAILED. 44 passed; 2 failed` |
| Mutation run 4 — real page violates the untitled-groups-last rule | `test result: ok. 46 passed; 0 failed` — **the rule is not guarded** |
| `pwsh scripts/ci-local.ps1` | `ci-local: all checks passed.` |

All mutations were reverted with `git checkout --` and the tree is clean apart from
this file and the `*-verify-*.png` frames.

### The three packets have no platform-gate evidence at all

All three Commands tables read:

```
| `pwsh scripts/ci-local.ps1` | see the closing note below |
```

`US-0121:267`, `US-0122:333`, `US-0123:287`. **There is no closing note in any of the
three files.** The `Platform proof` and `Verify command passed` boxes are unticked in all
three `HARNESS:PROOF` blocks, and `ci-local`'s result is nowhere in the record. It is
supplied above: the gate passes on `9939b4a4`.

Minor, same class: every authored Acceptance checkbox in all three packets is still
`- [ ]`, including clauses the Evidence table calls MET and the one it calls NOT MET.

---

## 2. Mutation testing

Three decision functions were mutated and the suite re-run. A fourth mutation was added
to test the claim that the page-composition rule is now guarded by a test.

| # | Mutation | Caught by |
| --- | --- | --- |
| 1 | `appearance.rs:143` — delete `names.sort_by_key(\|name\| name.as_ref() != current);` (the selected theme no longer leads its section) | `the_selected_theme_leads_its_section_and_its_section_leads_the_list`, `a_light_selection_puts_the_light_section_first` |
| 2 | `state.rs:209` — `is_at_default` becomes `effective.is_empty() \|\| effective == default.unwrap_or("")` (an action the user unbound counts as "at default") | `a_rebound_row_still_names_the_default_reset_would_restore` (UI side), `overrides_keep_only_entries_that_differ_from_the_default` (persistence side) |
| 3 | `state.rs:190` — `collisions_with_overrides` returns `(winner.id, action.id)` instead of `(action.id, winner.id)` (the wrong action is unbound) | `a_surviving_override_beats_a_new_default_and_unbinds_the_displaced_action`, `modifier_order_does_not_hide_a_collision` |
| 4 | `general.rs:31` — drop `.title("Interface")`, putting an untitled group before a titled one on the landing page | **nothing. 46 passed, 0 failed.** |

Mutation 2 is also the proof for `US-0121`'s "the line and the override can never
disagree" claim, and for a weakness in the test that claims it: both sides of
`the_default_line_agrees_with_what_is_persisted_as_an_override`
(`key_bindings_ui.rs:343-359`) call `is_at_default`, so mutating `is_at_default` moves
them together and that test stays green. It is the two *other* tests that bite. The claim
holds structurally — `overrides_from_effective` (`state.rs:215`) and `show_default`
(`key_bindings_ui.rs:78`) both route through the one predicate — but the test written to
assert it cannot fail.

---

## 3. US-0121 — PASS with findings

### 3.1 Claims attacked and upheld

**"The kit cannot pre-scroll a popup."** Verified, and the packet's line numbers are
exact. `PopupMenu::selected_index` is a private field
(`reference/gpui-kit/crates/component/src/menu/popup_menu.rs:292`) initialised to `None`
(`:332`), `scroll_handle` at `:339`; the only code that scrolls is the private
`set_selected_index` (`:879-886`), reached only from `select_up`/`select_down`
(`:886-922`). No `pub fn` on `PopupMenu` sets an index (grepped over
`crates/component/src`). `DropdownField::render`
(`.../setting/fields/dropdown.rs:41-88`) hands the menu nothing but `(value, label)`
pairs and `.scrollable(...)` — the packet cites `:64-84`, which is that body.

**"Clicking a header does nothing — it writes nothing."** Verified three ways.
`dropdown.rs:79-81` calls `set_value(T::from(value))`, and `set_value`
(`.../setting/fields/mod.rs:49-58`) does nothing but return the caller's own setter, so
the sentinel reaches only `apply_theme_named` (`appearance.rs:101-108`), which returns
early when the registry has no theme of that name. Walked: clicking `Dark themes`
dismisses the popup, the field still reads `Zed One Dark`
(`evidence/US-0121-verify-32b-header-click-does-nothing.png`), and
`target/ui_config.json` after the whole walk is `{"right_dock_mode": "ssh_client"}` —
no sentinel, no blanked field, nothing persisted.

**"Check for Updates runs the dialog's action."** Verified: `about.rs:103` and
`updates/groups.rs:253` both call `updates::check_now(window, cx)`.

**Network move, persistence shape.** Verified: `proxy_item` / `certificate_item` are
byte-identical, still writing `UpdateConfig` → `update_config.json`. No schema owner,
field or version changes, so `docs/agents/persistence.md` needs nothing — its Schema
owners table still describes the file correctly. Re-walked:
`evidence/US-0121-verify-35b-network.png`.

**`is_at_default` shared with `overrides_from_effective`.** Verified structurally
(`state.rs:215-228` calls it) and by mutation 2 above.

**The theme list opens on the current theme.** Re-walked on my own build, own pid:
`evidence/US-0121-verify-32-theme-dropdown.png` is byte-identical in size to the
implementer's `US-0121-32-theme-dropdown.png` (60 869 B) — the same frame, independently
produced. `Dark themes` leads, `✓ Zed One Dark` is the first selectable row, the tick is
on screen without scrolling. `F18`'s scroll half is met by ordering.

### 3.2 Findings

**MAJOR 1 — the "Mode and Color Theme can still contradict each other" Gap is false.**
`US-0121` Gaps and Risks both record this as a standing design defect, and `US-0126`'s
before/after report would inherit it. It cannot happen. `Theme::apply_config`
(`reference/gpui-kit/crates/component/src/theme/schema.rs:1060`) ends with
`self.mode = config.mode;` (`:1104`), so selecting a light theme flips Mode to Light in
the same call; `Theme::change` (`.../theme/mod.rs:237-255`) sets `theme.mode` and then
re-applies the stored per-mode config, so selecting a Mode changes the theme. Both
dropdowns read the one `Theme` global (`appearance.rs:47-52` reads `cx.theme().mode`,
`:82` reads `cx.theme().theme_name()`). They cannot disagree. Remove the Gap, or replace
it with the real consequence: switching Mode silently swaps the colour theme to whatever
was last used for that mode.

**MAJOR 2 — the "DropdownField does not expose it" Gap understates what this crate
already does.** The Gap says a disabled or element header row is something "`PopupMenu`
supports but `DropdownField` does not expose", and stops there. `SettingField::element`
is public (`.../setting/fields/mod.rs:232`) and `crates/settings-ui` already uses it five
times precisely to escape a kit field's limits — `terminal/font.rs:323`, whose comment at
`:306` says so in as many words, plus `about.rs:180`, `terminal/logging.rs:96`,
`updates/groups.rs:148,244,266`. An element field rendering its own
`Button::dropdown_menu_with_anchor` can build the menu itself and use
`PopupMenuItem::label(...)` (`popup_menu.rs:120`), which `is_clickable()` filters out of
navigation and clicking, plus `.separator()`. That removes the dead rows entirely. The
pre-scroll half would still be unreachable, so the ordering stays either way. The Gap
should say "reachable at the cost of an element field" rather than "not exposed".

**MINOR 3 — a header row is visually indistinguishable from a theme.** In
`evidence/US-0121-verify-32-theme-dropdown.png`, `Dark themes` has the same colour,
indent, row height and hover target as `Adventure` below it. A user reads it as a theme
named "Dark themes" and clicking it is a dead click.

**MINOR 4 — keyboard navigation now starts on the dead row.** `PopupMenu::select_down`
with `selected_index == None` sets index 0 unconditionally
(`popup_menu.rs:905-909`) — the header — and `Confirm` there dismisses the menu with no
change (`:832-864`). Before this packet, row 0 was a real theme. Down-arrow-then-Enter
used to select the first theme; now it selects nothing.

**MINOR 5 — the agreement test cannot fail.** See §2, mutation 2. Round-trip the
assertion through `overrides_from_effective` instead of through `is_at_default`.

**MINOR 6 — the list reorders between opens.** `theme_entries` is recomputed from
`cx.theme().theme_name()` on every render (`appearance.rs:79-84`, page rebuilt per render
per `panel.rs:86`), so picking a light theme moves the whole `Light themes` block to the
top for the next open. That is the deliberate trade, but it is not recorded as a
consequence anywhere: a user browsing themes gets a list that reshuffles under them.

**MINOR 7 (cross-packet) — Network re-creates the shape `F16` filed.**
`evidence/US-0121-verify-35b-network.png` is a page with one group, two fields, ~85 %
empty — the exact complaint `F16` made about General, and the opposite of the reasoning
`US-0122` used when it folded Appearance away ("two controls did not earn a page"). Two
packets in one wave gave opposite answers to the same question. The decision is recorded
in `US-0121` §"Decisions taken before writing code" and is defensible on ownership
grounds; it is the inconsistency that is worth a line, so the round does not ship a new
empty page while calling an empty page a finding.

---

## 4. US-0122 — FAIL

### 4.1 Every upstream citation is exact

I checked each line number rather than trusting it. All correct:

| Cited | What is there |
| --- | --- |
| `component/src/setting/settings.rs:209-216` | `.when(page.groups.len() > 1, ...)` → `page.groups.iter().filter(\|g\| g.title.is_some()).enumerate()` |
| `settings.rs:222-230` (assignment at `:228`) | `state.deferred_scroll_group_ix = Some(group_ix);` |
| `settings.rs:193` | `.click_to_open(true)` on the page row |
| `settings.rs:248-253` | `pub(super) struct SettingsState { selected_index, deferred_scroll_group_ix, search_input }` |
| `settings.rs:101-104` | `pub fn default_selected_index`, which writes `selected_index` only |
| `settings.rs:366-377` | `SettingsState { ..., deferred_scroll_group_ix: None }` |
| `settings.rs:112-140` | `filtered_pages`, per-`SettingItem` search |
| `setting/page.rs:111-119` | `is_resettable` / `reset_all` — the page-level Reset All |
| `page.rs:121-128` | `pub(super) fn render` |
| `page.rs:131-137` | groups filtered by query, indexed in full |
| `page.rs:139-143` | `ListState::new(groups_count, ListAlignment::Top, px(100.))` via `use_keyed_state("list-state:{ix}")` |
| `page.rs:152-158` | deferred index consumed once at `:155`, `scroll_to_reveal_item(ix)` at `:157` |
| `zed/crates/gpui/src/elements/list.rs:664-694` | `scroll_to_reveal_item`; backward branch exact, forward branch sums `state.items` heights |
| `list.rs:245-297` | `ListItem::Unmeasured` at `:245-249`, `ListItemSummary` declared at `:292` |
| `sidebar/menu.rs:312-333` | the caret `Button`, `cx.stop_propagation()` at `:327`, toggles the open state |
| `menu.rs:344-350` | `if click_to_open { *is_open = true }` — force open, never toggle |

The mechanism is confirmed one step further than the packet took it: `ListItem::summary`
(`list.rs:1622-1637`) gives an `Unmeasured` item `height: px(0.)` when its `size_hint` is
`None`, and `ListState::new` creates every item with `size_hint: None` (`list.rs:527`).
So the diagnosis is exactly right, and the page-wrapper rejection is sound: `ListState`
is keyed element state inside the kit's own id stack (`page.rs:139-145`) and
`SettingsState` is `pub(super)`.

I also reproduced the failure independently on my own build:
`evidence/US-0122-verify-30-completion.png` — clicking "Completion" (9th of 9) scrolled
~95 px and landed on Font/Cursor, the same frame the implementer captured.
`evidence/US-0122-verify-26-general.png` confirms General's three groups (byte-identical
in size to the implementer's frame at 56 369 B).

### 4.2 Findings

**MAJOR 1 — "the rule now fails a build" is false, and I broke the rule to prove it.**
`US-0122` §"What this packet ships instead" says *"The index-alignment rule becomes a
test… The rule was prose in `docs/gui-layout.md`; now it fails a build."*
`docs/gui-layout.md` §"Sidebar navigation, and the two things upstream owns" repeats it:
*"`panel.rs`'s `sidebar_group_to_scroll_index` tests pin the rule so it fails a build
rather than a walkthrough."*

`sidebar_group_to_scroll_index` is declared `#[cfg(test)]` at `panel.rs:138-146`. It
takes a hand-written `&[bool]` and is never called with a real page; the three tests
(`panel.rs:162-196`) assert properties of that five-line reimplementation of the kit's
own logic. It cannot see a real page, because `SettingPage::groups`
(`component/src/setting/page.rs:29`) and `SettingGroup::title`
(`.../setting/group.rs:19`) are both `pub(super)` — invisible from OneTerm.

Proof (mutation 4): removing `.title("Interface")` from `general.rs:31` puts an untitled
group between two titled ones on the landing page — the precise violation the rule
exists to prevent — and `cargo test -p oneterm-settings-ui` reports
`46 passed; 0 failed`. The rule is exactly as unguarded as it was before this packet.

Either delete the helper and its three tests as dead weight and keep the prose, or keep
them and stop claiming they guard anything. Do not leave the claim in `docs/gui-layout.md`,
where the next agent will trust it.

**MAJOR 2 — the one reachable OneTerm-side fix was named, priced at zero, and not
measured.** The packet rejects `P16`'s wrapper because it would cost sub-items, per-item
search and page-level Reset All, and then ships "keep pages short" as the mitigation
while leaving Terminal at nine groups — whereupon its own acceptance still fails.

Splitting a long page into several shorter *pages* costs none of those three things.
Sub-items survive on any page with more than one group (`settings.rs:209` gates only on
`page.groups.len() > 1`); search is per `SettingItem` and page-independent
(`settings.rs:112-140`); Reset All is per page (`page.rs:111-119`). And page selection
itself involves no scroll at all — it swaps `selected_index.page_ix`
(`settings.rs:197-207`) — so it is exact today. A page whose groups fit
viewport + 100 px overdraw is measured in full and then `scroll_to_reveal_item` is exact,
which is the packet's own reasoning; it simply was not carried to a number. From the
walked frames a 708 px window gives roughly 600 px of page viewport and Font alone is
~370 px, so the threshold looks like two to three groups, not nine. Nothing in the packet
measures it, and no build was made to test it.

`IN-0042.md:198-204` asks for "a OneTerm-side solution — a page wrapper that owns its own
scroll for `US-0122`" **or** the recorded limit. The wrapper is not the only OneTerm-side
solution, and the cheaper one was left untried.

**MAJOR 3 — neither half of the escape clause is satisfied, so the packet cannot close on
it.** `IN-0042.md:201-203` requires, for a packet that records the limit: "what the
OneTerm-side attempt cost, and the follow-up raised upstream". `US-0122` §"Why neither is
reachable" says the wrapper "was scoped and rejected … so it was not built" — there is no
attempt and therefore no measured cost — and its Gap says "**Follow-up: raise it with
GPUI Kit, quoting `setting/page.rs:152-158`**", which is an instruction to a future self,
not a raised item. `US-0122`'s own Acceptance repeats the requirement and adds
*"'Upstream owns it' alone does not close this packet."*

**MINOR 4 — the upstream fix the Gap proposes is not the smallest one.** The Gap offers
"retry the deferred scroll while the target is still not visible, or scroll to the item's
top with `ListState::scroll_to`". There is a one-call option: `ListState::reset_with_uniform_height`
is public (`list.rs:372-375`) and seeds every item with a `size_hint`
(`:377-394`), which `ListItem::summary` then counts (`:1631-1637`). A kit that used it at
`page.rs:143` would make the forward branch approximately right from the first frame with
no retry loop. Worth putting in the upstream report.

**MINOR 5 — an acceptance marked MET is met only in one sidebar state.** "Appearance and
About reachable without the sidebar scrolling past the fold" is recorded **MET**, with the
counter-case in Gaps. The counter-case is the default reading path: open Key Bindings and
Terminal together and the sidebar scrolls, with SSH, Network and About below the fold —
reproduced at `evidence/US-0123-verify-27-keybindings.png` (note the sidebar scrollbar and
the truncated `SSH` row). Mark it PARTIAL rather than MET with a footnote.

**MINOR 6 — "no setting lost" independently confirmed.** 58 `SettingItem`s on `main`, 59
on `HEAD`; the addition is "Check for Updates" and the deltas are the naming sweep
(`Cursor Shape`→`Shape`, `Font Family`→`Family`, `Bell Enabled`→`Enabled`, …). Group
descriptions that survive all say something their title does not (checked across
`completion.rs:35-36`, `font.rs:37-38`, `logging.rs:26-27`, `shell.rs:37-38`,
`groups.rs:58-59`). The only removed group description carrying information was
`ssh.rs`'s SFTP edit-limit group, whose item description keeps the meaning.

---

## 5. US-0123 — PASS on code, FAIL on the record

### 5.1 Claims attacked and upheld

**The table is exactly `DEC-0018`.** Re-read from
`key_bindings_actions.rs:85-125`: `new_ssh_session` → `ctrl-shift-n`, `quit` →
`ctrl-shift-q`, `about` → `f1`, `toggle_gutter` → `None`; `close_panel` `ctrl-w` and
`new_terminal_tab` `ctrl-t` untouched; `toggle_zoom` `shift-escape` and `open_settings`
`ctrl-,` untouched. Reproduced live on a fresh profile:
`evidence/US-0123-verify-25-app-menu.png` (the app menu advertises **About F1** and
**Quit Ctrl+Shift+Q**) and `evidence/US-0123-verify-27-keybindings.png` (all eight App
Menu rows, `Toggle Gutter —`, every row a single line).

**The migration is the table edit.** After the whole walk `target/ui_config.json` is
`{"right_dock_mode": "ssh_client"}` — no `key_bindings` key was created. Nothing rewritten.

**The collision rule.** `resolve_default_collisions` (`state.rs:138-165`) runs at the head
of `apply_key_bindings` (`state.rs:89`) over the pure `collisions_with_overrides`
(`state.rs:167-192`); the override wins, the displaced action is emptied in `effective`,
one `warn` per resolution that changes something. Mutation 3 confirms the tests bite on
which action gets unbound.

**Hand-edited-file cases I walked through the code:**

- an override equal to an **old** default (`{"quit": "ctrl-q"}`): `is_at_default("ctrl-q", Some("ctrl-shift-q"))`
  is false, so it survives as an override and nothing else defaults to `ctrl-q`. Correct.
- `about` rebound to `f1` **before** the migration: after it,
  `is_at_default("f1", Some("f1"))` is true, so `overrides_from_effective` drops the entry
  on the next save and the user is silently folded onto the new default. Correct, and
  invisible — which is the intended outcome.
- an override colliding with a kept default (`{"close_panel": "ctrl-shift-n"}`):
  `new_ssh_session` is displaced and unbound. Correct.

**`f1` really is taken from the terminal — confirmed as mechanism, not inference.** Every
row in `BINDABLE_ACTIONS` has `context: None` (all 37 dumped), so the binding has no
predicate and matches at every node of the dispatch path. gpui dispatches matched
bindings **before** key-down listeners: `Window::dispatch_key_event`
(`reference/zed/crates/gpui/src/window.rs:4754`) runs `for binding in match_result.bindings { dispatch_action_on_node(...) }`
at `:4901-4915` and only reaches `finish_dispatch_key_event` → `dispatch_key_down_up_event`
(`:4918`, `:4923`) if propagation survives. The escape hatch at `:4886-4899`
(`skip_bindings`) applies only to a keystroke with a `key_char`, which `F1` has none. The
terminal view receives keys through `.key_context("Terminal").on_key_down(...)`
(`crates/terminal-view/src/terminal_view/render.rs:297-298`) and maps `f1` to
`NamedKey::F1` (`crates/terminal-view/src/input/keys.rs:350`) — a path F1 can no longer
reach. **What a terminal user loses:** F1 in `mc`, `nano`, `htop`, `vim`/`less` help, and
every full-screen TUI that uses the function row, for as long as OneTerm is focused. The
packet records this as an observation against the decision; it is confirmed here as a
certainty, not a risk, and the owner accepted `f1` in `DEC-0018` before this was proved.

### 5.2 Findings

**MAJOR 1 — `DEC-0018`'s Status section contradicts itself on this branch, and the packet
is stale around it.** The merge of `main` kept the owner's line — *"Accepted 2026-09-17 by
the owner, with the explicit ruling that `ctrl-w` … and `ctrl-t` … stay as they are"* —
and the branch adds, three lines below it:

> `US-0123` is implemented against this record and is waiting on that acceptance;
> it has not been accepted by the owner…

The Accepted status is **not** regressed (that was the specific risk to check, and it
survived the merge intact). What regressed is the record around it. `US-0123` still
carries, post-merge:

- Acceptance walked: "`DEC-0018` Accepted and the owner has accepted the defaults | **NOT MET**";
- Gaps: "**`DEC-0018` is still Proposed and the owner has not accepted the new defaults.**";
- Docs reconciled: "Status stays **Proposed**";
- Handoff: "Blocked until: `DEC-0018` is Accepted…".

`docs/HARNESS.md` §Completion Contract: *"Do not mark a work packet implemented while
owning docs are stale."* Delete the contradicting paragraph from `DEC-0018` (keep the
"Where it landed" paragraph, which is useful and true) and reconcile the four passages in
`US-0123`.

**MAJOR 2 — the `DEC-0018` rule test is narrower than the rule it is named after, and it
hides a live instance.** `no_app_level_default_sits_on_a_single_ctrl_control_character`
skips every row whose `group != "App Menu"`. But group is a display heading, not a scope:
every `BINDABLE_ACTIONS` row has `context: None`, so every one of them is app-level and
global. `find` still defaults to **`ctrl-f`** (Edit Menu) — a bare Ctrl+letter that is
readline `forward-char`, page-forward in `less`, and `^F` in `vim`/`man`. It is visible in
the walked frame (`evidence/US-0123-verify-27-keybindings.png`, Edit Menu ▸ Find
`Ctrl+F`). `DEC-0018` §Decision says *"App-level default key bindings do not use a bare
`Ctrl` plus a letter, digit or Space that carries a terminal control character"*, and
`key_bindings/mod.rs` repeats it; the table violates that sentence today, and the test is
scoped so it cannot say so. Either move `find` (a fifth default change needing the owner),
or narrow the wording in `DEC-0018`, `key_bindings/mod.rs` and
`key_bindings_actions.rs`'s header to "the App Menu group" and say why the Edit Menu row
is exempt. Leaving the rule broader than its guard is the failure mode `DEC-0018` exists
to prevent.

**MINOR 3 — override-vs-override collisions are unhandled, and the module doc overstates
the guarantee.** `collisions_with_overrides` only considers an action **at its default**
as a candidate (`state.rs:184-187`), so a hand-edited `ui_config.json` with two overrides
on one keystroke (`{"quit": "ctrl-alt-x", "about": "ctrl-alt-x"}`) registers both and gpui
picks one. The capture UI's `conflicting_action` (`state.rs:234`) blocks this when
rebinding through the app, so the hole is only reachable by hand-editing — but
`key_bindings/mod.rs` now says "the keymap and the page can never disagree" and "Any
future source of bindings must route through the same function or this guarantee stops
holding", which is broader than what the function does. Narrow the wording, or widen the
function.

**MINOR 4 — Reset on a displaced action is a silent no-op while the collision stands.**
Reset writes the default back into `effective` (`key_bindings_ui.rs:226`), the following
`apply_key_bindings` re-detects the collision and empties the row again, plus another
`warn`. The row snaps back to `—` with no message to the user. The Gap covers the
"remove the colliding override later" path and says "the user resets that row, which is
the same one click"; it does not cover the click that happens first and appears to do
nothing.

**MINOR 5 — the release-notes clause is NOT MET, not PARTIAL.** Confirmed in
`.github/workflows/release.yml`: the generator builds each item from the commit
**subject** (`item = f"- {prefix}{description} …"`), reads the body only for the
`BREAKING CHANGE:` trailer to set `is_breaking`, and appends that same subject line to the
Breaking Changes section. The commit (`62562385`) is correct — `feat(key-bindings)!:` with
a `BREAKING CHANGE:` trailer listing old → new — but the rendered notes will read
"**key-bindings:** app defaults leave the single-Ctrl keys to the terminal" and name no
keystroke. The acceptance says "the release notes for the version carrying this packet
list the four moved defaults"; they will not. The packet's own reasoning is right and its
grade is one notch generous.

---

## 6. GUI walk

Own `fast-dev` build of `9939b4a4`, own process id, scratch `HOME`, 1016×708 (the
walkthrough's size), `PrintWindow(hwnd, dc, 2)` + posted `WM_*` from a driver written for
this verification. Only the launched pid was addressed and only it was closed.
`target/fast-dev` was deleted afterwards.

| Scene | Frame | Result |
| --- | --- | --- |
| 32 — theme dropdown open | `evidence/US-0121-verify-32-theme-dropdown.png` | **MET.** `Dark themes` then `✓ Zed One Dark` as the first selectable row, tick on screen, no scrolling. Byte-size-identical to the implementer's frame. |
| 32b — click the section header | `evidence/US-0121-verify-32b-header-click-does-nothing.png` | Menu dismisses, field still `Zed One Dark`, `ui_config.json` untouched. A dead click, and nothing is written. |
| 35b — Network | `evidence/US-0121-verify-35b-network.png` | **MET**, and see MINOR 7: one group, two fields, ~85 % empty. |
| 26 — General | `evidence/US-0122-verify-26-general.png` | **MET.** Theme (Mode, Color Theme), Interface (UI Font Size), Shell (Shell, Custom Program). |
| 30 — click "Completion" | `evidence/US-0122-verify-30-completion.png` | **NOT MET**, reproduced independently. The 9th sub-item scrolls ~95 px and lands on Font/Cursor. |
| 27 — Key Bindings | `evidence/US-0123-verify-27-keybindings.png` | **MET.** New SSH Session `Ctrl+Shift+N`, Toggle Gutter `—`, About `F1`, Quit `Ctrl+Shift+Q`; every row a single line. Also shows the sidebar overflowing with two groups open, and Edit Menu ▸ Find `Ctrl+F`. |
| 25 — app menu (not requested, taken as a cross-check) | `evidence/US-0123-verify-25-app-menu.png` | The running app advertises **About F1**, **Quit Ctrl+Shift+Q**, **Settings Ctrl+,** on a profile with no `key_bindings` entry. |

---

## 7. Gaps in this verification

- **No real-keyboard proof, same ceiling as the packet.** The walk posts `WM_*` messages,
  which set no modifier state, so no `Ctrl` chord and no `F1` was delivered. "`Ctrl-S`
  now reaches the shell" and "F1 no longer reaches `mc`" are proved here from the gpui
  dispatch order and the keymap contexts (§5.1), not from a keypress. A human at the
  keyboard is still required, and `DEC-0018`'s first Consequence remains unverified.
- **The page-split option in `US-0122` MAJOR 2 is argued from the kit source and the
  walked frames, not built.** I did not produce a build with a split Terminal page and
  measure it. The claim is that it is reachable and untested, not that it is proved to
  work; the measurement is the work `US-0122` owes.
- **`cargo test --workspace` was not re-run separately.** `ci-local` runs it; its result
  is folded into the gate line above.
- **The `ctrl-f` finding (MAJOR 2, §5.2) is a scope question for the owner**, not a
  deviation from `DEC-0018` as written. `US-0123` correctly implemented the decision it
  was given.

---

# Re-verification of `dab9cac4` — 2026-09-17

Second independent pass, by a session that wrote none of this code and none of the first
report. Branch `worktree-agent-a2f7d43e6af96f087` @ `dab9cac4` (rework commits `c19018a0`
and `dab9cac4` on top of `d87f9fda`, with `main` @ `23d0fc15` merged at `d99fb3b3`).
Verified from a separate worktree reset to that sha; nothing outside it was touched, and
`target/fast-dev` was deleted after the walk.

## Verdicts

| Packet | Verdict |
| --- | --- |
| `US-0121` | **PASS.** Both wrong Gaps are withdrawn and both withdrawals are correct. The theme picker is now a `SettingField::element` whose headings are real `PopupMenuItem::label`s; the dead click is gone, and selecting a theme writes the same `ui_config.json` it always did. The tautological test is rewritten and fails under the mutation the packet names. Two prose overstatements remain (F-R1). |
| `US-0122` | **PASS with findings.** The page split is real, the headline acceptance is met, every sub-item lands, and the false build-guard claim is deleted from both the packet and `docs/gui-layout.md`. Three new findings, none of them a reason to hold the packet: `Reset All` now reaches across pages (F-R2), two split pages carry one group (F-R3), and one page states its name three times (F-R4). |
| `US-0123` | **PASS with findings.** `DEC-0018` carries the owner's Accepted line and no longer contradicts itself; the rule test now runs over the whole registry and pins the exact exception set; Reset on a displaced row says so. The notification it adds is drawn but is partly overprinted by the page beneath it (F-R6), and the accepted exception set was widened from the owner's two to four by the implementer (F-R5). |
| **Overall** | **PASS with findings.** Every blocking item from the first pass is closed and re-proved here. Nothing found this round blocks the round from closing; F-R2 and F-R6 are defects in new work and should be picked up, in this packet's follow-up or a new one, before the round is called finished. |

## 1. Commands run

| Command | Final line |
| --- | --- |
| `cargo test -p oneterm-settings-ui` | `test result: ok. 48 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` |
| Mutation A — a registry group left off every page | `test result: FAILED. 47 passed; 1 failed` |
| Mutation B — `is_at_default` treats an unbound action as default | `test result: FAILED. 45 passed; 3 failed` |
| Mutation C — a terminal group dropped from `TERMINAL_PAGES` | `test result: FAILED. 47 passed; 1 failed` |
| Mutation C, under clippy | `error: function `group` is never used` → `-D warnings` fails the build |
| Probe D — `.relative()` on `SettingsPanel`'s root (F-R6) | builds, **does not change the symptom**; reverted |
| `pwsh scripts/ci-local.ps1` | **`ci-local: all checks passed.`** |

Every mutation and the probe were reverted with `git checkout --`; the only files this
session leaves changed are this report and the `*-reverify-*.png` frames.

## 2. Mutation testing

| # | Mutation | Caught by |
| --- | --- | --- |
| A | `key_bindings_actions.rs` — `toggle_zoom`'s `group` renamed to `"Mutation Group"`, so a registry group sits on no page | `key_binding_pages_cover_every_group_exactly_once` (`key_bindings_ui.rs:371`), `assertion left == right failed`. **The coverage claim is real.** |
| B | `state.rs:209` — `is_at_default` becomes `effective.is_empty() \|\| effective == default.unwrap_or("")` | three tests, and `the_default_line_agrees_with_what_is_persisted_as_an_override` is one of them (`…the row's Default: line for ""`). This is exactly the `48 → 45 passed; 3 failed` the packet claims; the tautology the first pass found is gone. |
| C | `terminal/mod.rs:48` — `bell::group` removed from `TERMINAL_PAGES` | `the_terminal_groups_are_spread_over_pages_none_of_which_is_long` (`terminal/mod.rs:97`), `a terminal group was added or dropped`. Also caught by clippy: the orphaned builder becomes dead code under `-D warnings`. |

## 3. The first pass's findings, one line each

### `US-0122` (was **FAIL**)

| First-pass finding | Status |
| --- | --- |
| MAJOR 1 — "the index-alignment rule now fails a build" is false | **FIXED.** `sidebar_group_to_scroll_index` and its three tests are deleted (`panel.rs`), and `docs/gui-layout.md` §"Sidebar navigation" now says plainly that **no** test can guard the rule from this side and why (`SettingPage::groups` and `SettingGroup::title` are `pub(super)`). No claim is left for the next agent to trust. |
| MAJOR 2 — the page split was named, priced at zero and never built | **FIXED and built.** `TERMINAL_PAGES` (`terminal/mod.rs:48`, five pages) and `KEY_BINDING_PAGES` (`key_bindings_ui.rs:43`, three pages) hold the split exactly as the first pass predicted it: Terminal → Font, Cursor / Terminal Display → Layout, Scroll, Bell / Mouse & Clipboard → Mouse, Security / Terminal Logging / Completion, and Key Bindings → App Menu, Edit Menu / Terminal → Terminal Context Menu, Input Channel / Sessions → Session Tabs, SFTP. Twelve pages. Walked below: every sub-item of every page lands with its group title on screen. |
| MAJOR 3 — neither half of `IN-0042`'s escape clause was satisfied | **MOOT, correctly.** `IN-0042.md:198-204` offers "a OneTerm-side solution **or** the recorded limit"; the split is a OneTerm-side solution, so the escape clause no longer applies, and the packet says so rather than claiming it was met. The upstream report is written out in full in Handoff and is explicitly marked as not filed — honest, and not load-bearing any more. |
| MINOR 4 — the smallest upstream fix was not named | **FIXED.** `reset_with_uniform_height` is fix #1 in the Handoff report text. |
| MINOR 5 — an acceptance marked MET is met in one sidebar state only | **FIXED.** Now PARTIAL, with the overflowing frame. |
| MINOR 6 — "no setting lost" | Still true; now asserted (mutation C) rather than counted. |

### `US-0121` (was **PASS with findings**)

| First-pass finding | Status |
| --- | --- |
| MAJOR 1 — the "Mode and Color Theme can still contradict" Gap is false | **WITHDRAWN, and the withdrawal is right.** Re-read here: `Theme::apply_config` ends with `self.mode = config.mode` (`reference/gpui-kit/crates/component/src/theme/schema.rs:1104`) and `Theme::change` sets the mode then re-applies that mode's stored config (`.../theme/mod.rs:237-255`). They cannot disagree. The real consequence is now the Mode row's own description — "Switching this also swaps the colour theme to the last one used in that mode." — visible in `evidence/US-0121-reverify-theme-dropdown.png`. |
| MAJOR 2 — the "`DropdownField` does not expose it" Gap understated the crate's own practice | **FIXED by building it.** `color_theme_field` (`appearance.rs`) is a `SettingField::element` that builds its own `PopupMenu`; headings are `PopupMenuItem::label`, which `is_clickable` excludes (`popup_menu.rs:229-241` — `Label` is not one of the `Item`/`ElementItem`/`Submenu` arms) and which `render_item` draws `.disabled(true).cursor_default()` (`:1221-1228`). |
| MINOR 3 — a header is visually indistinguishable from a theme | **FIXED.** Walked: in `evidence/US-0121-reverify-theme-dropdown.png` "Dark themes" is muted and flat while `✓ Zed One Dark` below it is foreground-coloured and hoverable. |
| MINOR 4 — keyboard navigation starts on the dead row | **HALF FIXED, and the packet says so.** `confirm` matches only `Item`/`ElementItem` (`popup_menu.rs:833-861`), so Enter on a heading is inert instead of dismissing; but `select_down` with no selection still does `set_selected_index(0)` unconditionally (`:906-911`), so the first Down still lands on the heading. The packet's Gap records this accurately. See F-R1 for the prose that does not. |
| MINOR 5 — the agreement test cannot fail | **FIXED.** Rewritten against a written-out truth table and against `overrides_for_test` (`state.rs:230-235`); mutation B proves it bites. |
| MINOR 6 — the list reorders between opens | **RECORDED**, in Gaps and in `docs/gui-layout.md`. |
| MINOR 7 — Network re-creates the shape `F16` filed | **RECORDED**, with the ownership-vs-findability distinction, and now consistent: after the split the round ships several one-group pages by design. |

### `US-0123` (was **PASS on code, FAIL on the record**)

| First-pass finding | Status |
| --- | --- |
| MAJOR 1 — `DEC-0018` contradicts itself and the packet is stale | **FIXED.** On this branch `DEC-0018` §Status is the owner's line — "Accepted 2026-09-17 by the owner, with the explicit ruling that `ctrl-w` … and `ctrl-t` … stay as they are" — plus "Where it landed", and nothing else; the "waiting on that acceptance" paragraph is gone. All four stale passages in `US-0123` (Acceptance, Gaps, Docs reconciled, Handoff) are reconciled. |
| MAJOR 2 — the rule test is narrower than the rule and hides `ctrl-f` | **FIXED as a guard.** `the_only_bare_ctrl_defaults_are_the_ones_dec_0018_accepted` walks the whole `BINDABLE_ACTIONS` registry and asserts set **equality** against `{close_panel/ctrl-w, new_terminal_tab/ctrl-t, find/ctrl-f, open_settings/ctrl-,}`, so a fifth fails the build and so does dropping one. `key_bindings_actions.rs`'s header and `DEC-0018` §"What future work inherits" both restate the rule at registry scope. See F-R5 for the authority question this leaves. |
| MINOR 3 — override-vs-override collisions unhandled, module doc overstated | **FIXED.** `key_bindings/mod.rs` now states the one shape the rule covers and says two overrides on one keystroke are not resolved. |
| MINOR 4 — Reset on a displaced row is a silent no-op | **FIXED, and walked.** `displacing_action` (`key_bindings_ui.rs:290`) plus a `Warning` notification; reproduced independently — see §5. |
| MINOR 5 — the release-notes clause is NOT MET, not PARTIAL | **FIXED.** Now NOT MET with the `release.yml` reasoning. |

## 4. New findings

**F-R1 (MINOR, `US-0121`) — three prose sites claim more than the kit does.**
`appearance.rs`'s module doc, the `ThemeRow::Section` doc comment and `docs/gui-layout.md`
§Settings window all say `PopupMenuItem::label` is excluded "from clicking **and from
keyboard navigation**". The first half is true; the second is not, and the packet's own
Gap says so: `select_down` with `selected_index == None` sets index 0 without asking
whether row 0 is clickable (`popup_menu.rs:906-911`), so the first Down-arrow still
highlights the heading. Three places now assert what a fourth place refutes. Say
"excluded from clicking, and skipped by `select_up`/`select_down` once a row is selected".

**F-R2 (MAJOR, `US-0122`) — `Reset All` on the first Key Bindings page resets the two
other pages too, and the two other pages have no `Reset All` at all.** `binding_group`
attaches `on_reset(key_bindings_are_dirty, reset_all_key_bindings)` to the item whose id
is `BINDABLE_ACTIONS[0].id` (`key_bindings_ui.rs`), which is an App Menu row and therefore
only ever on page 1; `reset_all_key_bindings` (`:311-324`) writes the default back for
**every** action in the registry. The kit gates the button on
`page.resettable && any group is resettable` (`setting/page.rs:111-113`) and fires it with
no confirmation (`:181-194`). Walked, on my own build:

1. On **Key Bindings: Terminal**, Split Right was rebound to `F7` (`Default: ctrl-shift-right`
   appears). That page's header shows **no** `Reset All` — it has no resettable item.
2. **Key Bindings** was then opened. Every row on it is at its default, yet its header
   now shows the `Reset All` undo icon, because a row on another page is dirty.
3. One click on it, and Split Right on the other page is back to `Ctrl+Shift+Right`
   (`evidence/US-0122-reverify-reset-all-crossed-pages.png`), with `ui_config.json` back to
   no `key_bindings` key.

So a destructive control appears on a page whose thirteen visible rows are all clean,
silently reverts twenty-four rows the user cannot see from there, and is unreachable from
the pages that show them. The code comment at the `on_reset` call is honest about the
intent; nothing the user can see is. This is new behaviour created by the split — before
it, one page carried all thirty-seven rows and `Reset All` meant what it looked like.

**F-R3 (MINOR, `US-0122`) — the "every split page has more than one group" property holds
for one of the two tables.** `every_key_binding_page_holds_more_than_one_group`
(`key_bindings_ui.rs:394`) enforces it for `KEY_BINDING_PAGES`;
`the_terminal_groups_are_spread_over_pages_none_of_which_is_long`
(`terminal/mod.rs:97`) accepts `1..=3`, and two terminal pages — Terminal Logging and
Completion — carry one group and therefore get no sidebar sub-items. That is defensible
(a page row needs no scroll and is exact by construction, which the packet says), but the
two tables are not held to the same rule and only one of them says so. Same table, second
half: `US-0122` says "both are asserted against their own group source", which is true of
`KEY_BINDING_PAGES` (checked against `BINDABLE_ACTIONS` itself, mutation A) but not of
`TERMINAL_PAGES` — its coverage assertion is the literal `assert_eq!(total, 9)`, and there
is no enumerable source of terminal groups for it to read. A *new* group module left off
every page would keep the total at nine and pass. It is still caught, just not there: the
orphaned `group()` becomes dead code and `cargo clippy -- -D warnings` fails on it, which
is what mutation C showed. Worth saying that way round rather than claiming a source check
the code does not do.

**F-R4 (MINOR, `US-0122`) — "Completion" is now stated three times, which is the shape
`F16` filed.** Sidebar row "Completion" → page header "Completion" → group heading
"Completion" (`evidence/US-0122-28d-settings-completion.png`, reproduced here on my own
build while walking F-R6). The acceptance
clause "No page states the same name three times" is ticked `[x]` and described as swept
across every page; this page was created by the split after that sweep's reasoning was
written. Terminal Logging is one step away from the same thing ("Terminal Logging" /
"Logging"). Dropping the group title on a single-group page, or naming the page after what
it contains, fixes it.

**F-R5 (MINOR, `US-0123`) — the accepted exception set was widened from the owner's two to
four inside an already-Accepted decision.** The Status line records the owner accepting the
record "with the explicit ruling that `ctrl-w` … and `ctrl-t` … stay as they are". §"What
future work inherits" now reads "The **complete** set of bare-`Ctrl` defaults this record
accepts is therefore four", adding `ctrl-,` and `ctrl-f`, and argues `find`'s case in the
record's own voice. `ctrl-,` is harmless — the first pass's own test comment notes
punctuation carries no control character, and it is only in the list because the new
predicate `is_one_key` deliberately does not discriminate. `ctrl-f` is not: it is a real
widening of what the owner ruled on, made by the implementer, with no second acceptance
line or date. The record is transparent about it and the alternative (moving `find`) was
correctly refused as out of scope — but an Accepted decision grew a new exception without
the owner, and that is worth one sentence from them rather than silence.

**F-R6 (MINOR→MAJOR, `US-0123`) — the new notification layer is drawn *under* the page, so
every Settings toast is partly overprinted.** The layer itself is right: `SettingsPanel` is
constructed in exactly one place (`window.rs:107`, inside its own `Root`), `Root::render`
draws no notification layer of its own (`reference/gpui-kit/crates/component/src/root.rs:577-608`),
and `notification` is a per-`Root` entity (`root.rs:42`, `:108`), so **there is no
double-render** when Settings is opened from the main window and no cross-window leakage —
that part of the change is correct and I tried to break it. The layer is also the same
element the main window gets: `div().absolute().inset_0()` (`root.rs:199-208`), appended as
the last child, exactly as `OneTermWorkspace::render` does
(`crates/workspace/src/layout/workspace/mod.rs:540`, `:561`). But the page's own controls
paint **on top of** the card. Walked twice:

- `evidence/US-0123-reverify-notification-overprinted.png` — the Edit Menu rows' `Ctrl+Shift+C`
  chip and their `Edit` / `Reset` buttons are printed over the toast's third line; the
  sentence "Rebind either one to free the key." reads as overlapping text.
- The same toast, with the page switched to **Completion** while it was still up, has three
  of Completion's blue switches printed over it. So it is not specific to the Key Bindings
  page — it is any `SettingPage` content.

The frame captured 1.5 s after the click is identical to the one captured at 0.5 s, so it is
not an entrance animation. `US-0123`'s own evidence frame
(`evidence/US-0123-40e-reset-on-a-displaced-row-says-so.png`) shows the same overprint and is
presented as the fix working. **Probe D:** adding `.relative()` to `SettingsPanel`'s root —
the one styling difference from `OneTermWorkspace::render` — was built and walked and changed
nothing, so that is *not* the cause; something in the kit's `SettingPage` paints its controls
after the layer. Reported as a symptom with one candidate ruled out, not as a diagnosis.

**F-R7 (MINOR, `US-0122`) — a search that filters the current page out blanks the content
pane, and picking a result lands on the wrong page once the query is cleared.** Upstream, but
the split multiplies the exposure from six pages to twelve. `Settings::render_active_page`
(`reference/gpui-kit/crates/component/src/setting/settings.rs:145-163`) indexes
`selected_index.page_ix` into the **filtered** list, and `filtered_pages` (`:112-140`) drops
every page with no matching item. Walked:

- From **Terminal Display** (page 7 of 12) I typed `gutter`. Search does reach across pages —
  the sidebar correctly offers **Key Bindings** (Toggle Gutter) and **Terminal Display**
  (Show Gutter) — but the content pane goes **empty**, including for the page that is still
  selected (`evidence/US-0122-reverify-search-blanks-the-pane.png`).
- Clicking "Terminal Display" in that filtered list sets `page_ix = 1`; clearing the query
  then lands on **Key Bindings**, the unfiltered list's page 1.

Nothing in this round caused it and nothing in this round can fix it; it belongs in the
upstream report `US-0122`'s Handoff already carries, as a second item beside the scroll
under-shoot.

**F-R8 (trivial, process) — a code change shipped inside a docs commit.**
`SettingsPanel::render`'s notification layer (`panel.rs`, +13) landed in `dab9cac4`, whose
subject is `docs(harness): rework US-0121..US-0123 records after verification`. The change
is explained in the packet; the commit type is not.

## 5. GUI walk

Own `fast-dev` build of `dab9cac4`, own process id, scratch `HOME`, 1016×708 (the window
opens at that size by itself), `PrintWindow(hwnd, dc, 2)` + posted `WM_*` from a driver
written for this pass. Only the launched pid was addressed and only it was closed;
`target/fast-dev` was deleted afterwards.

**All twelve pages were opened and the deepest sub-item of every page with sub-items was
clicked from a freshly opened page.** Every one landed with its group title on screen.

| Page | Deepest sub-item | Result |
| --- | --- | --- |
| General | Shell (3 of 3) | **MET**, page fits, no scroll needed |
| Key Bindings | Edit Menu (2 of 2) | **MET**, heading + all five rows |
| Key Bindings: Terminal | Input Channel (2 of 2) | **MET**, heading + all seven rows |
| Key Bindings: Sessions | SFTP Context Menu (2 of 2) | **MET**, heading + all nine rows |
| Terminal | Cursor (2 of 2) | **MET**, whole Cursor group |
| Terminal Display | Bell (3 of 3) | **MET**, framed: `evidence/US-0122-reverify-terminal-display-bell.png` |
| Mouse & Clipboard | Security (2 of 2) | **MET**, page fits entirely |
| SSH | SFTP Edit Limit (3 of 3) | **MET**, whole group |
| About | Updates (3 of 3) | **MET**, framed: `evidence/US-0122-reverify-about-updates.png` — Check Now and Update Status included |
| Terminal Logging, Completion, Network | — | one group each, no sub-items; the page row is the navigation and it scrolls nothing |

Other scenes:

| Scene | Frame | Result |
| --- | --- | --- |
| Sidebar, six pages expanded | `evidence/US-0122-reverify-sidebar-all-expanded.png` | Reproduces the packet's frame exactly: overflows, scrollbar, cut at "Scroll". The "34 rows" claim checks out as arithmetic — 12 page rows + 22 titled groups. |
| Theme picker open | `evidence/US-0121-reverify-theme-dropdown.png` | "Dark themes" muted and disabled, `✓ Zed One Dark` the first selectable row, tick on screen, no scrolling. |
| Click the "Dark themes" heading | — | **Nothing happens at all**: the menu stays open, no highlight, field unchanged. Strictly better than the first pass, where the click dismissed the menu. |
| Select "Adventure" | — | `target/ui_config.json` becomes `{"ui_font_size":16.0,"theme_name":"Adventure","right_dock_mode":"ssh_client","schema_version":1}`. **The persisted shape is unchanged** — same four keys, same `schema_version: 1`, no new field anywhere. The picker still reaches persistence through `Theme::global_mut(cx).apply_config(...)` and `UiConfig::observe_theme`, exactly as the dropdown did. |
| Search reaches across pages | `evidence/US-0122-reverify-search-blanks-the-pane.png` | **Yes** — and see F-R7 for the half that does not work. |
| `Reset All` across pages | `evidence/US-0122-reverify-reset-all-crossed-pages.png` | See F-R2. |
| Reset on a displaced row | `evidence/US-0123-reverify-notification-overprinted.png` | The toast appears — *"Key already taken — New SSH Session is left unbound: its default is your own binding for Quit. Rebind either one to free the key."* — and `app-stderr.log` carries one `warn` per resolution (startup, then the reset), not a repeat. Overprinted; see F-R6. |
| App menu on a seeded profile | — | **About F1**, **Quit Ctrl+Shift+Q**, **Settings Ctrl+,** — `US-0123`'s table reaching the running application, reproduced on my own build. |

## 6. Gaps in this re-verification

- **No real-keyboard proof, same ceiling as both previous passes.** Posted `WM_*` messages
  set no modifier state, so no `Ctrl` chord and no `F1` was delivered. `DEC-0018`'s first
  Consequence and the `f1` cost remain traced from the gpui dispatch order, not pressed.
- **F-R6 is reported as a symptom, not diagnosed.** I ruled out the entrance animation and
  ruled out the missing `.relative()` by building and walking the change; I did not find
  what does paint over the layer, and did not read gpui's paint ordering far enough to say.
- **The sidebar-overflow frame expands six pages, not nine.** Expanding the pages below the
  fold needs the sidebar scrolled first; the visible state is identical to the packet's
  frame and the 34-row total is arithmetic over the page/group table, not counted on screen.
- **`cargo test --workspace` was not run separately**; `ci-local` runs it and its line is in
  §1.
- **F-R2's scope was walked with one rebound row, not thirty-seven.** That one row lives on
  a different page from the button, which is the whole claim; I did not enumerate every row.
