# BUG-0068, BUG-0069, US-0118, US-0119, US-0120 — independent verification

Verifier: adversarial second party. Nothing here was written by the implementer.
Target: `f4ea1765` (7 commits over `main` @ `ffc02c19`), `crates/session-ui`,
`crates/state/src/form_dialog.rs`, two lines in `crates/app`, four docs.
Method: every kit claim re-checked against the **registry copy of the dependency**
(`~/.cargo/registry/src/index.crates.io-*/gpui-component-0.6.0`, `gpui-base-0.6.0`,
`gpui-pre-0.3.3`), not only against `reference/gpui-kit`. Every decision function was
mutated and the tests re-run. A GUI walk was driven against a build of this commit, with
a seeded store containing the case the packets never captured (a Private Key session with
three port forwards and agent forwarding on), plus a successful connect against the
repository's loopback `sftp-dev-server`.

## Verdicts

| Packet | Verdict |
|---|---|
| `BUG-0068` | **PASS** |
| `BUG-0069` | **PASS with findings** — outcome met and root cause proven, but one sibling with the identical cause is unfixed and unrecorded, and the Gaps list names two crates that have no such control. |
| `US-0119` | **PASS with findings** — every acceptance line holds in the walk except the wording one. |
| `US-0118` | **FAIL** — the Enter-to-create fix leaves the combobox in a state the user can see is wrong, and can create a group the user never typed. Everything else in the packet is correct and now has the success-path frame it was missing. |
| `US-0120` | **FAIL** — the disclosure removes the only keyboard route to three existing fields, and can hide the field a refused Save is about. The scroll, the cap, the expanded-when-configured rule and the colour row are all correct. |
| **Overall** | **FAIL** — two packets need rework. The failures are in the two new interaction surfaces (the combobox commit and the Advanced disclosure), not in the mechanisms the packets were mostly about. |

Both FAILs are acceptance rework of their own packets (`harness story reopen`), not new BUGs:
the behaviour was never accepted.

---

## Claims re-checked

### BUG-0068 — an error names itself once

| Claim | Verdict |
|---|---|
| `connect_failure_message` decides per variant, one place | **True.** `crates/session-ui/src/common.rs:102-108`. `Connect` / `HostKeyUnknown` / `HostKeyChanged` verbatim, everything else prefixed. |
| Toast and inline error share one value | **True.** `crates/session-ui/src/common.rs:522-530`: one `SharedString` is handed to `on_failed` and to `push_notification`. |
| Two unit tests pin the shape | **True and they bite.** See Mutations. |
| "No other double prefix in `crates/session-ui`" | **Re-grepped, true.** The only other error interpolations are `auth_form.rs:303,307` (`std::io::Error`), `forward_rows.rs:146` (a parse error), `connect_dialog.rs:259` / `session_dialog.rs:411` (`UserHostPortError` / `JumpChainError`) and `common.rs:354` (a bare `&str`); none carries a subject of its own. |
| `crates/ssh` adds no second prefix | **True.** The only re-wrap is `crates/ssh/src/route.rs:207-214` (`hop_error`), which prefixes the **message**, giving `SSH connect failed: jump host <label>: …`. Correct. |
| Each variant's final text reads well | **True.** Live frame `US-0118-verify-16c` / `BUG-0068-verify-16`: `SSH connect failed: timed out after 20 s`, once, in the inline block and in the toast. |
| The `error-policy.md` edit is right | **True.** `docs/agents/error-policy.md:21-25`, in the runtime-rules list, states the rule generally and names the per-variant remedy. |

Findings (all minor):

- **B68-m1.** `docs/ssh-client-connect.md:1109-1115` says the verbatim variants "already begin with
  `SSH`". `AppError::HostKeyUnknown` renders `Unknown SSH host key for {host}:{port} …`
  (`crates/core/src/error.rs:88-91`) — it does not begin with `SSH`. The behaviour is right
  (it names SSH and the host); the stated reason is wrong. The rustdoc on
  `connect_failure_message` gets this right ("name SSH and the host"); only the doc overstates.
- **B68-m2.** The `HostKeyUnknown` arm is unreachable from the only caller:
  `crates/session-ui/src/common.rs:500-516` matches `Err(AppError::HostKeyUnknown { .. })`
  first and opens the host-key dialog. Confirmed live — the loopback connect produced the
  **Unknown SSH Host Key** dialog, not a toast. Harmless, but the doc enumerates a path that
  cannot be taken.
- **B68-m3.** Evidence hygiene: `evidence/BUG-0068-16-connect-failed.png`,
  `evidence/BUG-0068-17b-connect-timeout-22s.png` and `evidence/US-0118-16-connect-failed.png`
  are **byte-identical** (`md5 d9a262a1bf9afeb4a4faecc8e2dc4492`). One capture is presented as
  three frames, and the "22 s" in the name is unsupported: the toast in that image reads
  "timed out after 20 s". Nothing is misrepresented about the fix — the frame does show both
  the toast and the dialog — but two of the three filenames claim captures that were not taken.

### BUG-0069 — checkbox and radio labels keep their descenders

| Claim | Verdict |
|---|---|
| The kit wraps `.label(...)` in `line_height(relative(1.))` | **True, in the dependency itself.** `gpui-component-0.6.0/src/checkbox.rs:329` and `src/radio.rs:245` — exactly the cited lines, byte-checked in `~/.cargo/registry`, not only in `reference/`. |
| Children go into a taller box | **True.** Both controls put `children` inside a `v_flex().line_height(relative(1.2))` (`checkbox.rs:316-336`, `radio.rs:236-253`); `control_label` adds `relative(1.5)` on top. |
| The accessibility name is kept | **True, and this is the non-obvious part the packet got right.** `.label(...)` used to *derive* the screen-reader name (`checkbox.rs:213-215`, `radio.rs:165-167`: `self.accessibility_label.or_else(|| self.label…)`), so moving the text to a child would have dropped it. Every one of the five converted sites passes `.accessibility_label(...)` explicitly: `auth_form.rs:121-123`, `connect_dialog.rs:197-199`, `quick_connect_dialog.rs:121-123`, `session_dialog.rs:231-233`, `session_dialog.rs:502-504`. |
| The finding's diagnosis was wrong; "Logging" was never clipped | **True.** Measured independently on `evidence/BUG-0069-11c-before-above-after.png`: in the *before* half "Logging", "Group" and "Port forwards" all carry full descenders while "agent" and "Use global" end flat at the baseline; in the *after* half the two controls gain their tails and the plain labels are unchanged. |
| Descenders whole in the shipped build | **True.** Live frame `US-0120-verify-12b`: "Use global" renders its `g` whole. |
| `crates/session-ui` fully converted | **True.** No `Checkbox`/`Radio` in the crate still calls `.label(...)`. |

