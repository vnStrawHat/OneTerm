# High-Level Design: Patch-level lock move off the yanked chacha20 and der

Intake: IN-0035
Lane: normal
Date: 2026-09-15

## Idea

Two crates in `Cargo.lock` are yanked on crates.io:

| Crate | Locked (yanked) | Non-yanked replacement | Requirement it must satisfy |
|---|---|---|---|
| `chacha20` | `0.10.1` | `0.10.2` | `^0.10` (from `rand 0.10`, `ssh-cipher 0.3.0-rc.9`) |
| `der` | `0.8.0` | `0.8.2` | `^0.8` (from `ecdsa`, `pkcs1`, `pkcs5`, `pkcs8`, `sec1`, `spki`, `rsa`, `ssh-key`, `russh`) |

Nothing in the workspace declares either crate. Both replacements are patch releases inside the
ranges the whole russh crypto stack already states, so the fix is a `Cargo.lock` re-resolution:

```text
cargo update -p chacha20 --precise 0.10.2
cargo update -p der@0.8.0 --precise 0.8.2
```

No manifest change, no `deny.toml` entry, no russh-family bump, and therefore no russh API
surface crossed at all.

`THIRD-PARTY-NOTICES.md` is generated from `Cargo.lock`, so it is regenerated in the same change
to keep `scripts/third-party-notices.py --check` green.

## Rejected alternatives

1. **Bump the russh family in `Cargo.toml`** (`russh 0.61 -> 0.63.3`, `russh-sftp 2.3 -> 3.0.0`,
   `russh-cryptovec 0.61 -> 0.62`). Rejected: two semver-major bumps across the SSH auth,
   host-key, channel and SFTP APIs that `crates/ssh` and `crates/sftp-ui` call directly
   (`russh::client::connect`, `Handler::check_server_key`, `keys::agent::client::AgentClient`,
   `keys::known_hosts::*`, `MethodSet`/`MethodKind`, `russh_sftp::client::SftpSession`), for a
   problem that does not need any of it. russh 0.63 would resolve the same two crates to the
   same patch releases this change picks directly.
2. **`cargo update -p russh` inside the existing `^0.61` range.** Rejected because it is a
   no-op: `cargo update -p russh --dry-run` reports `Locking 0 packages` — `0.61.2` is already
   the newest `0.61.x`. Same for `russh-sftp` and `russh-cryptovec`.
3. **Add `chacha20` / `der` to `deny.toml`.** Rejected: `yanked` has no ignore list, and
   silencing a resolvable yank would defer a real fix. `yanked = "warn"` stays as it is.
4. **`cargo update` unpinned (whole-graph refresh).** Rejected: it moves crates unrelated to the
   report and makes the blast radius unreviewable for a warning-only fix.

## Diagram

```text
Cargo.toml  russh = "0.61"   russh-sftp = "2.3"   rand = "0.10"    <- all unchanged
                    |
                    v
Cargo.lock  chacha20 0.10.1 (yanked)  --cargo update-->  chacha20 0.10.2
            der      0.8.0  (yanked)  --cargo update-->  der      0.8.2
                    |
                    v
            THIRD-PARTY-NOTICES.md --regenerate--> same licences, new versions
```

## UI Wireframe

N/A — no UI surface. The change is confined to the resolved dependency graph.

## API surface at risk

None is crossed by this change, but these are the paths the two crates sit under, and what the
patch releases actually change:

- `chacha20 0.10.1 -> 0.10.2` fixes "use of an SSE4.1 intrinsic in the SSE2 backend" of the RNG
  and legacy (64-bit counter) variants (RustCrypto/stream-ciphers #580) — the reason for the
  yank. It affects the `chacha20-poly1305@openssh.com` transport cipher via `ssh-cipher`, and
  the ChaCha RNG that `rand` gives `internal-russh-num-bigint` and `pageant`. No API change.
- `der 0.8.0 -> 0.8.2` is a `minimal-versions` CI yank of `0.8.0`, plus two ASN.1 correctness
  fixes across `0.8.1`/`0.8.2`: `SET OF` duplicates are permitted again, nesting is capped at
  64 levels, and nested trailing data now errors. These sit under key and host-key decoding
  (`pkcs1`/`pkcs8`/`sec1`/`spki`/`ssh-key`), so a malformed key is rejected slightly more
  strictly; well-formed OpenSSH keys decode identically.
- OneTerm's own russh surface — `russh::client::connect`, `client::Handler::check_server_key`,
  `keys::known_hosts::{known_host_keys, learn_known_hosts}`, `keys::agent::client::AgentClient`,
  `PrivateKeyWithHashAlg`, `MethodSet`/`MethodKind`, `Channel`/`ChannelMsg`, and
  `russh_sftp::client::SftpSession` — is untouched: no russh version moves.

## Data Flow

1. `cargo deny check advisories` reads `Cargo.lock` and matches it against the crates.io yank
   status.
2. The lock pins the non-yanked patch releases; no entry matches.
3. `scripts/third-party-notices.py` re-reads the same lock and rewrites the notices table.

## Detail Design

- [ ] Detail design: not needed
- Reason: normal lane, a lockfile re-resolution plus a regenerated generated file. There is no
  code to shape.
