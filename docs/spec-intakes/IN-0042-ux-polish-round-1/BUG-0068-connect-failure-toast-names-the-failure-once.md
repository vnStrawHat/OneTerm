# Work: The connect failure toast names the failure once

ID: BUG-0068
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

- Change type: bug
- Risk lane: normal
- Spec Intake, when required: `IN-0042` — `docs/spec-intakes/IN-0042-ux-polish-round-1/IN-0042.md`

## Outcome

A failed SSH connect reports itself once. The notification reads
`SSH connect failed: timed out after 20 s`, not `SSH connect failed: SSH connect failed:
timed out after 20 s`.

## Findings and proposals covered

`P3` — *"Stop double-prefixing the connect error: `format!("{error}")`, since
`AppError::Connect` already says "SSH connect failed: …"."*

Addresses `F3` (medium), quoted from `research/ux-walkthrough-2026-09-16.md`:

> | F3 | SSH connect (failure) | An unreachable host produces, after 20 s, a bottom-right
> toast reading **"SSH connect failed: SSH connect failed: timed out after 20 s"** — the
> prefix is doubled. | Polish / trust. `AppError::Connect` already renders
> `"SSH {phase} failed: {message}"` (`crates/core/src/error.rs:129`) and the notification
> prefixes it again (`crates/session-ui/src/common.rs:418`). | medium | 17b |

## Scope

- [ ] In scope:
  - `crates/session-ui/src/common.rs:418` — the `format!("SSH connect failed: {error}")` that
    prefixes an already-prefixed error.
  - Every other place in `crates/session-ui` that builds a user-visible string from an
    `AppError`, checked for the same pattern (see Context — a single caller fix here would be
    the symptom fix, not the root-cause fix).
- [ ] Out of scope:
  - `AppError::Connect`'s own `#[error]` format string
    (`crates/core/src/error.rs:128-135`). It is correct: it names the phase and the message,
    and it is what every other consumer of the error relies on.
  - Where the notification appears, how long it stays, and whether the dialog also shows the
    error inline — that is `US-0118`, which consumes the string this packet fixes.
  - The 20 s timeout itself and the connect flow's asynchrony
    (`docs/ssh-client-connect.md` §1.3 decision 7 — settled, not reopened).
  - Error messages from SFTP, the host-key prompt, or the update checker.

## Acceptance

- [ ] An unreachable host produces a notification whose text names the failure once.
- [ ] The phase is still visible: a failure in a phase other than "connect" still says which
      phase (`AppError::Connect` carries a `ConnectPhase`, and the whole point of the format
      string is that it is rendered).
- [ ] No other user-visible string in `crates/session-ui` re-prefixes an `AppError` that
      already carries its own prefix. The packet lists the sites it checked in Evidence.
- [ ] An error that is **not** an `AppError::Connect` — one with no prefix of its own — still
      reaches the user with enough context to be actionable. Fixing the double prefix must not
      produce a bare "timed out" with no subject.
- [ ] A focused test pins the message shape, so the prefix cannot come back.
- [ ] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

## Documentation

### Owning Docs Reviewed

- `docs/ssh-client-connect.md` §9.2 — how connect failures are reported. `P3` names it as the
  owning section. Read it and confirm whether it quotes the message text; **update required**
  if it does, **no change** if it only describes the mechanism. Record which.
- `docs/agents/error-policy.md` — the project's runtime error handling and recovery rules,
  including how errors reach the user. This is the document that should say "do not re-prefix
  an error that carries its own prefix", and if it does not, that is worth one sentence —
  because the same mistake is available at every notification site in the application.
  **Decide during implementation and record it.**
- `crates/core/src/error.rs:128-135` — `AppError::Connect`'s format string. The source of the
  prefix that already exists. **No change.**
- `docs/PROJECT.md` — read for standing invariants. **No change.**

### Documentation Action

Decide during implementation between:
- Update required: `docs/agents/error-policy.md` gains the "do not double-prefix" rule, and
  `docs/ssh-client-connect.md` §9.2 if it quotes the message.
