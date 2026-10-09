# Work: Vault core (`oneterm-vault`), no UI

ID: US-0156
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

- [x] RFC 9106 § 5.3 Argon2id vector passes against the `argon2` crate; a frozen vector passes
  through OneTerm's own KDF wrapper; the NIST CAVP AES-256-GCM vector (`gcmEncryptExtIV256`,
  96-bit IV, 128-bit PT and AAD) passes through OneTerm's own seal / open internals.
- [x] The `otv1:` layout of an SSH-password envelope is frozen by a known-answer test (fixed key,
  vault id and nonce through a test-only constructor), and `vault-v1.json` plus a sealed password
  made by an **independent** implementation unlock and open through the crate.
- [x] Seal / open round trip; each seal uses a fresh nonce; a 1024-byte secret seals, 1025 does
  not.
- [x] Wrong password -> `WrongPassword` (never trimmed: a trailing space is wrong); empty ->
  `EmptyPassword`; a flipped kid -> `OtherVault`; a flipped nonce, ciphertext or tag byte, or
  another host / port / user -> `Rejected`; another vault -> `OtherVault`; `otv2:` ->
  `NewerFormat`; plain text -> `Malformed`.
- [x] An unknown-version or plain-text value round-trips through serde byte for byte.
- [x] `change_password` checks the old password, rewraps the same data key under a fresh salt:
  the old password stops working, the new one works, items sealed before still open,
  `vault.bak` equals `vault.json`, the old wrapped key is gone from both, and unknown fields at
  top level and inside `kdf` survive. Refused (`VaultReplaced`, file untouched) when the file on
  disk is another vault.
- [x] `reset` removes `vault.json` and `vault.bak`, works on a damaged or absent file.
- [x] Set-up is create-only: a second set-up or two concurrent ones give exactly one vault and
  `AlreadyInitialised`; a damaged file is never overwritten.
- [x] Load: missing -> `Uninitialised`; read error -> `AppError::ConfigLoad`; every field rule of
  `persistence-and-migration.md` § 2-3 gives its `Damaged` / `Unsupported` outcome, including
  parameters below the floor (19 MiB / 2 passes) or above the ceiling; unknown fields are
  ignored; **no load ever writes, renames or quarantines**.
- [x] Elevated: set-up, change and reset each return `ReadOnly` with nothing changed on disk,
  proved by ordinary unit tests through the `elevated` parameter (no new `#[ignore]`d test).
- [x] An unlock verifies only against the header the caller holds: replacing `vault.json` with a
  passer-by's vault neither accepts the passer-by's password nor rejects the owner's.
- [x] No `Debug` / `Display` output carries a password, a key or an envelope payload;
  `UnlockedVault` is neither `Clone` nor `Debug`.
- [x] Unlock cost contract (ratios only): a wrong password costs a full KDF like a right one
  (ratio within 0.5..=2), and the defaults cost more than twice the floor. The default-parameter
  unlock time on this machine is recorded below.
- [x] `Cargo.lock` gains no `[[package]]` other than `oneterm-vault`;
  `python scripts/third-party-notices.py --check` passes unchanged; `cargo tree -i argon2` and
  `-i aes-gcm` list `oneterm-vault` as the only first-party user (enforced by
  `verify-dependency-graph.py`).
- [x] No test leaves a temporary directory behind (RAII guard, `BUG-0083` pattern).
- [x] Gate in the worktree: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
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

Docs changed, as planned: `DEC-0021` (new), `DEC-0001` status, `docs/PROJECT.md`,
`docs/ssh-authentication.md` (non-goal + "Master-password vault" subsection),
`docs/agents/persistence.md` (`vault.json` row, never-quarantine exception, "Secrets at rest",
elevation row), `docs/agents/structure.md`, `docs/agents/crate-dependency-rules.md`,
`docs/agents/dependencies.md`, `docs/architecture.md`, `IN-0050.md`. Not changed:
`docs/gui-layout.md` (no UI in this packet). `persistence-and-migration.md`'s `vault.json` rows and
the LLD interfaces stay as designed; the deviations are listed under Plan.

