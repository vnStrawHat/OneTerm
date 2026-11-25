# High-Level Design: Master password vault and idle lock

Intake: IN-0050
Lane: high-risk
Date: 2026-10-09

## Idea

One small vault, three uses.

- The **vault** turns a master password the user remembers into the ability to seal and open
  secrets. It stores a salt, KDF parameters and a wrapped random data key in `vault.json`, and
  nothing that reveals the password. Sealed values are short envelope strings (`otv1:...`) that
  live next to the data they belong to.
- **Saved SSH passwords** are the first sealed value: an optional `sealed_password` on a saved
  session, written by the session dialog and opened by the connect dialog.
- **Screen protection** reuses the master password as the unlock secret: once a vault exists,
  an idle timeout drops the data key and replaces every OneTerm window's content with a lock
  screen until the password is typed.

No new third-party crate: Argon2id and AES-256-GCM come from `argon2 0.6` and `aes-gcm 0.11`,
already compiled into OneTerm through `russh`.

## Architecture

### Crate placement (checked against R1-R12)

| Piece | Crate | Layer | Why there |
| --- | --- | --- | --- |
| KDF, key wrap, seal / open, envelope, `vault.json` read / write | **`oneterm-vault`** (new, `crates/vault`) | L1 | gpui-free (R6/R7 style), depends on `oneterm-core` only (persistence, `config_dir`, elevation, `SecretString`). Keeps every crypto dependency behind one crate, so `cargo tree -i argon2` / `-i aes-gcm` listing only `oneterm-vault` among first-party crates is a checkable property. Mirrors `oneterm-update`, an L1 crate that owns its own document. |
| `VaultState` global (phase, unlocked key, back-off, events) and the four master-password dialogs | `oneterm-state` | L2 | Needed by `session-ui`, `settings-ui` and the shell; features may not depend on each other (R5), shared cross-feature logic goes in `state` (R10). `state` already hosts shared dialog scaffolding (`form_dialog`). |
| Lock screen (`RootPlugin`), idle clock, idle timer, Lock action handler | `oneterm-workspace` | L3 | Window chrome is the shell's job; it reaches the vault only through `oneterm-state` (R4 holds: no feature, no backend). Registered from `app::init` like every other init. |
| `sealed_password` field, save and use flows | `oneterm-session-ui` | L3 | Document owner of `ssh_session.json`; names `oneterm_vault::SealedSecret` directly (new edge `session-ui -> vault`, downward). |
| "Security" Settings page | `oneterm-settings-ui` | L3 | Uses `VaultState` and the dialogs from `oneterm-state`; no direct `vault` edge needed. |
| `lock_after_idle_minutes` | `oneterm-settings` (`ui_config.json`) | L1 | Same pattern as `show_fps`. |
| `LockOneTerm` action | `oneterm-actions` | L1 | Where gpui action structs live. |

New internal edges: `oneterm-vault -> oneterm-core`, `oneterm-state -> oneterm-vault`,
`oneterm-session-ui -> oneterm-vault`. All point down (R2), create no cycle (R1), and add no
UI -> backend edge (R3). The crate is a workspace member named `oneterm-vault` (R11); it
registers no panel (R12 not applicable).

**Rejected alternative: a `vault` module in `oneterm-core`.** It saves a manifest, a policy entry
and two doc rows. Against it: R6 describes `core` as the pure domain ("types + traits"), and
every crate that depends on `core` (including `terminal`, `completion`, both backends) would
compile and link the crypto crates; the review boundary for secret-handling code would be a
directory instead of a crate. Kept as the fallback if the owner prefers fewer crates (Q9).

### Vault API (owned by `oneterm-vault`; exact signatures in `crypto-and-format.md`)