Findings:

- **B69-MAJOR-1. The same root cause is unfixed on `Button`, in the same dialogs, and is not
  in Gaps.** `Button::label` wraps its text in the identical `line_height(relative(1.))` —
  `gpui-component-0.6.0/src/button/button.rs:679-687`. The Connect button's in-flight label is
  "Connectin**g**". Captured at 6x from my own walk of this commit:
  `evidence/BUG-0069-verify-15-connecting-button-6x.png` — the `g` is cut flat, zero descender
  rows. The same defect is in the implementer's own after-frame
  `evidence/US-0118-15-connect-inflight.png`, and in the before-frame
  `research/before/15-connect-inflight.png`, so it is pre-existing, not a regression. It is
  nonetheless **in scope**: the packet's Scope says "Any sibling that shares the same cause, if
  the cause turns out not to be local to this block", and its Risks section names exactly this
  failure mode. The Gaps section lists only checkboxes and radios, which makes the record read
  as complete when it is not. Other `Button::label` sites in the same crate that can hold a
  descender: the combobox footer's `Create "<typed text>"` (`group_combo.rs:243`), which takes
  arbitrary user text. Remedy is a `control_label`-shaped child on `Button` too, or one
  sentence in Gaps naming `button.rs:685`.
- **B69-m2.** Gaps says checkboxes and radios "outside `crates/session-ui` still use
  `.label(...)`… settings-ui, sftp-ui and terminal-view". `crates/settings-ui` and
  `crates/terminal-view` contain **no** `Checkbox`, `Radio`, `RadioGroup` or `Switch` at all.
  The only remaining sites are `crates/sftp-ui/src/render.rs:276` and
  `crates/sftp-ui/src/edit.rs:588`. Over-claiming a gap is the safe direction, but it is still
  inaccurate.
- **B69-m3.** `CONTROL_LABEL_LINE_HEIGHT = 1.5` and the rustdoc's "about 1.35 em for ascent
  plus descent" are pinned by nothing — no test, no check. A future value change is silent.

### US-0119 — add from the header, disclose with a chevron, separate Delete

Walked on a seeded store (two ungrouped sessions, one Private Key session, an `infra` group of
two) in this worktree's own `target/`.

| Claim | Verdict |
|---|---|
| Menu is Open / Properties / — / New Session / — / Delete, danger | **True.** `evidence/US-0119-verify-10-session-context-menu.png`. |
| Header `+` opens the **full** session dialog | **True.** `evidence/US-0119-verify-53-new-session-from-header.png` — "New SSH Session", not quick connect, Advanced collapsed. |
| Blank-area right-click offers New Session | **True.** `evidence/US-0119-verify-52-blank-area-menu.png`. |
| Right-click on a group row | **True.** `evidence/US-0119-verify-57-group-menu.png` — `Rename Group…` / — / `New Session`, and **no** stray blank-area menu. |
| The container/row flag survives fast right-clicks | **True, probed and did not reproduce.** Two `WM_RBUTTONDOWN/UP` pairs posted back-to-back with no wait produced only the row menu. The mechanism is sound by construction: `ContextMenu::paint` builds the menu inside `window.defer` (`gpui-component-0.6.0/src/menu/context_menu.rs:303`), so the row's bubble-phase `on_mouse_down` always runs first within the same dispatch; both hitboxes are `HitboxBehavior::Normal` (`context_menu.rs:248`), so both handlers fire; an empty `PopupMenu` renders nothing (`context_menu.rs:176-181`). The flag is a `bool` rather than a counter, so two right-clicks whose deferred builders both ran after both flag sets would leak one blank-area menu — I could not make that happen. |
| Row menus never fire below the last row | **True by construction.** The kit attaches the row menu to a per-row `div()` (`gpui-component-0.6.0/src/tree.rs:91-103`), whose hitbox is the row. |
| Delete confirms, from the menu | **True.** `evidence/US-0119-verify-10b-delete-confirm.png` — "Delete the saved SSH session "DevServer"? This cannot be undone.", danger confirm. Cancel left `ssh_session.json` byte-identical. |
| Delete confirms, from the key binding | **Same function** (`panel.rs:245-247` → `panel::confirm_delete_session:53`), so it cannot diverge — but it is **not walkable**: `DeleteSession` has `default: None` (`crates/settings-ui/src/key_bindings/key_bindings_actions.rs:298-306`), so there is no key to press. Verified by code, not by frame. |
| Chevron for groups, maximise arrow gone | **True.** `IconName::ChevronDown`/`ChevronRight` (`tree_render.rs:78-85`); no `Maximize`/`Minimize` remains anywhere in `crates/session-ui`. |
| §6.5 accuracy | **Accurate**, including the four-surface "one dialog" claim: the centre tab bar's `+` already dispatches `NewSession` (`crates/terminal-view/src/panel/terminal_panel.rs:716`), which routes to `open_session_dialog` (`panel.rs:209-213`). |

Findings:

- **U119-m1. The wording acceptance is not met.** Acceptance: *""Property" reads as a proper
  label … and the same wording is used **wherever else that action appears**."* Three sites
  still say "Property":
  - `crates/settings-ui/src/key_bindings/key_bindings_actions.rs:309` — `label: "Session
    Property"`, the user-visible row on the Keybindings settings page, in a group literally
    called "Session Tabs Context Menu".
  - `crates/session-ui/src/tree_render.rs:58` — rustdoc still says "(Open/Delete/Property)", in
    the file this packet rewrote.
  - `docs/ssh-client-connect.md` §1.3 decision 8 — "right-click keeps the context menu
    (Open/Delete/Property)", in a document this packet edited three rows above.
