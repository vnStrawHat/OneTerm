# Low-Level Design: Crypto suite and format

Intake: IN-0050
HLD: `docs/spec-intakes/IN-0050-master-password/high-level-design.md`
Topic: KDF, key hierarchy, AEAD, verifier, envelope, versioning, memory hygiene, forgot = wipe
Date: 2026-10-09

## Concern

How a master password becomes the ability to seal and open secrets, and the exact bytes that
reach disk. Owned by the new crate `oneterm-vault` (`crates/vault`). Where the bytes are stored
is `persistence-and-migration.md`.

## Design

### 1. Choices and why

| Concern | Choice | Rejected, and why |
| --- | --- | --- |
| KDF | **Argon2id**, version 0x13, `argon2 0.6` | **PBKDF2** (in the lock via `pkcs5` / `bcrypt-pbkdf`): not memory hard, cheap on GPUs. **scrypt 0.12** (in the lock via `russh`): memory hard and acceptable, but Argon2id is the RFC 9106 and OWASP first recommendation and is also already compiled. **bcrypt-pbkdf**: an SSH key format KDF, not a general one. |
| AEAD | **AES-256-GCM**, `aes-gcm 0.11` | **XChaCha20-Poly1305**: its 192-bit nonce would remove the nonce budget question, but `chacha20poly1305` is **not** in `Cargo.lock` (only `chacha20` and `poly1305` for SSH's own construction), so it is a new dependency, and the dependency policy (`docs/agents/dependencies.md` § 3) asks for an existing crate when one does the job. It does: see the nonce budget below. **`ring`'s AEAD** (in the lock via `russh` / `rustls`): workable, but its API is built for sealing in place with typed nonce sequences; the RustCrypto pair shares one `aead` trait and its `zeroize` feature is already on. |
| Key hierarchy | **Wrapped random data key**: Argon2id -> key-encryption key (KEK) -> wraps a random 256-bit data key (DEK); items are sealed under the DEK | **One key straight from the password**: changing the password would re-encrypt every item in every document, and a crash between two files would leave items under a key nothing can derive. **Per-item keys via HKDF** (`hkdf` is in the lock): adds nothing when nonces are random and the volume is tiny; domain separation comes from the associated data's purpose field. HKDF would matter for deterministic nonces or a SIV mode, neither used. |
| Password check (verifier) | **The GCM tag of the wrapped data key.** Unwrapping succeeds only with the right KEK (forgery probability 2^-128) | **A stored known plaintext** encrypted under the KEK: redundant with the wrap. **A password hash** (PHC string): a second artefact to keep in sync, and the `password-hash` path adds nothing over the wrap. |
| Randomness | OS CSPRNG through `getrandom 0.4` (enabled already as `crypto-common`'s feature; reached through `aes-gcm`'s `getrandom` feature and `aead::Generate`), or the workspace `rand 0.10` `SysRng` | `rand::rng()` thread RNG: fine, but the OS source needs no reseeding argument. The packet picks one and proves the lock does not change. |
| Encoding | `base64 0.22` (workspace), standard alphabet with padding, strict decode | Hex (doubles the size), URL-safe (no gain inside JSON). |

**Workspace declarations** (`US-0156`), both resolving to the versions already locked:

```toml
argon2 = { version = "0.6", default-features = false, features = ["alloc", "zeroize"] }
aes-gcm = { version = "0.11", default-features = false, features = ["aes", "alloc", "getrandom", "zeroize"] }
```

`default-features = false` keeps `argon2` off the `password-hash` path (we call the raw KDF).
Acceptance: `Cargo.lock` gains no `[[package]]`; `python scripts/third-party-notices.py --check`
passes unchanged. Both crates reach OneTerm today through `ssh-key 0.7.0-rc.11` (a release
candidate that "moves only when russh does", `dependencies.md` § 3); a future `russh` bump may
move them to another version and leave a second copy in the lock (the policy tolerates
duplicates with a warning). The format does not depend on the crate version: Argon2id and GCM
outputs are fixed by RFC 9106 and NIST SP 800-38D, and the known-answer tests below fail if a
crate update ever changes an output.

### 2. Key hierarchy

```text
 master password (typed; SecretString; never stored)
        |
        |  Argon2id(password bytes, salt[16], m = memory_kib, t = iterations, p = parallelism) -> 32 bytes
        v
 KEK  (Zeroizing<[u8; 32]>, lives for one init / unlock / change call)
        |
        |  AES-256-GCM(KEK, nonce, DEK, aad("key-wrap", vault_id, []))  ==  vault.json "wrapped_key"
        v
 DEK  (random 32 bytes at init; held only as an Aes256Gcm instance while Unlocked)
        |
        |  AES-256-GCM(DEK, nonce, secret, aad(purpose, vault_id, context))
        v
 item envelopes ("otv1:..." in the owning document)
```

- `vault_id`: 16 random bytes generated at init, stored in hex. It names one vault generation:
  reset and a new set-up produce a new id, so envelopes from an earlier vault are recognisable.
- **Change password** rewraps the **same** DEK under a fresh salt and the current default
  parameters. Items are untouched, so one atomic write of one file is the whole change.
  Consequence: someone who already holds a copy of the old `vault.json` **and** the old
  password can still open items sealed later. Rotating the DEK (re-seal every item) is a
  follow-up action, not part of change password.
- **Raise the cost later:** after a successful unlock, if the header's parameters are below the
  binary's defaults, OneTerm rewraps the DEK with the new parameters (silent, non-elevated
  only). No version change is needed for that.

