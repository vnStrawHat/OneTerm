# Work: Vault core (`oneterm-vault`), no UI

ID: US-0156
Intake: IN-0050
Created: 2026-10-09

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [ ] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability
- Risk lane: high-risk (secrets, encryption at rest, a new persisted document and a new
  persisted value format, a rule that supersedes `DEC-0001` in part)
- Spec Intake, when required: [`IN-0050`](IN-0050.md); owner ruling 2026-10-09: every Q1-Q10
  recommendation accepted as written (Q9: new crate `oneterm-vault`; Q10: no DPAPI);
  [`DEC-0021`](../../decisions/DEC-0021-secrets-at-rest-are-sealed-by-a-master-password-vault.md)
  accepted.

## Outcome

A new gpui-free crate `oneterm-vault` turns a master password into the ability to seal and open
secrets, and owns the two formats that become contracts: the `vault.json` header (schema 1) and
the `otv1:` envelope. Nothing in this packet is reachable from the UI yet: no dialog, no Settings
page, no saved SSH password, no lock screen (`US-0157`..`US-0159`).

## Scope

- [x] In scope:
  - `crates/vault` (`oneterm-vault`, L1, depends on `oneterm-core` only among workspace crates):
    Argon2id KEK (`kdf.rs`), wrapped random data key and seal / open with associated data
    (`vault.rs`), the envelope and its strict parser (`envelope.rs`), `vault.json` load outcomes
    and the one write function with the elevated refusal (`document.rs`), typed errors
    (`error.rs`), fixtures under `tests/fixtures/persistence/`.
  - `crates/core/src/persistence.rs`: `update_secret_json_file` (backup scrubbed to a copy of the
    new document; owner-only files on Unix) and `remove_document` (file + `.bak` under the lock).
  - Root `Cargo.toml`: member `crates/vault`, workspace declarations `argon2` 0.6 and `aes-gcm`
    0.11 (already in `Cargo.lock`), `oneterm-vault` path entry.
  - `scripts/dependency-graph-policy.json` + `scripts/verify-dependency-graph.py`: the crate's
    edges, and `restricted_third_party_dependencies` (only `oneterm-vault` may name `argon2` /
    `aes-gcm`, `DEC-0021` rule 3).
  - Docs listed under Documentation.
- [x] Out of scope: `VaultState`, the four dialogs and the Settings "Security" page (`US-0157`);
  `sealed_password` and the save / connect flows (`US-0158`); the idle lock (`US-0159`);
  `atomic_write_scrubbing_backup` (whole-document variant for `ssh_session.json`, first needed by
  `US-0158`'s "Remove saved password" and reset wipe; with the private `Sensitivity` switch in
  `persistence.rs` it is a three-line wrapper); the silent rehash after unlock (the caller's job:
  `needs_rehash()` + `change_password(dir, pw, pw)`, wired by `US-0157`).

## Acceptance

- [ ] RFC 9106 § 5.3 Argon2id vector passes against the `argon2` crate; a frozen vector passes
  through OneTerm's own KDF wrapper; the NIST CAVP AES-256-GCM vector (`gcmEncryptExtIV256`,
  96-bit IV, 128-bit PT and AAD) passes through OneTerm's own seal / open internals.
- [ ] The `otv1:` layout of an SSH-password envelope is frozen by a known-answer test (fixed key,
  vault id and nonce through a test-only constructor), and `vault-v1.json` plus a sealed password
  made by an **independent** implementation unlock and open through the crate.
- [ ] Seal / open round trip; each seal uses a fresh nonce; a 1024-byte secret seals, 1025 does
  not.
- [ ] Wrong password -> `WrongPassword` (never trimmed: a trailing space is wrong); empty ->
  `EmptyPassword`; a flipped kid -> `OtherVault`; a flipped nonce, ciphertext or tag byte, or
  another host / port / user -> `Rejected`; another vault -> `OtherVault`; `otv2:` ->
  `NewerFormat`; plain text -> `Malformed`.
