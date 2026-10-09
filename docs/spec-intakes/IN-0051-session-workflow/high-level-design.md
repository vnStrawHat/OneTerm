# High-Level Design: Connect all in group and per-session startup command

Intake: IN-0051
Lane: normal (Connect All); high-risk recommended (startup command)
Date: 2026-10-09

## Idea

Two independent additions to the saved-session workflow, sharing only the connect path.

1. **Connect All.** A group folder's menu gains "Connect All". It plans every session the
   folder shows, asks for confirmation above 5, asks for the master password at most once, then
   runs the sessions that need no typing through the existing `connect_ssh_session`, at most 4
   at a time, **without a dialog per session**. Each success opens a tab exactly as a single
   connect does. Whatever did not connect (needs typing, untrusted host key, network, a
   rejected saved password) is named in one summary. The batch never closes a dialog it does
   not own, which today's connect path cannot promise (`BUG-0084`).
2. **Startup command.** A saved session may carry one line of text. The connect path hands it
   to the SSH backend inside `SshConfig`; `ssh_main_task`, which owns the channel and sees the
   OSC 133 marks, types it once, after the shell reports its first prompt, unless the user typed
   first. It never runs inside `connect`, never blind, never twice. The shell-integration
   bootstrap is not changed; its marks become the readiness signal.

Recommendations in this design follow the intake's Q1-Q11; a different owner ruling changes
the named step only.

## Diagram

```text
 oneterm-session-ui                                   oneterm-core          oneterm-ssh
 +-------------------------------------------+       +---------------+     +-------------------------+
 | tree_render.rs  folder menu               |       | SshConfig     |     | connect()               |
 |   "Connect All" / "Cancel Connect All"    |       |  + startup_   |     |   hops, target auth     |
 |        |                                  |       |    command    |     |   channel, pty (ECHO 0) |
 |        v                                  |       | validate_     |     |   request_shell         |
 | connect_all.rs                            |       |  startup_     |     |   bootstrap (type-ahead)|
 |   plan(children) -> Ready | NeedsTyping   |       |  command()    |     |   sftp, forwards        |
 |                     | Broken              |       +-------^-------+     |   spawn ssh_main_task --+--+
 |   confirm (> 5) -> ensure_unlocked (once) |               |             +-------------------------+  |
 |   Batch { queue, running <= 4, results }  |               |                                          |
 |        | per session                      |               |             ssh_main_task               |
 |        v                                  |               |             +-------------------------+  |
 | common.rs connect_ssh_session(cfg, req) --+---------------+------------>| feed chunk -> OscRouter |<-+
 |   req.presenter = Dialog|Detached|Batch   |                             |  counts A and B marks   |
 |   success -> TerminalPanel -> new tab     |                             | pending startup cmd:    |
 |   failure -> on_failed (batch: record)    |                             |  ready & !typed -> send |
 | connect_dialog.rs  (single session)       |                             |  deadline -> drop+toast |
 |   cfg.startup_command = session.cmd       |                             +-------------------------+
 | session_dialog.rs  Advanced > Startup cmd |
 +-------------------------------------------+
 oneterm-state: VaultState, ensure_unlocked (unchanged)
```

## UI Wireframe

Folder context menu, idle (new row first: the action on the thing right-clicked, as the
session menu puts "Open" first):

```text
+----------------------------+
| Connect All                |
| Rename Group...            |
| -------------------------- |
| New Session                |
+----------------------------+
```

The same menu while a batch for this group runs (the row swaps; nothing else changes):

```text
+-------------------------------------+
| Cancel Connect All (7 left)         |
| Rename Group...                     |
| ----------------------------------- |
| New Session                         |
+-------------------------------------+
```

"Connect All" is disabled when the folder shows no session (a search hides them all). It
appears in no other menu and has no default key binding.

Confirmation, only when the folder shows more than 5 sessions (Q4); the existing alert-dialog
style:

```text
+----------------------------------------------------------+
| Connect to 12 sessions?                                  |
|                                                          |
| "prod" has 12 sessions. 10 will connect now, 4 at a      |
| time. 2 need a password and will be skipped:             |
| web-03, web-07.                                          |
|                                                          |
|                              [ Cancel ]  [ Connect 10 ]  |
+----------------------------------------------------------+
```

