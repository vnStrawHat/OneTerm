# SSH Authentication

> **Status:** Accepted current product contract.

OneTerm supports password/no-password and private-key SSH authentication through the backend-neutral `SessionFactory` boundary. Strict host-key verification applies equally to every authentication method.

## User Flows

Both saved-session Connect and Quick Connect provide an authentication-method choice:

- **Password** shows an optional masked password field. An empty password requests SSH `none` authentication, preserving existing behavior. When the server rejects `password` but advertises `keyboard-interactive` (typical for `PasswordAuthentication no` + `KbdInteractiveAuthentication yes` PAM setups), the backend transparently retries with keyboard-interactive and answers the prompts with the same password. Only a single round is supported: every prompt of the first info request is answered with the password; a prompt that echoes its input (so it is not a password prompt) or a second round of prompts aborts with an explicit error instead of guessing.
- **Private Key** shows a required editable key-path field, a **Browse** button backed by the operating-system single-file picker, and an optional masked passphrase field.

Saved-session create/edit persists the preferred authentication method and private-key path. Quick Connect persists those values only when the user selects **Save to SSH Sessions**.

## Validation and Errors

Before starting a private-key SSH connection, OneTerm requires:

- a non-empty key path;
- a path that resolves to an existing regular file;
- a file that can be opened for reading.

Validation failures keep the dialog open and show a corrective notification. Key parsing, decryption, unsupported format, server rejection, network, and timeout failures are reported through the normal SSH connection error path without process panic or implicit retry (the keyboard-interactive fallback above is the one deliberate exception, and it stays inside the same 20 s authentication-phase deadline and cancellation check).

When the server rejects every attempted method, the error names the methods the server still accepts (`SSH authentication failed; the server accepts: publickey, keyboard-interactive`) so a wrong method choice is diagnosable from the notification.

## Secret and Persistence Policy

`ssh_session.json` may contain:

- the selected authentication method;
- the private-key filesystem path.

It must never contain a password, private-key contents, or private-key passphrase. Passwords and passphrases are wrapped in `SecretString`, omitted from debug output, cleared from the corresponding UI field after submission, and dropped after authentication. The existing explicit unknown-host-key confirmation may retain one short-lived zeroizing config clone for the approved retry.

Existing session documents with no authentication metadata load as Password. The added fields are backward-compatible and do not change the current document schema version.

## Architecture

- `oneterm-session-ui` owns persisted session authentication preferences, credential collection, path selection, and user-facing validation.
- `oneterm-core` owns `SshConfig`, `SshAuthMethod`, and zeroizing secret types.
- `oneterm-app` provides the concrete `SessionFactory`.
- `oneterm-ssh` loads/decrypts the selected key and calls russh public-key authentication; it also owns the keyboard-interactive fallback (`authenticate_with_password` in `crates/ssh/src/session.rs`) and the transport keepalive (`keepalive@openssh.com` every 30 s, disconnect after 3 unanswered).

No UI crate depends directly on `oneterm-ssh`.

## Out of Scope

- SSH agent authentication.
- Interactive keyboard-interactive dialogs (multi-round prompts, one-time codes, prompts that are not password prompts).
- Automatic discovery or fallback across `~/.ssh/id_*` identities.
- Key generation, conversion, or import.
- Persisting passphrases or integrating a credential vault/platform keychain.