- [ ] An unknown-version or plain-text value round-trips through serde byte for byte.
- [ ] `change_password` checks the old password, rewraps the same data key under a fresh salt:
  the old password stops working, the new one works, items sealed before still open,
  `vault.bak` equals `vault.json`, the old wrapped key is gone from both, and unknown fields at
  top level and inside `kdf` survive. Refused (`VaultReplaced`, file untouched) when the file on
  disk is another vault.
- [ ] `reset` removes `vault.json` and `vault.bak`, works on a damaged or absent file.
- [ ] Set-up is create-only: a second set-up or two concurrent ones give exactly one vault and
  `AlreadyInitialised`; a damaged file is never overwritten.
- [ ] Load: missing -> `Uninitialised`; read error -> `AppError::ConfigLoad`; every field rule of
  `persistence-and-migration.md` § 2-3 gives its `Damaged` / `Unsupported` outcome, including
  parameters below the floor (19 MiB / 2 passes) or above the ceiling; unknown fields are
  ignored; **no load ever writes, renames or quarantines**.
- [ ] Elevated: set-up, change and reset each return `ReadOnly` with nothing changed on disk,
  proved by ordinary unit tests through the `elevated` parameter (no new `#[ignore]`d test).
- [ ] An unlock verifies only against the header the caller holds: replacing `vault.json` with a
  passer-by's vault neither accepts the passer-by's password nor rejects the owner's.
- [ ] No `Debug` / `Display` output carries a password, a key or an envelope payload;
  `UnlockedVault` is neither `Clone` nor `Debug`.
- [ ] Unlock cost contract (ratios only): a wrong password costs a full KDF like a right one
  (ratio within 0.5..=2), and the defaults cost more than twice the floor. The default-parameter
  unlock time on this machine is recorded below.
- [ ] `Cargo.lock` gains no `[[package]]` other than `oneterm-vault`;
  `python scripts/third-party-notices.py --check` passes unchanged; `cargo tree -i argon2` and
  `-i aes-gcm` list `oneterm-vault` as the only first-party user (enforced by
  `verify-dependency-graph.py`).
- [ ] No test leaves a temporary directory behind (RAII guard, `BUG-0083` pattern).
- [ ] Gate in the worktree: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
  -- -D warnings`, `cargo test -p oneterm-vault`, `cargo test -p oneterm-core`, the Python
  checks of `AGENTS.md` § 4 that this change touches.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0050-master-password/IN-0050.md`, `high-level-design.md`,
  `low-level-design/crypto-and-format.md`, `low-level-design/persistence-and-migration.md`
  (owning contracts); `idle-lock-overlay.md`, `ssh-password-save-flow.md` (skimmed for the vault
  API they consume: `unlock(&header)` without a file read, `SealedSecret`).
- `docs/decisions/0001-ssh-key-secret-persistence.md` — superseded in part.
- `docs/PROJECT.md` — "No secrets are persisted" invariant and the SSH boundary line.
- `docs/ssh-authentication.md` — persistence policy and the vault non-goal.
- `docs/agents/persistence.md` — document table, load outcomes, elevation table.
- `docs/agents/structure.md`, `docs/agents/crate-dependency-rules.md`,
  `docs/agents/dependencies.md`, `docs/architecture.md` — crate registration and the dependency
  policy.
- `docs/agents/error-policy.md`, `docs/agents/code-style.md` — typed errors, no `let _` without a
  reason, sibling `*_tests.rs`.

### Documentation Action

- Update required:
  - `docs/decisions/DEC-0021-secrets-at-rest-are-sealed-by-a-master-password-vault.md` (new) and
    the `DEC-0001` status line.
  - `docs/PROJECT.md`: the invariant, the SSH boundary line, `vault.json` in the persistence list.
  - `docs/ssh-authentication.md`: the non-goal and a vault section.
  - `docs/agents/persistence.md`: `vault.json` row, the never-quarantine exception, the
    "Secrets at rest" envelope rule, the elevation row, `update_secret_json_file`.
  - `docs/agents/structure.md` (tree, table, conventions), `docs/agents/crate-dependency-rules.md`
    (L1 row, verification line), `docs/agents/dependencies.md` (Secrets row),
    `docs/architecture.md` (crate map, ownership shortcut).
  - `IN-0050.md`: rulings ticked, `US-0156` created.

Reason: a new crate, a new persisted document and a rule that reverses `DEC-0001` in part.