| Operation | Effect | I/O | Cost |
| --- | --- | --- | --- |
| `load(dir)` | Read `vault.json` -> `Uninitialised` / `Ready(header)` / `Unsupported` / `Damaged` | read | trivial |
| `init(dir, password)` | New vault id, salt, data key; create `vault.json` exclusively; return an unlocked vault | write (create only) | 1 KDF |
| `unlock(header, password)` | KDF, unwrap data key; the GCM tag is the password check | none | 1 KDF |
| lock | Drop the unlocked vault (zeroizes the key) | none | none |
| `seal(purpose, context, plaintext)` | Encrypt to an `otv1:` envelope | none | microseconds |
| `open(purpose, context, envelope)` | Decrypt, or a typed reason it cannot | none | microseconds |
| `change_password(dir, new)` | Rewrap the same data key under a new salt and current parameters; scrub `vault.bak` | write | 1 KDF |
| `reset(dir)` | Remove `vault.json` and `vault.bak`; owners wipe their sealed values (event) | delete | none |

Every write refuses when `oneterm_core::elevation::is_restricted()` is true (M4).

### Lifecycle state machine (per process)

Two facts make the state: is the data key in memory, and is the screen covered.

```text
                     vault.json absent
                  +---------------------+
   start -------->|   Uninitialised     |---- Set up (create dialog, 1 KDF) ----+
                  |  no lock, no timer  |                                       |
                  +---------------------+                                       v
                            ^                                       +-------------------+
                            | Reset (Settings, confirmed)           |     Unlocked      |
                            |                                       |  key in memory    |
   start (vault.json ok) -->+-------------+                         |  screen open      |
                  +---------------------+ |                         +-------------------+
                  |       Locked        | |  unlock prompt (1 KDF)    |   |        ^
                  |  key not in memory  |-+-------------------------->+   |        |
                  |  screen open        |<---- Lock now (key only) -------+        |
                  +---------------------+     / other instance reset              |
                            |                                                     |
                            | idle timeout / Lock action                          | password ok (1 KDF)
                            v                                                     |
                  +---------------------+                                         |
                  |     IdleLocked      |-----------------------------------------+
                  |  key not in memory  |
                  |  every window shows |---- wrong password: inline error, back-off after 5
                  |  only the lock      |---- Quit: process ends
                  +---------------------+
   Unlocked --- idle timeout / Lock action ---> IdleLocked   (key dropped first, then screen)
   start (vault.json ok) ======================> IdleLocked   (Q1 re-ruled 2026-10-13: from
                                                              the first frame; replaces the
                                                              "start -> Locked" edge above)

   start (vault.json damaged or newer) --> Unavailable: no saving, no lock; Settings explains
```

As built by `US-0157` (`crates/state/src/vault_state.rs`): `VaultPhase` is Uninitialised /
Locked / Unlocked / Unavailable (`IdleLocked` arrives with `US-0159`); Locked holds the header
read once at start and an unlock verifies against it, never against a fresh read of
`vault.json`; one operation runs at a time; a successful unlock of a header below the default
parameters rehashes it in the background; `VaultEvent::Reset` is emitted before the file is
removed. A lock asked for while an operation runs wins: the operation's result never puts a key
back (an unlock answers `LockedMeanwhile`; a set-up, change or rehash keeps only the new
header). The state applies every result itself, so a caller dropping its task never leaves it
busy.

- `Locked -> IdleLocked` on idle too: screen protection does not depend on the user ever having
  unlocked.
- As built by `US-0159`: `IdleLocked` is the two facts made explicit, not a fifth `VaultPhase`
  value. `VaultPhase` stays Locked (the key is not in memory) and `VaultState::is_screen_locked()`
  says the screen is covered; it is true only beside a held header. `lock_screen()` drops the key
  (emitting `Locked` when one was held), sets the screen lock and emits `ScreenLocked`; it does
  nothing without a vault (Uninitialised or Unavailable) and, while an operation runs, sets the
  same `locked_meanwhile` flag `lock()` does, so the operation's result neither puts a key back
  nor lifts the screen lock. `unlock_screen(password)` is the only way out: it verifies against
  the in-memory header, shares the back-off count with the dialogs, and ends Unlocked. While the
  screen is locked, the dialog `unlock`, set-up, change and reset are refused (`WrongPhase`); a
  reset that started before the lock still lands and clears the lock with the vault.