## Context

- Crypto crates already in the lock via `russh 0.63.3 -> ssh-key 0.7.0-rc.11`: `argon2 0.6.0`,
  `aes-gcm 0.11.1`. Enabling `argon2`'s `zeroize` feature adds one dependency **edge**
  (`argon2 -> zeroize`, both already packages) to `Cargo.lock`, no package.
- `oneterm_core::SecretString` (zeroizing, `Debug` = `***`) carries passwords in and plaintexts
  out.

## Plan

- [x] Records first (commit 1, docs only): `DEC-0021`, `DEC-0001` note, `PROJECT.md`,
  `ssh-authentication.md`, `IN-0050.md` rulings, this packet.
- [x] Crate, core helpers, policy, tests, fixtures, docs (commit 2).
- [x] Gate; record evidence and the Argon2id timing here.

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
- [x] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Worktree gate, 2026-10-09, Windows 11, `CARGO_BUILD_JOBS=6`:

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo test -p oneterm-vault` | 34 passed (debug, about 12 s) |
| `cargo test -p oneterm-core` | 91 passed (3 new: secret backup, `remove_document`, and the Unix `0600` test, which compiles only on Unix) |
| `python scripts/verify-dependency-graph.py` | pass (21 packages; `argon2` / `aes-gcm` named only by `oneterm-vault`) |
| `python scripts/check-doc-paths.py` | pass |
| `python scripts/check-english.py` | pass |
| `python scripts/third-party-notices.py --check` | pass, file unchanged (nothing reaches `oneterm-vault` from `oneterm-app` yet) |
| `python scripts/check-ignored-tests.py` | pass, 19 ignored tests, all recorded (no new one). The first two attempts crashed rustc with `0xc0000409` while it built `windows` / `gpui`: free commit was 5.8 GB. The third ran with `CARGO_BUILD_JOBS=2` |

- **`Cargo.lock`:** one new `[[package]]`, `oneterm-vault`, and one new dependency **edge**,
  `argon2 -> zeroize` (the `zeroize` feature; both were already packages). No package added.
- **Independent cross-checks:** the frozen KEK vector and the frozen `otv1:` envelope were each
  reproduced byte for byte by Python `cryptography` 44 (OpenSSL); `vault-v1.json` and its sealed
  SSH password were generated by that implementation and unlock / open through the crate.
- **Temporary directories:** none left in `%TEMP%` after the runs (`oneterm-vault-*`,
  `oneterm-persistence-*`: 0).

**Argon2id unlock time (Q11, first part)** — `unlock_cost_contract`, fastest of 3:

| Build | Defaults (64 MiB, 3 passes, 1 lane) | Floor (19 MiB, 2 passes) |
| --- | --- | --- |
| `cargo test -p oneterm-vault --release` | **80-84 ms** | 14-15 ms |
| debug (`dev`, argon2 at opt-level 0) | 1.4-1.7 s | 0.27-0.35 s |

The release time is **below** the LLD's 0.25-1 s target. Moving `iterations` within the allowed
2..=4 would give about 110 ms, still below it. Reaching the target needs more memory (for
example 256 MiB, 3 passes, about 0.3 s), which `DEC-0021` and the brief fix at 64 MiB, so the
defaults are **unchanged** and this is an **open owner question**. Parameters are data and
`needs_rehash()` upgrades existing vaults after an unlock, so raising them later needs no
format change. Separately, the 1.4-1.7 s debug unlock makes `US-0157`'s dev loop slow. Adding
`argon2` and `blake2` to `[profile.dev.package]` with opt-level 3, as `gpui-pre` already is,
would fix that. It is not done here.

Gaps:

- No integration, E2E or platform proof: no UI reaches the crate (by scope).
- The Unix `0600` tests (`oneterm-core`, `oneterm-vault`) were not run locally; CI runs them on
  Linux and macOS.
- `cargo deny` was not run (not installed in this session's gate).
- The full `cargo test --workspace` runs on `main` after independent verification, not here.

## Handoff

Implementer: a subagent in an isolated worktree; the integrating actor verifies independently
before merging into `main`.
