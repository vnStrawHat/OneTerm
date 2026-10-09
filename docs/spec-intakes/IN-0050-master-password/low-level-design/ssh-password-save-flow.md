# Low-Level Design: Saving and using SSH passwords

Intake: IN-0050
HLD: `docs/spec-intakes/IN-0050-master-password/high-level-design.md`
Topic: session dialog, connect dialog, jump hops, failures, what is out of scope
Date: 2026-10-09

## Concern

The user-facing flows of owner request items 1 and 2: saving an SSH password under the master
password, and connecting with it. Owned by `oneterm-session-ui`; the vault calls go through
`oneterm-state`'s `VaultState` and dialogs, the envelope type is `oneterm_vault::SealedSecret`.
The backend (`oneterm-ssh`, `oneterm_core::SshConfig`) does not change: a saved password reaches
it as the same `SshAuthMethod::Password { password: SecretString }` a typed one does.

## Design

### 1. Where things are today

- Session dialog (`crates/session-ui/src/session_dialog.rs`): Label, Host, Port, Username, the
  auth method radio and key path (`auth_form.render(false, cx)`, line 588: **no** secret
  inputs), Group, colour, advanced (logging, jump host, forwards, agent forwarding). Save builds
  an `SshSession` through `SessionForm::into_session` (line 74) and calls `store.add` / `update`.
- Connect dialog (`connect_dialog.rs:51` `open_connect_dialog`): resolves the jump chain first
  (`store.jump_chain`), one `SshAuthForm` per hop above the target's own (`jump_hops.rs`
  `JumpHopForms`), optional username with a "Save username to session" checkbox, a
  `ConnectButton` that keeps the dialog open while the background attempt runs, and
  `SshConnectRequest { on_connected, on_failed, .. }` callbacks (`common.rs:383-390`).
- `SshAuthForm::take_auth` (`auth_form.rs:224`) turns the password input into
  `SshAuthMethod::Password` (empty -> `None`), never clearing the field (`US-0118`).

### 2. Session dialog: saving

Shown only when the method is **Password** and the process is not elevated (M1 already hides the
dialog there).

```text
|  Username        [ deploy                                    ]          |
|  Authentication *   (o) Password  ( ) Private Key  ( ) SSH Agent         |
|  Password        [ ••••••••••                              ] (eye)       |
|  [x] Save password (encrypted with your master password)                 |
|      Enter a username to save a password.        <- when Username empty  |
```

Editing a session that has a `sealed_password`:

```text
|  Password        (key) Saved password         [ Replace ]  [ Remove ]    |
```

`Replace` shows the empty input with the box ticked; `Remove` marks the field for removal on
Save. The saved password is never shown, revealed or copied (there is no "show" for it).

**Save, step by step** (the dialog's submit returns `false` and keeps the dialog open whenever
a vault step runs, then closes itself when the step completes, as `FormDialog`'s `SubmitFn`
allows):

```text
validate (existing rules; plus: "Save password" needs a username and a non-empty password)
match (save_password_requested, VaultState.phase):
  (false, _)                        -> existing save; field kept / resealed / removed per section 3
  (true, Uninitialised)             -> Create master password dialog (stacked)
                                         ok     -> seal, save, close both
                                         cancel -> session dialog stays open, box stays ticked,
                                                   inline note "Not saved: a master password is needed."
  (true, Locked)                    -> Unlock dialog (stacked), then as above
  (true, Unlocked)                  -> seal, save, close
  (true, Unsupported | Unavailable) -> inline error naming why; nothing saved until the box is
                                       unticked (the session can then be saved without it)
seal = UnlockedVault::seal(SshPassword, {host, port, username}, SecretString(typed))
       -> SshSession.sealed_password = Some(envelope); the password input is cleared
```

The typed password is copied into a `SecretString` once, at Save; the input is cleared after a
successful seal. Saving a password is a deliberate act: the box defaults to **unticked**, like
"Save username to session".