- No contract change: the reviewed docs already describe the correct behaviour and the bug is
  purely in the call site.

Reason: this is a one-line fix whose value is that it does not come back. Whether the guard
belongs in a document or in a test is the real question; the test is mandatory either way.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

## Context

- The two halves, read directly:
  - `crates/core/src/error.rs:128-135`:
    ```rust
    /// One phase of an SSH connection attempt failed (or timed out).
    #[error("SSH {phase} failed: {message}")]
    Connect { phase: ConnectPhase, message: String },
    ```
  - `crates/session-ui/src/common.rs:413-421`:
    ```rust
    window.push_notification(
        notify(NotificationType::Error, format!("SSH connect failed: {error}"), cx),
        cx,
    );
    ```
  `{error}` renders the `#[error]` string, so the literal prefix is applied twice.
- **Root cause, not symptom.** The literal is at one call site, but the mistake it represents
  — a caller adding context an `AppError` variant already carries — is available everywhere an
  error is pushed to a notification. Before fixing line 418, grep `crates/session-ui` for
  `push_notification` and for `format!("...{error}")` and check each. One guard in the right
  place is smaller than the same fix three times later, and the walkthrough only saw the one
  path it could reach.
- Deleting the prefix entirely is the obvious fix and is probably right — but check the other
  error types that can arrive at this site. If some arrive prefixless, the correct fix is not
  "delete the literal" but "let the error say its own name", which may mean a different
  variant rather than a different `format!`. Say which in Evidence.
- Ladder: no new error type, no new formatting helper, no new dependency. The smallest fix
  that is also the root-cause fix.
- `US-0118` depends on this: the inline error it shows in the dialog is this same string, and
  showing the doubled version inline would put the bug in two places.
- `research/before/17b-connect-timeout-22s.png` is the before picture — the toast with the
  doubled prefix. `16-connect-failed.png` and `17-connect-timeout-20s.png` are the same path
  at earlier moments.

## Plan

- [ ] Grep every user-visible error string in `crates/session-ui`; list them.
- [ ] Write the focused test against the current behaviour; watch it fail.
- [ ] Fix, at whichever level the grep says is right.
- [ ] Decide the documentation question and record it.
- [ ] Re-capture the scene.

## Decisions

None. There is no choice here future work inherits beyond "an error names itself once", which
belongs in `docs/agents/error-policy.md` if anywhere.

## Verification Plan

1. **Focused:** a unit test in `crates/session-ui` that builds the notification text from an
   `AppError::Connect` and asserts the prefix appears exactly once, plus one over a
   non-`Connect` error asserting the message is still self-describing. Pure string work, no
   gpui — this is where the proof lives, because the toast itself is not queryable.
2. **Unit:** `cargo test -p oneterm-session-ui`, `cargo test -p oneterm-core`.
3. **Integration:** `cargo test --workspace`.
4. **Platform:** `pwsh scripts/ci-local.ps1`.
5. **E2E (GUI walk, re-capture this scene):**
   - `17b-connect-timeout-22s.png` — connect to an unreachable host (the walkthrough used
     `10.10.10.10`) and capture the toast after the 20 s timeout. The after frame must show a
     single prefix.
   - `16-connect-failed.png` — the dialog state at the moment of failure, for `US-0118` to
     build on.
   A live host is not needed for this path; an unreachable address is the whole test.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Risks

- **Fixing the one line the finding names and leaving the siblings.** The walkthrough reached
  one failure path. The grep is not optional.
- **Losing the subject.** Deleting the prefix from a site that also receives prefixless errors
  turns a clear message into "timed out after 20 s" with no indication of what timed out.
  Covered by the second focused test.
- **Regressing the phase.** `AppError::Connect` carries a `ConnectPhase` so a handshake failure
  and a connect failure read differently. A fix that hardcodes "connect" throws that away.

## Evidence and Gaps

### Sites checked (the grep the packet made mandatory)

Every user-visible string built from an error in `crates/session-ui`, via
`push_notification` and `format!("...{error}")`:

