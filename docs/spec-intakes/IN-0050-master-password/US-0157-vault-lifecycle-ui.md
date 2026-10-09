# Work: Master password lifecycle UI

ID: US-0157
Intake: IN-0050
Created: 2026-10-09

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
- [ ] Changed
- [x] Reopened (acceptance rework)
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

- [x] Settings > Security shows the status (Not set up / Set, locked / Set, unlocked /
  Unavailable with the reason) and only the buttons that apply: Set up (Uninitialised), Unlock
  (Locked), Change (Locked or Unlocked), Lock now (Unlocked), Reset (Locked, Unlocked or a
  damaged file).
- [x] Set up: entered twice, at least 8 characters, no composition rules, a length-based
  strength hint, the irreversibility warning ("forgot means wipe"), mismatch shown inline.
  Verified on Windows: `vault.json` holds no password and carries the Argon2id parameters.
- [x] Restart, then Unlock from the page; Change (old + new twice): `vault.bak` equals the new
  `vault.json` afterwards; Reset (type `RESET`): `vault.json` and `vault.bak` gone. Each step
  verified on Windows with the files inspected.
- [x] A wrong password shows an inline error and clears the input; from the 5th consecutive
  failure the next attempt waits `2, 4, 8, 16, 30, 30 ...` seconds with a visible countdown;
  never a lockout, never a wipe; a success resets the count.
- [x] Argon2id runs on the background executor: the confirm button shows a spinner while it
  runs and the window keeps drawing (a frame captured during an unlock).
- [x] After an unlock whose header is below the default parameters, a background rehash
  rewraps it with the typed password kept until it finishes; `ReadOnly`, `VaultReplaced` and
  `ChangedElsewhere` skip it silently.
- [x] Locked never re-reads `vault.json`: unlock verifies against the header held in memory.
- [ ] Elevated window: status and "Managed from the normal OneTerm window."; no Set up, Change
  or Reset (refused in `VaultState` too); Unlock and Lock now allowed (Q5).
- [x] Unit tests: Uninitialised -> set up -> Unlocked -> lock -> unlock; wrong-password count
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

Docs changed, as planned: `docs/gui-layout.md` (Security row in the page table, a paragraph on
the page, "thirteen pages"), `high-level-design.md` (state machine as built, Settings > Security
wireframe as built), `docs/agents/structure.md` (state tree and row, vault `test-support`,
settings-ui file list), `IN-0050.md` (US-0157 created). Also `docs/architecture.md` (the
`oneterm-state` ownership row), which the plan had not listed. Not changed: `persistence.md`
(no new document or field), `crate-dependency-rules.md` (`state -> vault` is L2 -> L1, allowed by
R2; the edge is in `scripts/dependency-graph-policy.json`).

Rework (after [`evidence/US-0157-verify.md`](evidence/US-0157-verify.md)): also changed
`low-level-design/persistence-and-migration.md` § 8 (no unlock re-reads `vault.json`; the one
re-read is `AlreadyInitialised` from Uninitialised), `low-level-design/idle-lock-overlay.md`
§ 5 (back-off formula `min(2^(n-4), 30)`, matching its own list) and § Interfaces (`unlock`
comment), and the as-built state-machine note in `high-level-design.md` (a lock taken during an
operation wins).

## Context

- `oneterm_vault::unlock(&header, pw)` never reads the file; `UnlockedVault::change_password`
  takes the old password and is refused (`VaultReplaced` / `ChangedElsewhere`) when the file is
  not the header it was unlocked from; `reset` refuses an `Unsupported` file.
- gpui `InputState` keeps its text as a `SharedString`, which cannot be zeroized: the dialogs
  copy it into a `SecretString` once at submit, clear the input after a failure and drop it with
  the dialog (`IN-0050` Security Analysis 4, documented limit).

## Plan

- [x] Records first (commit 1, docs only): this packet, `IN-0050.md` candidate ticked.
- [x] `VaultState` + tests; dialogs; Security page; app init; policy; docs (commit 2).
- [x] Gate; GUI walk on Windows with a private config directory; evidence here.

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
  Locked, immediately when Unlocked); cancel calls nothing, and so does an Unavailable vault (the
  caller shows why). The LLD sketch's `UnlockReason` / `UnlockOutcome` (with "Type passwords
  instead") are left to `US-0158`, which owns that button.
- **Back-off sequence.** The idle-lock LLD gives the formula `min(2^(n-5), 30)` but lists
  `2, 4, 8, 16, 30`; the intake says "2, 4, 8 ... capped at 30 s". The listed sequence is built
  (`2^(n-4)` from the 5th failure).
- **Cancel while busy.** Cancel closes the dialog at once; the background operation still
  completes and `VaultState` applies it (a set-up started and then cancelled leaves a vault).
- **Read errors** (`vault.json` unreadable, not just damaged) are Unavailable with no Reset.
- **Spinner.** GPUI Kit draws a button's spinner only in its icon slot, so a busy confirm button
  is given an icon (`LoaderCircle`) that the spinner replaces.
- **`VaultState::load(dir, read_only)`** is public: `init` uses it, and other crates' tests (the
  Security page here, the session dialogs in `US-0158`) build a state on a temporary directory.
