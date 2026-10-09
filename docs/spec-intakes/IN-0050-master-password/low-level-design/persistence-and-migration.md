# Low-Level Design: Persistence and migration

Intake: IN-0050
HLD: `docs/spec-intakes/IN-0050-master-password/high-level-design.md`
Topic: `vault.json`, the `sealed_password` field, compatibility, backups, elevation, several instances, sync
Date: 2026-10-09

## Concern

Where the vault header and the sealed values are stored, how they load and save through the
shared persistence mechanics (`docs/agents/persistence.md`), and what every other binary and
process sees. The byte formats are `crypto-and-format.md`.

## Design

### 1. Where the vault header lives: its own document

`config_dir()/vault.json`, owned by `oneterm-vault`. Not a field in `ui_config.json` or
`terminal.json`, because:

- those are rewritten on every theme or setting change, and every rewrite copies the previous
  generation to `.bak`; the header changes only on set-up, change and reset, and its backup has
  to be scrubbed on two of those;
- its load policy is the opposite of theirs: an unreadable settings file is quarantined and
  replaced by defaults, while an unreadable header must **never** be replaced (defaults would
  be "no vault", and the next set-up would orphan every sealed value);
- one owner per document (`persistence.md` "Schema owners"); the settings crate should not
  own crypto material;
- it travels with `ssh_session.json` and nothing else, which is easy to say about a file.

### 2. `vault.json` schema 1

```json
{
  "schema_version": 1,
  "vault_id": "3f9c0a4e8b1d4c27a6e2f0b9c1d2e3f4",
  "kdf": {
    "algorithm": "argon2id",
    "version": 19,
    "memory_kib": 65536,
    "iterations": 3,
    "parallelism": 1,
    "salt": "q8Rk2m3x9C0hX6PpY1nW0g=="
  },
  "wrapped_key": "otv1:P5wKTosbLCcAAAAAAAAAAAAAAAA...=="
}
```

| Field | Type | Rule |
| --- | --- | --- |
| `schema_version` | integer | 1. Absent or newer -> see load table. |
| `vault_id` | 32 lowercase hex chars | 16 random bytes; new on every set-up. |
| `kdf.algorithm` | string | `argon2id` only; anything else -> `Unsupported`. |
| `kdf.version` | integer | 19 (0x13). |
| `kdf.memory_kib`, `iterations`, `parallelism` | integers | Within the bounds in `crypto-and-format.md` § 3. |
| `kdf.salt` | base64 | 16..=64 bytes. |
| `wrapped_key` | envelope | `otv1:` with purpose `key-wrap`; its kid must equal `vault_id[0..8]`. |

Unknown extra fields are ignored on read and preserved on rewrite (the writers go through
`update_json_file` and touch only their fields), so a later additive field does not need a
schema bump.

### 3. Load outcomes for `vault.json`

| On disk | Outcome | Persisted effect |
| --- | --- | --- |
| Missing | `Uninitialised` | none (no first-run default write) |
| Valid schema 1 | `Ready(header)` | none |
| `schema_version` newer than 1, unknown algorithm, parameters out of bounds | `Unsupported(reason)`: no set-up, no change, no reset from this binary; saving passwords and the idle lock are off; Settings says "created by a newer OneTerm" | none |
| Not JSON, wrong types, bad hex or base64, kid mismatch | `Damaged(reason)`: same restrictions, Settings offers Reset as the only action | **none: never quarantined** |
| Read error other than not-found | `AppError::ConfigLoad`; treated as `Unsupported` for this run | none |

**Exception to `persistence.md` "Load outcomes" (to be written there by `US-0156`):** the vault
header is the only document that is never quarantined and never replaced by a default. Only
Reset removes it.

### 4. Writes to `vault.json`

All through `oneterm-core` mechanics, all refused when `elevation::is_restricted()` is true, the
refusal living inside `oneterm-vault`'s one write function (`persistence.md` elevation rule 3:
guard the shared function, not the callers).

| Operation | Mechanism | Detail |
| --- | --- | --- |
| Set up | `update_json_file` | The closure fails with `AlreadyExists` unless the value is the `{}` a missing file reads as, so two windows or two processes cannot both create a vault. |
| Change password / rehash | `update_json_file` | The closure checks the file's `vault_id` equals the unlocked vault's; otherwise it refuses (the vault was reset elsewhere). Then the backup is scrubbed (below). |
| Reset | new helper `remove_document(path)` in `oneterm_core::persistence` | Takes the same inter-process lock, removes `vault.json` and `vault.bak`. |

**Scrubbing `.bak`.** `atomic_write` copies the previous generation to `<stem>.bak`
(`vault.bak`, `ssh_session.bak`; `path.with_extension("bak")`). After change password the old
generation holds the data key wrapped under the **old** password; after removing a saved
password or after reset the old `ssh_session.bak` holds the envelope. A new helper,
`atomic_write_scrubbing_backup(path, bytes)` (or a flag on the existing function), replaces the
`.bak` with a copy of the new document under the same lock. Used for: change password, rehash,
"Remove saved password", the reset wipe. Not used for ordinary session edits.

