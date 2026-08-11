# Work: Connect with an SSH private key

ID: US-004
Created: 2026-08-11

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Keep operational status in `harness.db`.

## Classification

- Change type: new capability
- Risk lane: high-risk (authentication and secrets)
- Spec Intake, when required: `docs/intakes/IN-0004-ssh-private-key-authentication.md`

## Outcome

Users can choose password or private-key authentication for saved SSH sessions and Quick Connect. Private-key authentication accepts an editable file path or native file picker selection and an optional in-memory passphrase.

## Scope

- In scope: saved authentication method and private-key path; connect-time key path and optional passphrase; native single-file picker; backend config construction; validation; persistence and focused tests.
- Out of scope: SSH agent authentication, key generation/import, passphrase storage, automatic key discovery, identity fallback chains, and changes to host-key verification.

## Acceptance

- Saved-session create/edit lets the user choose Password or Private Key and persists only that method and key path.
- Existing session documents without auth metadata load as Password without migration failure.
- Saved-session connect and Quick Connect expose Password or Private Key fields appropriate to the selected method.
- A private key can be entered as a path or selected through the native single-file picker.
- Private-key connect builds `SshAuthMethod::PrivateKey`; an empty passphrase becomes `None`, while a supplied passphrase uses `SecretString` and is cleared from the input after submission.
- Missing, non-file, or unreadable key paths fail as corrective user-input errors before an SSH attempt starts.
- Passwords and passphrases are never serialized or included in debug output.
- Existing password and no-password behavior remains available.

## Documentation

### Owning Docs Reviewed

- `docs/PROJECT.md` — app/backend boundaries and secret-sensitive project invariants.
- `docs/ssh-client-connect.md` — historical connect flow and existing private-key backend statement.
- `docs/terminal-backend.md` — SSH backend ownership and russh integration.
- `docs/agents/persistence.md` — session schema ownership and migration rules.
- `docs/agents/error-policy.md` — user-input and transport error behavior.
- `docs/agents/crate-dependency-rules.md` — UI must use `SessionFactory` rather than depend on the SSH backend.
- `reference/gpui-component/crates/story/examples/editor.rs` — pinned GPUI native path-picker usage.

### Documentation Action

Update required: establish `docs/ssh-authentication.md` as the current product contract and link it from `docs/ssh-client-connect.md`.

Reason: the historical design describes password-only UI despite the backend already supporting private keys, and no accepted current contract describes persistence or passphrase handling.

### Reconciliation

Updated `docs/ssh-authentication.md` with the accepted current behavior and linked it from `docs/ssh-client-connect.md`. Added decision `docs/decisions/0001-ssh-key-secret-persistence.md` for the secret-lifecycle rule.

## Context

`oneterm-core::SshAuthMethod::PrivateKey` and `oneterm-ssh::session::load_private_key` already support private-key authentication. The missing seam is `oneterm-session-ui`, whose dialogs always construct `None` or `Password`. `ssh_session.json` is owned by `oneterm-session-ui`; backward-compatible defaulted fields do not require a schema-version bump.

## Plan

1. Add a persisted auth preference and optional key path to `SshSession` with secure backward-compatible defaults.
2. Add shared auth form state/rendering, native file selection, validation, and config construction.
3. Integrate it into saved-session create/edit, saved-session connect, and Quick Connect.
4. Add unit and persistence coverage, then run affected and workspace quality gates.

## Decisions

- `docs/decisions/0001-ssh-key-secret-persistence.md`

## Verification Plan

- `cargo test -p oneterm-session-ui`
- `cargo test -p oneterm-core -p oneterm-ssh`
- `python scripts/verify-dependency-graph.py`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo build --workspace`
- Manual E2E is required for native picker and real encrypted/unencrypted SSH key authentication; report if unavailable.

## Evidence and Gaps

- `cargo test -p oneterm-session-ui -p oneterm-core -p oneterm-ssh` — passed; focused run reported 57 tests passed.
- `cargo clippy -p oneterm-session-ui --all-targets -- -D warnings` — passed.
- `python scripts/verify-dependency-graph.py` — passed for all 18 workspace packages and explicit members.
- `cargo fmt --all -- --check` — passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed.
- `cargo build --workspace` — passed.
- `cargo test --workspace` — passed: 570 passed, 2 ignored.
- Manual native-file-picker and real-server E2E with encrypted and unencrypted keys was unavailable in this non-interactive session. The UI/platform flow and real server authentication therefore remain manually unverified.

## Handoff

Not applicable; one session owns implementation and verification.