### 3. Argon2id parameters

| Parameter | Default | Floor accepted on read | Ceiling accepted on read |
| --- | --- | --- | --- |
| `memory_kib` | 65536 (64 MiB) | 19456 (19 MiB, OWASP minimum) | 1048576 (1 GiB) |
| `iterations` | 3 | 2 | 16 |
| `parallelism` | 1 | 1 | 4 |
| salt length | 16 bytes | 16 | 64 |
| output | 32 bytes | - | - |

- 64 MiB and 3 passes follow RFC 9106 § 4's second recommended option, with one lane because
  `argon2` computes lanes sequentially without its `parallel` (rayon) feature, so more lanes
  would add time and no memory hardness. Target: 0.25 to 1 s on the owner's machine; `US-0156`
  measures and may move `iterations` within 2..=4 before the format ships.
- The ceilings stop a tampered header from turning an unlock into a memory bomb; a header
  outside the bounds reads as `Unsupported`, never as an attempt.
- The KDF runs on gpui's background executor. The 64 MiB is transient; an allocation failure
  goes through OneTerm's existing OOM policy (`IN-0012`).

### 4. Associated data

```text
aad = "oneterm-vault" 0x00          (13 bytes + separator, constant)
    | 0x01                          (format version of this layout)
    | u8 len(purpose) | purpose     (ASCII: "key-wrap" | "ssh-password")
    | vault_id                      (16 bytes)
    | for each context field: u32 big-endian length | bytes
```

| Purpose | Key | Context fields | Plaintext |
| --- | --- | --- | --- |
| `key-wrap` | KEK | none | the 32-byte DEK |
| `ssh-password` | DEK | host (UTF-8, as stored), port (2 bytes, big endian), username (UTF-8) | the password (UTF-8, at most 1024 bytes) |

Binding the host, port and username (owner question Q3) means a sealed password opens only for
the address it was saved for: a hand-edited `host` makes `open` fail instead of sending the
password elsewhere. The session id is deliberately **not** bound, so Duplicate Session
(`duplicate_in`, `session_state.rs:556`) keeps a working copy.

### 5. Envelope

```text
envelope = "otv1:" base64std( kid[8] | nonce[12] | ciphertext[n] | tag[16] )
kid      = vault_id[0..8]
```

- One string, safe as a JSON value, about `4/3 * (36 + n) + 5` characters (a 20-character
  password gives a 81-character envelope).
- Parsing is strict: the prefix must be exactly `otv1:`; the payload must decode with padding;
  decoded length must be at least 36 bytes and at most 36 + 1024 for `ssh-password`.
- `kid` is not secret. It lets OneTerm tell "sealed under another vault" (reset elsewhere, a
  restored backup of one file only) from "damaged", so the UI can say which.
- The same envelope format carries `wrapped_key` in `vault.json`, so there is one parser.

`open` returns one of:

