# 0001-ssh-key-secret-persistence Persist SSH key paths but never key passphrases

Date: 2026-08-11

## Status

Accepted

## Context

Private-key authentication needs a reusable identity selection for saved sessions, but an encrypted key's passphrase is a credential. Persisting the passphrase would expand OneTerm's secret-storage and encryption responsibilities, while asking users to select the same key on every connection would make saved sessions unnecessarily cumbersome.

## Decision

Saved SSH sessions may persist an authentication-method preference and the filesystem path to a private key. Passwords, private-key contents, and private-key passphrases must never be written to `ssh_session.json` or another OneTerm persistence document. A passphrase is collected only when connecting, wrapped in `SecretString`, cleared from UI state after submission, and retained only for the active attempt and an explicit unknown-host-key retry.

Quick Connect may retain the selected path only when the user explicitly saves that connection as a session. Existing sessions without authentication metadata default to password authentication.

## Alternatives

- Persist an encrypted passphrase: rejected because OneTerm has no credential-vault contract, master-key lifecycle, or platform keychain integration.
- Persist no key path: rejected because it defeats the convenience and intent of a saved SSH session.
- Automatically search standard SSH identities: deferred because ordering, user control, agent interaction, and unexpected key disclosure require a separate contract.

## Consequences

- Saved sessions can reconnect conveniently without storing credential material.
- Anyone who can read `ssh_session.json` can learn the local path and filename of the selected identity; this is accepted as non-secret connection metadata.
- Encrypted keys require the user to enter the passphrase for each connection attempt.
- Future credential-vault or SSH-agent support must be designed as a new capability rather than silently changing this persistence rule.