- **U119-m2. Two identical context menus on an empty list.** `render.rs:73` (pre-existing, on
  the empty-state element) and `render.rs:126` (new, on the list container) both build a
  one-row "New Session" menu, and with no sessions both fire for the same right-click — the
  flag is false because no row was hit. They anchor at the same point so the user sees one
  menu, but one of them is now redundant and should go.
- **U119-m3.** §6.5 says Delete "is styled destructive … This matches the SFTP browser's
  delete". Walked against the loopback server: the SFTP browser's own Delete row is **not** red —
  its menu renders Edit / Download / Rename / Delete / Properties / Upload Files / Upload Folder /
  New Folder / Refresh all in the plain foreground. What SFTP matches is the danger **confirm button**,
  which is what the packet's Evidence section says correctly. §6.5 overstates it, and the two
  menus are now inconsistent with each other.

### US-0118 — a failed connect keeps the form

| Claim | Verdict |
|---|---|
| `take_auth` no longer clears | **True.** `auth_form.rs:210-246`; `take_hops` likewise `jump_hops.rs:137-141`. |
| The password survives a failure and Connect is re-enabled | **True, walked.** `evidence/BUG-0068-verify-16-connect-failed.png` — this is the **Connect** dialog (saved session), a path the implementer never captured; they walked quick connect only. |
| The secret reaches no store, no config file, no log | **True, and provable more strongly than the packet's file hash.** `SshSession` has no password field; `SecretString` is not `Serialize` and its `Debug` prints `***` (`crates/core/src/ssh_config.rs:40-44`). Independently: after a failed attempt with a typed password, none of the seven `target/*.json` files contains it, `ssh_session.json` is byte-identical to the seed, and the owner's `~/.OneTerm` was not touched. |
| The inline error clears on edit | **True, walked.** `evidence/US-0118-verify-16c-error-cleared-on-edit.png` — one character typed, error gone, password kept and extended. |
| A successful connect closes the dialog | **True — gap closed.** The packet recorded "a successful connect was not walked". I walked it against the loopback `sftp-dev-server` on `127.0.0.1:2222`: host-key prompt → Trust and Connect → dialog closed, tab opened, `Connected to "Loop"`, SFTP listed the served file. `evidence/US-0118-verify-success-connect-loopback.png`. |
| The capture-phase rationale | **True, verbatim.** `gpui-pre-0.3.3/src/window.rs:6138` — `cx.propagate_event = false; // Actions stop propagation by default during the bubble phase`; the capture loop at `window.rs:6105-6126` does not reset it and runs root-first. The wrapper `div` is a dispatch ancestor of the popup: the popup is a `deferred` **child** of the combobox's own div (`gpui-component-0.6.0/src/combobox.rs:688-700`), and `deferred` changes paint order, not parentage. |
| `match_count` is read after a synchronous search | **True, and this was the one way it could have been wrong.** `SearchableVec::perform_search` filters inline and returns `Task::ready(())` (`gpui-component-0.6.0/src/searchable_list/vec.rs:130-139`), so `items_count(0)` immediately after is the post-search count. |
| Enter creates, selects and closes; Enter does not submit the dialog | **True, walked.** `evidence/US-0118-verify-56b-group-created-on-enter.png` — trigger reads "Lab", dropdown closed, dialog still open. |
| The no-match area carries text | **True.** `No group matches "Lab". Press Enter to create it.` |
| CORR-54 stands and the dialog says so | **True.** `unsaved_note` is a pure function of the tick; the frame shows the two-line block. |
| DEC-0001 / §1.3 decision 1 and 7 texts | **Truthful.** Decision 1 now says the secret survives in the open modal's state and reaches no store; decision 7 now says the dialog closes *on success* and stays open on failure. Both match the code and the walk. |

Findings:

- **U118-MAJOR-1. The combobox is left in a visibly inconsistent state, and Enter can create a
  group the user never typed.** `group_combo.rs:152-155` clears the packet's own `query_cell`
  after creating, but nothing clears the kit's search input. The two then disagree. Walked, in
  order:
  1. Type "Lab", Enter → "Lab" created, dropdown closes. Correct.
  2. Reopen the dropdown, type nothing:
     `evidence/US-0118-verify-56c-combobox-stale-query.png` — the search box still shows the
     spent query, the empty area reads **"No groups yet. Type a name to create one."** although
     `infra` exists (the list is empty only because of the stale filter), and the footer reads
     the disabled **"Type to create new group"** although text is visibly in the box. Three
     surfaces, three different stories.
  3. Reopen and type "inf" (intending the existing `infra`): the text **appends** to the stale
     query, giving `Labinf`; the footer offers `Create "Labinf"`; Enter creates a group called
     **"Labinf"**, which the user never typed and cannot have wanted.
  Step 3 is the part that matters: before this packet Enter did nothing, so the same stale
  query could only produce a group the user clicked on and could read in full. The new Enter
  path turns it into a silent one-keystroke mistake. This is `F22`'s third symptom
  ("after creating, the dropdown stays open showing `Create "Lab"` again") half-fixed: the
  dropdown now closes, but reopening restores the offer. Remedy is to clear the kit's search
  input as well when Enter commits — e.g. re-running `perform_search("")` through the state —
  rather than clearing a private copy of the query.
- **U118-m2.** The inline error does not clear when a **jump host's** credential is edited:
  `InlineError::new` is given only the auth form's two secrets plus username/host/port
  (`connect_dialog.rs:102-106`, `quick_connect_dialog.rs:241-249`); `JumpHopForms`' own inputs
  are not watched. A corrected jump-host password stands beside a stale error.
- **U118-m3.** The inline error is unavailable on the host-key path. `AppError::HostKeyUnknown`
  closes the dialog before prompting (`common.rs:500-516`), so a failure on the *retried*
  connect sets an `InlineError` belonging to a dialog that no longer exists; only the toast
  survives. Observed in the walk (the dialog was gone behind the host-key prompt). Acceptable,
  but neither the packet nor §4.7 says so.