| Result | Meaning | UI treatment |
| --- | --- | --- |
| `Ok(SecretString)` | Authentic and for this context | use it |
| `OtherVault` | `kid` differs from this vault's id | "saved under a master password that was reset"; ignore, offer to replace |
| `NewerFormat` | prefix `otvN:` with N > 1 | "saved by a newer OneTerm"; keep untouched, never overwrite unless the user saves a new one |
| `Malformed` | anything else, including a plain-text value | ignore, never use as a password, offer to replace |
| `Rejected` | tag check failed (wrong context, tampering) | "can't be used for this address"; ignore, offer to replace |

### 6. Format versioning

- Two independent versions: `vault.json` `schema_version` (document layout, sequential
  migrations as `persistence.md` requires) and the envelope tag `otvN` (crypto suite and byte
  layout).
- A new suite is a new tag. On unlock, v1 items stay readable; each is re-sealed as the new
  version the next time its owner writes it (or by an explicit "upgrade" sweep). The decoder for
  every released tag is kept.
- KDF cost changes need no version (parameters are data).
- **Considered, not included in v1:** a key commitment for the wrap (for example
  `SHA-256("oneterm-vault key-commit" | KEK)` in the header). GCM is not key-committing, which
  matters for partitioning-oracle attacks where an attacker supplies a crafted ciphertext and
  watches success or failure across many keys. Here that needs write access to `vault.json`
  plus watching the user's unlock attempts, which the threat model already concedes. Adding it
  later is a `schema_version` 2 change.

### 7. Memory hygiene

- The master password is read from the gpui `InputState`, copied into `oneterm_core::SecretString`
  (zeroized on drop), and the input is cleared (`set_value("")`) and dropped with its dialog.
  The input's own `SharedString` / rope copy cannot be zeroized: a documented limit, the same
  one the SSH password field already has.
- KEK and the raw DEK bytes live in `zeroize::Zeroizing` buffers and are dropped at the end of the
  call. While Unlocked the DEK exists only inside an `Aes256Gcm` instance; with the `zeroize`
  feature on, its key schedule is cleared on drop. Lock = drop.
- `UnlockedVault` is neither `Clone` nor `Debug`, and not `Serialize`. `SealedSecret`'s `Debug`
  prints `otv1:<sealed>`; `VaultHeader`'s `Debug` omits the wrapped key.
- No log line formats a password, a key, an envelope or a plaintext length. Errors are typed and
  carry no secret material. A unit test asserts `format!("{sealed:?}")` does not contain the
  payload.
- Seal and open run on the UI thread (microseconds); only the KDF crosses to a background task,
  and what crosses back is the `UnlockedVault` (`Send`), never the KEK.
- Not in scope: `VirtualLock` / `mlock`, guard pages, protection against a debugger.

### 8. Forgot master password = reset = wipe

- No recovery, hint, escrow or "security question". Without the password the DEK cannot be
  unwrapped, by construction.
- Reset is reached only from Settings > Security (not from the lock screen, intake Q7). It is
  available while `Locked`, because a forgotten password cannot be proved.
- The confirmation states the loss in plain words, counts what will go, and needs a typed word:

```text
+------------------------------------------------------------+
| Reset master password                                  [x] |
+------------------------------------------------------------+
|  This deletes your master password and 3 saved passwords.  |
|  Sessions, host keys and settings are kept. You will type   |
|  SSH passwords again until you save them under a new master |
|  password. This cannot be undone.                           |
|                                                             |
|  Type RESET to confirm                                      |
|  [                                      ]                   |
+------------------------------------------------------------+
|                         [ Cancel ]  [ Delete everything ]   |
+------------------------------------------------------------+
```

- Order: every owner wipes its sealed values first (`VaultEvent::Reset`; `ssh_session.json`
  written with its backup scrubbed), then `vault.json` and `vault.bak` are removed. A crash in
  between leaves a vault with no items (consistent) or items under a removed vault (recognised
  as `OtherVault`, harmless). Failed attempts never trigger a wipe.

## Interfaces

