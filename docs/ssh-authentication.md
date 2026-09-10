# SSH Authentication

> **Status:** Accepted current product contract.

OneTerm supports password/no-password, private-key, and SSH agent authentication through the backend-neutral `SessionFactory` boundary. Strict host-key verification applies equally to every authentication method.

## User Flows

Both saved-session Connect and Quick Connect provide an authentication-method choice:

- **Password** shows an optional masked password field. An empty password requests SSH `none` authentication, preserving existing behavior. When the server rejects `password` but advertises `keyboard-interactive` (typical for `PasswordAuthentication no` + `KbdInteractiveAuthentication yes` PAM setups), the backend transparently retries with keyboard-interactive and answers the prompts with the same password. Only a single round is supported: every prompt of the first info request is answered with the password; a prompt that echoes its input (so it is not a password prompt) or a second round of prompts aborts with an explicit error instead of guessing.
- **Private Key** shows a required editable key-path field, a **Browse** button backed by the operating-system single-file picker, and an optional masked passphrase field.
- **SSH Agent** shows no credential field (the connect dialog shows one caption line instead). The backend connects to the local agent and offers its identities in the agent's order until the server accepts one. Discovery order follows OpenSSH: on Windows the `\\.\pipe\openssh-ssh-agent` named pipe (also served by 1Password and gpg-agent emulation) then Pageant; on Unix `$SSH_AUTH_SOCK`. At most 6 identities are offered (OpenSSH's default `MaxAuthTries`), and the loop stops early when the server no longer lists `publickey` among the methods it accepts. RSA identities sign with the `rsa-sha2-*` hash the server advertises, exactly like RSA key files. Three failures have their own text: no agent reachable (each candidate and its error are named), an agent with no identities (`add one with ssh-add`), and no identity accepted (the remaining-methods message below plus how many identities were offered). Certificates held by the agent are offered as their bare public key.

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

It must never contain a password, private-key contents, or private-key passphrase. The SSH agent method persists nothing but the method name: identities stay in the agent, and OneTerm only relays public keys and signature requests. Passwords and passphrases are wrapped in `SecretString`, omitted from debug output, cleared from the corresponding UI field after submission, and dropped after authentication. The existing explicit unknown-host-key confirmation may retain one short-lived zeroizing config clone for the approved retry.

`auth_method` is `password` (the default, omitted when serialized), `private_key`, or `agent`; `key_path` only accompanies `private_key`. Existing session documents with no authentication metadata load as Password. The added fields are backward-compatible; the document schema itself is versioned separately (v2 adds a stable `id` per session, see `crates/session-ui/src/session_state.rs`).

## Architecture

- `oneterm-session-ui` owns persisted session authentication preferences, credential collection, path selection, and user-facing validation.
- `oneterm-core` owns `SshConfig`, `SshAuthMethod`, and zeroizing secret types.
- `oneterm-app` provides the concrete `SessionFactory`.
- `oneterm-ssh` loads/decrypts the selected key and calls russh public-key authentication. The SSH agent path lives in `crates/ssh/src/agent.rs`: agent discovery, the identity loop (`authenticate_with_agent_client`, unit-tested against russh's in-process agent server), and the error texts above. RSA keys never sign with the legacy SHA-1 `ssh-rsa` flavour: the client asks the server for its `server-sig-algs` (RFC 8308) and signs with the advertised `rsa-sha2-*` hash, falling back to SHA-512 (`rsa_hash_alg` in `crates/ssh/src/session.rs`). A session dropped without an explicit `close()` still requests the transport close (`impl Drop for SshSession`). It also owns the keyboard-interactive fallback (`authenticate_with_password` in `crates/ssh/src/session.rs`) and the transport keepalive (`keepalive@openssh.com` every 30 s, disconnect after 3 unanswered).

No UI crate depends directly on `oneterm-ssh`.

## Out of Scope

- SSH agent forwarding (planned as US-0060 under IN-0023).
- Interactive keyboard-interactive dialogs (multi-round prompts, one-time codes, prompts that are not password prompts).
- Automatic discovery or fallback across `~/.ssh/id_*` identities.
- Key generation, conversion, or import.
- Persisting passphrases or integrating a credential vault/platform keychain.
