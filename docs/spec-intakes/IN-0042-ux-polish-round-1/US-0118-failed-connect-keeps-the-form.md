# Work: A failed connect keeps the form, explains itself inline, honours Save, and Enter creates a group

ID: US-0118
Intake: IN-0042
Created: 2026-09-17

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [x] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: existing-contract change
- Risk lane: normal
- Spec Intake, when required: `IN-0042` — `docs/spec-intakes/IN-0042-ux-polish-round-1/IN-0042.md`

## Outcome

A connect that fails leaves the user able to try again. The typed password survives the
failure, the dialog says what went wrong where the user is looking, a ticked "Save to SSH
Sessions" either saves or says why it did not, and pressing Enter in the group combobox
creates the group.

## Findings and proposals covered

`P5` — *"Keep the password on a failed connect (only clear on success) and show the error
inline in the dialog as well as the toast."*

`P24` — *"Save a quick-connect session even when the connect fails (or tell the user it was
not saved), and let Enter commit a new group in the combobox."*

Addresses `F4`, `F5` (medium) and `F22` (low), quoted from
`research/ux-walkthrough-2026-09-16.md`:

> | F4 | SSH connect (failure) | The password field is **cleared the moment Connect is
> pressed** and stays empty after the failure; the dialog offers no inline error and no Retry.
> The toast is in the opposite corner from the modal and auto-dismisses. | Error recovery.
> Every retry costs a full re-type of the password; a user who looked away for 25 s sees only
> an empty form and a re-enabled button with no explanation (15, 16 show exactly that state).
> | medium | 15, 16, 17b |

> | F5 | SSH connect (failure) | **"Save to SSH Sessions"** was ticked, the connect failed, and
> nothing was saved — with no message. | Feedback: an explicit user intent is silently
> dropped. | medium | 14, 16 |

> | F22 | group combobox | Typing a new group name and pressing **Enter** does not create it —
> only clicking `+ Create "Lab"` does — and after creating, the dropdown stays open showing
> `Create "Lab"` again. The no-match area shows a bare inbox icon with no text. | Keyboard
> reach + feedback. | low | 55, 56 |

## Scope

