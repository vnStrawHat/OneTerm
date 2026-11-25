# US-0156 independent verification

Packet: [`US-0156`](../US-0156-vault-core.md) (high-risk lane: secrets at rest)
Verified head: `e61ac466` (implementer branch `worktree-agent-a5be41de81c13b985`, base `main` @ `28c3728b`)
Verifier branch: `verify/US-0156`
Date: 2026-10-09, Windows 11, `CARGO_BUILD_JOBS=4`, worktree-local `target`, builds limited to
`-p oneterm-vault -p oneterm-core`.

## Verdict: FAIL (one blocking defect, D2)

The crypto is right and independently confirmed, the envelope parser is strict, the gate is
green, and no secret-bearing type leaks through `Debug`. It fails on one persisted-format
contract that becomes permanent once a binary with it ships: a password change or the silent
rehash overwrites a `vault.json` that this binary would itself load as `Unsupported` or
`Damaged`, if the file still carries this vault's id (D2). The fix is a few lines in the
rewrap closure (see D2). D1, D3 and D4 are low and do not block alone.

## Defects

| # | Severity | Where | What | Reproduction |
| --- | --- | --- | --- | --- |
| D1 | Low | `crates/vault/src/document.rs:221` | Set-up overwrites a `vault.json` that holds exactly `{}`. `load` reports that file as `Damaged("no schema_version")`, but the create-only check compares the parsed value with the `{}` a missing file reads as, so it cannot tell "missing" from "`{}`". Breaks "a damaged file is never overwritten" (packet Acceptance, LLD persistence § 3). No key material is lost. | `document_tests::verify::set_up_over_an_empty_object_file_current_behaviour_d1` |
| D2 | **Medium, blocking** | `crates/vault/src/document.rs:227-232` | The rewrap (change password and the silent rehash `US-0157` wires) checks only `schema_version == 1` and `vault_id`. A schema-1 file with this vault's id that `load` reports as `Unsupported` (`wrapped_key` with `otv2:`, parameters above the ceiling, another algorithm) or as `Damaged` (`kdf` not an object) is overwritten, and `vault.bak` is scrubbed with it. LLD persistence § 3 says "Unsupported: no set-up, **no change**, no reset from this binary". `DEC-0021` rule 1 says a reader that does not know a version "keeps the value untouched". LLD crypto § 6 expects exactly such files: an envelope tag is independent of `schema_version`, and parameters "may be raised" with no version change. An older binary that is still unlocked downgrades a newer binary's header. Nothing is lost (same data key), but the KDF cost or suite is downgraded silently. Once shipped, every future format change has to live with old binaries doing this. | `document_tests::verify::rewrap_over_a_damaged_or_unsupported_file_of_the_same_vault_current_behaviour_d2` (4 cases, all overwritten) |
| D3 | Low | `crates/vault/src/vault.rs:123-125` | `reset` removes a file of any status, including `Unsupported`. LLD persistence § 3 says no reset from this binary for `Unsupported` (only `Damaged` offers Reset). Not reachable today because there is no UI. `US-0157` must gate it, or the crate should refuse, following `persistence.md` elevation rule 3: guard the shared function. | Read the code: `reset_with` calls `document::write(dir, Remove, ..)` with no status check. |
| D4 | Low | `crates/vault/src/kdf.rs:83-85` | The Argon2id working memory (64 MiB at defaults) is freed **without zeroization**. The KEK can be computed from its last blocks. `argon2 0.6.0`'s `zeroize` feature clears only the initial hash and the final block hash: `Blocks::drop` in `argon2-0.6.0/src/block.rs:191-200` only deallocates. LLD crypto § 7 keeps the KEK in `Zeroizing`, and this memory is equivalent to it. In practice a 64 MiB allocation goes back to the OS on free, so the risk is small. Fix: allocate `Zeroizing<Vec<argon2::Block>>` and call `hash_password_into_with_memory`. | Source read of `argon2-0.6.0` (`hash_password_into` -> `Blocks::new` / `Blocks::drop`). |
| N1 | Nit | `docs/agents/persistence.md` (`vault.json` row) | "set-up is create-only (`AlreadyInitialised` otherwise)": for a file that is not JSON, set-up returns `VaultError::Io` (InvalidData), not `AlreadyInitialised`. The file is still untouched. | `document_tests::verify::set_up_never_overwrites_a_damaged_or_unsupported_file` |

