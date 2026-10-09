# Work: Save and use SSH passwords

ID: US-0158
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

- [ ] In scope:
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
- [ ] Out of scope (follow-ups per the LLD § 7): Quick Connect saving a password; Duplicate of
  a live tab reusing saved passwords; private-key passphrases; one-click connect; updating a
  hop's saved password from the connect dialog (the hop's own session dialog does it); the idle
  lock (`US-0159`).

## Acceptance

From the `IN-0050` US-0158 candidate, made concrete:

- [ ] Session dialog (Password method): "Save password" defaults to unticked; ticking it
  requires a username and a non-empty password; Save runs Set up (Uninitialised) or Unlock
  (Locked) first and seals only after it succeeds; cancel leaves the dialog open and nothing
  saved; an Unavailable vault refuses with its reason.
- [ ] Save, restart, connect after one unlock (loopback `sftp-dev-server --password`).
- [ ] A two-hop chain with two saved passwords connects without typing either (loopback
  `direct-tcpip` if the dev server gets it; otherwise the owner's real OpenSSH host).
- [ ] A wrong saved password is rejected by `--password`: the row reveals the input with
  "Update saved password" ticked, and the sealed value stays in `ssh_session.json` until a new
  password connects.
- [ ] Editing the host while unlocked keeps a working saved password (resealed for the new
  address); while locked, Save asks to unlock or remove.
- [ ] Duplicate Session keeps the sealed value and it still opens.
- [ ] Reset wipes every `sealed_password` (idempotent; the file and its `.bak` hold none
  afterwards).
- [ ] `ssh_session.json` holds only `otv1:` envelopes: a search of the config directory
  (including `.bak`) for the plaintext finds nothing.
- [ ] No schema bump: a v2 document with `sealed_password` loads and round-trips the envelope
  byte for byte; a non-string `sealed_password` loads as `None` without quarantine; an
  unknown-version envelope (`otv2:`) round-trips untouched; the pre-change struct (old binary
  simulation) loads the new document, keeps every session and drops the field on write.
- [ ] Before each seal `vault.json` is re-read: a different or missing vault refuses the seal
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

Filled at completion.

## Context

- `oneterm-ssh` already takes `SshAuthMethod::Password { password: SecretString }`
  (zeroizing, `Debug` = `***`) and logs only "authenticating with password"; a saved password
  reaches it the same way a typed one does, so the backend does not change.
- `VaultState::unlocked()` hands out the `UnlockedVault` for `open`; `ensure_unlocked` runs its
  callback only on success.

## Plan

- [x] Records first (commit 1, docs only): this packet, `IN-0050.md` candidate ticked.
- [ ] Core write, session field + store wipe, `VaultState::seal`, dialogs, dev server, tests,
  docs.
- [ ] Gate; E2E on Windows with a private profile and loopback dev servers; evidence here.

### Choices the LLD leaves open

Filled as built.

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
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Filled at completion.

## Handoff

Implementer: a subagent in an isolated worktree; the integrating actor verifies independently
before merging into `main`.