- [ ] In scope:
  - `crates/session-ui/src/common.rs:412-424` — the failure path: keep the password in the
    open dialog, render the error inline as well as pushing the toast.
  - `crates/session-ui/src/quick_connect_dialog.rs:99-140` — the "Save to SSH Sessions"
    decision on a failed connect.
  - `crates/session-ui/src/session_dialog.rs` — the group combobox: Enter commits, the
    dropdown closes after creating, and the no-match area carries text.
  - The empty-result copy in the combobox (`F22`'s "bare inbox icon with no text").
- [ ] Out of scope:
  - **Persisting the password.** `docs/ssh-client-connect.md` §1.3 decision 1 and `DEC-0001`
    say passwords are never persisted, and the walkthrough records that as settled. The
    password stays in the open dialog's memory for the retry and reaches no store, no config
    file and no log.
  - **Making connect blocking.** §1.3 decision 7 (connect runs async, the dialog reports
    failure by notification) stands; this packet adds an inline echo beside the toast, it does
    not replace it.
  - The message text itself, which `BUG-0068` fixes. This packet displays it.
  - The full session dialog's other fields (`US-0120`) and the session tree (`US-0119`).
  - The 20 s timeout.

## Acceptance

- [ ] After a failed connect, the password field still holds what the user typed, and Connect
      is re-enabled. Pressing Connect again retries without re-typing.
- [ ] The dialog shows the failure inline, in the dialog, using the same single-prefix text
      `BUG-0068` produces. The toast still appears.
- [ ] The inline error clears when the user edits the form or retries — it does not persist
      over a subsequent success.
- [ ] A **successful** connect still clears the password and closes the dialog exactly as
      today.
- [ ] The password is not written to `ssh_session.json`, to any other config file, or to the
      log, on either path. The packet checks the files on disk after a failed connect with
      Save ticked and records the result.
- [ ] With "Save to SSH Sessions" ticked and the connect failing, the user's intent is
      honoured: the session is saved, or the dialog says it was not and why. The packet states
      which before implementing, and the choice is visible in the after frame.
- [ ] Typing a new group name in the combobox and pressing Enter creates the group and selects
      it, and the dropdown closes.
- [ ] The combobox's no-match area carries readable text, not a bare icon.
- [ ] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

## Documentation

### Owning Docs Reviewed

- `docs/ssh-client-connect.md` §1.3 — the quick-connect dialog's decisions, including
  decision 1 (passwords are never persisted) and decision 7 (connect is async and reports by
  notification). **Update required:** what happens to the form on failure, and what a ticked
  Save means when the connect fails. Both are behaviour §1.3 currently describes differently.
- `docs/ssh-client-connect.md` §9.2 — failure reporting. **Update required:** the inline echo
  beside the toast.
- `docs/decisions/0001-ssh-key-secret-persistence.md` — the decision this packet must not
  break. Read it before writing the retry path, and record in Reconciliation that it still
  holds. **No change.**
- `docs/ssh-client-connect.md` §4 — the full session dialog's fields, including the group
  combobox. **Update required** if the combobox's commit behaviour is described there.
- `docs/agents/error-policy.md` — how errors reach the user; an inline error in a dialog is a
  second channel beside the notification. Confirm this is consistent with the policy.
- `docs/PROJECT.md` — read for standing invariants. **No change.**

### Documentation Action

Update required: `docs/ssh-client-connect.md` §1.3, §9.2 and §4.

Reason: this packet changes documented behaviour on the failure path — what the form keeps,
what a ticked checkbox means, and where the error is shown.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.
Include an explicit confirmation that `DEC-0001` / §1.3 decision 1 still holds, with the
on-disk check that proves it.

## Context

- **The Save question is the one real decision, and it is not obviously "just save it".**
  Saving a session whose credentials have never worked puts an unverified entry in the user's
  session list, and the entry's auth mode may be wrong (that is often *why* the connect
  failed). Telling the user "not saved, because the connect failed" is honest, cheap and
  keeps the store clean — but it also discards an explicit intent, which is exactly what `F5`
  complains about. A third option: save it, and say so, so the user knows an untested entry
  now exists.
  Decide before implementing, write the choice here with its reason, and make it visible in
  the after frame. The acceptance deliberately allows either, because the finding is "silently
  dropped", not "not saved".
- **The password must stay in memory only.** The dialog is an open modal with its own state;
  keeping the typed value there for a retry is not persistence. What must be checked, and
  recorded, is that nothing on the failure path writes it out: not `ssh_session.json`, not the
  log (`~/.OneTerm/logs`), not a crash report. Check the files, do not assume.
- The inline error and the toast show the same string, which is why this packet depends on
  `BUG-0068`. Rendering the doubled prefix in two places would make the bug twice as visible.
- `F22`'s three symptoms are one control: Enter does not commit, the dropdown does not close
  after creating, and the empty state has no text. Fix them together — they are the same
  combobox and the same frame.
- Ladder: no new dialog, no new state machine. The failure path already re-enables the button
  and already has the error in hand; it currently throws both the password and the error away.
- `research/before/14-quick-connect-filled.png` (Save ticked, form filled),
  `15-connect-inflight.png`, `16-connect-failed.png` (the empty form after failure),
  `17b-connect-timeout-22s.png` (the toast), `55-group-combobox.png` and `56-group-typed.png`
  (the combobox) are the before pictures.

## Plan

- [ ] Decide the Save question and record it here.
- [ ] Keep the password; add the inline error; check the on-disk files.
- [ ] Implement the Save decision.
- [ ] Fix the combobox's three symptoms together.
- [ ] Update `docs/ssh-client-connect.md` §1.3, §9.2, §4.
- [ ] Re-capture the scenes.