### 3. Session dialog: keeping a saved password valid

The envelope is bound to host, port and username (`crypto-and-format.md` § 4, intake Q3).

| Edit on Save | Unlocked | Locked |
| --- | --- | --- |
| Host, port and username unchanged | keep the envelope as is | keep |
| Any of the three changed | open with the old context, seal with the new | Unlock dialog: "Unlock to keep the saved password for the new address" with **Remove saved password** as the alternative |
| Method changed to Private Key or SSH Agent | remove (backup scrubbed) | remove |
| Username cleared | remove, with an inline note | remove |
| `Remove` clicked | remove (backup scrubbed) | remove |

Removal never needs the vault. If re-opening fails (`OtherVault`, `Rejected` ...), the old value
is dropped with an inline note, never carried forward unverified.

Duplicate Session (`duplicate_in`) copies the envelope unchanged: same address, still valid.

### 4. Connect dialog: using saved passwords

For the target and each hop, a password form is in **saved** mode when that session's
`auth_method` is Password and it has a `sealed_password`. `HopSpec` (`jump_hops.rs:38`
`from_entry`) gains the hop session's `sealed_password` and copies it the way it copies
`key_path`.

On open:

```text
any form in saved mode?
  no                         -> the dialog as today
  yes, phase Unlocked        -> saved-mode rows (below)
  yes, phase Locked          -> Unlock dialog stacked on top:
                                  "prod (and 1 jump host) have saved passwords."
                                  [ Type passwords instead ]  [ Unlock ]
                                Unlock ok -> saved-mode rows; Type instead -> ordinary inputs
  yes, Uninitialised / Unsupported / Unavailable
                             -> ordinary inputs plus a muted note "Saved password unavailable: <why>"
```

A saved-mode row replaces the password input:

```text
|  Jump host bastion (ops@10.0.0.2:22)                                     |
|  Password   (key) Using saved password            [ Use a different one ] |
|  ---------------------------------------------------------------------- |
|  ssh://deploy@10.0.0.1:22                                                |
|  Password   (key) Using saved password            [ Use a different one ] |
```

`Use a different one` swaps the row back to an empty masked input with an **Update saved
password** checkbox (unticked). Initial focus goes to the Connect button when every row is in
saved mode, so **Enter connects** (intake Q4: the dialog still opens; one-click connect is a
follow-up).