- **U118-m4.** The acceptance line "a successful connect still **clears the password** and
  closes the dialog" is met only in the weaker sense the packet states: nothing clears the
  `InputState`; the dialog closes and the entity is dropped. Now demonstrated by a frame rather
  than by argument.

### US-0120 — the form scrolls, folds, and shortens the colour row

| Claim | Verdict |
|---|---|
| `form_body_max_height` = window − 260, floored at 240 | **True**, and the test pins window-following, the floor and monotonicity (`form_dialog.rs:302-324`). |
| No scrollbar when the body fits | **True, and the cited reason is exact.** `gpui-base-0.6.0/src/scrollbar.rs:1293`: `if scroll_area_size <= container_size { … continue; }` — the theme's always-visible mode makes a bar *visible*, not *present*. Walked: collapsed Advanced, no bar; expanded, bar. |
| The footer never scrolls away | **True.** It is a sibling of the capped box in the kit's own layout. |
| The cap on a 700 px-tall window | **True and exact.** Measured on `evidence/US-0120-verify-12b-privatekey-700-scrolled.png`: body spans y≈105→545 = **440 px** = 700 − 260, footer at y≈574, dialog bottom ≈605. Save reachable. |
| The acceptance scenario itself | **Holds — but the packet never captured it.** Acceptance: *"In a 1000 px-tall window with **Private Key selected and three port forwards configured**, Cancel and Save are both visible and clickable without resizing."* `evidence/US-0120-12-session-dialog-scrolled.png`, labelled **the acceptance frame**, shows **Password** selected and **zero** port forwards. I seeded that exact session and walked it: `evidence/US-0120-verify-12-privatekey-3-forwards-1000.png` (1000 px) and `…-12b-privatekey-700-scrolled.png` (700 px, scrolled to the Logging row). It passes at both heights. The mechanism is fine; the evidence was not of the stated case. |
| Advanced expanded when configured | **True, on real data.** The seeded session (jump host none, agent forwarding on, three forwards) opened with Advanced **expanded**; a new session from the header `+` opened **collapsed**. |
| A swatch writes through the same `ColorPickerState` | **True, walked.** Clicking swatch 2 and reopening the picker showed `#E06C75` in its hex field (`evidence/US-0120-verify-54b-swatch-writes-picker-state.png`). `to_hex()` emits `#RRGGBB` (or `#RRGGBBAA` when `a < 1`), and `Hsla::parse_hex` accepts lengths 6 and 8 (`gpui-component-0.6.0/src/theme/color.rs:269-296`), so `session_color_hex` accepts every value the row can produce. |
| The four SFTP `FormDialog` dialogs still render | **Partly closed.** With the loopback server up I opened the SFTP **Rename** dialog: natural height, no scrollbar, footer in place. The other three were not reached. |

Findings:

- **U120-MAJOR-1. The Advanced disclosure is not keyboard-reachable, so three existing fields
  lost their only keyboard route.** `advanced_header` is
  `h_flex().id("advanced-disclosure")…​.on_click(…)` (`session_dialog.rs:140-155`) — no
  `track_focus`, no `tab_stop`, no role. Walked with Tab from the Username field: the order runs
  Username → Password/Private Key/SSH Agent radios → Private Key path → **Browse** → **Group
  combobox**. "Advanced" is skipped entirely
  (`evidence/US-0120-verify-tab-skips-advanced.png`). A keyboard-only user can no longer reach
  Jump host, agent forwarding or port forwards **at all**; before this packet they were plain
  Tab stops. The same applies to the eight colour swatches, which are
  `div().id(…).on_click(…)` with no tab stop and no accessibility label
  (`session_dialog.rs:180-203`) — there the `ColorPicker` button remains as a keyboard route,
  so that half is a downgrade rather than a removal.
- **U120-MAJOR-2. A collapsed Advanced can hide the field a refused Save is about.** `submit`
  calls `forward_rows.take(cx)` and `jump_chain(...)` unconditionally
  (`session_dialog.rs:400-414`), while the rows render only when expanded
  (`session_dialog.rs:496-518`), and nothing expands the disclosure on a validation error.
  Reproduced: expanded Advanced, set the first forward's target port to `0`, collapsed
  Advanced, pressed Save →
  `evidence/US-0120-verify-advanced-hides-the-invalid-field.png`: the dialog stays open, the
  toast reads *"Port forward L 127.0.0.1:15432 -> db.internal:0: the target port must be
  1..65535."*, and the offending row is behind a collapsed `> Advanced`. No data is lost (Save
  is refused and the values survive collapse), but the user is blocked with the cause off
  screen. Remedy is one line: expand the disclosure when `forward_rows.take` or `jump_chain`
  fails.
- **U120-m3. "Custom…" is inert.** `session_dialog.rs:203` renders it as a plain
  `div().text_xs()` with no id and no click handler. Clicking the word does nothing — walked
  (`w25`, no picker opened); only the adjacent unlabelled `ColorPicker` square opens it. The
  packet and `docs/ssh-client-connect.md` §6.6 both call it "a 'Custom…' **square**", which is
  not what is drawn: the row reads as **nine** swatches, the ninth being the picker's
  current-value square — visually identical to swatch 1 whenever the default colour is selected
  (`evidence/US-0120-verify-54-colour-row-4x.png`).
- **U120-m4.** The packet says of the swatch set "no colour is hard-coded here". The first entry
  is `Hsla::parse_hex(SshSession::DEFAULT_COLOR_HEX)` — a hard-coded `#56B6C2`
  (`session_state.rs:206`). The set is also **not** the picker's featured row: the kit's default
  featured row is twelve colours — red, red_light, blue, blue_light, green, green_light, yellow,
  yellow_light, cyan, cyan_light, magenta, magenta_light
  (`gpui-component-0.6.0/src/color_picker.rs:206-219`) — while the short row is eight and omits
  five of them. Visible side by side in `…-54b`.
- **U120-m5.** `DIALOG_CHROME_HEIGHT = 260` is about 170 px more than the chrome actually needs.
  Measured on `evidence/US-0120-11b-advanced-expanded.png` in a 1000 px window: dialog occupies
  y≈105→935, of which 740 is body — 830 used, 170 free. Conservative, therefore safe; the
  packet's own gap admits the constant is tuned rather than measured.