## Decisions

**Decided before implementing: not saved, and the dialog says so. No `DEC` needed.**

`CORR-54` already rules that a quick-connect session is saved only once the connection is
authenticated — it is why `on_connected` exists and is named that
(`crates/session-ui/src/common.rs:276`). That rule is right and this packet keeps it: a session
whose credentials have never worked is an entry whose auth mode is quite possibly *why* the
connect failed, and the user gets no signal that the thing in their list is untested. Saving it
would also be a new rule about what the store may contain, which would need a `DEC` and an
owner ruling, for a finding whose complaint is not "it was not saved" but "it was **silently**
dropped".

So the tick stays on, the form stays open, and the inline error carries a second line: the
session was not saved because the connect failed, and it is saved when one succeeds. Pressing
Connect again after fixing the password saves it. The intent is honoured on the next attempt
rather than discarded, and nothing is silent.

## Verification Plan

1. **Focused:** `cargo test -p oneterm-session-ui` over the pure halves — the save decision as
   a function of `(save_ticked, connect_result)`; the group-commit function (a typed name plus
   Enter yields the group, a blank name does not, an existing name selects rather than
   duplicates); and the form-state reducer that decides whether the password is cleared. These
   are the only automated proofs; the inline error's rendering is not queryable.
2. **Unit:** `cargo test -p oneterm-session-ui`.
3. **Integration:** `cargo test --workspace`.
4. **Platform:** `pwsh scripts/ci-local.ps1`.
5. **E2E (GUI walk, re-capture these scenes):**
   - `14-quick-connect-filled.png` — the filled form with Save ticked.
   - `15-connect-inflight.png` — the in-flight state, which the walkthrough praised; regression
     check that it is unchanged.
   - `16-connect-failed.png` — the after frame must show the password still present and the
     error inline.
   - `17b-connect-timeout-22s.png` — the toast, single-prefixed.
   - `55-group-combobox.png`, `56-group-typed.png` — the combobox before and after Enter.
   Plus, not in the walkthrough: the session tree or `ssh_session.json` immediately after the
   failed connect with Save ticked, showing what the Save decision did. Capture it as `16b`.
   Use an unreachable address (`10.10.10.10`) for the failure and, if a success path is needed,
   the repository's loopback `sftp-dev-server` as the walkthrough did.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Risks

- **Leaking the password.** This is the packet's one real hazard. Keeping a secret alive longer
  than before means every path out of the dialog — cancel, escape, window close, a crash
  report, a log line at debug level — is a place it could escape. Check them, and record the
  check. An `AppError` that embeds the password in its message would defeat the whole thing.
- **Saving an unverified session silently.** Whichever way the Save question goes, doing it
  without telling the user reproduces `F5` in the opposite direction.
- **Two error channels disagreeing.** If the inline error and the toast are built from
  different strings, they will drift. Build both from one value.
- **A stale inline error.** An error left visible after a successful retry is worse than no
  error. The acceptance pins the clearing rule.
- **The combobox's Enter key colliding with the dialog's Enter.** `FormDialog`'s submit fires
  on `on_ok` (`crates/state/src/form_dialog.rs:25-28`), so Enter inside the combobox must
  commit the group and **not** submit the dialog. Walk that specific keystroke.

## Evidence and Gaps

### The password: what changed, and the check that it is still not persisted

`SshAuthForm::take_auth` blanked the password and the passphrase as it read them - that was
`F4`'s cause. It no longer clears, and it no longer needs a `&mut Window` at all, so
`take_auth(&self, cx: &App)` and `JumpHopForms::take_hops(&self, cx: &App)` lost that
parameter. The secret now lives exactly as long as the dialog: a successful connect closes it,
and closing it drops the form and its `InputState` entities.

The hazard the packet named - a secret alive longer than before - was checked on disk after a
failed connect with Save ticked and a password typed into the field:

