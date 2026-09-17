# Work: A failed connect keeps the form, explains itself inline, honours Save, and Enter creates a group

ID: US-0118
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

Possibly one: if the packet decides a quick-connect session **is** saved on a failed connect,
that changes what the session store may contain (unverified entries) and is a rule future work
inherits — raise it as a `DEC` rather than recording it only here. If the decision is "not
saved, but say so", no `DEC` is needed.

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
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
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

After implementation, record commands, results, and anything skipped, unavailable, partial, or failing.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