- **Icon:** `crates/theme/assets/icons/lock.svg` (Lucide, like the other app icons) gives
  `AppIcon::Lock` to the page; `US-0159`'s lock screen can reuse it.
- **Set-up text** does not mention the screen lock yet (it does not exist until `US-0159`).

### Rework after independent verification (2026-10-09)

[`evidence/US-0157-verify.md`](evidence/US-0157-verify.md): PASS with two Medium latent defects,
fixed before merge because `US-0158` / `US-0159` build on `VaultState`. The verifier's commit
(`8033a6f0`, 7 tests) is the base of the rework.

| Defect | Fix |
| --- | --- |
| D1 (Medium): `busy` was cleared only inside the task handed to the caller, so a dropped task left the state busy until restart | `crates/state/src/vault_state.rs` `run` (`:419`): every operation (set-up, unlock, change, reset, rehash) goes through it. The state spawns **and detaches** the apply step itself and answers through a `futures::channel::oneshot`; the caller's task only awaits the receiver, so dropping it loses the answer, never the state update. `futures` (already in `Cargo.lock`) is a new direct dependency of `oneterm-state`. Test `verifier_d1_a_dropped_operation_task_still_applies` (inverted). |
| D2 (Medium): a Lock taken while a change ran was undone (key put back, `Unlocked` emitted) | `lock()` (`:351`) records `locked_meanwhile` whenever an operation runs, whatever the phase; `begin` / `end` (`:442`, `:448`) reset and read it. Every `finish_*` lets the lock win: set-up keeps only the new header (emits `Initialised` then `Locked`, `:468`), unlock drops the key and returns `VaultActionError::LockedMeanwhile` (`:500`; the right password still resets the failure count, no rehash), change keeps only the new header and emits nothing (`:526`), rehash keeps only the new header (`:579`), reset had already dropped the key (`:548`). `US-0159`'s `IdleLocked` reuses the same flag. Tests `verifier_d2_a_lock_during_a_change_wins` (inverted), `a_lock_during_an_unlock_wins`, `a_lock_during_a_set_up_wins`. |
| D3 (Low): `InputState::value()` copied the text on every call (once per field at submit, once per frame for the hint) | `crates/state/src/vault_dialogs.rs` `submit` (`:286`): each field is copied once, `text().to_string()` straight into a `SecretString`, and the new password / confirmation are compared through `expose_secret()`; the hint takes `text().len_chars()` (`:437`), which copies nothing. Module doc (`:10`) states the real behaviour. |
| D4 (Low, docs) | LLD corrections listed under Reconciliation. |
| Elevated `ensure_unlocked` with no vault (verifier attack f) | `vault_dialogs.rs` `ensure_unlocked` (`:89`): shows a warning notification "No master password is set up. Managed from the normal OneTerm window." instead of a Set up dialog every submit would refuse. The note is one constant, `vault_dialogs::READ_ONLY_NOTE` (`:42`), which the Security page reuses. |

### Notes for `US-0158` (and `US-0159`) from the verification

- **Wipe idempotently on `VaultEvent::Reset`.** It is emitted before the removal; the removal
  can still fail, leaving the state Locked with `vault.json` in place. Do not assume
  Uninitialised follows `Reset`. A reset from Unlocked emits only `Reset`, never `Locked`.
- **`ensure_unlocked`** runs `on_ready` on success only: never on cancel, never when the vault
  is Unavailable (show the reason yourself), and in an elevated window with no vault it shows
  the read-only note. Check `is_read_only()` before offering to save a password.
- **A cancelled dialog does not mean nothing changed:** the operation completes and applies;
  re-read the phase.
- **An unlock can answer `LockedMeanwhile`** (the password was right but a lock won).
- **Still owed by `US-0158`:** the re-read of `vault.json` before each seal
  (`persistence-and-migration.md` § 8, "Reset in A while B is unlocked").