| Checked | Result |
|---|---|
| `target/ssh_session.json` | Byte-identical to the pre-walk copy (`Get-FileHash` equal). |
| Every `target/*.json` (`docks`, `terminal`, `ui_config`, `update_config`, `ssh_session`) | No occurrence of the password. |
| The app's stdout and stderr logs for the walk | No occurrence of the password. |

`DEC-0001` / section 1.3 decision 1 therefore still holds, and that decision row now says so
explicitly: an open modal's own state is not persistence.

### The Save decision, as built

Recorded in Decisions **before** implementing: `CORR-54` stands, the session is not saved on a
failed connect, and the dialog says so. `unsaved_note` is a pure function of the tick, and the
frame shows the result: under the error, in muted text, *"Not saved to SSH Sessions: a session
is saved once its connection succeeds. The tick is still on - connect again and it will be
saved."* No `DEC` was needed, because nothing about what the store may contain changed.

### The inline error

`SshConnectRequest::on_failed` carries the message `connect_failure_message` produced - the
same `SharedString` the toast renders, so `BUG-0068`'s single prefix appears in both and they
cannot drift. `InlineError` holds it and clears it on a retry and on `InputEvent::Change` from
any watched field.

One thing worth recording for the next person: the first attempt used `cx.observe` on the
inputs and the error vanished a few hundred milliseconds after appearing - an `InputState`
notifies on focus changes and on every cursor blink. `InputEvent::Change` is the event that
means "the user typed".

### The group combobox (`F22`), all three symptoms

- **Enter creates.** The decision is `group_commit(query, match_count)` - create only when
  something is typed and the search left no row, because with a row on screen Enter belongs to
  the list, and creating "Lab" while "Laboratory" is highlighted would both ignore the
  selection and add a group nobody asked for.
- **The listener is capture-phase, and that is the whole trick.** `on_action` did not work and
  the first GUI walk proved it: gpui sets `propagate_event = false` before every bubble-phase
  action listener (`window.rs`, "Actions stop propagation by default during the bubble phase"),
  so the list consumed `Confirm` - even though `ListState::on_action_confirm` returns early
  with no rows - and no ancestor ever saw it. `capture_action` runs root-first and does not
  stop propagation, so the list still gets its Enter whenever it does have a row.
- **The dropdown closes.** `ComboboxState::set_open` is private; dispatching the kit's `Cancel`
  is the path that closes without touching the selection. It is deferred out of the capture
  handler so it does not re-enter the dispatch in progress.
- **The no-match area carries text**, via `Combobox::empty`: *No group matches "Lab". Press
  Enter to create it.* - instead of the kit's bare inbox icon.
- Guards: the handler acts only while the dropdown is open (recorded from
  `ComboboxTriggerContext::is_open`, the only place the kit exposes it), so Enter on a closed
  trigger still opens the list; and the query is cleared after creating, so a second Enter
  cannot create the same group twice.

### Commands

- `cargo test -p oneterm-session-ui` - 68 passed, including
  `group_combo::tests::enter_creates_only_a_typed_name_the_list_cannot_offer`
  (the Enter decision) and `common::tests::*` (the message both channels share).
- `cargo clippy -p oneterm-session-ui --all-targets -- -D warnings` - clean.
- `cargo test --workspace` - passed.
- `pwsh scripts/ci-local.ps1` - "ci-local: all checks passed".

### Evidence frames

Connecting to `10.10.10.10` with Save ticked, as the walkthrough did.

- `evidence/US-0118-14-quick-connect-filled.png` - the filled form, Save ticked.
- `evidence/US-0118-15-connect-inflight.png` - the in-flight state, unchanged.
- `evidence/US-0118-16-connect-failed.png` - **the acceptance frame**: the password still
  there, Connect re-enabled, the error inline, and the Save note under it.
- `evidence/US-0118-16c-error-cleared-on-edit.png` - one character typed into the password and
  the error is gone, the password kept and extended.