- **U120-m6.** `evidence/US-0120-11-session-property-dialog.png` and
  `evidence/US-0120-54-session-color-row.png` are byte-identical
  (`md5 7276b766ce7c6772fb4f4227fb23629b`). Defensible — the colour row is in that dialog — but
  scene 54 was not separately captured; the 4x crop `54c` is the only real colour-row evidence.

---

## Mutations (four decision functions, all caught, all restored)

| Mutation | Test result |
|---|---|
| `connect_failure_message`: drop the `AppError::Connect` arm so it falls to the prefixing branch | `a_connect_error_names_the_failure_once` **FAILED** — `left: "SSH connect failed: SSH connect failed: timed out after 20 s"`. |
| `SESSION_MENU_ROWS`: move `NewSession` back to the first slot | `the_session_menu_leads_with_the_session_and_ends_with_delete` **FAILED** — `left: [NewSession, Open, Properties, Delete]`. |
| `form_body_max_height`: `.max(MIN_BODY_HEIGHT)` → `.min(...)` | `the_body_cap_follows_the_window_and_has_a_floor` **FAILED** — `left: 240px right: 740px`. |
| `advanced_is_configured`: drop the `!port_forwards.is_empty()` term | `advanced_opens_only_when_the_session_already_uses_it` **FAILED** — `assertion failed: advanced_is_configured(None, &[forward], false)`. |

`git status` clean after restoring all four.

## Commands

```
cargo test -p oneterm-session-ui
  test result: ok. 69 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
cargo test -p oneterm-state
  test result: ok. 38 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
pwsh scripts/ci-local.ps1
  ci-local: all checks passed.
```

The six new/changed unit tests all ran and all pass:
`common::tests::a_connect_error_names_the_failure_once`,
`common::tests::an_error_without_a_subject_still_gets_one`,
`group_combo::tests::enter_creates_only_a_typed_name_the_list_cannot_offer`,
`session_dialog::tests::advanced_opens_only_when_the_session_already_uses_it`,
`tree_render::tests::the_session_menu_leads_with_the_session_and_ends_with_delete`,
`tree_render::tests::delete_is_destructive_and_stands_behind_a_separator`,
plus `form_dialog::tests::the_body_cap_follows_the_window_and_has_a_floor` in `oneterm-state`.

## GUI walk

Own build of this commit, own pid, driven entirely by posted `WM_*` messages to that window
(the owner was using the mouse; `SetCursorPos` was abandoned after it was observed to be
overridden within 300 ms, and no synthetic global input was used afterwards). Captures by
`PrintWindow(PW_RENDERFULLCONTENT)`. Config directory was this worktree's own `target/`
(`oneterm_core::config_dir()` is `"target"` in a debug build); `~/.OneTerm` was never written.
Store seeded with five sessions including `KeyBox` — Private Key, agent forwarding on, three
port forwards — the case the packets never captured. A successful connect was walked against
`cargo run -p oneterm-tools --bin sftp-dev-server -- --port 2222 --root <scratch>`.

Frames, all taken by the verifier:

| File | Shows |
|---|---|
| `BUG-0068-verify-16-connect-failed.png` | Connect dialog after a 20 s timeout: password kept, Connect re-enabled, single-prefix inline error. |
| `BUG-0069-verify-15-connecting-button-6x.png` | `Button::label` "Connecting" with the `g` cut flat — the unfixed sibling. |
| `US-0118-verify-16c-error-cleared-on-edit.png` | One keystroke clears the inline error. |
| `US-0118-verify-56b-group-created-on-enter.png` | Enter creates and selects "Lab", dropdown closed, dialog not submitted. |
| `US-0118-verify-56c-combobox-stale-query.png` | The three-way disagreement: stale search text, "No groups yet", disabled footer. |
| `US-0118-verify-success-connect-loopback.png` | Successful connect: dialog closed, tab open, toast, SFTP listing. |
| `US-0119-verify-10-session-context-menu.png` | The reordered, separated, danger-styled menu — only one menu. |
| `US-0119-verify-10b-delete-confirm.png` | Confirmation from the context menu. |
| `US-0119-verify-52-blank-area-menu.png` | Blank-area "New Session". |
| `US-0119-verify-57-group-menu.png` | Group row: Rename Group… / New Session, no stray menu. |
| `US-0119-verify-53-new-session-from-header.png` | Header `+` opens the full dialog, Advanced collapsed. |
| `US-0120-verify-12-privatekey-3-forwards-1000.png` | The real acceptance case at 1000 px, Advanced auto-expanded. |
| `US-0120-verify-12b-privatekey-700-scrolled.png` | Same session at 700 px, scrolled to Logging, Save on screen. |
| `US-0120-verify-54-colour-row-4x.png` | The colour row at 4x: eight swatches, the picker square, the inert "Custom…". |
| `US-0120-verify-54b-swatch-writes-picker-state.png` | A swatch click reflected in the picker's hex field. |
| `US-0120-verify-advanced-hides-the-invalid-field.png` | Save refused for a field behind a collapsed Advanced. |
| `US-0120-verify-tab-skips-advanced.png` | Tab order jumps Browse → Group; Advanced never focused. |

## Gaps in this verification

- **No fast-dev build.** The walk used the `dev` profile binary this worktree's gate had already
  built, to avoid a second full dependency compile and the disk it needs. Layout, menus and text
  rendering are profile-independent; frame rate is not exercised by any of these packets. No
  `target/fast-dev` was created, so none was deleted.
- **Three of the four SFTP `FormDialog` dialogs were still not opened** (new-folder, edit,
  transfer-replace). Rename was, and it is the same code path.
- **The `DeleteSession` key binding was not exercised** — it has no default binding, so there is
  no key to press. Both surfaces provably call one function.
- **Screen-reader output was not verified.** `accessibility_label` is confirmed to be set and to
  be what the kit forwards; nothing here reads it back from an AT client.
- **The `row_was_right_clicked` bool-vs-counter race was probed and not reproduced**, not proven
  impossible.