- "Lock now" from the Settings page before `US-0159` drops the key only (`Unlocked -> Locked`);
  from `US-0159` on, the Lock action goes to `IdleLocked`.
- Reset can be done from `Locked` (a forgotten password cannot be proved) but not from
  `IdleLocked` (the lock screen offers no reset; Q7).
- **Lock at startup** (`US-0159` acceptance rework; owner re-ruled Q1 on 2026-10-13): a process
  that reads a usable `vault.json` (Ready) starts in `IdleLocked`, not `Locked`
  (`VaultState::load_at_start`, used by `VaultState::init`): every window is born covered, its
  first frame is the lock screen, and the application view is not rendered before the first
  unlock. No vault (Uninitialised) and an unusable one (Unavailable: damaged, newer, unreadable)
  start unlocked as before; the elevated instance follows the same rule read-only (Q5). No
  setting turns it off. The idle timer does not run until the first unlock, which starts the
  idle clock. With a vault, `Locked` with an open screen is therefore left only to a refused
  seal (`US-0158`, a vault reset or replaced by another process); Lock Now locks the screen
  since `US-0159`, and no caller drops the key alone any more. Because `IdleLocked` has no Reset (Q7), a forgotten master password is recovered by quitting
  and deleting `vault.json` by hand (the lock screen names the folder), which loses every saved
  password (`DEC-0021` rule 2); envelopes left in `ssh_session.json` then ask for the password
  ("the master password was reset", `US-0158`).

### Persisted format (details in the LLDs)

```json
// config_dir()/vault.json  (owner: oneterm-vault)
{
  "schema_version": 1,
  "vault_id": "3f9c0a4e8b1d4c27a6e2f0b9c1d2e3f4",
  "kdf": { "algorithm": "argon2id", "version": 19, "memory_kib": 65536,
           "iterations": 3, "parallelism": 1, "salt": "q8Rk2m3x9C0hX6PpY1nW0g==" },
  "wrapped_key": "otv1:P5wKTosbLCcAAAAAAAAAAAAAAAA...=="
}

// config_dir()/ssh_session.json, one session (owner: oneterm-session-ui; schema stays v2)
{ "id": 4, "label": "prod", "host": "10.0.0.1", "port": 22, "username": "deploy",
  "group": null, "sealed_password": "otv1:P5wKTosbLCcB2s...==" }
```

## Diagram

```text
             +---------------------------- oneterm-app (init order) -----------------------+
             | gpui_component::init -> ... -> app::init -> lock_screen::init (plugin last)  |
             +------------------------------------------------------------------------------+

 session-ui (L3)                settings-ui (L3)                 workspace (L3, shell)
 +------------------------+     +-------------------------+      +-------------------------------+
 | session dialog:        |     | Security page:          |      | LockScreen: RootPlugin        |
 |  password + "Save"     |     |  Set up / Change /      |      |  decorate(): locked ->        |
 | connect dialog:        |     |  Lock now / Reset /     |      |   lock UI replaces surface    |
 |  saved pw, hops        |     |  Lock after N minutes   |      |  canvas: raw mouse listeners  |
 | SshSessionStore        |     +-----------+-------------+      | IdleClock + deadline timer    |
 |  sealed_password       |                 |                    | intercept_keystrokes          |
 |  wipe on Reset event   |                 |                    +---------------+---------------+
 +-----------+------------+                 |                                    |
             |     dialogs, phase, events   |                                    |
             v                              v                                    v
        +----------------------------- oneterm-state (L2) ----------------------------------+
        | VaultState (Global entity): phase, Option<UnlockedVault>, back-off, VaultEvent      |
        | vault_dialogs: create / unlock / change / reset (FormDialog, KDF off the UI thread) |
        +---------------------------------------+---------------------------------------------+
                                                |
                                                v
        +----------------------------- oneterm-vault (L1) ----------------------------------+
        | VaultHeader, UnlockedVault, SealedSecret, seal/open, init/unlock/change/reset     |
        | argon2 0.6 (Argon2id) . aes-gcm 0.11 (AES-256-GCM) . zeroize . base64 . serde     |
        +---------------------------------------+---------------------------------------------+
                                                |
                                                v
        oneterm-core (L0): atomic_write / update_json_file, config_dir, elevation, SecretString

 files: vault.json (+ vault.bak, scrubbed)      ssh_session.json (+ ssh_session.bak)
```