- **`US-0159`:** `lock()` acts only on Unlocked but records a lock during any operation;
  `IdleLocked` must set the same `locked_meanwhile` flag so every `finish_*` keeps the screen
  locked.

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
- [x] Unit proof
- [ ] Integration proof
- [x] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Worktree gate, 2026-10-09, Windows 11, `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=3` (2 for the
app build and the census: the first app build crashed rustc with `0xc0000409` on `windows` at
3 jobs, the known memory limit; the retry at 2 passed):

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo test -p oneterm-state` | 59 passed (16 new: 13 `vault_state_tests`, 3 dialog helpers) |
| `cargo test -p oneterm-settings-ui` | 58 passed, 1 ignored (4 new: Security page rows) |
| `cargo test -p oneterm-settings-ui --lib -- --ignored --test-threads=1` | 1 passed (the existing elevation test) |
| `cargo test -p oneterm-actions` | 4 passed |
| `cargo test -p oneterm-vault` | 50 passed |
| `cargo test -p oneterm-theme -p oneterm-app` | pass |
| `python scripts/verify-dependency-graph.py` | pass (21 packages; `argon2` / `aes-gcm` still named only by `oneterm-vault`) |
| `python scripts/check-doc-paths.py` | pass (223 paths) |
| `python scripts/check-english.py` | pass (1112 files) |
| `python scripts/third-party-notices.py --check` | pass, unchanged (`argon2` / `aes-gcm` already reached the app through `russh`) |
| `python scripts/check-ignored-tests.py` (`CARGO_BUILD_JOBS=2`) | pass, 19 ignored tests, all recorded (none added) |

- `%TEMP%` after the runs: no `oneterm-vault-state-*` or `oneterm-security-page-*` directory.
- The unit tests run Argon2id at `KdfParams::FLOOR` (fixture `vault-v1.json` and the
  `test-support` constructors); timing is never asserted.

**GUI walk (debug `oneterm.exe` from this worktree, private profile: `USERPROFILE` and the
working directory in a scratch folder, so `config_dir()` = `<scratch>/cwd/target`; driven by
posted window messages to the launched pid only; screenshots `us0157-*.png` in the session
scratchpad):**

| Step | Result |
| --- | --- |
| Settings > Security, no vault (`03`) | "Not set up." and **Set Up...** only |
| Set up dialog (`04`-`06`) | intro, two masked fields with eye toggles, the warning; "abc" shows "Too short: use at least 8 characters."; 12 characters shows "Strength: good."; a different confirmation shows "The two passwords differ." inline |
| Create (`07b`, `08`) | while Argon2id runs the inputs, Create and the page's Set Up are disabled and the window keeps drawing; then "Set, unlocked." with Change, Lock Now, Reset |
| `vault.json` after set-up | schema 1, `argon2id` v19, 65536 KiB / 3 passes / 1 lane, 16-byte salt, `otv1:` wrapped key; the password text is absent; `vault.bak` byte-equal |
| Restart (`09`) | "Set, locked. Saved passwords need it before they can be used." with Unlock, Change, Reset |
| Wrong password (`11a`, `11b`) | spinner in the Unlock button during the run (the frame proves the UI thread drew while Argon2id ran); then "Wrong master password." and the input cleared |
| Wrong 5, 6, 7 times (`12`-`14`) | after the 6th: "Too many wrong attempts. Try again in 4 s." and Unlock disabled; the 7th, typed after the wait, gave 8 s ("7 s" a second later); the countdown ticks and clears (`16`) |
| Right password (`17`) | "Set, unlocked."; `vault.json` unchanged (defaults: no rehash) |
| Lock Now (`18`) | back to "Set, locked." |
| Change from Locked (`19`-`21b`) | current + new twice, strength hint, spinner during the run; then "Set, unlocked."; `vault.json` has a new salt and wrapped key under the same vault id, `vault.bak` byte-equal to it, the old wrapped key gone from both, no plaintext in either |
| Lock, unlock with the new password (`22`) | unlocked |
| Reset (`23`-`25`) | typing `reset` shows "Type RESET to confirm."; `RESET` removes `vault.json` and `vault.bak`; page back to "Not set up." |
| Rehash (`26`) | the floor-parameter fixture placed as `vault.json`, unlocked from the page: the file is rewritten at 65536 KiB / 3 passes with a new salt, same vault id, `vault.bak` equal |
| Change after the rehash (`27`) | succeeds (the state holds the rewrapped header, so no "changed elsewhere") |
| Logs | no vault warning; only the pre-existing DirectX debug-layer lines |

Gaps:

- **Elevated window not walked.** It needs a UAC-elevated second process; the read-only
  behaviour is proved by unit tests only (`the_elevated_window_unlocks_but_never_writes`,
  `the_elevated_page_is_read_only_but_unlocks`). The Elevated acceptance item stays open for
  the verifier or the owner.
- No frame-rate measurement during Argon2id: responsiveness is shown by frames captured while
  the operation ran (spinner drawn, buttons disabled), not by a timing.
- Two-process races (set-up in two instances, change in one while the other is locked) are
  covered by `oneterm-vault`'s tests and the `VaultState` tests with a second vault written to
  the directory, not by two GUI instances.
- Linux / macOS not run (CI). The full `cargo test --workspace` runs on `main` after
  independent verification.

**Rework gate** (worktree, `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`):

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo test -p oneterm-state` | 68 passed (66 after the verifier's 7, plus 2 new; D1 and D2 tests inverted) |
| `cargo test -p oneterm-settings-ui` | 58 passed, 1 ignored; `--ignored --test-threads=1`: 1 passed |
| `cargo test -p oneterm-vault` / `-p oneterm-actions` | 50 / 4 passed |
| `python scripts/verify-dependency-graph.py` | pass |
| `python scripts/check-doc-paths.py` | pass (223 paths) |
| `python scripts/check-english.py` | pass (1113 files) |
| `python scripts/third-party-notices.py --check` | pass, unchanged |
| `python scripts/check-ignored-tests.py` | pass, 19 ignored tests, all recorded |

The rework changes no UI: the GUI walk was not repeated. The Elevated acceptance item stays
open for the owner's walk.

## Handoff

Implementer: a subagent in an isolated worktree; the integrating actor verifies independently
before merging into `main`.