- The toast, single-prefixed and carrying the same text, is in the same frame as the inline
  error above; the separate `BUG-0068-17b-…` file was a byte-identical copy and was deleted.
- `evidence/US-0118-55-group-combobox.png`, `US-0118-56-group-typed.png` - the dropdown and the
  no-match text.
- `evidence/US-0118-56b-group-created-on-enter.png` - after Enter: "Lab" selected in the
  trigger and the dropdown closed.
- `16b` (the session tree / `ssh_session.json` after the failed connect) is the table above
  rather than a frame: the file is byte-identical, which a screenshot cannot show.

### Documentation

- `docs/ssh-client-connect.md` 1.3 decision 1 - the password survives a failed connect in the
  dialog's own state, and why that is not persistence.
- `docs/ssh-client-connect.md` 1.3 decision 7 - the dialog stays open on failure with the form
  intact; connect is still asynchronous.
- `docs/ssh-client-connect.md` 4.7 - **new**: the whole failure behaviour, the Save rule, and
  the combobox's Enter. Note for the record: the packet expected the group combobox to be
  described in section 4; it was not described anywhere, so 4.7 is where it now lives.
- `docs/ssh-client-connect.md` 9.2 - the inline echo beside the toast, built from one value.
- `docs/decisions/0001-ssh-key-secret-persistence.md` - read, **no change**, and the on-disk
  check above is recorded as the proof that it still holds.
- `docs/agents/error-policy.md` - read: an inline error in the open dialog is the policy's
  "show a notification with a corrective message" for a user-action failure, with the message
  additionally placed where the user is looking. Consistent, **no change** beyond the
  double-prefix rule `BUG-0068` added.

### Gaps

- **A successful connect was not walked.** The loopback `sftp-dev-server` was not started for
  this packet, so "a successful connect still clears the password and closes the dialog" rests
  on the unchanged success path (`connect_ssh_session` calls `window.close_dialog`, which drops
  the form) rather than on a frame. The failure path, which is what changed, is fully captured.
- **A match on screen with no row highlighted still ignores Enter.** Type "inf" while the group
  "infra" exists, do not press Down, press Enter: nothing happens, because the kit's
  `on_action_confirm` needs a `selected_index`. That is pre-existing kit behaviour, outside
  `F22`'s three symptoms, and this packet deliberately does not change it - creating "inf" when
  "infra" is on screen would be worse.
- **Only the three SSH dialogs call `take_auth`**, so its signature change has no other
  callers; `cargo test --workspace` and clippy cover the compile side.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.

## Rework after independent verification

Verdict **FAIL**, on the combobox only. Everything else in the packet held, and the verifier
closed the packet's own "a successful connect was not walked" gap with
`evidence/US-0118-verify-success-connect-loopback.png`.

### U118-MAJOR-1 — the combobox was left in a state the user could see was wrong

The Enter path cleared **a private copy of the query** (`query_cell`) and not the kit's search
input. The two then disagreed, and the verifier walked the consequence:

1. Type "Lab", Enter → created. Correct.
2. Reopen: the search box still held "Lab", the empty area read *"No groups yet"* although
   `infra` existed, and the footer read the disabled *"Type to create new group"* although text
   was visibly in the box. Three surfaces, three stories.
3. Reopen and type "inf": the text **appended** to the stale query, and Enter created a group
   called **"Labinf"** that the user never typed.

Step 3 is the one that matters. Before this packet Enter did nothing, so a stale query could
only produce a group the user clicked and could read in full; the new Enter path turned it into
a one-keystroke mistake.

**Fixed at the source.** Creating now calls `ComboboxState::set_query("")`, which writes the
kit's input *and* re-runs the search — and it is `perform_search` that writes `query_cell` and
`match_count`. So one call refreshes every derived value at once and the four cannot disagree.
The footer's Create button takes the same path.