- **`BUG-0069`'s 20 px UI-font re-check was not repeated**; the implementer's method
  (`ui_font_size` in `target/ui_config.json`) is a real key (`crates/settings/src/ui_config.rs:39`)
  and the Appearance page indeed offers no font-size control, so that gap is honestly stated.

---

# Re-verification of `40fc78d2` — 2026-09-17

Second, independent pass over the rework (`104d5504`, then `main` @ `23d0fc15` merged in as
`40fc78d2`). Nothing below was written by the implementer or by the first verifier. Every kit
claim was re-checked against the **registry copy of the dependency**
(`~/.cargo/registry/src/index.crates.io-*/gpui-component-0.6.0`, `gpui-base-0.6.0`,
`gpui-pre-0.3.3`), every decision function named in the rework was mutated and the tests re-run,
and the descender claim was **re-measured pixel by pixel** from the packets' own frames rather
than read off them.

## Verdicts

| Packet | Verdict |
|---|---|
| `BUG-0068` | **PASS** — all three minors addressed; §9.2 now matches the code. |
| `BUG-0069` | **PASS with findings** — the original checkbox/radio outcome holds, but `B69-MAJOR-1` was a **false finding** and its remedy is inert; the packet now records a measurement its own frames contradict, and the conversion costs the label's ellipsis. |
| `US-0118` | **PASS with findings** — `M1` is fixed at the source and proven; `U118-m2` is fixed on two of the three paths and still open on the third. |
| `US-0119` | **PASS** — every minor addressed; no user-visible "Property" left. |
| `US-0120` | **PASS with findings** — `M2` and `M3` are both properly fixed and mutation-tested; two doc sentences contradict the packet's own rework text, and `U120-m3`/`U120-m4` are half-closed. |
| **Overall** | **PASS with findings** — no packet needs another code rework. One real code gap remains (`U118-m2` on the Quick Connect duplicate path) plus documentation corrections. |

## The first report's findings, re-checked

| Finding | Status |
|---|---|
| **M1** `US-0118` Enter left the kit's search input stale ("Lab" + "inf" → "Labinf") | **FIXED, at the source.** |
| **M2** `US-0120` the Advanced disclosure had no tab stop | **FIXED.** Space-only is correct; see below. |
| **M3** `US-0120` Save validated hidden advanced fields without expanding | **FIXED**, and the decision is mutation-tested. |
| **M4** `BUG-0069` `Button::label` still clipped "Connecting" | **The finding itself was wrong.** The remedy is harmless but inert. |
| **B68-m1 / m2** §9.2 wording and the unreachable `HostKeyUnknown` arm | **FIXED** (`docs/ssh-client-connect.md` §9.2). |
| **B68-m3** three filenames, one capture | **FIXED** — two PNGs deleted; see `R-m3` for the reference they left behind. |
| **B69-m2** wrong crate names in Gaps | **FIXED.** Grep over `crates/` confirms the only remaining `Checkbox`/`Radio`/`Switch` `.label(...)` sites are `crates/sftp-ui/src/render.rs:276` and `crates/sftp-ui/src/edit.rs:588`. |
| **B69-m3** `CONTROL_LABEL_LINE_HEIGHT` pinned by nothing | Recorded in Gaps, not fixed. Accepted. |
| **U119-m1** "Property" → "Properties" | **FIXED**, all three sites. |
| **U119-m2** duplicate empty-list menu | **FIXED.** |
| **U119-m3** §6.5 overstated what SFTP matches | **FIXED.** |
| **U118-m2** inline error on jump-hop edits | **PARTLY FIXED** — see `R-M1`. |
| **U118-m3 / m4** host-key path, "clears the password" | Recorded in §4.7 and in the packet. Accepted. |
| **U120-m3** inert "Custom…" | **PARTLY FIXED** — see `R-m5`. |
| **U120-m4** swatch set source | **FIXED in code**, unverified by any frame — see `R-m6`. |
| **U120-m5** `DIALOG_CHROME_HEIGHT` | Unchanged, recorded. Accepted. |
| **U120-m6 / the acceptance frame** | **FIXED** — `evidence/US-0120-rw-12-privatekey-3-forwards-1000.png` shows KeyBox (Private Key, agent forwarding, three forwards) at 1000 px with Advanced auto-expanded and Save on screen. |

### M1 — the combobox commit

Mechanism re-derived from the dependency, not from the packet:

- `ComboboxState::set_query` exists — `gpui-component-0.6.0/src/combobox.rs:396`. It delegates to
  `List::set_query` (`src/list/list.rs:215-223`), which calls `input.set_value(...)` **and then**
  `start_search(...)` with the comment *"`set_value` does not emit `InputEvent::Change`, so start
  the search here."*
- `start_search` calls `self.delegate.perform_search(&query, window, cx)` **synchronously**
  (`list.rs:291`), so by the time `set_query` returns, `GroupComboDelegate::perform_search`
  (`crates/session-ui/src/group_combo.rs:138-143`) has already written `query_cell` and
  `match_count`. One call refreshes all four surfaces; they cannot disagree.
- `combobox.rs:396` has no "same value" early return (the one in `command/state.rs:215` belongs to
  a different type), so `set_query("")` always re-runs the search.

The re-entrancy rationale also checks out: `empty`/`footer` run inside the combobox's own render,
so they read `query_cell` rather than `state.read(cx)`. `group_combo::empty_message` is a pure
function and is unit-tested.

Walked sequence, on the packet's own frames — the search box, the list and the footer agree in
all four:

| Step | Frame | What it shows |
|---|---|---|
| "Lab" + Enter | `US-0118-rw-56b-enter-created-lab.png` | created and selected |
| reopen | `US-0118-rw-56c-reopened-clean.png` | box **empty**, `infra` listed, footer disabled "Type to create new group" |
| type "inf" | `US-0118-rw-56d-typed-inf-not-labinf.png` | box reads **"inf"**, footer offers `Create "inf"` |
| Enter | `US-0118-rw-56e-enter-selects-infra.png` | **infra** selected — not "Labinf" |

All four frames are distinct files (md5) and carry today's clock in the status bar.

### M2 — the disclosure, and why Space-only is right

Read out of the dependency, in dispatch order:

1. The disclosure is now `Button::new("advanced-disclosure")` (`session_dialog.rs:148`). `Button`
   defaults to `tab_stop: true` (`gpui-component-0.6.0/src/button/button.rs:267`) and ends its
   render with `.track_focus(&focus_handle).tab_index(...).tab_stop(self.tab_stop)`
   (`button.rs:733-735`), with a focus ring at `button.rs:790`. So it is a real tab stop with a
   visible ring — `evidence/US-0120-rw-tab-reaches-advanced.png`.
2. **Space** toggles it: the focused `div` records an unmodified `enter`/`space` **key down**
   (`gpui-pre-0.3.3/src/elements/div.rs:2954-2976`) and turns the matching **key up** into a
   `ClickEvent::Keyboard` (`div.rs:2978-3020`) — `evidence/US-0120-rw-advanced-toggled-by-space.png`.
3. **Enter cannot reach it.** `gpui-base-0.6.0/src/dialog.rs:91` binds `enter` → `Confirm` in the
   `Dialog` key context and `dialog.rs:525` handles it. `Window::dispatch_key_event` dispatches
   keymap bindings (`window.rs:5740-5746`) **before** `finish_dispatch_key_event` runs any
   element key listener, and returns as soon as a handler stops propagation — which an action
   handler does by default. The button's key-down listener therefore never runs,
   `pending_keyboard_down` stays `None`, and the key-up produces no click.

The implementer's claim is correct and **Space-only is acceptable**: Enter is the dialog's default
action from every field, so the disclosure behaves exactly like Cancel, Save and Browse in the same
dialog. It is not an exception; it is the rule. See `R-m2` for the two places that say otherwise.

### M3 — a refused Save reveals what it is about

`PortForwardRows::take` now returns `ForwardError { row, message }` (`forward_rows.rs:153-158,
182-208`) and `focus_row` puts the cursor in that row (`forward_rows.rs:212-221`); `submit` reveals
before it notifies (`session_dialog.rs:460-486`). `evidence/US-0120-rw-save-opens-advanced.png`
shows the disclosure opened, the focus ring on row 1 and the toast
*"Port forward: the target port must be a number 0..65535."* — which is verbatim
`forward_rows.rs:120`.

Both reveal paths also move focus, and `Window::focus` refreshes, so the newly expanded rows are
painted in the same frame.

### M4 — the finding was wrong, and so is its replacement measurement

**`Button::label` never clipped "Connecting".** Measured, not eyeballed, from three pre-rework
captures at 1x (all 1600x1000, the button's fill is `rgb(42,54,77)`, baseline taken from the
non-descender glyphs of the same word):

| Frame | Commit | Baseline row | Lowest `g` row | Descender rows |
|---|---|---|---|---|
| `research/before/15-connect-inflight.png` | pre-`IN-0042` | 644 | 648 | **4** |
| `evidence/US-0118-15-connect-inflight.png` | `f4ea1765` (`.label(...)`) | 660 | 664 | **4** |
| `evidence/BUG-0069-rw-15-connecting-button.png` | `104d5504` (`control_label` child) | 636 | 640 | **4** |

The first verifier's own 6x crop, `evidence/BUG-0069-verify-15-connecting-button-6x.png`, is a
nearest-neighbour 6x of the `f4ea1765` button: its `g` reaches 24 sub-rows below the baseline —
**4 device rows**, not zero. Side by side at 6x:
`evidence/BUG-0069-reverify-connecting-before-6x.png` (`f4ea1765`) and
`evidence/BUG-0069-reverify-connecting-after-6x.png` (`104d5504`) — the glyphs are pixel-for-pixel
the same shape.

**Why `Checkbox` clips and `Button` does not.** The checkbox wraps its label in
`v_flex().flex_1().overflow_hidden().line_height(relative(1.2))` (`checkbox.rs:315-318`) — it is
the `overflow_hidden()` on a box whose height is the child's 1-em line box that removes the tail.
`Button`'s label div (`button.rs:679-687`) has the same `line_height(relative(1.))` but sits in an
`h_flex().size_full()` sized to the whole button, and nothing clips it vertically, so the glyph
paints outside its own 1-em box unharmed. The original `BUG-0069` outcome is real and unchanged —
re-measured on `evidence/BUG-0069-11c-before-above-after.png`, where the *before* half cuts the `g`
of "agent" and "global" flat and the *after* half does not.

So the rework's sentence *"the `g` of 'Connecting' occupies 4 pixel rows below the baseline, against
the zero the verifier measured on the same button at `f4ea1765`"* is **not supported by any frame in
the packet**: the before-frames show the same 4 rows.

`.label(` sites across `crates/` were re-grepped. Checkbox/Radio/Switch: only the two `sftp-ui` sites
the Gaps now names — correct. `Button::label` sites whose text carries a descender and which the
Gaps does not name: `crates/app/src/crash_report_dialog.rs:113` ("Copy"),
`crates/sftp-ui/src/transfer.rs:477` ("Replace"),
`crates/terminal-view/src/terminal_view/input.rs:422` ("Open"),
`crates/settings-ui/src/about.rs:61` ("Downloading…"). Given the measurement above **none of them is
clipped**, so the omission is harmless — but the Gaps entry that calls "Browse", "Add" and "Cancel"
*latent* clips is itself wrong.

## New findings

- **`R-M1`. `U118-m2` is still open on the Quick Connect duplicate path.** `InlineError::watch` is
  called from `QuickConnectHops::forms` only inside the `if self.built.borrow().0 != selected`
  rebuild, which is unreachable when `picker` is `None` (`quick_connect_dialog.rs:78-84`; the early
  return at `:79-81`). A duplicate builds its hops eagerly through
  `QuickConnectHops::fixed(JumpHopForms::new(duplicate_hops, …))`
  (`quick_connect_dialog.rs:245-250`), and the dialog's `InlineError::new` is given
  host/port/username plus the auth form's secrets only (`quick_connect_dialog.rs:252-261`) — never
  the hop inputs. So duplicating a session that has a jump chain reproduces exactly the defect the
  rework fixed elsewhere: a corrected jump-host password stands beside a stale inline error. The
  path is explicitly supported — `initial_focus` even puts the cursor in the first hop secret for a
  duplicate (`quick_connect_dialog.rs:394-408`). One line fixes it:
  `inline_error.watch(&forms.secret_inputs(), cx)` after the `fixed(...)` construction, or extend
  the `InlineError::new` input list.
