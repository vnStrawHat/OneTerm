# Work: Master password lifecycle UI

ID: US-0157
Intake: IN-0050
Created: 2026-10-09

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

- Change type: new capability
- Risk lane: high-risk (secrets in memory, a UI that writes and deletes `vault.json`)
- Spec Intake, when required: [`IN-0050`](IN-0050.md); owner rulings Q1-Q10 accepted as
  recommended (2026-10-09); [`DEC-0021`](../../decisions/DEC-0021-secrets-at-rest-are-sealed-by-a-master-password-vault.md).
  Depends on `US-0156` (merged, main @edf77c55).

## Outcome

A user can set up a master password from Settings > Security, see whether it is set and
whether this process holds the key, unlock it, change it, drop the key ("Lock now") and reset it
(typed confirmation). The runtime phase lives in one `VaultState` global in `oneterm-state`
that `US-0158` (saved SSH passwords) and `US-0159` (idle lock) build on, and every Argon2id run
happens off the UI thread. The elevated window shows the page read-only.

## Scope

- [ ] In scope:
  - `crates/state/src/vault_state.rs` (new): `VaultState` global entity, phases
    Uninitialised / Locked (the header read once at start) / Unlocked (`UnlockedVault`) /
    Unavailable (damaged or newer `vault.json`), `VaultEvent`, set up / unlock / change / lock /
    reset, the failure count and back-off, the silent rehash after an unlock.
  - `crates/state/src/vault_dialogs.rs` (new): the four dialogs (Set up, Unlock, Change, Reset)
    on `FormDialog`, and `ensure_unlocked(window, cx, on_ready)` for `US-0158`.
  - `crates/settings-ui/src/security.rs` (new): the "Security" page, registered after SSH.
  - `oneterm-state -> oneterm-vault` edge (`Cargo.toml`, `scripts/dependency-graph-policy.json`);
    `VaultState::init` called from `crates/app/src/init.rs`.
  - `oneterm-vault`: a `test-support` cargo feature exposing set-up and rewrap with explicit KDF
    parameters, so other crates' tests stay at `KdfParams::FLOOR` (the established
    `oneterm-terminal` `test-support` pattern; production code never enables it).
  - Docs listed under Documentation.
- [ ] Out of scope: the lock screen, idle timer, `IdleLocked` phase, `LockOneTerm` action and the
  "Lock after idle" setting (`US-0159`; "Lock now" here drops the key only); `sealed_password`,
  the session / connect dialog flows and the wipe of sealed values on reset (`US-0158`, which
  subscribes to `VaultEvent::Reset`); re-reading `vault.json` before a seal (`US-0158`,
  `persistence-and-migration.md` § 8).

## Acceptance

From the `IN-0050` US-0157 candidate, made concrete:

- [ ] Settings > Security shows the status (Not set up / Set, locked / Set, unlocked /
  Unavailable with the reason) and only the buttons that apply: Set up (Uninitialised), Unlock
  (Locked), Change (Locked or Unlocked), Lock now (Unlocked), Reset (Locked, Unlocked or a
  damaged file).
- [ ] Set up: entered twice, at least 8 characters, no composition rules, a length-based
  strength hint, the irreversibility warning ("forgot means wipe"), mismatch shown inline.
  Verified on Windows: `vault.json` holds no password and carries the Argon2id parameters.
- [ ] Restart, then Unlock from the page; Change (old + new twice): `vault.bak` equals the new
  `vault.json` afterwards; Reset (type `RESET`): `vault.json` and `vault.bak` gone. Each step
  verified on Windows with the files inspected.
- [ ] A wrong password shows an inline error and clears the input; from the 5th consecutive
  failure the next attempt waits `2, 4, 8, 16, 30, 30 ...` seconds with a visible countdown;
  never a lockout, never a wipe; a success resets the count.
- [ ] Argon2id runs on the background executor: the confirm button shows a spinner while it
  runs and the window keeps drawing (a frame captured during an unlock).
- [ ] After an unlock whose header is below the default parameters, a background rehash
  rewraps it with the typed password kept until it finishes; `ReadOnly`, `VaultReplaced` and
  `ChangedElsewhere` skip it silently.
- [ ] Locked never re-reads `vault.json`: unlock verifies against the header held in memory.
- [ ] Elevated window: status and "Managed from the normal OneTerm window."; no Set up, Change
  or Reset (refused in `VaultState` too); Unlock and Lock now allowed (Q5).