| Site | Verdict |
|---|---|
| `common.rs:418` (was) — `format!("SSH connect failed: {error}")` over an `AppError` | **The bug.** Fixed. |
| `common.rs:255` — `format!("Connected, but the terminal tab could not be opened: {reason}.")` | `reason` is a bare `&str` (`"the main workspace is not registered"`), not an error with a prefix. No change. |
| `quick_connect_dialog.rs:253,264,272`, `connect_dialog.rs:86,228,241,260,267`, `session_dialog.rs:291,297,320`, `rename_group.rs:33` | All push a validation `String` or `UserHostPortError`/`jump_chain` `Display` that carries no subject prefix of its own. No change. |
| `auth_form.rs:289,293`, `forward_rows.rs:146` | Wrap `std::io::Error` / a parse error, neither of which names its own subject. Correct as written. |
| `panel.rs:203`, `tree_render.rs:231` | Fixed success strings, no error interpolation. |

So the literal at `common.rs:418` was the only double prefix, but the fix is not
"delete the literal": the same site also receives `AppError::Cancelled`
(`"operation cancelled"`), `AppError::Io` and `AppError::Other`, none of which names
SSH. Deleting the prefix would have produced a bare "operation cancelled". The fix is
therefore a per-variant decision in one function,
`common.rs::connect_failure_message`, which is what the notification and (from
`US-0118`) the inline error both call.

### Commands

- `cargo test -p oneterm-session-ui --lib common::` — 8 passed, including the two new
  tests `a_connect_error_names_the_failure_once` and
  `an_error_without_a_subject_still_gets_one`.
- `cargo test --workspace` — passed (run once at the end of the packet series).
- `pwsh scripts/ci-local.ps1` — "ci-local: all checks passed".

### Documentation decision

**Update required**, both halves:

- `docs/agents/error-policy.md` — gains the "an error names itself once" review rule.
  The packet asked for this call explicitly: the mistake is available at every
  notification site in the application, so the guard belongs in the policy as well as
  in the test.
- `docs/ssh-client-connect.md` §9.2 — it *does* quote the message text
  (`SSH <phase> failed: <message>`), so it gains the paragraph naming
  `connect_failure_message` as the one place the reporting text is built and what it
  does with an error that carries no subject.
- `crates/core/src/error.rs` — unchanged, as scoped.

### Evidence frames

- `evidence/US-0118-16-connect-failed.png` — the quick-connect dialog after the 20 s
  timeout: the toast bottom-right reads `SSH connect failed: timed out after 20 s`, once,
  and the inline block above the footer carries the same string.
- `evidence/BUG-0068-verify-16-connect-failed.png` — the **Connect** dialog (saved session)
  on the same path, taken by the verifier, which this packet's own walk never reached.

Two filenames were removed in rework: `BUG-0068-16-connect-failed.png` and
`BUG-0068-17b-connect-timeout-22s.png` were byte-identical copies of
`US-0118-16-connect-failed.png`, and the "22 s" in the second was unsupported — the toast in
that image says 20 s. One capture is now presented as one frame.

### Gaps

- None for this packet. The toast's own pixels are proved by the frames; the string is
  proved by the unit tests.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.

## Rework after independent verification

Verdict **PASS**; three minor findings, all addressed.

- **B68-m1.** `docs/ssh-client-connect.md` §9.2 said the verbatim variants "already begin with
  `SSH`". `AppError::HostKeyUnknown` renders `Unknown SSH host key for …`, which names SSH
  without leading with it. The behaviour was right and the rustdoc already said it correctly;
  §9.2 now says "name SSH and what failed" and spells out all three renderings.
- **B68-m2.** The `HostKeyUnknown` arm of `connect_failure_message` is unreachable from its only
  caller, which matches that variant first and opens the host-key dialog. §9.2 now says so, and
  says why the arm stays: the function is total over the variants a connect can produce rather
  than correct only by the order of the arms above it.
- **B68-m3.** Three evidence filenames, one capture. Two removed; see Evidence frames.