- **`R-m2`. Two places now say the disclosure "toggles on Enter and Space", which the same commit
  disproves.** `crates/session-ui/src/session_dialog.rs:146-147` (rustdoc) and
  `docs/ssh-client-connect.md` §6.6 both claim Enter toggles it; `US-0120`'s own rework section says
  *"Enter does not toggle it, and must not"*, and the dispatch order above confirms the rework
  section. Two of the three statements in one commit are wrong.
- **`R-m3`. Two Evidence-frame bullets now point at deleted files.**
  `US-0118-failed-connect-keeps-the-form.md:320` cites `evidence/BUG-0068-17b-connect-timeout-22s.png`
  and `US-0120-…md:299` cites `evidence/US-0120-54-session-color-row.png`; both were removed by this
  rework. `US-0120` corrects itself 113 lines later, `US-0118` does not. (The prior report's own
  citations of those filenames are prose about the duplicates and are fine.)
- **`R-m4`. `control_label` as a `Button` child drops the label's ellipsis.** `.label(...)` wraps the
  text in `min_w_0().whitespace_nowrap().text_ellipsis()` (`button.rs:679-687`); `control_label` is a
  plain `div` with a line height, and the button's content row is
  `overflow_hidden().whitespace_nowrap()` (`button.rs:658-663`). A flex child without `min_w_0` will
  not shrink, so an over-long label is now hard-clipped at the button edge instead of ending in "…".
  One of the three converted sites — `group_combo.rs`'s `Create "<typed text>"` — takes arbitrary
  user text and is `w_full`.
- **`R-m5`. `U120-m3` is half-closed: the row still draws nine squares.** "Custom…" is now the
  picker's own trigger label (`ColorPicker::label` → `ColorPickerButton`,
  `gpui-component-0.6.0/src/color_picker.rs:103-106, 497`), so the word opens the picker — that half
  is fixed. But the picker's current-value square is still rendered beside the eight swatches and is
  still indistinguishable from swatch 1 whenever the default colour is selected:
  `evidence/US-0120-reverify-colour-row-4x.png` (4x crop of `US-0118-rw-56c-reopened-clean.png`).
- **`R-m6`. `U120-m4` is correct in code but unevidenced.** `ColorPicker::featured_colors`
  (`color_picker.rs:86-88`) is read at `color_picker.rs:207` and rendered as the popup's top row at
  `:226-229`, so passing `swatch_colors(cx).to_vec()` does make the two rows one list. No frame of
  the **open** picker was taken after the change; the only picker frame in the packet
  (`US-0120-54b-session-color-picker.png`) predates it and still shows the kit's twelve.
- **`R-m7`. `InlineError::edits` only grows.** Each Quick Connect picker move pushes a fresh set of
  subscriptions and drops none (`common.rs:155-168`). Subscriptions to dropped `InputState` entities
  are inert, so this is a small bounded leak inside one modal's life, not a defect.
- **`R-m8`. Nits.** `US-0120`'s rework says a refused Save "puts the cursor in the offending field";
  `focus_row` puts it in the row's **first** field (the bind address) — the code's own rustdoc says
  so correctly, and the next clause of the packet says "on that row", so only the one phrase
  overstates. `advanced_header`'s `.justify_start()` has no effect on the label, which the kit
  centres inside the button's content row; the disclosure renders centred and full-width.

## Mutations (three, all caught, all restored)

| Mutation | Test result |
|---|---|
| `reveals_advanced`: drop the `InvalidField::JumpHost` arm | `a_refused_save_opens_the_disclosure_only_for_a_field_it_hides` **FAILED** — `assertion failed: reveals_advanced(InvalidField::JumpHost)` |
| `reveals_advanced`: add `InvalidField::Basic` | same test **FAILED** — `assertion failed: !reveals_advanced(InvalidField::Basic)` |
| `empty_message`: drop the `!has_any_group` term | `the_empty_area_tells_no_groups_from_no_match` **FAILED** — `left: NoMatch("Lab") right: NoGroupsYet` |

`git status` clean after restoring all three.

## Commands

```
cargo test -p oneterm-session-ui
  test result: ok. 72 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
cargo test -p oneterm-state
  test result: ok. 38 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
pwsh scripts/ci-local.ps1
  ci-local: all checks passed.
```

## Frames taken by this re-verification

| File | Shows |
|---|---|
| `BUG-0069-reverify-connecting-before-6x.png` | The Connect button at `f4ea1765`, `.label("Connecting")`, 6x nearest-neighbour: the `g` keeps its full 4-row descender. |
| `BUG-0069-reverify-connecting-after-6x.png` | The same button at `104d5504` with `control_label`: the same glyph, the same 4 rows. |
| `US-0120-reverify-colour-row-4x.png` | The colour row after the rework: eight swatches, a ninth square (the picker's value, identical to swatch 1), then the now-clickable "Custom…". |

## Gaps in this re-verification

- **No GUI walk of my own.** No app binary existed in this worktree and building one plus a
  posted-message driver was not worth the shared disk and CPU, so `M1`, `M2` and `M3` were verified
  from the dependency source, the unit tests (with mutations) and the rework's own frames, which
  were checked for distinctness and for internal consistency rather than re-taken. `M4` needed no
  walk — it was settled by measuring the packets' existing captures.
- **`R-M1` was found by reading, not by walking.** Duplicating a session with a jump chain was not
  driven through the UI; the gap is proven by the call graph above, not by a frame.
- **No screen-reader output was read back.** `accessibility_label` on the disclosure and
  `aria_label` on the swatches are confirmed set and confirmed to be what the kit forwards; the
  disclosure carries its state in its name rather than an `aria-expanded`, which no AT client was
  used to check.
- **The eight colour swatches are still not tab stops**, as `US-0120`'s Gaps records. Not re-argued.
- **No `target/fast-dev` was created, so none was deleted.**