**Suggested fix for D1 + D2 (and O1):** inside the lock, have the closure run the same `parse` that
`load` uses. Create proceeds only when the file is absent: check `path.exists()` inside the
closure, which runs under the inter-process lock. Rewrap proceeds only when `parse` gives `Ready`
with the same `vault_id`. Stronger: also require the file's `kdf.salt` and `wrapped_key` to equal
those of the header this process unlocked from, and otherwise refuse with `VaultReplaced`. That
also closes O1.

## Command log

| # | Command | Result |
| --- | --- | --- |
| 1 | `git reset --hard e61ac466`; `git checkout -b verify/US-0156` | HEAD `e61ac466ccd5450851f3c404154d85cfb6b3c978` |
| 2 | `%TEMP%` count of `oneterm-vault-*` / `oneterm-persistence-*` before any run | 0 / 0 |
| 3 | `cargo fmt --all -- --check` | pass (before and after the added tests) |
| 4 | `cargo clippy -p oneterm-vault -p oneterm-core --all-targets -- -D warnings` | pass (before and after the added tests) |
| 5 | `cargo test -p oneterm-vault` | 34 passed at `e61ac466`; **46 passed** with the 12 verifier tests |
| 6 | `cargo test -p oneterm-core` | 91 passed |
| 7 | `python scripts/verify-dependency-graph.py` | pass (21 packages) |
| 8 | `python scripts/check-doc-paths.py` | pass (222 paths, 11 documents) |
| 9 | `python scripts/check-english.py` | pass (1105 files; re-run after the added tests and this file) |
| 10 | `python scripts/third-party-notices.py --check` | pass, up to date |
| 11 | `RUSTDOCFLAGS='-D warnings' cargo doc -p oneterm-vault --no-deps` | pass |
| 12 | `git diff main..HEAD -- Cargo.lock` | +15 lines: one `[[package]] oneterm-vault` (deps `aes-gcm`, `argon2`, `base64`, `oneterm-core`, `serde`, `serde_json`, `thiserror 2.0.20`, `zeroize`) and one edge `argon2 -> zeroize`. Exactly one `+name =` line. No third-party package added |
| 13 | `cargo tree -i argon2 -e normal --depth 1`; same for `aes-gcm` | `argon2`: `oneterm-vault`, `ssh-key`. `aes-gcm`: `oneterm-vault`, `ssh-cipher`. No other first-party user |
| 14 | `%TEMP%` count after all runs (including the 48-run timing stress) | 0 / 0; no `oneterm-*` directory newer than 30 minutes |
| 15 | `python <scratchpad>/xcheck.py crates/vault/tests/fixtures/persistence/vault-v1.json` (the verifier's own script, written from the LLD text only, Python `cryptography` 44.0.2) | KEK from the fixture parameters unwraps the DEK with AAD `"oneterm-vault"\0 \x01 \x08"key-wrap" vault_id`. The fixture's SSH envelope opens to `s3cret` with AAD `... \x0c"ssh-password" vault_id \|len\|"10.0.0.1" \|2\|port BE \|len\|"deploy"`; another host, port or user is rejected. The frozen KEK vector (`kdf.rs`) and the frozen envelope (`vault_tests.rs`) are reproduced byte for byte |
| 16 | Graph rule (g), case 1: add `[dev-dependencies.aes-gcm] workspace = true` to `crates/core/Cargo.toml`, run `verify-dependency-graph.py`, then `git checkout -- crates/core/Cargo.toml Cargo.lock` | exit 1: `aes-gcm may be a direct dependency of only ['oneterm-vault']; found ['oneterm-core', 'oneterm-vault']`. After the revert: pass, tree clean |
| 17 | Graph rule (g), case 2: add a **renamed, target-specific** `kdf = { package = "argon2", version = "0.6" }` under `[target.'cfg(unix)'.dependencies]` of `crates/ssh/Cargo.toml`, run the script, revert | exit 1: `argon2 may be a direct dependency of only ['oneterm-vault']; found ['oneterm-ssh', 'oneterm-vault']`. After the revert: pass, tree clean |
| 18 | Timing stress (f): the `unlock_cost_contract` test binary run 48 times, 24 at once (20 logical CPUs) | **48 / 48 passed**. Under that load: defaults 2.9-3.7 s, floor 0.57-0.74 s (debug), a defaults/floor ratio of about 5 against an asserted bound of 2 |
| 19 | Negative check of the new compile-time `!Clone/!Debug/!Serialize` assertion: temporarily pointed it at `VaultHeader` (which is `Clone + Debug`) | `error[E0283]: type annotations needed`: the assertion bites. Reverted |

Not run here (out of the brief's disk budget): workspace-wide clippy/test, the `--release` timing
(the implementer recorded 80-84 ms at defaults), `cargo deny`, and the Unix-only tests.

## Attacks

### (a) `document::write`: create-only, vault-id check, refusal vs I/O error

- Refusal plumbing (`refuse` + the `(result, refusal)` match) is sound. A refusal always wins
  over the closure's placeholder error. An I/O failure without a refusal surfaces as
  `VaultError::Io`. A refused transaction writes nothing: the closure fails before
  `atomic_write_unlocked`. The lock is taken before the read, so the check and the write are
  one transaction across processes.
- Set-up over every `Damaged` / `Unsupported` variant: not JSON, empty file, `[1,2]`, `null`, text
  `schema_version`, kid of another vault, schema 9, memory above the ceiling, `argon2i`. All
  are **refused, byte for byte untouched, and no `vault.bak` is created**: `AlreadyInitialised`
  for JSON, `Io` for non-JSON (N1). The one exception is `{}` (**D1**).
- `vault.json` is a directory: set-up gives `Io`, not a refusal.
- Rewrap refuses, writes nothing, leaves `vault.bak` alone and keeps the in-memory vault on the
  old password when the file is missing (`VaultReplaced`, and no file is created), not JSON
  (`Io`), or schema 2 with the same id (`VaultReplaced`).
- Rewrap over a schema-1 file with the same id that `load` calls `Damaged` or `Unsupported`:
  **overwritten (D2)**.
- Reset removes a file of any status (**D3** for `Unsupported`).

### (b) `Sensitivity::Secret` in `atomic_write_unlocked` (`crates/core/src/persistence.rs:90-120`)

**Unverified on this machine.** Only `x86_64-pc-windows-msvc` is installed, and installing a Unix
target writes outside the project, which `AGENTS.md` § 3.1 forbids. The two `#[cfg(unix)]` tests
(`secret_documents_and_their_backups_are_owner_only` in core and `the_header_is_owner_only` in
the vault crate) run only in CI on Linux and macOS. Line-by-line review:

- `#[cfg(unix)] if sensitivity == Sensitivity::Secret { use ...OpenOptionsExt; options.mode(0o600); }`.
  A `cfg` on an `if` expression statement is the same form `replace_file` already uses on `main`
  (line 191), which CI compiles on Unix. `options.mode` takes `&mut self`, and `options` is
  `let mut`. `Sensitivity` derives `PartialEq` and `Copy`. On Windows the comparison is
  compiled out, and the derive does not make an unused warning. **No compile error found.**
- Semantics: `mode(0o600)` with `create_new` creates the temporary file `0600 & !umask`. Every
  real umask only removes bits, so the result is `0600`. `rename` keeps the inode and its mode,
  so `vault.json` is `0600` and replaces any earlier `0644` file. `fs::copy(&temporary, .bak)`
  copies the source's permission bits: std opens the destination with the source mode and then
  `fchmod`s it, and macOS uses `fcopyfile`. So `vault.bak` is `0600` even when an older `0644`
  backup was there. The lock file and the directory are untouched (not secret).
- Order: the backup is written from the temporary file **before** the rename. A crash or a
  failed rename between the two leaves `vault.bak` = new and `vault.json` = old. Both are valid
  headers of the same vault, and nothing reads `.bak` automatically, so this is acceptable.
- A temporary file that fails to write is removed by the existing cleanup path (unchanged).

### (c) Envelope strictness (`crates/vault/src/envelope.rs:43-91`)

Through `UnlockedVault::open`, each case gets the stated result:

| Case | Result |
| --- | --- |
| Canonical frozen envelope | opens (`hunter2`) |
| extra `=`; padding stripped; **non-canonical trailing bits** (the same bytes under a lenient decoder); trailing `\n`; a space after the prefix; a leading space; an embedded `\r\n`; URL-safe alphabet; `OTV1:`; `Otv1:`; `otv01:`; `otv1x:`; `otv1` without a colon; `otv1:` with nothing after it; `otv1:====`; 4 MiB of base64 | `Malformed` |
| `otv2:`, `otv10:` | `NewerFormat` |
| exactly 36 bytes (empty ciphertext) | decodes, then `Rejected` at the tag |
| the wrapped data key fed to `open` (kid matches) | `Rejected` |

The base64 0.22 `STANDARD` engine refuses non-canonical trailing bits and missing or extra
padding. A "2^31-length claim" has nowhere to live: the envelope carries no length field, and the
length is implicit and bounded to `36..=36+1024` after decode. The 4 MiB input is decoded before
the length check. That is bounded by the document size and not a defect, but a cheap length
pre-check on the text is possible (O7). Unknown and plain-text values round-trip through serde
byte for byte (existing test), and `open` never returns `Ok` for them.

### (d) Memory hygiene (`crates/vault/src/vault.rs`)

- KEK: `Zeroizing<[u8; 32]>` (`kdf.rs:82`), then an `Aes256Gcm` that `aes-gcm 0.11.1` with
  `zeroize` makes `ZeroizeOnDrop` (the `aes` key schedule plus `ghash`). Dropped at the end of the
  call on every path, because `?` returns drop it too.
- Raw DEK: `Zeroizing<[u8; 32]>` at set-up and `Zeroizing<Vec<u8>>` from `open_bytes` in
  unwrap and change. The length check failure drops it zeroized. While unlocked, the DEK exists
  only inside `Aes256Gcm`.
- `seal`: `aead::encrypt` copies the plaintext into a buffer reserved at `len + 16` and encrypts
  in place, so no reallocation leaves a plaintext copy. `open`: aes-gcm checks the tag before it
  applies the keystream, so a failure never produces plaintext. The success buffer is
  `Zeroizing`, UTF-8 is checked on it, and it is copied once into a `SecretString`
  (`ZeroizeOnDrop`).
- Not cleared: the Argon2 working memory (**D4**); stack temporaries such as the
  `[u8; N]` returned by `random()` before `Zeroizing::new` and the key array inside
  `Aes256Gcm::new_from_slice` (O6, outside the LLD's stated scope).
- `Debug` / `Display`: `UnlockedVault` has neither. A new compile-time assertion proves it is
  not `Clone`, `Debug` or `Serialize`, and that assertion was shown to fail on a type that is.
  `VaultHeader`'s `Debug` prints the id and the parameters only. A new test checks that
  `Debug` of `VaultStatus::Ready` carries neither the wrapped key nor the salt. `SealedSecret`'s
  `Debug` prints `otvN:<sealed>`. The other `Debug` derives (`KdfParams`, `SecretPurpose`,
  `SealContext`, `VaultError`, `OpenError`) carry no secret. No `log::`, `println!` or
  `eprintln!` in non-test code. The only `{:?}` formats outside tests is of `KdfParams` in an
  `Unsupported` reason.

### (e) Crypto against the LLD

- Argon2id `Version::V0x13`, `algorithm` / `version` / `memory_kib` / `iterations` /
  `parallelism` / `salt` persisted, 16-byte salt, 32-byte output. Pass.
- AAD layout matches § 4 exactly. This was proved by the verifier's own script written from the
  LLD text (log #15), not only by the implementer's frozen tests. A host, port or user mismatch
  fails `open`. Length prefixes stop field-boundary shifting (existing test). Pass.
- Nonce: 12 bytes from `crypto-common 0.2.2` `Generate::try_generate` -> `getrandom::SysRng`
  (OS RNG), source-checked. Production `seal` always draws a fresh nonce, and the explicit-nonce
  path (`seal_bytes`) is private and used with fixed nonces only by `cfg(test)` code
  (`fixed_vault`, frozen vectors). New test: 4096 seals, 4096 distinct nonces. Pass.
- Wrap tag as the sole password check (plus a 32-byte length check). A wrong password costs
  one full KDF. Pass.
- `change_password` rewraps the same DEK. Items sealed before still open, and the old password
  fails afterwards on the in-memory header and also on a header **reloaded from disk** (new
  test at floor parameters). `vault.bak` equals `vault.json`. Pass.
- `reset` removes `vault.json` and `vault.bak` and tolerates absent and damaged files. Pass
  (see D3 for `Unsupported`).
- Floor and ceiling: rejected on load (existing table). On set-up, the public `init` always
  writes `KdfParams::DEFAULT`, so no public path writes below the floor. `init_with` accepts any
  parameters but is `pub(crate)` and used with the sub-floor `FAST_PARAMS` only by tests. Pass.
- Unknown extra fields load. A file without `schema_version` is `Damaged`. Pass.

### (f) `unlock_cost_contract`

It asserts only ratios: wrong/right in `0.5..=2.0`, and defaults > 2 x floor. It uses the fastest
of 3 runs of each, interleaved, and has no wall-clock bound (`BUG-0075` pattern). The
theoretical defaults/floor work ratio is (65536 x 3)/(19456 x 2), about 5.1. The wrong/right
ratio is about 1, because both sides run identical work. 48/48 runs passed under 24-way
parallel load (log #18). **Flake risk: low.** If it ever flakes on a two-core CI runner, take the
fastest of 5 instead of 3 rather than widening the bounds.

### (g) `DEC-0021` rule 3

`verify-dependency-graph.py` fails for a second user of `aes-gcm` (a dev-dependency in core) and of
`argon2` (a renamed, target-specific dependency in ssh), and passes again after the reverts
(log #16-17). The equality check also fails if `oneterm-vault` stops naming them, which is
harmless. Pass.

### (h) Code style and error policy

Every public item has rustdoc. `cargo doc -D warnings` is clean. Tests sit in sibling
`*_tests.rs` files. The only `let _` (the test temporary-directory drop) has a reason comment.
`unwrap` appears only in `cfg(test)` code. Errors are typed (`VaultError`, `OpenError`), carry no
secret, and name themselves once. There are no absolute paths in the code, and the text is
English. `Damaged` / `Unsupported` loads never quarantine, which follows the
`persistence.md` exception. Pass.

## Deviations listed by the implementer

1. **No `lock()` and no state enum.** This matches the LLD ("`lock` is not a function: dropping
   the `UnlockedVault` is the lock"). `UnlockedVault::header()` returns what the idle lock needs
   to re-unlock without re-reading the file (`idle-lock-overlay.md`). Safe for `US-0157`, whose
   `VaultState` owns the phases.
2. **No `Display` / `FromStr` on `SealedSecret`.** This matches LLD § 7, which allows no
   `Display` of an envelope. `US-0158` builds the value through serde (`lenient_sealed`), which is
   all it needs. Safe.
3. **`change_password` takes the old password.** Justified: § 7 keeps the DEK only inside the
   cipher, so the raw key has to come from an unwrap. It costs a second KDF, about 80 ms in
   release. For the silent rehash, `US-0157` must keep the typed `SecretString` until the
   background rehash finishes. The old password is checked against the **in-memory** header,
   not the file (see O1).
4. **One `update_secret_json_file`.** Fine for `vault.json`. Carry-forward for `US-0158` (O3):
   `Sensitivity::Secret` couples "backup = new document" with "`0600` on Unix". If
   `ssh_session.json` is to be `0600`, **every** write must create the temporary file `0600`.
   Otherwise the next ordinary write renames a umask-mode temporary file over it and the mode
   reverts. The scrub must stay limited to retiring writes, because ordinary session edits need
   the previous generation in `.bak`. The two properties need separating there.
5. **Rehash left to the caller.** This is what the LLD wants ("silent, non-elevated only"). The
   caller must treat `ReadOnly` as "skip". This silent path is where D2 bites without the user
   doing anything.
6. **OS RNG via `aead::Generate`.** Source-checked (`SysRng`). `VaultError::Randomness` on
   failure leaves nothing written. The lock is unchanged. Safe.
7. **`VaultHeader::vault_id()`.** This is what LLD persistence § 8 needs to detect "reset in
   another window" by comparing ids before a seal. Safe.

Also: a missing `schema_version` reads as `Damaged`. That is in tension with `persistence.md`'s
general rule that a missing version means the original schema (O4). It is acceptable because
OneTerm never writes a `vault.json` without the field, and `Damaged` is the safer reading. But
together with the create-only check it produces D1.

## Observations (not defects)

- **O1:** A process holding a stale header can override a newer password set elsewhere. A
  changes the password to P2 on disk. B, still unlocked from the P1 header, calls
  `change_password(P1, P3)`: P1 is checked against B's in-memory header, the vault id matches,
  and P3 overwrites P2. Whoever does this already holds an unlocked vault, so the impact is low.
  The stronger D2 fix closes it.
- **O2:** `oneterm_core::remove_document` is a new destructive core primitive with no elevation
  guard (unlike `quarantine_file`). Today it is reached only through the guarded
  `document::write`. If `US-0158` calls it directly, guard it inside, per rule 3.
- **O3:** see deviation 4.
- **O4:** see above.
- **O5:** `kdf` numbers or a `schema_version` beyond `u32` read as `Damaged` rather than
  `Unsupported`. Harmless.
- **O6:** stack temporaries of key bytes are not zeroized. The LLD puts guard pages and locking
  out of scope.
- **O7:** `Envelope::decode` base64-decodes the whole text before the length bound. That is
  bounded by the document size.
- **O8:** the debug-build unlock is 1.4-3.7 s depending on load. The implementer's suggestion of
  `opt-level = 3` for `argon2` / `blake2` in `[profile.dev.package]` is worth taking in `US-0157`.

## Tests added by the verifier (kept; all pass)

`crates/vault/src/document_tests.rs`, `mod verify`:

- `set_up_never_overwrites_a_damaged_or_unsupported_file`
- `set_up_over_an_empty_object_file_current_behaviour_d1`: pins D1. Flip the assertion when
  the defect is fixed.
- `rewrap_refuses_a_missing_non_json_or_newer_schema_file`
- `rewrap_over_a_damaged_or_unsupported_file_of_the_same_vault_current_behaviour_d2`: pins D2.
  Flip the assertion when the defect is fixed.
- `an_io_failure_is_not_reported_as_a_refusal`
- `debug_of_a_ready_status_carries_no_wrapped_key_or_salt`
- `change_password_survives_a_reload_from_disk`

`crates/vault/src/vault_tests.rs`, `mod verify`:

- the compile-time `!Clone + !Debug + !Serialize` assertion on `UnlockedVault`
- `an_unlocked_vault_is_send_and_sync`
- `the_envelope_parser_accepts_only_the_canonical_spelling`
- `an_empty_ciphertext_is_rejected_at_the_tag`
- `the_wrapped_key_never_opens_as_an_item`
- `nonces_never_repeat_and_an_empty_secret_round_trips`

## Re-verification of 50bbb219

Head verified: `50bbb219`, the rework, on top of `faa5cf27`, which is the commit above
cherry-picked. Branch `verify/US-0156` was reset to it with `git checkout -B verify/US-0156
50bbb219`. Same rules as before: worktree-local `target`, only `-p oneterm-vault -p oneterm-core`,
`CARGO_BUILD_JOBS=4`. The D1 and D2 tests above were renamed by the rework and now assert the
fixed behaviour: `set_up_refuses_an_empty_object_file_d1` and
`rewrap_refuses_a_damaged_or_unsupported_file_of_the_same_vault_d2`.

### Verdict: PASS

D1, D2 (with O1), D3, D4, N1 and O2 are fixed. I could not break any of the fixes. What remains is
informational (R1-R5 below), and none of it blocks.

### Command log

| # | Command | Result |
| --- | --- | --- |
| 1 | `%TEMP%` count of `oneterm-vault-*` / `oneterm-persistence-*` before | 0 / 0 |
| 2 | `cargo fmt --all -- --check` | pass |
| 3 | `cargo clippy -p oneterm-vault -p oneterm-core --all-targets -- -D warnings` | pass |
| 4 | `cargo test -p oneterm-vault` | **50 passed**: 46 from the first verification, plus 2 tests the rework added (O1, D3), plus the 2 new verifier tests below |
| 5 | `cargo test -p oneterm-core` | 91 passed |
| 6 | `RUSTDOCFLAGS='-D warnings' cargo doc -p oneterm-vault --no-deps` | pass |
| 7 | `python scripts/verify-dependency-graph.py` | pass |
| 8 | `python scripts/check-doc-paths.py` | pass (222 paths) |
| 9 | `python scripts/check-english.py` | pass (1106 files, and again after this section was added) |
| 10 | `python scripts/third-party-notices.py --check` | pass |
| 11 | `git diff e61ac466..HEAD -- Cargo.lock` | empty: the lock is unchanged from the first verification (`main..HEAD` is still the same +15 lines) |
| 12 | Timing stress: `unlock_cost_contract` 48 times, 24 at once | 48 / 48 passed. Under load: defaults 1.3-1.5 s, floor 0.36-0.40 s, ratio about 3.7 against the bound of 2 |
| 13 | Same test, once, unloaded (debug, `argon2` / `blake2` at opt-level 3) | defaults 637 ms, floor 161 ms |
| 14 | `cargo test -p oneterm-vault --release unlock_cost_contract -- --nocapture` | defaults 133 ms, floor 26 ms (the first verification recorded 80-84 ms; see R4) |
| 15 | `%TEMP%` count after all runs | 0 / 0 |

### Attacks on each fix

- **D2 / O1, the rewrap arm (`document.rs:263-271`).**
  - `update_json_file_as` takes the inter-process lock, then reads the file, then runs the
    closure, then writes. So `parse_value` runs on the bytes the write replaces, **inside the
    lock**. No other OneTerm process can write between the parse and the write. A non-OneTerm
    writer ignores an advisory lock, but the threat model excludes it.
  - The four `Damaged` / `Unsupported` cases with the same id are refused with
    `VaultReplaced`, and `vault.json` and `vault.bak` are byte for byte unchanged (renamed d2
    test).
  - Rewrapped by another process (salt and wrapped key both differ): refused with
    `ChangedElsewhere`, newer header kept (the rework's test).
  - New test, `rewrap_refuses_a_header_that_differs_only_in_salt_or_only_in_wrapped_key`. A
    `Ready` file with the same id differing **only** in the salt, and one differing **only** in
    the wrapped key (another valid envelope with this vault's kid), are each refused with
    `ChangedElsewhere`, and `vault.json` and `vault.bak` stay unchanged.
  - Missing, non-JSON and schema-2 files are still refused (`VaultReplaced`, `Io`,
    `VaultReplaced`), with `vault.bak` intact (existing verifier test).
  - A refusal is never `Io`: the closure's placeholder error is replaced by the recorded
    refusal. Pass.
- **D1, the create arm (`document.rs:247-256`).**
  - Every existing file is refused with `AlreadyInitialised` and left untouched, with no
    `vault.bak` created: `{ }`, non-JSON, a 0-byte file, `[1,2]`, `null`, schema 9, parameters
    above the ceiling, `argon2i`, kid of another vault (renamed tests).
  - A directory in place gives `Io`.
  - The check runs before the lock and again inside the closure. That second check is reached
    only after the transaction has parsed the file. So a **non-JSON** file created by another
    process inside that window surfaces as `Io` (InvalidData) rather than `AlreadyInitialised`
    (R2). It is still never overwritten. Pass.
- **D3, the remove arm (`document.rs:234-245`).**
  - `Unsupported` is refused with `VaultError::Unsupported` and the file kept. `Damaged` and
    `Ready` are reset. A read error refuses with `Io`. Pass.
  - The window the code comments on is real: the status is read by `load` before
    `remove_document` takes the lock. Losing that race requires another process to turn the
    file `Unsupported` (a newer OneTerm's set-up or rewrap) in the microseconds between, during
    a reset the user typed `RESET` to confirm. I accept it as documented (R3). Closing it would
    need a predicate checked inside `remove_document`'s lock.
- **D4 (`kdf.rs:83-97`).** Pass on every check below.
  - The working memory is a `Zeroizing<Vec<argon2::Block>>`.
  - `try_reserve_exact(block_count)` and then `resize(block_count)` cannot reallocate, so no
    copy is left behind.
  - `zeroize 1.9.0`'s `Vec<Z: Zeroize>` impl clears every element and the spare capacity, and
    `argon2::Block: Zeroize` comes from the `zeroize` feature.
  - Every return after the allocation drops `blocks` through `Zeroizing`: success, the
    `hash_password_into_with_memory` error, and a panic unwind. Before the allocation there is
    nothing to clear.
  - The block count is `params.block_count()`, which is exactly what argon2's own
    `hash_password_into` allocates.
  - The frozen KEK vector and the independent `vault-v1.json` fixture still derive the same
    keys (all tests pass).
- **O2 (`crates/core/src/persistence.rs:179-184`).** `remove_document` returns
  `PermissionDenied` when `elevation::is_restricted()`, before it takes the lock. This is review
  only, like the other M4 guards: flipping the process global needs an `#[ignore]`d test.
  `document::write` refuses even earlier with `ReadOnly`.
- **Tests.**
  - The flipped tests assert the fixed behaviour, and that `vault.json` and `vault.bak` are
    untouched on refusal.
  - `FAST_PARAMS = KdfParams::FLOOR` weakens nothing. The only test that relied on sub-floor
    parameters (a set-up header loading as `Unsupported`) now asserts `Ready` and an unlock
    through `load`, which is stronger. `needs_rehash` is still true at the floor. The cost
    contract names its parameters explicitly.
- **Profiles and docs.**
  - `argon2` and `blake2` are at `opt-level = 3` in `[profile.dev.package]`, which `test`
    inherits, and in `[profile.fast-dev.package]`. Release is unaffected.
  - The `dependencies.md` note and the `persistence.md` `vault.json` row match the code. N1 is
    fixed: non-JSON is now `AlreadyInitialised`, as the row says.

### Remaining (informational, non-blocking)

- **R1:** the rewrap does not compare the Argon2id parameters. A `Ready` file with this vault's
  id, salt and wrapped key but other parameters is rewritten into a valid header. That file can
  only come from a hand edit or corruption, and no password unlocks it. This matches the
  documented rule ("same id, salt and wrapped key") and is pinned by
  `rewrap_over_a_parameters_only_edit_rewrites_it`.
- **R2:** see D1 above. A create race with a non-JSON file gives `Io` instead of
  `AlreadyInitialised`, and the file is never overwritten.
- **R3:** see D3 above. The status read happens outside the lock, as commented.
- **R4:** the explicit `resize` and the volatile zeroization of 64 MiB cost real time:
  - Release unlock at the defaults is 133 ms here, against 80-84 ms before the rework. The
    added time is memory writes, not KDF hardness.
  - Debug unlock is 0.64 s even with `argon2` optimized, because `oneterm-vault` and `zeroize`
    stay at opt-level 0.
  - If the debug loop matters to `US-0157`, add `zeroize` to the dev overrides. If release
    cost matters, allocate zeroed memory (`alloc_zeroed`) instead of the explicit `resize`.
- **R5:** the Unix `0600` code is unchanged and still unverified on this Windows host. CI on
  Linux and macOS runs it. The carry-forward O3 for `US-0158` still stands.

### Tests added in the re-verification (kept; both pass)

`crates/vault/src/document_tests.rs`, `mod verify`:

- `rewrap_refuses_a_header_that_differs_only_in_salt_or_only_in_wrapped_key`
- `rewrap_over_a_parameters_only_edit_rewrites_it` (pins R1)