```rust
// crates/vault/src/lib.rs (sketch; names are proposals)
pub const DOCUMENT_NAME: &str = "vault.json";

pub struct KdfParams { pub memory_kib: u32, pub iterations: u32, pub parallelism: u32 }
impl KdfParams { pub const DEFAULT: Self; pub const FLOOR: Self; pub const CEILING: Self; }

pub struct VaultHeader { /* vault_id, kdf params + salt, wrapped_key */ }

pub enum VaultStatus {
    Uninitialised,
    Ready(VaultHeader),
    /// Newer schema, unknown algorithm, or parameters outside the accepted bounds.
    Unsupported(String),
    /// Present but not parseable: never quarantined, never overwritten except by reset.
    Damaged(String),
}

pub enum SecretPurpose { SshPassword }

pub struct SealContext<'a> { pub host: &'a str, pub port: u16, pub username: &'a str }

#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct SealedSecret(String);          // Debug: "otv1:<sealed>"

pub enum OpenError { OtherVault, NewerFormat, Malformed, Rejected }

pub enum VaultError {
    WrongPassword,
    AlreadyInitialised,                    // another window or process created it first
    ReadOnly,                              // elevation::is_restricted()
    Unsupported(String),
    Io(oneterm_core::AppError),
}

pub fn load(dir: &Path) -> Result<VaultStatus, oneterm_core::AppError>;
pub fn init(dir: &Path, password: &SecretString) -> Result<UnlockedVault, VaultError>;
pub fn unlock(header: &VaultHeader, password: &SecretString) -> Result<UnlockedVault, VaultError>;
pub fn reset(dir: &Path) -> Result<(), VaultError>;

pub struct UnlockedVault { /* header, Aes256Gcm under the DEK */ }   // !Clone, !Debug
impl UnlockedVault {
    pub fn vault_id(&self) -> [u8; 16];
    pub fn seal(&self, purpose: SecretPurpose, context: &SealContext, plaintext: &SecretString)
        -> Result<SealedSecret, VaultError>;
    pub fn open(&self, purpose: SecretPurpose, context: &SealContext, sealed: &SealedSecret)
        -> Result<SecretString, OpenError>;
    pub fn change_password(&mut self, dir: &Path, new_password: &SecretString)
        -> Result<(), VaultError>;
    pub fn needs_rehash(&self) -> bool;
}
```

`lock` is not a function: dropping the `UnlockedVault` is the lock.

## Edge Cases and Failure Modes

- [ ] Wrong password: `WrongPassword` after one full KDF; no timing difference between "wrong"
  and "right but tampered wrap" worth exploiting (both fail at the tag).
- [ ] Empty master password: refused by the dialog (minimum length, Q8); the API also refuses
  an empty one.
- [ ] Password with leading or trailing spaces: kept as typed (never trimmed).
- [ ] Two processes run `init` at once: the create-only transaction under the file lock lets one
  win; the other gets `AlreadyInitialised` and offers Unlock instead.
- [ ] Header parameters outside bounds or unknown `algorithm`: `Unsupported`, nothing attempted.
- [ ] Envelope with `otv2:` read by a v1 binary: `NewerFormat`, value preserved on every write.
- [ ] Plain text or junk in `sealed_password`: `Malformed`, never sent to a server.
- [ ] RNG failure (`getrandom` error): `init` / `seal` fail with an error; nothing written.
- [ ] More than 2^32 seals under one DEK: unreachable in practice (one seal per password save);
  documented as the reason no persistent nonce counter exists.

## Verification

- [ ] RFC 9106 § 5.3 Argon2id test vector run against the `argon2` crate directly (the vector
  uses a secret and associated data that OneTerm does not), plus a OneTerm-specific frozen
  vector through OneTerm's own wrapper: fixed password, salt and parameters -> fixed KEK hex.
- [ ] NIST GCM AES-256 vector through `seal` / `open` internals.
- [ ] Round trip for each purpose; one-bit flips in kid, nonce, ciphertext and tag each fail;
  a different host, port, user or vault id fails.
- [ ] Envelope parser table: missing prefix, wrong prefix case, bad base64, short payload,
  oversize payload, `otv2:`.
- [ ] `change_password` then `unlock` with the old password fails and with the new one succeeds;
  items sealed before still open.
- [ ] `needs_rehash` true for a header at the floor; after rewrap, false.
- [ ] Unlock time at defaults, measured once on the owner's machine and recorded in the packet.
- [ ] `cargo tree -i argon2 -e normal` and `-i aes-gcm` show `oneterm-vault` as the only
  first-party user; `Cargo.lock` package list unchanged.
