# 0002-ssh-duplicate-auth Prompt again for SSH duplicate authentication

Date: 2026-08-11

## Status

Accepted

## Context

OneTerm currently keeps SSH password and private-key passphrase material only for the connection attempt. Zeroizing wrappers clear their allocations after the final short-lived `SshConfig` owner is dropped. A one-click SSH Duplicate Session could retain a credential-bearing config for the lifetime of the original session, but that would expand secret lifetime and attack surface.

## Decision

SSH Duplicate Session retains and passes only non-secret connection metadata: host, port, username, authentication method, optional private-key path, host-key policy inputs that contain no credentials, and shell-integration preference. It opens a prefilled authentication dialog and requires the user to enter password/passphrase material again. Password and passphrase fields are never prefilled from the source session.

## Alternatives

- Retain the complete `SshConfig` in the live session: rejected because it keeps password/passphrase material alive for the full session lifetime.
- Try reconnecting first and prompt only on failure: rejected because password auth cannot be retried without retained credentials and failure-driven prompting creates inconsistent UX and unnecessary external connection attempts.

## Consequences

- SSH session duplication preserves the existing short secret lifetime and zeroization model.
- SSH duplication requires one additional authentication interaction, while local duplication remains immediate.
- Private-key paths may be prefilled because they are non-secret persisted metadata; encrypted-key passphrases remain empty.
