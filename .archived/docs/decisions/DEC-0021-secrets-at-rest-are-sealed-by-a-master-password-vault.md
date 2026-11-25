# DEC-0021 Secrets at rest are sealed by a master-password vault

Date: 2026-10-09

## Status

Accepted (owner, 2026-10-09). Recorded from the candidate text in
[`IN-0050`](../spec-intakes/IN-0050-master-password/IN-0050.md) § Decision Candidates; the owner
accepted every Q1-Q10 recommendation of that intake as written, including Q9 (a new crate
`oneterm-vault`) and Q10 (no OS keychain or DPAPI layer). First implementation packet:
[`US-0156`](../spec-intakes/IN-0050-master-password/US-0156-vault-core.md).

**Supersedes [`DEC-0001`](0001-ssh-key-secret-persistence.md) in part:** passwords may be
persisted when sealed under rule 1; private-key passphrases and key contents stay forbidden until
a packet adds a purpose for them.

## Context

`DEC-0001` forbids writing any password or passphrase to a OneTerm document and says a credential
vault "must be designed as a new capability rather than silently changing this persistence rule".
The owner asked for that capability (2026-10-09): a master password that only the user knows,
which encrypts secrets such as a saved SSH password before they reach a persistence file, and
which also unlocks an idle screen lock. The threat model, the crypto analysis and the persisted
formats are in `IN-0050`'s high-level design and its `crypto-and-format.md` and
`persistence-and-migration.md` detail designs. Three rules come out of that work that every later
packet (and every later secret) must inherit.

## Decision

1. **Crypto suite and format.** A secret OneTerm persists is sealed, never stored in plain text.
   The master password derives a key-encryption key with Argon2id (version 0x13; parameters and a
   16-byte random salt stored in `vault.json`; defaults 64 MiB, 3 passes, 1 lane; parameters may
   be raised, never lowered below 19 MiB / 2 passes). The key-encryption key wraps a random
   256-bit data key with AES-256-GCM; items are AES-256-GCM under the data key with a fresh random
   96-bit nonce and associated data binding the vault id, the purpose and the item's context.
   Every persisted ciphertext is an envelope string beginning with a version tag (`otv1:`); a
   reader that does not know the version keeps the value untouched and never treats it as plain
   text. A new suite or layout is a new version tag, migrated item by item while unlocked. The
   master password, a hash of it, the key-encryption key and the data key are never written
   anywhere.
2. **Forgot means wipe.** There is no recovery path, escrow, hint or backdoor. Reset removes
   `vault.json` and every sealed value, and nothing else; it is explicit, confirmed, and never
   automatic (no wipe after failed attempts).
3. **Location and ownership.** The vault header lives in its own document,
   `config_dir()/vault.json`, owned by `oneterm-vault`, never quarantined automatically, and
   written only by a non-elevated process. Each sealed value lives in the document of the data it
   belongs to (today `ssh_session.json`'s `sealed_password`), owned by that document's owner. Only
   `oneterm-vault` depends on the crypto crates.

## Alternatives

- [x] Selected approach described above.
- [ ] OS keychain / DPAPI: not a password only the user knows, and the platforms diverge (owner
  ruling Q10: not now; it could wrap the data key a second time later).
- [ ] XChaCha20-Poly1305: a new dependency (`chacha20poly1305` is not in `Cargo.lock`), and its
  larger nonce buys nothing at the volume of a session list.
- [ ] PBKDF2: not memory hard.
- [ ] scrypt: acceptable, but Argon2id is the current recommendation (RFC 9106, OWASP) and is
  already in the lock.
- [ ] One key derived directly from the password, no data key: changing the password would rewrite
  every document, and two files could disagree after a crash.
- [ ] A separate known-plaintext verifier: redundant with the GCM tag of the wrapped data key.

## Consequences

- [ ] Saved SSH passwords become possible (`US-0158`) without a plain-text secret on disk; a copy
  of `~/.OneTerm` leaves only an offline Argon2id guessing attack.
- [ ] Two persisted formats become contracts: `vault.json` schema 1 and the `otv1:` envelope. A
  decoder for every released envelope tag is kept forever.
- [ ] Forgetting the master password loses every saved password, by design.
- [ ] `vault.json` is the one document that is never quarantined or defaulted
  (`docs/agents/persistence.md`).
- [ ] `docs/PROJECT.md`'s "No secrets are persisted" invariant becomes "secrets are persisted only
  sealed", and `docs/ssh-authentication.md` drops the vault non-goal.