Two things the fix had to respect, both found by walking:

- **The render closures must not read the entity.** `Combobox::empty` and `Combobox::footer`
  run inside `ComboboxState`'s own render, so `state.read(cx)` there panics with *"cannot read
  … while it is already being updated"* — reproduced, then fixed. They read `query_cell`, which
  is now genuinely the live value because every path that changes the query goes through
  `perform_search`.
- **"No groups yet" was a second, smaller lie.** It was shown whenever the filtered list came
  back empty. `group_combo::empty_message(query, has_any_group)` now distinguishes "the store
  holds none" from "none match what you typed", and is unit-tested.

### Walked, exactly the sequence the verifier used

| Step | Frame | Result |
|---|---|---|
| Type "Lab", Enter | `evidence/US-0118-rw-56b-enter-created-lab.png` | Created and selected; dropdown closed. |
| Reopen | `evidence/US-0118-rw-56c-reopened-clean.png` | Search box **empty**, `infra` listed, footer correctly disabled. No stale query, no "No groups yet". |
| Type "inf" | `evidence/US-0118-rw-56d-typed-inf-not-labinf.png` | The box reads **"inf"**, the footer offers `Create "inf"`. Not "Labinf". |
| Enter | `evidence/US-0118-rw-56e-enter-selects-infra.png` | **infra** selected — the list had a match and took its own Enter. "Labinf" is unreachable. |

### U118-m2 — the inline error now clears on a jump-hop edit

`InlineError` watched only the target's own credentials, so a corrected jump-host password
stood beside a stale error. It now watches the hop inputs too. Two shapes were needed because
the two dialogs differ: the Connect dialog resolves its hops once at open, so they are handed
over at construction; a Quick Connect **rebuilds** its hop credential blocks whenever the
picker's selection moves, so `InlineError::watch` is additive and `QuickConnectHops::forms`
hands each freshly built set over as it creates it.

### U118-m3 — the host-key path has no dialog to echo into

Recorded rather than changed: `AppError::HostKeyUnknown` closes the dialog before prompting, so
a failure on the *retried* connect has no dialog left and only the toast survives. That is the
intended shape — the retry is a new attempt from a different surface — and
`docs/ssh-client-connect.md` §4.7 now says so instead of leaving it to be discovered.

### U118-m4

The acceptance line "a successful connect still clears the password" is met in the sense the
packet always stated (the dialog closes and its state is dropped), and the verifier's loopback
frame now demonstrates it rather than the packet arguing it.

## Second rework — `R-M1`, from the re-verification of `40fc78d2`

**`U118-m2` was fixed on two of its three paths.** `InlineError::watch` is called from
`QuickConnectHops::forms`, but only inside the branch that rebuilds the hop forms when the
picker's selection moves — and `forms()` returns before that branch when there is no picker.
A **duplicate** has no picker: its chain is fixed, built once by `QuickConnectHops::fixed(...)`
before the dialog opens. So duplicating a session that has a jump chain still left a corrected
jump-host password standing beside a stale inline error, which is exactly the defect the rework
closed everywhere else. The path is a real one — `initial_focus` deliberately puts the cursor
in the first hop secret for a duplicate.

Fixed by watching those inputs where they are created: `QuickConnectHops::fixed_secret_inputs`
returns the fixed chain's credential inputs (and nothing for a picked chain, which hands its
own over as it rebuilds them), and `InlineError::new` is given them along with host, port,
username and the target's own secrets. The two paths now both hand over exactly once.

**No pure piece to test.** The decision is `self.picker.is_some()` — a duplicate's hops are
fixed, a picked chain's are rebuilt — and everything either side of it is `Entity<InputState>`
values and gpui subscriptions. A test would assert `is_some()`. The behaviour is reachable only
by duplicating a session with a jump chain in the GUI, which this rework did not walk: the gap
was found by reading the call graph and is closed the same way. Recorded rather than claimed.
