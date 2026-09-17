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