## UI Wireframe

### Create master password (dialog; opened from Settings or from "Save password")

```text
+------------------------------------------------------------+
| Create a master password                               [x] |
+------------------------------------------------------------+
|  OneTerm encrypts saved passwords with it and asks for it  |
|  to unlock the screen after you have been away.            |
|                                                            |
|  Master password *                                         |
|  [ ••••••••••••••                                 ] (eye)  |
|  Strength: long enough                                     |
|  Confirm master password *                                 |
|  [ ••••••••••••••                                 ] (eye)  |
|                                                            |
|  ! OneTerm cannot recover this password. If you forget it, |
|    the only way back is Reset, which deletes every saved   |
|    password. Your sessions are kept.                       |
|                                                            |
|  (error line: "The two passwords differ.")                 |
+------------------------------------------------------------+
|                               [ Cancel ]  [ Create ] (spin)|
+------------------------------------------------------------+
```

### Unlock (dialog; when a saved password is needed and the vault is Locked)

```text
+------------------------------------------------------------+
| Unlock saved passwords                                 [x] |
+------------------------------------------------------------+
|  "prod" has a saved password. Enter your master password.  |
|  Master password                                           |
|  [ ••••••••••••                                   ] (eye)  |
|  (error: "Wrong master password." / "Try again in 8 s.")   |
+------------------------------------------------------------+
|           [ Type passwords instead ]   [ Unlock ] (spin)   |
+------------------------------------------------------------+
```

### Lock screen (replaces the whole content of every OneTerm window)

```text
+--------------------------------------------------------------------------+
| OneTerm (Administrator)?                                  [_] [ ] [x]    |  <- own TitleBar
+--------------------------------------------------------------------------+
|                                                                          |
|                                (lock icon)                               |
|                            OneTerm is locked                             |
|                                                                          |
|                Master password                                           |
|                [ •••••••••••                              ]              |
|                (error / back-off line)                                   |
|                                              [ Unlock ]                  |
|                                                                          |
|                Forgot it?  quit, delete vault.json in <config folder,    |
|                absolute path>; every saved password is lost [ Quit ]     |
|                                                                          |
+--------------------------------------------------------------------------+
   no host names, no tab titles, no counts, no notifications
```

### Settings > Security (new page)

As built by `US-0157` (the screen-lock group and the shortcut on Lock Now arrive with `US-0159`):

```text
+-------------------+------------------------------------------------------+
| ...               |  Security                                            |
| SSH               |  Master Password                                     |
| Security   <      |  +------------------------------------------------+  |
| Network           |  | Status                                         |  |
| About             |  |  Set, locked. Saved passwords need it ...      |  |
|                   |  |                                  [ Unlock... ] |  |
|                   |  | Change                                         |  |
|                   |  |  Saved passwords stay valid; ... [ Change... ] |  |
|                   |  | Lock Now   (Unlocked only)       [ Lock Now ]  |  |
|                   |  | Reset                                          |  |
|                   |  |  Delete the master password and  [ Reset... ]  |  |
|                   |  |  every saved password. Sessions are kept.      |  |
|                   |  +------------------------------------------------+  |
|                   |  Uninitialised: "Not set up." + [ Set Up... ] only.  |
|                   |  Damaged vault.json: the reason + [ Reset... ] only; |
|                   |  newer vault.json: the reason, no button.            |
|                   |  Elevated: status + "Managed from the normal OneTerm |
|                   |  window."; Unlock / Lock Now only.                   |
+-------------------+------------------------------------------------------+
```

The design sketch it refines:

