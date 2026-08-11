# Spec Intake: SSH private-key authentication

ID: IN-0004
Date: 2026-08-11
Type: new_spec
Lane: high_risk

## Source

The user requested completion of SSH key authentication because OneTerm currently exposes only username/password authentication. Clarifications accepted on 2026-08-11 require support in both saved-session and Quick Connect flows, persistence of only the private-key path, and both an editable path field and an operating-system file picker.

## Requested Outcome

Support SSH private-key authentication in saved-session and Quick Connect flows, persist only the selected key path, keep passphrases in memory, and provide both native file selection and editable path input.

## Project Impact

- Adds an authentication-method choice and private-key inputs to `oneterm-session-ui`.
- Extends the `ssh_session.json` session schema with backward-compatible optional/defaulted authentication metadata.
- Uses the existing `oneterm-core::SshAuthMethod::PrivateKey` contract and `oneterm-ssh` key loader; no UI-to-backend dependency is added.
- Changes credential handling, so secret lifetime, debug output, validation, and persistence require focused proof.

## Candidate Product Contracts

| Contract | Purpose | Source or owner |
| --- | --- | --- |
| `docs/ssh-authentication.md` | Current SSH authentication UX, persistence, validation, and security contract | `oneterm-session-ui`, `oneterm-core`, `oneterm-ssh` |
| `docs/ssh-client-connect.md` | Historical connection-flow design and links to the current contract | Project documentation |

## Candidate Work Packets

| Packet | Outcome | Dependencies |
| --- | --- | --- |
| `US-004` | Connect saved and ad-hoc sessions with a private key | This intake and decision `0001-ssh-key-secret-persistence` |

## Architecture and Boundary Questions

- Runtime and owning boundary: UI selects authentication and builds `SshConfig`; `oneterm-app` routes through `SessionFactory`; `oneterm-ssh` loads and uses the key.
- Data ownership and lifecycle: `oneterm-session-ui` owns saved auth method and key path; passphrases remain ephemeral `SecretString` values.
- Auth, security, privacy, or audit: never persist or log passwords, private-key contents, or passphrases; validate key paths before starting the network operation.
- External systems and side effects: native file selection and local private-key reads; SSH server authentication remains subject to strict host-key verification.
- Public interfaces and compatibility: existing saved sessions without auth metadata continue to mean password authentication; no crate boundary changes.

## Validation Shape

| Layer | Expected proof |
| --- | --- |
| Focused | Auth selection/config construction and key-path/passphrase validation unit tests |
| Unit | Session serialization defaults to password and persists key path without any passphrase |
| Integration | Existing SSH backend private-key loader tests and affected crate tests pass |
| E2E | Manual saved-session and Quick Connect flows against encrypted and unencrypted keys |
| Platform / Release | Format, warning-free workspace clippy, workspace build, dependency graph policy |

## Open Decisions and Questions

- Resolved by the user: support both saved-session and Quick Connect flows.
- Resolved by the user: persist only the key path and always collect passphrases at connection time.
- Resolved by the user: expose both editable path input and a native file picker.
- SSH agent authentication remains out of scope.

## First Action or Handoff

Implement `US-004` after establishing `docs/ssh-authentication.md` and decision `0001-ssh-key-secret-persistence`; stop if the pinned GPUI file-picker API cannot safely update dialog state after asynchronous selection.

## Harness Delta

None.
