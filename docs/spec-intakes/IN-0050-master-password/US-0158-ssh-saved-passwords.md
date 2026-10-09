# Work: Save and use SSH passwords

ID: US-0158
Intake: IN-0050
Created: 2026-10-09

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability
- Risk lane: high-risk (secrets at rest, a persisted field in `ssh_session.json`, credentials
  sent to a server)
- Spec Intake, when required: [`IN-0050`](IN-0050.md); owner rulings Q1-Q10 accepted as
  recommended (2026-10-09; Q3: a saved password is bound to host, port and user; Q4: the
  connect dialog still opens, shows "Using saved password", Enter connects);
  [`DEC-0021`](../../decisions/DEC-0021-secrets-at-rest-are-sealed-by-a-master-password-vault.md).
  Depends on `US-0156` and `US-0157` (both on main @0e844d43).

## Outcome

A user can tick "Save password" in the session dialog, have the password sealed under the
master password (setting one up or unlocking first), and later connect to that session, and to
every jump hop that has a saved password, after typing the master password once per unlock.
`ssh_session.json` carries the password only as an `otv1:` envelope bound to the session's
host, port and user, in an optional field older OneTerm binaries ignore.

## Scope

- [x] In scope:
  - `crates/session-ui/src/session_state.rs`: `SshSession.sealed_password:
    Option<SealedSecret>` (optional, lenient, no schema bump); writes of `ssh_session.json`
    through a secret-aware core write (owner-only temp file on Unix on every write, `.bak`
    scrubbed only when the write retires a sealed value); wipe on `VaultEvent::Reset`.
  - `crates/core/src/persistence.rs`: the secret-aware whole-document write that separates
    "owner-only" from "scrub the backup" (`US-0156` carry-forward).
  - `crates/state/src/vault_state.rs`: seal through `VaultState`, which re-reads `vault.json`
    and compares `vault_id()` first (`persistence-and-migration.md` § 8, owed by `US-0157`).
  - Session dialog: password input + "Save password" for the Password method; saved-password
    row with Replace / Remove; reseal on host / port / user edits (Unlocked: transparent;
    Locked: unlock, or remove); removal on method change, cleared username or Remove.
  - Connect dialog: saved rows for the target and every hop ("Using saved password", "Use a
    different one"), one unlock for the whole chain, Enter connects; "Save password" on a typed
    target password, sealed only after the server accepted it; a rejected saved password
    reveals the input and keeps the sealed value until a new one is saved.
  - `crates/tools/src/bin/sftp-dev-server.rs`: `--password <value>` and, if cheap, a loopback
    `direct-tcpip` channel for a two-hop chain.
  - Docs listed under Documentation.
- [x] Out of scope (follow-ups per the LLD § 7): Quick Connect saving a password; Duplicate of
  a live tab reusing saved passwords; private-key passphrases; one-click connect; updating a
  hop's saved password from the connect dialog (the hop's own session dialog does it); the idle
  lock (`US-0159`).

## Acceptance

From the `IN-0050` US-0158 candidate, made concrete:

- [x] Session dialog (Password method): "Save password" defaults to unticked; ticking it
  requires a username and a non-empty password; Save runs Set up (Uninitialised) or Unlock
  (Locked) first and seals only after it succeeds; cancel leaves the dialog open and nothing
  saved; an Unavailable vault refuses with its reason.