**File mode.** On Unix the temporary file is created with the process umask (usually `0644`).
`vault.json` is created `0600` (an `OpenOptionsExt::mode` on the temporary file in the
`oneterm-vault` write path); `ssh_session.json` is recommended to follow (it now holds
envelopes), as a one-line change in the same packet as the field. Windows: the profile ACL
(owner, SYSTEM, Administrators) applies; nothing to change.

### 5. `ssh_session.json`: the `sealed_password` field

```rust
// crates/session-ui/src/session_state.rs, SshSession (US-0158)
/// Password sealed under the master password vault (IN-0050). Never plain text.
#[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "lenient_sealed")]
pub sealed_password: Option<oneterm_vault::SealedSecret>,
```

- **No schema bump.** The document stays at schema v2. A bump to v3 would be fatal for older
  binaries: `migrate_json_value` rejects a newer `schema_version`
  (`crates/core/src/schema.rs`), `SshSessionStore::load_from` then quarantines the file and the
  older OneTerm starts with **no sessions**. An optional field is invisible to them instead.
- **Lenient field parse.** A value that is not a string (a hand edit) must not fail the whole
  document, because a failed parse quarantines every session. `lenient_sealed` maps any
  non-string to `None` with one `warn` naming the session id; a string is kept as an opaque
  `SealedSecret` and judged only when opened (`crypto-and-format.md` § 5).
- Only `oneterm-session-ui` writes the field (document owner). `oneterm-vault` never edits
  `ssh_session.json`; reset reaches the store through `VaultEvent::Reset`.
- Duplicate keeps the envelope; it stays valid because the session id is not bound.
- Valid only when `auth_method` is `password`. Switching the method to key or agent and saving
  removes the field (with the backup scrubbed).

### 6. Compatibility matrix

| Binary | Document | Result |
| --- | --- | --- |
| new | old `ssh_session.json` (no field), no `vault.json` | loads; `Uninitialised`; nothing written until the user acts |
| new | new document | loads; envelopes opened only when unlocked |
| **old (v0.6.8 and earlier)** | new `ssh_session.json` | loads every session (`SshSession` has no `deny_unknown_fields`); `sealed_password` ignored. **Its next write of the session list drops every `sealed_password`**; the previous generation survives one write in `ssh_session.bak`. `vault.json` is never read or touched. Upgrading again: vault intact, saved passwords gone, re-save them. Documented in the release notes of the version that ships `US-0158`. |
| v1-aware | envelope `otv2:` from a later binary | kept byte for byte through every load and save; shown as "saved by a newer OneTerm" |
| any | hand-edited plain text in `sealed_password` | `Malformed`: never used, never logged |

### 7. The elevated instance (`IN-0043`, M1 and M4)