When saved passwords are needed and the vault is locked, the existing Unlock dialog
(`US-0157`) follows, once. Its Cancel cancels the whole Connect All; nothing has started.

Summary toast, one per batch (Success when everything connected, Warning otherwise):

```text
+----------------------------------------------------------+
| (!) "prod": 9 of 12 connected.                           |
|     web-03, web-07: need a password (open to connect)    |
|     db-02: host key not trusted yet (open to verify)     |
+----------------------------------------------------------+
```

At most 5 names are listed, then "and N more"; a cancelled batch says "Cancelled: 9 of 12
connected." Per-session "Connected to X" and error toasts are not shown for batch members.

Session dialog, Advanced section (the field joins jump host, agent forwarding and port
forwards; Advanced opens by itself when the session has a command):

```text
|  v Advanced                                                              |
|  Jump host        [ (none)                                     v ]       |
|  [ ] Forward the SSH agent to the remote host                            |
|  Port forwards    [ + Add forward ]                                      |
|  Startup command  [ cd /srv/app && tail -f log/app.log            ]     |
|                   Typed into the remote shell after its first prompt.    |
|                   Stored in plain text. Do not put passwords or tokens   |
|                   here.                                                  |
```

Save refuses, with the usual toast, a value with a line break or control character ("The
startup command must be one line of text.") or longer than 1024 bytes. Empty or whitespace-only
means none.

The command running, in the new tab (bash; the line editor shows it as typed):

```text
Last login: Thu Oct  9 10:12:03 2026 from 10.0.0.9
deploy@web-01:~$ cd /srv/app && tail -f log/app.log
2026-10-09 10:12:04 INFO started
```

Not sent (no prompt mark within 15 s, or the user typed first), a Warning toast from the tab:

```text
+----------------------------------------------------------+
| (!) Startup command not sent to deploy@web-01:22: the    |
|     remote shell did not report a prompt within 15 s.    |
+----------------------------------------------------------+
```

## Data Flow

### Connect All

1. The folder menu reads the folder `TreeItem`'s children (visible sessions, tree order) and
   calls `connect_all::start(group, ids, window, cx)`. If a batch for that group is running,
   the row is "Cancel Connect All" instead and calls `connect_all::cancel(group)`.
2. **Plan.** Each id is read from the store and classified (pure function, unit tested):

   | Session shape | Plan |
   | --- | --- |
   | No username | NeedsTyping("needs a username") |
   | `jump_chain` error (missing, cycle, too long, a hop without username) | Broken(message) |
   | Target or any hop: Password with a saved password | Ready, needs vault |
   | Target or any hop: Password without a saved password | NeedsTyping("needs a password") |
   | PrivateKey (passphrase unknown, tried without one) | Ready |
   | Agent | Ready |

   A Password session without a saved password is not tried with an empty password: that
   would be an authentication attempt certain to fail on most servers and counts toward
   `MaxAuthTries` / fail2ban. An encrypted private key fails at authentication and is reported
   "needs a passphrase".
3. **Confirm** when more than 5 sessions are shown (Q4).
4. **Unlock once** when any Ready session needs the vault and the phase is `Locked`:
   `ensure_unlocked(window, cx, start)`; Cancel runs nothing, so nothing starts. Phase
   `Uninitialised` or `Unavailable`: those sessions move to NeedsTyping("saved password
   unavailable: <why>") and the rest start. No unlock is asked for when no Ready session needs
   one.
5. **Run.** `Batch` (a `Global` keyed by group name; one main window per process) keeps the queue, the
   running count and the results. `pump()` starts sessions while fewer than
   `CONNECT_ALL_PARALLEL = 4` run. Each start re-reads the session by id (deleted -> recorded
   as skipped), opens saved passwords **at that moment** through `VaultState::unlocked()` (as
   `take_auth` does at Connect; locked since -> recorded "needs the master password"), builds the
   same `SshConfig` the connect dialog builds (including `startup_command`) and calls
   `connect_ssh_session` with `presenter: Batch` and two callbacks: `on_connected` records
   success and pumps; `on_failed` records the reason and pumps.
6. **Presentation `Batch`** (in `connect_ssh_session`): no `close_dialog`, no per-session
   toast, and `AppError::HostKeyUnknown` goes to `on_failed` (recorded "host key not trusted
   yet") instead of opening an alert. A changed key is already an error (`DEC-0010`).
   Success still opens the tab with `add_ssh_terminal_to_dock`, as a single connect does.
7. **Summary** when the queue is empty and nothing runs: one toast (UI Wireframe); the
   `Batch` entry is removed, so the menu row returns to "Connect All".
8. **Cancel**: queued sessions are dropped, running attempts are cancelled through their
   `ConnectionCancellation` (the existing cooperative cancel), tabs already opened stay, and
   the summary says "Cancelled". A cancelled attempt reports nothing back (`connect_ssh_session`
   returns early once its cancellation is set, and drops a session that connected meanwhile),
   so Cancel marks the running ones itself and shows the summary at once. The `Batch` entry is
   created only when the run starts (after confirmation and unlock), so cancelling either of
   those leaves nothing behind.

```text
 state per batch:   Planned --confirm--> Unlocking --ok--> Running --queue empty & 0 running--> Done
                       |                    |                 |                                  |
                       +--cancel/no--> (gone) <--cancel-------+--cancel--> Cancelling --0 running-+
 per session:   Queued -> Starting -> Connecting -> Connected | Failed(reason) | Cancelled
```

Failure policy: a session's failure is recorded and the pump continues; nothing stops the
batch except Cancel or the window closing (the tasks are window-scoped, as today).

### Startup command

1. `connect_dialog.rs` (and Connect All) set `SshConfig.startup_command` from the saved
   session; Quick Connect and Duplicate of a live tab set `None` (Q10).
2. `oneterm_ssh::connect` runs as today (bootstrap included) and hands the command to
   `ssh_main_task` as a `PendingStartup { text, deadline }` (deadline = 15 s after the shell
   request, Q9). It never writes it inside `connect`.
3. After every received chunk, and on a short timer while a grace runs, `ssh_main_task`
   decides (LLD § 2): the shell is **ready** at the first `B` (bash, zsh: drawn with the first
   real prompt, just before the line editor reads), or 250 ms after the first `A` when no `B`
   came (`sh`, `dash`, shells with their own integration). Ready and not typed: it writes
   `text + "\r"` on the channel once and clears the pending value.
4. If the user's input path wrote first (`SshSession::pty_write` sets a flag), or the
   deadline passes, the pending value is dropped and one `SessionEvent::Notification` says why.
5. The terminal shows the command because the shell echoes it: the line editor of bash or zsh
   displays what it reads, and `dash` reads in canonical mode, where the kernel echoes input
   once the bootstrap's closing `stty echo` has run (it runs right after the `A`, well inside
   the grace).

```text
 UI thread                ssh connect (blocking)              remote sshd / shell        ssh_main_task
     | connect(cfg+cmd) ----->|                                      |                        |
     |                        | auth, channel, pty ECHO 0 ---------->|                        |
     |                        | request_shell(true) ---------------->| shell starts           |
     |                        | bootstrap line + \r (type-ahead) --->| (reads it later)       |
     |                        | spawn task(PendingStartup) ----------+----------------------->| pending, deadline
     |<-- Ok(session): tab ---|                                      |                        |
     |                                                               | runs bootstrap:        |
     |                                                               |  OSC 7; A; stty echo ->| A seen (grace starts)
     |                                                               | first prompt + B ----->| ready
     |                                                               |                        | !typed -> data(cmd\r)
     |                                                               |<-----------------------| (once)
     |                                                               | line editor echoes     |
     |                                                               | "cmd", runs it         |
     |  (no mark before the deadline, or the user typed) ............................ drop + Notification
```

## Interfaces

```rust
// oneterm-core (ssh_config.rs)
pub struct SshConfig { /* ... */ pub startup_command: Option<String> }
pub const STARTUP_COMMAND_MAX_BYTES: usize = 1024;
pub fn validate_startup_command(text: &str) -> Result<(), StartupCommandError>;
// StartupCommandError: Empty | TooLong { bytes } | ControlCharacter

// oneterm-session-ui (session_state.rs)
pub struct SshSession {
    /* ... */
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub startup_command: Option<String>,
}

// oneterm-session-ui (common.rs) — BUG-0084 adds Dialog/Detached, US-0160 adds Batch
pub(crate) enum Presenter { Dialog, Detached, Batch }
pub(crate) struct SshConnectRequest { /* ... */ pub presenter: Presenter }

// oneterm-session-ui (connect_all.rs)
pub(crate) fn start(group: String, ids: Vec<SshSessionId>, window: &mut Window, cx: &mut App);
pub(crate) fn cancel(group: &str, cx: &mut App);
pub(crate) fn running(group: &str, cx: &App) -> Option<usize>;      // queued + in flight
fn plan(session: &SshSession, chain: Result<Vec<SshSessionEntry>, JumpChainError>) -> Plan;
const CONNECT_ALL_PARALLEL: usize = 4;
const CONFIRM_ABOVE: usize = 5;

// oneterm-terminal (backend/state.rs): counted beside `prompt_count`, in the same router arm
pub struct SessionState { /* ... */ pub prompt_end_count: usize }   // OSC 133;B

// oneterm-ssh (task.rs)
pub(crate) struct PendingStartup { text: String, deadline: tokio::time::Instant,
                                   first_a_at: Option<tokio::time::Instant> }
```

`Presenter` decides three things in `connect_ssh_session`; nothing else reads it:

| | closes the top dialog on success / before a host-key alert | per-session toasts | unknown host key |
| --- | --- | --- | --- |
| `Dialog` (connect, quick connect, duplicate dialogs) | yes (it is theirs) | yes | alert |
| `Detached` (the retry after "Trust and Connect") | **no** | yes | alert (next hop) |
| `Batch` (Connect All) | no | no (summary) | reported to `on_failed` |

## Alternatives considered

- **Connect All as a loop over `open_connect_dialog`.** Reuses every prompt, but each dialog
  is modal and the success path closes the top dialog, so N dialogs either stack (the issue's
  concern) or close each other. Rejected for the MVP; a one-at-a-time dialog queue for the
  sessions that need typing is the Q2 follow-up.
- **Filter the store by `group`** (the issue's proposal). Repeats the tree's trim rule and
  ignores the search filter and the tree order the user sees. The folder's children already
  are the list.
- **Unbounded parallelism.** Each attempt holds a background-executor thread for up to 60 s.
- **Prompt unknown host keys during the batch.** Stacked fingerprint alerts invite clicking
  through. Reported instead (Q3).
- **Startup command sent in `connect` after the bootstrap** (the issue's send point). It is
  type-ahead into whatever reads the PTY first, invisible on shells without a line editor
  while ECHO is still off, and a write failure there would fail the connection.
- **Startup command sent from the UI after connect returns**, like Duplicate's `cd`
  (`common.rs:482`). Same blind type-ahead; and the UI cannot see the prompt marks (a
  session's one event receiver belongs to its view).
- **A UI-side readiness check.** Would need a new `TerminalSession` accessor or a second event
  consumer; the backend already has the state and the channel.
- **Ready at the first `A` alone.** For bash and zsh the bootstrap's `A` comes before the
  prompt is drawn; a command arriving before the line editor takes the terminal is echoed by
  the kernel and then again by the line editor (shown twice), which a slow prompt (a git
  prompt in a large repository) makes likely. `B` marks the moment the editor is about to read.
- **Reorder the bootstrap** (`stty echo` before the final `__oneterm_precmd`) so that `A`
  implies echo on. Unneeded with the grace, and it would touch a line every SSH session runs.

## Detail Design

- [x] Detail design: required (high-risk) for the startup command; not needed for Connect All
- Reason: the startup command's readiness, bytes, deadline, guard, validation, logging and the
  sealing analysis are in [`low-level-design/startup-command.md`](low-level-design/startup-command.md).
  Connect All composes existing paths; its mechanics fit in this document and `US-0160`.