- [x] Save, restart, connect after one unlock (loopback `sftp-dev-server --password`).
- [x] A two-hop chain with two saved passwords connects without typing either (loopback
  `direct-tcpip` if the dev server gets it; otherwise the owner's real OpenSSH host).
- [x] A wrong saved password is rejected by `--password`: the row reveals the input with
  "Update saved password" ticked, and the sealed value stays in `ssh_session.json` until a new
  password connects.
- [x] Editing the host while unlocked keeps a working saved password (resealed for the new
  address); while locked, Save asks to unlock or remove.
- [x] Duplicate Session keeps the sealed value and it still opens.
- [x] Reset wipes every `sealed_password` (idempotent; the file and its `.bak` hold none
  afterwards).
- [x] `ssh_session.json` holds only `otv1:` envelopes: a search of the config directory
  (including `.bak`) for the plaintext finds nothing.
- [x] No schema bump: a v2 document with `sealed_password` loads and round-trips the envelope
  byte for byte; a non-string `sealed_password` loads as `None` without quarantine; an
  unknown-version envelope (`otv2:`) round-trips untouched; the pre-change struct (old binary
  simulation) loads the new document, keeps every session and drops the field on write.
- [x] Before each seal `vault.json` is re-read: a different or missing vault refuses the seal
  ("reset in another OneTerm window") and drops the key.
- [ ] The v0.6.8 release binary loads a document carrying `sealed_password` without losing
  sessions: verified only if such a binary is available locally (no release build in this
  packet); otherwise recorded as a gap for the owner.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0050-master-password/IN-0050.md` — US-0158 candidate and acceptance.
- `low-level-design/ssh-password-save-flow.md` — the UX contract (owned by this packet).
- `low-level-design/persistence-and-migration.md` § 4-8 — field, compatibility, `.bak`,
  several processes (re-read before seal).
- `low-level-design/crypto-and-format.md` § 4-5 — context binding, open errors.
- `US-0156-vault-core.md`, `US-0157-vault-lifecycle-ui.md` — carry-forwards and notes for
  `US-0158`; `evidence/US-0156-verify.md`, `evidence/US-0157-verify.md`.
- `docs/ssh-authentication.md`, `docs/ssh-client-connect.md` — persisted auth data, connect
  dialog.
- `docs/agents/persistence.md` — `ssh_session.json` row, Secrets at rest.
- `docs/decisions/0001-ssh-key-secret-persistence.md`, `DEC-0021`.
- `docs/agents/crate-dependency-rules.md`, `docs/agents/structure.md`,
  `scripts/dependency-graph-policy.json` — a new `session-ui -> vault` edge (downward).
- `docs/agents/error-policy.md`, `docs/agents/code-style.md`.

### Documentation Action

- Update required: `ssh-password-save-flow.md` (as-built notes), `docs/ssh-authentication.md`
  (persisted data, saved passwords), `docs/ssh-client-connect.md` (connect dialog saved rows),
  `docs/agents/persistence.md` (`ssh_session.json` row: `sealed_password`; the secret-aware
  write), `DEC-0001` status line (US-0158 reference), `docs/agents/structure.md` (session-ui
  depends on vault), `docs/gui-layout.md` if a dialog section describes the auth rows,
  `IN-0050.md` (US-0158 created).

Reason: a new persisted field, a new dialog surface, and the first consumer of the vault.

### Reconciliation

Docs changed, as planned: `low-level-design/ssh-password-save-flow.md` (new "As built"
section), `docs/ssh-authentication.md` (persisted data, the save / use flow, non-goal wording),
`docs/ssh-client-connect.md` (§1.3 decision 1, new §4.8 "Saved passwords"),
`docs/agents/persistence.md` (`ssh_session.json` row: `sealed_password`; Secrets at rest:
`atomic_write_sealed`), `docs/decisions/0001-ssh-key-secret-persistence.md` (status line names
`US-0158`), `docs/agents/structure.md` (session-ui row and file list: `vault` edge,
`auth_form.rs`), `docs/gui-layout.md` (session row menu paragraph: the password row of the
Properties dialog), `IN-0050.md` (US-0158 created), `scripts/dependency-graph-policy.json`
(`oneterm-session-ui -> oneterm-vault`). Not changed: `crate-dependency-rules.md` (the new edge
is downward, L3 -> L1, allowed by R2; the rules list no per-crate edges), `docs/PROJECT.md`
(its invariant was already reworded by `US-0156`), `DEC-0021` (implemented as written).

## Context

- `oneterm-ssh` already takes `SshAuthMethod::Password { password: SecretString }`
  (zeroizing, `Debug` = `***`) and logs only "authenticating with password"; a saved password
  reaches it the same way a typed one does, so the backend does not change.
- `VaultState::unlocked()` hands out the `UnlockedVault` for `open`; `ensure_unlocked` runs its
  callback only on success.

## Plan

- [x] Records first (commit 1, docs only): this packet, `IN-0050.md` candidate ticked.
- [x] Core write, session field + store wipe, `VaultState::seal`, dialogs, dev server, tests,
  docs.
- [x] Gate; E2E on Windows with a private profile and loopback dev servers; evidence here.

### Choices the LLD leaves open (simplest taken)

Recorded once, in the LLD's "As built (`US-0158`)" section: form state as cells on
`SshAuthForm` instead of a `PasswordSource` enum; the `US-0157` Unlock dialog reused unchanged
(no line naming the sessions); session-dialog messages as toasts, and no note after a cancelled
Set up / Unlock; reseal without a vault drops the old envelope with a note, an Unavailable vault
refuses; hop failures matched by the existing `jump host user@host:port:` prefix (no structured
hop index, so no core or backend change); `on_connected` / `on_failed` gain the window / the
`AppError`; `VaultState::seal` re-reads `vault.json` on the UI thread and answers a mismatch
with the new `VaultActionError::ResetElsewhere` (key dropped, phase follows the file,
`VaultEvent::Locked`); the `.bak` scrub is decided by `atomic_write_sealed` under the lock from
the replaced document's sealed values (so it survives the store's coalescing queue, and deleting
a session with a saved password scrubs too); `lenient_sealed` warns without the session id;
Quick Connect uses a hop's saved password only while already unlocked; a password sealed after
a connect is stored only if the session still has that address and method; the dev server's
`direct-tcpip` relay is loopback-only and only shell channels echo.

## Decisions

- [`DEC-0021`](../../decisions/DEC-0021-secrets-at-rest-are-sealed-by-a-master-password-vault.md)
  (inherited); no new decision expected.

## Verification Plan

- Unit: `cargo test -p oneterm-session-ui -p oneterm-state -p oneterm-core -p oneterm-vault
  -p oneterm-ssh` (+ `-p oneterm-tools` for the dev server): store round trip with the sealed
  field and fixtures (`ssh-session-v2-sealed.json`, `ssh-session-invalid-sealed-type.json`),
  old-binary simulation, `otv2:` round trip, reseal decision table, wipe idempotent, the
  `.bak` scrub only on a retiring write, `VaultState::seal` refusing a replaced / removed
  vault, the dev server's password check. Argon2 at `KdfParams::FLOOR` via the vault
  `test-support` feature; RAII temp dirs; no timing assertions.
- Gate: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `python scripts/verify-dependency-graph.py`, `check-doc-paths.py`, `check-english.py`,
  `check-ignored-tests.py` if memory allows.
- E2E on Windows (debug `oneterm.exe` from this worktree, private `USERPROFILE` + working
  directory so `config_dir()` lands in scratch; loopback `sftp-dev-server --password`): save ->
  restart -> connect after one unlock; wrong saved password -> input revealed, envelope kept;
  `ssh_session.json` / `.bak` searched for the plaintext; host edit while unlocked -> still
  connects; reset -> no `sealed_password`; a two-hop chain if the dev server gains
  `direct-tcpip`. Screenshots `us0158-*.png` in the session scratchpad.
- Owner's step (not verifiable here): a real OpenSSH host (rejection wording, a real chain),
  Linux / macOS (`0600` mode), the v0.6.8 release binary against a document with
  `sealed_password` unless a local copy exists.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Worktree gate, 2026-10-09, Windows 11, `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`:

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo test -p oneterm-session-ui` | 87 passed, 1 ignored (the existing elevation test); 10 new or extended: v2-with-sealed fixture round trip (incl. `otv2:`), non-string field without quarantine, old-binary simulation, `.bak` scrub only on a retiring write, reset wipe idempotent, store only for its own address, method / username rule, duplicate keeps the envelope, reseal decision table, reseal at `KdfParams::FLOOR`, rejected-row routing + locked open (gpui) |
| `cargo test -p oneterm-state` | 75 passed (2 new: seal under the vault on disk; seal after a reset or replacement elsewhere refused, key dropped, phase follows the file) |
| `cargo test -p oneterm-core` | 92 passed (2 new: sealed-write scrub rule; Unix `0600` on every write, `cfg(unix)`, not run here) |
| `cargo test -p oneterm-ssh` | 109 passed (unchanged: the password path is the existing `SshAuthMethod::Password` one) |
| `cargo test -p oneterm-vault` | 50 passed |
| `cargo test -p oneterm-tools` | pass (1 new: `--password` accepts only its value) |
| `python scripts/verify-dependency-graph.py` | pass (21 packages; new edge `oneterm-session-ui -> oneterm-vault` in the policy) |
| `python scripts/check-doc-paths.py` | pass (223 paths) |
| `python scripts/check-english.py` | pass (1114 files) |
| `python scripts/check-ignored-tests.py` | **not run**: it builds every crate's tests, and the disk had 14 GB free; no `#[ignore]` was added |

`%TEMP%` after the runs: no `oneterm-session-*`, `oneterm-vault-*` or `oneterm-persistence-*`
directory left.

**E2E (debug `oneterm.exe` from this worktree, private profile: `USERPROFILE` and the working
directory in a scratch folder so `config_dir()` = `<scratch>/cwd/target`; two loopback dev
servers, `127.0.0.1:2222 --password alpha-pass-1` (jump host "bastion") and `:2223 --password
bravo-pass-2` (target "prod", jump host bastion); driven by posted window messages to the
launched pid only; screenshots `us0158-*.png` in the session scratchpad):**

| Step | Result |
| --- | --- |
| Session dialog, Password method (`04`, `09`) | password input + "Save password (encrypted with your master password)", unticked |
| Save ticked, no vault (`10`) | "Create a master password" stacked on the session dialog; Cancel (`12`) leaves it open with the box ticked, nothing written, no `vault.json` |
| Create (`13`, `14`) | both dialogs close; `ssh_session.json` gains `"sealed_password": "otv1:..."` for bastion; prod saved the same way while unlocked (`17`), no prompt |
| Restart, open prod (`18`, `19`) | Unlock stacked on the connect dialog; after one unlock both rows (jump host and target) read "Using saved password" |
| Enter (`20`-`24`) | Enter connects; jump host key, then target key trusted; dev servers log `ops` accepted, `direct-tcpip ... relaying`, `deploy` accepted; tab + SFTP open: **two-hop chain, two saved passwords, nothing typed** (the first try exposed a dev-server echo of relayed bytes, fixed in the same commit series before this run) |
| Target password changed on the server (`27`) | "SSH authentication failed: rejected by the server"; the target row is an empty input with "Update saved password" ticked and "The server rejected the saved password. Type the current one."; the jump host row stays saved; the envelope is still in the file |
| New password typed, Connect (`29`, `30`) | connected; "Password saved."; the target's envelope replaced |
| Host edited to `localhost` while unlocked (`31`-`36`) | "Saved password [Replace] [Remove]" in the dialog; Save reseals (new envelope); connecting to `localhost:2223` through the jump host succeeds with no typing |
| Lock Now, host edited back (`39`-`42`) | Save shows "Unlock to keep the saved password for the new address, or click Remove." and the Unlock dialog; the file is unchanged; Cancel + Remove + Save stores the session without `sealed_password`, and `ssh_session.bak` equals the new document (scrubbed) |
| Duplicate Saved Session (`43`-`45`) | the copy holds the same envelope; it connects after one unlock |
| Settings > Security > Reset (`47`, `48`) | `vault.json` / `vault.bak` gone; `ssh_session.json` and `.bak` hold no `sealed_password` / `otv`; all 3 sessions kept |
| New vault, save, restart, one unlock, Enter (`49`-`52`) | connected to bastion with the saved password |
| Plaintext search | none of the five passwords (SSH and master) in any file of the private profile (incl. `.bak`, `known_hosts`), in the app logs or in the dev-server logs; no `otv1:` in the logs |

Gaps:

- **v0.6.8 release binary not run**: no `target/release/oneterm.exe` and no other pre-change
  build exists locally, and the packet builds none. The compatibility row is proved by the unit
  simulation (`an_older_binary_keeps_every_session_and_drops_the_field_on_write`); the owner's
  check with the real v0.6.8 binary remains.
- **Real OpenSSH host not used**: rejection wording and a real two-hop chain are walked on the
  loopback dev servers only.
- **Connect-dialog "Save password" on a first, typed password** (not after a rejection) and its
  set-up / unlock-first path were not walked; the "Update saved password" variant of the same
  `on_connected` path was.
- **Linux / macOS not run** (`0600` test is `cfg(unix)`); elevated window not walked (M1 hides
  every SSH surface there; no new code path).
- **Pre-existing debug-only panic seen once**: after Windows activated accessibility in the
  walked process, opening the session row's context menu panicked in gpui's a11y builder
  ("set_focus called more than once in a single frame", stack in
  `gpui_component::menu::popup_menu::PopupMenu` prepaint, `gpui-pre 0.3.7 a11y.rs:533`); release
  builds only warn. Not in code this packet touches; the walk continued after a restart.
- `check-ignored-tests.py` skipped for disk space (see the table).

## Handoff

Implementer: a subagent in an isolated worktree; the integrating actor verifies independently
before merging into `main`.