| Item | Elevated behaviour | Reason |
| --- | --- | --- |
| `vault.json` | read at start (recommended, Q5) | lets the idle lock use the same master password |
| set up, change, rehash, reset | refused in `oneterm-vault`'s write function | M4: no writes |
| `sealed_password` | never opened | M1: no SSH surface; `ssh_session.json` writes are already refused |
| unlock from the lock screen | allowed: verifies the password by unwrapping, then keeps the key only as long as the screen stays unlocked | read-only operation |
| Settings > Security | shown read-only: "Managed from the normal OneTerm window." | a page whose edits are refused is worse than an honest note (same reasoning as M2's Network page) |

Rule 4 of `DEC-0019` ("nothing the unelevated process writes may direct what the elevated
process executes") is unaffected: the header decides only whether a typed password unlocks the
screen. A same-user process that deletes or replaces `vault.json` can disable or change that
lock; it can do worse to that user anyway, and the threat model excludes it. Under
over-the-shoulder elevation the administrator's profile normally has no `vault.json`, so the
elevated window has no lock.

### 8. Several windows and several processes

- **Windows of one process** share one `VaultState` global: one unlock serves the main and the
  Settings window, one idle timer covers both.
- **Processes** (OneTerm has no single-instance guard; the elevated window is always a second
  process) each have their own phase, key and timer.
  - Set-up race: solved by the create-only transaction (section 4).
  - Change password in process A: B keeps working (same data key). B's next **dialog** unlock
    (`Locked -> Unlocked`) re-reads the file and so takes the new password when the `vault_id`
    is unchanged. B's **lock screen** never re-reads the file (`idle-lock-overlay.md`, edge
    cases): replacing `vault.json` must not be a way past a locked window.
  - A file whose `vault_id` matches but whose wrapped key was forged by someone who can write
    the folder would unwrap to a different data key: existing items then fail as `Rejected`,
    which shows. That attacker can also edit `terminal.json`; the threat model excludes them.
  - Reset in A while B is unlocked: before every seal, `VaultState` re-reads `vault.json` and
    compares `vault_id`; a difference (or a missing file) drops B's key, moves B to the new
    file's phase and refuses the seal with "The master password was reset in another OneTerm
    window." Opening an envelope whose kid is not the current vault's gives `OtherVault`.
  - B's stale in-memory session list can still write old envelopes back after A's reset (the
    session store is whole-document, last writer wins; a pre-existing limit of several
    processes, not new here). Those envelopes read as `OtherVault` and are dropped the next time
    their session is saved.

### 9. Sync and backup

- `vault.json` and `ssh_session.json` belong together. Restoring one without the other gives
  `OtherVault` for sealed values whose vault is gone; sessions are never affected.
- A synced or backed-up folder exposes the header and envelopes to whoever holds the copy; the
  defence is Argon2id plus the strength of the master password. The create dialog's hint says
  so in one line.
- Conflict copies written by sync tools (`vault (conflict).json`) are ignored; only
  `vault.json` is read.

### 10. `ui_config.json`: the idle timeout

`lock_after_idle_minutes` (integer, optional, omitted while equal to the default 15), owner
`oneterm-settings`, single writer `UiConfig::set_lock_after_idle_minutes` shared by the Settings
field. Out-of-range values (outside 1..=480) are clamped on load with a `warn`, not rejected, so
the document is never quarantined for it. Read by the elevated instance like the rest of
`ui_config.json`; never written there.

## Interfaces

```rust
// oneterm-core, persistence.rs (US-0156)
pub fn atomic_write_scrubbing_backup(path: &Path, bytes: &[u8]) -> io::Result<()>;
pub fn remove_document(path: &Path) -> io::Result<()>;   // removes path and its .bak under the lock

// oneterm-vault (see crypto-and-format.md for the rest)
pub fn load(dir: &Path) -> Result<VaultStatus, AppError>;

// oneterm-session-ui, SshSessionStore (US-0158)
pub fn clear_sealed_passwords(&mut self, cx: &mut Context<Self>);   // on VaultEvent::Reset
```

`persistence.md` rows to add (`US-0156`, `US-0158`, `US-0159`):

| Document | Owner | Notes |
| --- | --- | --- |
| `vault.json` | `oneterm-vault` | Master-password vault header (`IN-0050`, `DEC-0021`): vault id, Argon2id parameters and salt, the wrapped data key. Never holds a password. Never quarantined or defaulted; only Reset removes it. Writes are create-only (set-up) or vault-id-checked (change), and scrub `vault.bak`. Unix mode `0600`. |
| `ssh_session.json` (amend) | `oneterm-session-ui` | `sealed_password` (optional, omitted when absent): an `otv1:` envelope bound to host, port and user; parsed leniently; no schema bump (v2). |
| `ui_config.json` (amend) | `oneterm-settings` | `lock_after_idle_minutes` (optional, default 15, clamped to 1..=480). |

## Edge Cases and Failure Modes

- [ ] `vault.json` deleted by hand while sealed values exist: `Uninitialised`; envelopes read as
  `OtherVault` after a new set-up; sessions intact.
- [ ] `vault.json` replaced by another vault's file: kid mismatch -> `OtherVault` for every item.
- [ ] Disk full or write error during set-up: no file or the old file (atomic); phase unchanged;
  the dialog shows the error.
- [ ] Crash between "rewrite `vault.json`" and "scrub `vault.bak`": the backup holds the old wrap
  until the next change; recorded as an accepted window of one operation.
- [ ] `persist_blocked` on `ssh_session.json` (unreadable at start): saving a password is refused
  like every other session edit.
- [ ] Old binary concurrently running with a new one: last writer wins on `ssh_session.json`; the
  old one may drop envelopes (matrix row above).

## Verification

- [ ] Fixtures under `crates/session-ui/tests/fixtures/persistence/`:
  `ssh-session-v2-sealed.json` (loads, round-trips the envelope byte for byte),
  `ssh-session-invalid-sealed-type.json` (non-string field: loads, field `None`, no quarantine);
  the directory already holds `ssh-session-v0.json` and `ssh-session-v1.json`.
- [ ] Fixtures under `crates/vault/tests/fixtures/persistence/`: `vault-v1.json`,
  `vault-invalid-json.json` (`Damaged`, file untouched), `vault-v9.json` (`Unsupported`),
  `vault-invalid-params.json` (out-of-bounds memory).
- [ ] Set-up race: two threads call `init` on one temporary directory; exactly one succeeds.
- [ ] Change password then read `vault.bak`: equal to `vault.json`.
- [ ] Elevated refusal: the write function takes the restriction as a parameter internally
  (the `write_refusal(elevated)` shape of `ui_config.rs`), so an ordinary unit test proves every
  vault write returns `ReadOnly` with nothing changed on disk, without a new `#[ignore]`d
  process-global test (which `scripts/check-ignored-tests.py` would otherwise have to record).
- [ ] Old-binary check (manual, `US-0158`): v0.6.8 release build started on a copy of a config
  directory with sealed passwords lists every session.
- [ ] Unix: a unit test asserts the created `vault.json` has mode `0600`.