### Reconciliation

To be completed with the implementation commit.

## Context

- Crypto crates already in the lock via `russh 0.63.3 -> ssh-key 0.7.0-rc.11`: `argon2 0.6.0`,
  `aes-gcm 0.11.1`. Enabling `argon2`'s `zeroize` feature adds one dependency **edge**
  (`argon2 -> zeroize`, both already packages) to `Cargo.lock`, no package.
- `oneterm_core::SecretString` (zeroizing, `Debug` = `***`) carries passwords in and plaintexts
  out.

## Plan

- [x] Records first (commit 1, docs only): `DEC-0021`, `DEC-0001` note, `PROJECT.md`,
  `ssh-authentication.md`, `IN-0050.md` rulings, this packet.
- [ ] Crate, core helpers, policy, tests, fixtures, docs (commit 2).
- [ ] Gate; record evidence and the Argon2id timing here.

### Choices the LLDs left open (simplest taken)

- **Randomness:** OS CSPRNG through `aead::Generate::try_generate` (`getrandom 0.4`, reached by
  `aes-gcm`'s `getrandom` feature); an RNG failure is `VaultError::Randomness`, nothing written.
  No `rand` dependency.
- **Hex for `vault_id`:** two private helpers; no `hex` crate.
- **Elevated refusal:** one private `document::write(dir, change, elevated)` that every write
  goes through (create, rewrap, remove); public functions pass
  `oneterm_core::elevation::is_restricted()`, tests pass `true` / `false`.
- **Secret writes in core:** one public `update_secret_json_file` instead of the LLD's
  `atomic_write_scrubbing_backup` "or a flag": the vault needs a read-modify-write (set-up is
  create-only, change is vault-id-checked), and "holds secret material" decides both the
  scrubbed backup and the Unix `0600` mode, so one private `Sensitivity` switch carries both.
  Set-up therefore also writes `vault.bak` (equal to `vault.json`).
- **`change_password(dir, old, new)`** takes the old password (the LLD sketch did not): an
  unlocked vault keeps the data key only inside its cipher (§ 7), so the raw key needed for the
  rewrap is obtained by unwrapping with the old password, which also re-verifies it.
- **Runtime phases:** no `Vault` state enum in this crate. Uninitialised / Damaged / Unsupported
  come from `VaultStatus`; Locked = holding a `VaultHeader`; Unlocked = holding an
  `UnlockedVault`; lock = drop. `UnlockedVault::header()` hands the header back so a caller can
  lock and later unlock without re-reading the file. The phase machine is `US-0157`'s
  `VaultState`.
- **`SealedSecret`:** serde-transparent and opaque, no `Display` and no `FromStr` (§ 7: no
  `Display` of an envelope); `Debug` prints `otvN:<sealed>`.
- **Absent `schema_version` in `vault.json`:** `Damaged` (schema 1 is the first; a file without it is
  not a vault header).

## Decisions

- [`DEC-0021`](../../decisions/DEC-0021-secrets-at-rest-are-sealed-by-a-master-password-vault.md)
  — secrets at rest are sealed by a master-password vault (supersedes `DEC-0001` in part).

## Verification Plan

- Unit (`cargo test -p oneterm-vault`): every Acceptance item above except the gate and the lock
  diff; fixtures `vault-v1.json` (made with Python `cryptography` 44, independently of OneTerm),
  `vault-invalid-json.json`, `vault-v9.json`, `vault-invalid-params.json`.
- Unit (`cargo test -p oneterm-core`): `update_secret_json_file` backup equals the new document;
  owner-only mode on Unix (CI); `remove_document`.
- Timing: `cargo test -p oneterm-vault unlock_cost_contract -- --nocapture` in debug and
  `--release` (this crate only), numbers recorded below.
- Graph: `git diff Cargo.lock`, `python scripts/verify-dependency-graph.py`,
  `python scripts/third-party-notices.py --check`.
- No integration, E2E or platform proof in this packet: no UI reaches the crate yet. The Unix
  `0600` tests run in CI on Linux / macOS only.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

To be recorded after implementation.

## Handoff

Implementer: a subagent in an isolated worktree; the integrating actor verifies independently
before merging into `main`.