```text
+-------------------+------------------------------------------------------+
| General           |  Security                                            |
| Key Bindings ...  |  +------------------------------------------------+  |
| Terminal ...      |  | Master password                                |  |
| SSH               |  |  Status: Set, locked            [ Unlock ]     |  |
| Security   <      |  |  Change master password         [ Change... ]  |  |
| Network           |  |  Lock OneTerm now (Ctrl+Shift+L) [ Lock now ]  |  |
| About             |  |  Reset: delete the master password and every   |  |
|                   |  |  saved password                 [ Reset... ]   |  |
|                   |  +------------------------------------------------+  |
|                   |  +------------------------------------------------+  |
|                   |  | Screen lock                                    |  |
|                   |  |  Lock after idle (minutes)      [ 15      ]    |  |
|                   |  +------------------------------------------------+  |
|                   |  (Uninitialised: Status "Not set" + [ Set up... ];   |
|                   |   elevated: "Managed from the normal OneTerm         |
|                   |   window." and no buttons)                           |
+-------------------+------------------------------------------------------+
```

### Session dialog, Password method (changed part)

```text
|  Authentication *   (o) Password  ( ) Private Key  ( ) SSH Agent      |
|  Password                                                             |
|  [ •••••••••                                               ] (eye)    |
|  [x] Save password (encrypted with your master password)              |
|      (editing a session that has one: "Saved password  [Replace] [Remove]") |
```

### Connect dialog with a saved password (changed part)

```text
|  ssh://deploy@10.0.0.1:22                                             |
|  Password      (key icon) Using saved password   [Use a different one] |
|  (rejected: "The server rejected the saved password." + empty input   |
|   + [x] Update saved password)                                        |
```

## Data Flow

1. **Set up.** Create dialog -> `SecretString` -> background task: `oneterm_vault::init` (salt,
   vault id, data key, Argon2id, wrap, `update_json_file` create-only) -> `VaultState` becomes
   `Unlocked` and emits `Initialised`; the idle timer starts.
2. **Save a password.** Session dialog Save with the box ticked -> if `Uninitialised`, step 1
   first; if `Locked`, unlock dialog first -> `UnlockedVault::seal(SshPassword, {host, port,
   user}, bytes)` -> `SshSession.sealed_password = Some(envelope)` -> store add / update -> the
   existing coalesced background write of `ssh_session.json`.
3. **Connect.** Connect dialog for a session whose chain has sealed passwords -> if `Locked`,
   unlock dialog (or "Type passwords instead") -> on Connect, `open` each envelope into a
   `SecretString` -> `SshAuthMethod::Password` for that hop or the target -> unchanged backend.
4. **Idle.** Keystrokes (interceptor), raw mouse events and window activation stamp the
   `IdleClock`; the deadline timer finds the deadline passed -> `VaultState::lock_screen()`
   drops the key, sets `IdleLocked`, notifies -> each window's `LockScreen` plugin re-renders
   and its `decorate` returns only the lock UI; focus moves to its password input.
5. **Unlock.** Lock screen password -> background `unlock` against the header held in memory
   (never a fresh read of `vault.json`, so replacing the file cannot unlock a locked window) ->
   `Unlocked`; the plugins render the surface again and restore each window's previous focus.
   **At start** (Q1 re-ruled 2026-10-13) the process begins here: `app::init` ->
   `VaultState::init` reads `vault.json` once and, when it is Ready, sets the screen lock before
   `lock_screen::init` and before the main window opens, so the window's `LockScreen` plugin is
   born locked and its first frame is the lock screen; the unlock starts the idle clock.
6. **Change / Reset.** Settings dialogs -> `change_password` (one file, `.bak` scrubbed) or
   `reset` -> `VaultEvent::Reset` -> `SshSessionStore` removes every `sealed_password` and
   writes with the backup scrubbed -> `Uninitialised`; the lock timer stops.

## Detail Design

- [x] Detail design: required (high-risk)
- Reason: secrets, encryption at rest and a persisted schema change. Four files, one concern
  each: [`crypto-and-format.md`](low-level-design/crypto-and-format.md),
  [`persistence-and-migration.md`](low-level-design/persistence-and-migration.md),
  [`idle-lock-overlay.md`](low-level-design/idle-lock-overlay.md),
  [`ssh-password-save-flow.md`](low-level-design/ssh-password-save-flow.md).