On Connect, `take_auth` for a saved-mode form opens the envelope **at that moment** with the
form's own context (that hop's or the target's host, port, username) into a fresh
`SecretString`. Nothing is decrypted at dialog open, and nothing decrypted is kept in a widget.
If `open` fails, the row turns into an empty input with an inline reason ("Saved under a
master password that was reset", "Saved by a newer OneTerm", "Can't be used for this
address") and Connect is not sent.

### 5. Saving from the connect dialog

The target's password row also offers, when it is an ordinary input:

```text
|  [ ] Save password (encrypted with your master password)                 |
```

- Ticking it while the username is asked at connect time also ticks and locks **Save username
  to session**: a sealed password is bound to a username, so it cannot be saved without one.
- On Connect with the box ticked: if the vault is `Uninitialised` or `Locked`, the Create or
  Unlock dialog runs **first** (never after a connection is up); then the attempt starts.
- The password is sealed and written in `on_connected`, i.e. only after the server accepted
  it. A wrong password is never saved.
- Hops keep their own saved passwords; updating a hop's is done from that hop's session dialog
  (smaller surface; noted as a follow-up).

### 6. When the server rejects a saved password

`SshConnectRequest::on_failed` today receives only the message. It is extended to receive the
failure (`&AppError` or its `ConnectPhase` plus the hop index). `hop_error`
(`crates/ssh/src/route.rs:204-211`) already prefixes hop failures with the hop label, and the
phase survives; a structured hop index is preferred over matching the label.

On `ConnectPhase::Authentication` for a row that used a saved password:

```text
|  Password   [                                            ] (eye)        |
|  [x] Update saved password                                               |
|  ! The server rejected the saved password. Type the current one.        |
```

- The saved password is **not** removed automatically (a server-side lockout or a typo on the
  server must not destroy it).
- A successful connect with the box ticked reseals and updates that session (target only;
  for a hop, the note says to edit the hop's session).
- Any other phase (host key, network) leaves the row in saved mode.

### 7. Quick Connect, Duplicate of a live tab, key passphrases

- **Quick Connect** (`quick_connect_dialog.rs`) has no saved session, so nothing is saved by
  default. Its "Save to SSH Sessions" option could add "Save password" with the same rules:
  follow-up packet.
- **Duplicate of a live tab** (`SshDuplicateConfig`) keeps metadata only and prompts for every
  hop again (`docs/ssh-authentication.md`); using saved passwords there is a follow-up.
- **Private-key passphrases** are the obvious second purpose (`key-passphrase`, context = the
  canonical key path). They stay forbidden by `DEC-0001` until a packet adds the purpose; the
  envelope and AAD scheme already allow it without a format change.

### 8. What `oneterm-session-ui` must never do

- write a password to any field other than `sealed_password`, or ever in plain text;
- show, copy or log a saved password, or log an envelope;
- fall back to treating an unreadable `sealed_password` as a password;
- keep a decrypted password beyond the `SshConfig` it was opened for (the existing zeroizing
  path takes over from there).

## Interfaces

```rust
// session_state.rs
pub struct SshSession { /* ... */ pub sealed_password: Option<oneterm_vault::SealedSecret> }
impl SshSessionStore { pub fn clear_sealed_passwords(&mut self, cx: &mut Context<Self>); }

// auth_form.rs: the password half gains a saved mode
pub(crate) enum PasswordSource { Typed, Saved(oneterm_vault::SealedSecret, SealContextOwned) }
pub(crate) fn take_auth(&self, cx: &App) -> Result<SshAuthMethod, String>;   // opens Saved at call time

// common.rs
pub on_failed: Option<Rc<dyn Fn(&ConnectFailure, &mut App)>>,   // message + phase + hop index

// oneterm-state vault dialogs (US-0157), used here
pub fn ensure_unlocked(reason: UnlockReason, window: &mut Window, cx: &mut App,
                       then: impl FnOnce(UnlockOutcome, &mut Window, &mut App) + 'static);
// UnlockOutcome = Unlocked | Skipped | Cancelled; creates the vault first when Uninitialised
```

## Edge Cases and Failure Modes

- [ ] Saved password but the session's method is no longer Password (hand edit): ignored, not
  used, removed on the next save.
- [ ] Two jump hops point to the same saved session id: each opens its own copy; same envelope.
- [ ] Chain where one hop is saved and another is not: the unlock prompt appears once; the
  unsaved hop shows an ordinary input.
- [ ] Vault reset while a connect dialog shows saved rows: `open` at Connect returns
  `OtherVault`; the row switches to manual entry.
- [ ] Idle lock while a connect dialog is open: the dialog is hidden, not closed; after unlock
  it is as it was (its saved rows still open at Connect time).
- [ ] Unknown host key retry (`US-0118` keeps one zeroizing config clone for the approved
  retry): unchanged; the opened password lives in that clone like a typed one.
- [ ] Old binary edits the session: `sealed_password` dropped on its write (documented in
  `persistence-and-migration.md` § 6).

## Verification

- [ ] Unit: `SessionForm` with `save_password` and no username is refused; reseal decision table
  of section 3; `lenient_sealed`.
- [ ] gpui tests: the session dialog with a fake `VaultState` in each phase drives the
  expected stacked dialog; the connect dialog renders saved rows, focuses Connect, and
  `take_auth` opens at call time (a fake vault counts opens).
- [ ] E2E on Windows. Today's loopback `sftp-dev-server` (`crates/tools/src/bin/sftp-dev-server.rs`)
  accepts **any** password (`auth_password` returns `Accept`) and serves no `direct-tcpip`
  channel, so it proves save, restart and connect-after-one-unlock but not rejection or a jump
  chain. For those, either add a `--password <value>` check to the dev server (a small tools
  change inside the packet) and use a real OpenSSH host (WSL or a VM) for the 2-hop chain, or
  record the gap. Cases: save, restart, connect after one unlock; a 2-hop chain with two saved
  passwords connects without typing; a rejected password shows section 6 and the saved value
  survives until replaced; a host edit while locked asks to unlock or remove.
- [ ] `ssh_session.json` after the E2E: only `otv1:` values; a search for the plain password in
  the config directory (including `.bak` files) finds nothing.

## As built (`US-0158`, 2026-10-09)

The design above holds; where the build differs or chose among open options:

- **State on the form, not a `PasswordSource` enum.** `SshAuthForm` (`auth_form.rs`) carries
  the saved password with its address (`SavedPassword`), "saved row in use", the save box, and
  a note line, all `Rc` cells shared by its clones. `take_auth` opens the envelope at Connect
  through `VaultState::unlocked()`; a failure turns the row into an input with the reason.
- **One Unlock dialog, unchanged.** The connect dialog stacks the `US-0157` Unlock dialog
  ("Unlock saved passwords"); its Cancel is "type the passwords instead". The sketch's line
  naming the sessions ("prod (and 1 jump host) ...") is not shown.
- **Messages are toasts in the session dialog.** It has no inline-error area: "needs a
  username", "needs a password", "unavailable: <why>", "Unlock to keep the saved password for
  the new address, or click Remove." and the removal notes are notifications. A cancelled Set
  up / Unlock leaves the dialog open with the box ticked and says nothing (`ensure_unlocked`
  reports no cancel).
- **Reseal without a vault.** Uninitialised: the old envelope cannot open, so it is dropped
  with a note (no set-up offered for it). Unavailable: Save is refused with the reason (the user
  can Remove). A typed Seal with Uninitialised runs Set up first.
- **Failure routing.** `SshConnectRequest::on_failed` receives the `AppError`; a hop is found by
  the `jump host user@host:port:` prefix `oneterm-ssh` already puts on hop errors, not by a
  structured hop index (no core or backend change). `on_connected` also receives the window, to
  report "Password saved." or why not.
- **Seal path.** `VaultState::seal` (`oneterm-state`) re-reads `vault.json` and compares
  `vault_id()` first; a mismatch or a missing file drops the key, moves the phase to what the
  file says, emits `VaultEvent::Locked` and refuses with `VaultActionError::ResetElsewhere`.
  It runs on the UI thread (a small file read and one AES-GCM seal, no Argon2id).
- **`.bak` scrub.** `oneterm_core::atomic_write_sealed(path, bytes, retires_secret)` decides
  under the document's lock, against the document being replaced: the write scrubs when any
  `sealed_password` there is missing from the new document. This covers Remove, Replace,
  reseal, method change, a deleted session and the reset wipe, and survives the store's
  coalescing queue; every write is owner-only on Unix.
- **`lenient_sealed`** warns without the session id (a field deserializer cannot see it).
- **Connect-dialog save with an unavailable vault** is refused until the box is unticked,
  like the session dialog.
- **Quick Connect** uses a picked jump host's saved password only while the vault is already
  unlocked (no prompt) and never saves one.
- **`SshSessionStore::set_sealed_password`** refuses when the session's address or method
  changed while the connection was being made.
- **Dev server.** `sftp-dev-server --password PW` rejects every other password, and relays a
  `direct-tcpip` channel to a loopback address, so the rejection and a two-hop chain are walked
  on loopback (a shell channel is the only one it echoes).