- [ ] Unit tests: Uninitialised -> set up -> Unlocked -> lock -> unlock; wrong-password count
  and back-off; rehash skip on the three refusals; elevated read-only; all at
  `KdfParams::FLOOR`, no temporary directory left behind, timing asserted only as ratios.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0050-master-password/IN-0050.md` — US-0157 candidate, Q1-Q10 rulings.
- `high-level-design.md` — state machine, UI wireframes, `VaultState` placement.
- `low-level-design/ssh-password-save-flow.md` — dialog UX (twice-entry, strength hint,
  warning) and the `ensure_unlocked` interface `US-0158` consumes.
- `low-level-design/idle-lock-overlay.md` — `VaultPhase` / `VaultEvent` shapes, wrong-password
  and back-off rules, "the lock never reads `vault.json`".
- `low-level-design/persistence-and-migration.md` § 7-8 — elevated instance read-only, several
  processes.
- `US-0156-vault-core.md` (carry-forwards) and `evidence/US-0156-verify.md` (R1-R5).
- `docs/gui-layout.md` § Settings window — the page list and the short-page rule.
- `docs/agents/structure.md`, `docs/agents/crate-dependency-rules.md` — the new
  `state -> vault` edge is downward (L2 -> L1).
- `docs/agents/persistence.md`, `docs/agents/error-policy.md`, `docs/agents/code-style.md`.

### Documentation Action

- Update required:
  - `docs/gui-layout.md`: the Settings page table gains "Security".
  - `high-level-design.md`: the Settings > Security wireframe and the state machine notes as
    built (no Screen lock group until `US-0159`; the Unavailable phase).
  - `docs/agents/structure.md`: the `state` row (depends on `vault`; owns `VaultState` and the
    master-password dialogs) and the `vault` row (`test-support` feature).
  - `IN-0050.md`: US-0157 created.

Reason: a new Settings page and a new runtime global other packets build on.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

## Context

- `oneterm_vault::unlock(&header, pw)` never reads the file; `UnlockedVault::change_password`
  takes the old password and is refused (`VaultReplaced` / `ChangedElsewhere`) when the file is
  not the header it was unlocked from; `reset` refuses an `Unsupported` file.
- gpui `InputState` keeps its text as a `SharedString`, which cannot be zeroized: the dialogs
  copy it into a `SecretString` once at submit, clear the input after a failure and drop it with
  the dialog (`IN-0050` Security Analysis 4, documented limit).

## Plan

- [ ] Records first (commit 1, docs only): this packet, `IN-0050.md` candidate ticked.
- [ ] `VaultState` + tests; dialogs; Security page; app init; policy; docs (commit 2).
- [ ] Gate; GUI walk on Windows with a private config directory; evidence here.

### Choices the LLDs leave open (simplest taken)

- **One unlock path.** Both the page and (later) the lock screen verify against the header
  held in memory. The idle-lock LLD's interface sketch had the dialog unlock re-read
  `vault.json`; the brief and the passer-by rule win: a password changed in another process is
  picked up at the next start.
- **Change and rehash on a second instance.** The `UnlockedVault` cannot leave the state while
  it is unlocked, so change = `unlock(header, old)` + `change_password` on that instance (one
  extra Argon2id run), which then replaces the state's vault. One operation at a time (`busy`);
  "Lock now" is always allowed and a finishing rehash then updates the held header.
- **Failure count** counts a wrong *old* password in Change as well as Unlock.
- **Reset order:** `VaultEvent::Reset` is emitted before `vault.json` is removed, so owners of
  sealed values wipe them first (`oneterm_vault::reset` doc).
- **No new action** in this packet: `LockOneTerm` belongs to `US-0159`.
- **`ensure_unlocked`** calls `on_ready` only on success (set up when Uninitialised, unlock when
  Locked, immediately when Unlocked); cancel calls nothing.

## Decisions

- [`DEC-0021`](../../decisions/DEC-0021-secrets-at-rest-are-sealed-by-a-master-password-vault.md)
  (inherited); no new decision.

## Verification Plan

- Unit: `cargo test -p oneterm-state` (the `VaultState` transitions above, the back-off table,
  the strength hint, the rehash-skip classification), `cargo test -p oneterm-settings-ui`,
  `cargo test -p oneterm-vault`.
- Gate: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `python scripts/verify-dependency-graph.py`, `python scripts/check-doc-paths.py`,
  `python scripts/check-english.py`, `python scripts/third-party-notices.py --check`,
  `python scripts/check-ignored-tests.py` if memory allows.
- E2E on Windows (debug `oneterm-app`, private config directory): set up -> inspect
  `vault.json` -> restart -> unlock from the page -> change (`vault.bak` equals `vault.json`) ->
  reset (both files gone) -> wrong password 6 times (inline error, back-off countdown); a
  screenshot while an unlock runs shows the spinner. Screenshots in the session scratchpad.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

After implementation, record commands, results, and anything skipped, unavailable, partial, or failing.

## Handoff

Implementer: a subagent in an isolated worktree; the integrating actor verifies independently
before merging into `main`.
