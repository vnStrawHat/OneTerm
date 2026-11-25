# Low-Level Design: Per-session startup command

Intake: IN-0051
HLD: `docs/spec-intakes/IN-0051-session-workflow/high-level-design.md`
Topic: readiness detection, exact bytes, deadline, typed-first guard, validation, logging,
sealing option
Date: 2026-10-09

## Concern

How `oneterm-ssh` types a saved session's startup command into the remote shell: when it
counts the shell as ready, which bytes it writes, what stops it, what it tells the user and
what it never writes down. Owned by `US-0161` (backend) and `US-0162` (field and persistence).

## Design

### 1. What is carried where

```text
ssh_session.json  SshSession.startup_command: Option<String>          (US-0162, plain text, Q8)
      |  connect dialog / Connect All copy it; Quick Connect, live Duplicate: None (Q10)
      v
oneterm-core      SshConfig.startup_command: Option<String>           (validated again here)
      |  oneterm_ssh::connect moves it out of the config before any await that can fail
      v
ssh_main_task     PendingStartup { text, deadline, first_a_at }        (one per connection)
```

`connect` never writes it. A connect that fails (any phase, including SFTP or forwards being
skipped) simply drops it; a host-key retry carries it in its config clone like everything else.
Jump hops carry no command: `SshHop` has no such field, and `HopSpec::from_entry` copies none.

### 2. Readiness

The shell-integration bootstrap (unchanged) is the source of the marks. What each shell sends
after it, in order:

| Remote shell | Marks after the bootstrap runs | Ready when |
| --- | --- | --- |
| bash, zsh | `A` (bootstrap's closing call), then `A` + `B` with every prompt | first `B` |
| `sh`, `dash`, `ash` | one `A`, nothing per prompt | 250 ms after that `A` |
| fish 4 or later, or any shell with its own OSC 133 integration | whatever it emits (it cannot parse the bootstrap) | first `B`, else 250 ms after an `A` |
| csh, tcsh, older fish, PowerShell / cmd on a Windows OpenSSH server | none | never: deadline |
| not a shell: `passwd` on an expired password, an rc-file menu, an auto-attached `tmux` | none | never: deadline |

Why `B`: bash's readline and zsh's line editor take the terminal *before* drawing the prompt,
so a `B` (the end of `PS1`) means the editor is about to read and will display whatever it
reads. Bytes arriving earlier, while the shell is still busy (a slow `PROMPT_COMMAND`, a git
prompt), are echoed by the kernel and then again by the editor. Why the grace: `sh` and `dash`
emit no `B`; their bootstrap `A` comes just before its `stty echo`, and 250 ms covers that and
the first prompt. The grace is not a timeout for slow shells; the deadline is.

State machine, evaluated after every received chunk and when a timer fires:

```text
            +---------- user wrote (flag) ---------------------------> Dropped(TypedFirst)
            |
 Pending ---+---------- B seen (prompt_end_count >= 1) --------------> Send
   |        |
   |        +---------- A seen first time -> first_a_at = now
   |                    now >= first_a_at + 250 ms, no B ------------> Send
   |
   +------------------- now >= deadline (15 s after the task starts) --> Dropped(NoPrompt)
   +------------------- channel closed / task ends ------------------> Dropped (silent)

 Send: validate again -> channel.data(text + "\r") -> add_tx_bytes -> Sent (terminal state)
```

The check order inside one evaluation is: typed flag, then readiness, then deadline. So a key
typed in the same chunk window as the mark wins (nothing sent), and a mark that arrives exactly
at the deadline still sends.

Timer: one `tokio::time::sleep_until(next_wake)` arm in the task's `select!`, enabled only
while pending, where `next_wake` is the earlier of the deadline and `first_a_at + 250 ms`.
No other timer exists while nothing is pending.

### 3. "The user typed first"

`SshSession::pty_write` (`crates/ssh/src/session_terminal.rs:13`, the path of keys, text,
paste and IME commits, and of broadcast writes into this session) sets an `AtomicBool` on the
session's transport before queueing the bytes. Terminal replies (DA, colour and cursor
queries) are written by the router through the transport directly and do not set it.
`ssh_main_task` reads it. Once set, the pending command is dropped with `TypedFirst`.

Why drop, not prepend a line-kill (`Ctrl-U`): the user's partial input would be destroyed
silently, and `Ctrl-U` means other things in vi mode and in non-shell programs. Under Connect
All the case is real: a new SSH tab becomes the active tab, so keys meant for another tab can
land in one whose prompt has not appeared.

Duplicate-at-cwd's `cd` goes through `send_text` and would set the flag; a duplicate carries no
command (Q10), so they never meet.

### 4. Exact bytes and validation

- Sent: the stored text, unchanged (no trimming, no quoting, no escaping), then one `\r`
  (0x0D), in one `channel.data` call. `\r` is what the Enter key sends; the bootstrap ends the
  same way. Not `\n`: a line editor binds `C-j` on its own and some bind it differently.
- Not wrapped in bracketed paste: the point is that it reads and runs as typed.
- `validate_startup_command(text)` in `oneterm-core`, used by the session dialog on Save and by
  the backend before sending (a hand-edited file can hold anything):

  | Input | Result |
  | --- | --- |
  | empty or only whitespace | `Empty` (the dialog stores `None`; the backend sends nothing, silently) |
  | more than 1024 bytes of UTF-8 | `TooLong { bytes }` |
  | any C0 control (U+0000-U+001F, which includes tab, CR, LF and ESC), DEL (U+007F) or C1 (U+0080-U+009F) | `ControlCharacter` |
  | anything else | ok |

  Tab is refused because it triggers completion in a line editor. 1024 bytes stays far below
  the 4095-byte canonical-mode line limit of Linux, which `dash` reads under.
- The backend refusing a stored value sends one notification ("Startup command not sent: it
  is not one line of printable text.") and never a partial or sanitised version.

### 5. What the user sees and what is logged

- Sent: nothing extra. The shell shows the command at its first prompt as if typed, and its
  output follows. No toast.
- Dropped: one `SessionEvent::Notification` from the task (the same channel the agent-forwarding
  warning uses, `session.rs`), shown as a toast by the tab:
  - `NoPrompt`: "Startup command not sent to deploy@web-01:22: the remote shell did not report
    a prompt within 15 s."
  - `TypedFirst`: "Startup command not sent to deploy@web-01:22: you typed before the prompt
    appeared."
  - refused: as in § 4.
- Application log: `info` "startup command sent (N bytes)", `warn` "startup command not sent:
  <reason>". **Never the text**, at any level, including `debug` and `trace` (the task's
  existing "write N bytes" debug line logs counts only). `SshConfig` has no `Debug`; if one is
  ever derived, the field is redacted like the password.
- Terminal log (automatic logging, when on): it records terminal output, so it contains the
  echoed command, as it contains anything typed. Documented, not filtered.
- The remote shell's history keeps it like any typed command; a leading space keeps it out
  under `HISTCONTROL=ignorespace`, which is why the text is not trimmed.

### 6. Sealing (Q8): the alternative, priced

If the owner wants the command sealed under the vault instead of plain text:

- `SecretPurpose::StartupCommand` (label `startup-command`, at most 1024 bytes), bound to the
  session's host, port and username like the password; no envelope format change (the AAD
  scheme already allows a new purpose).
- The session dialog must unlock to show or edit the command, and reseal it when the address
  changes: the `US-0158` `PasswordPlan` rules a second time, for a second field.
- Every connect of a session with a command needs the vault unlocked, including key-only and
  agent sessions that need no vault today; with the vault `Unavailable` the command cannot run;
  a vault reset deletes every command.
- It still appears on screen, in scrollback, in the terminal log, in the remote history and in
  the remote process list. The at-rest protection would cover the one place that matters least.

Recommended: plain text (`DEC-0022` candidate). The dialog's note points secrets to the vault
(a saved password) or to the remote host (an environment file, the agent).

### 7. Interplay

- **Shell integration and CWD-follow:** the command is typed after the bootstrap in the byte
  stream (FIFO on one channel) and after a mark the bootstrap produced, so `PROMPT_COMMAND`,
  `precmd`, OSC 7 and OSC 133 are installed before it runs; a `cd` in the command is followed
  by the SFTP browser like any `cd`.
- **Broadcast (`IN-0022`):** fan-out happens in the origin view's input handlers; the task's
  write is not one of them, so a startup command never reaches another Space. A broadcast
  write *into* a session before its prompt counts as typing there (§ 3).
- **Jump hosts:** only the target runs its command.
- **Quick Connect:** no field, no command (follow-up).
- **Duplicate of a live tab:** no command (Q10); Duplicate Saved Session copies the field.
- **Connect All:** each session's own command, each with its own guard and deadline.
- **SFTP:** there is no SFTP-only connection; the SFTP channel is unaffected.
- **Elevated instance:** has no SSH surface (M1).

## Interfaces

```rust
// oneterm-core
pub const STARTUP_COMMAND_MAX_BYTES: usize = 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupCommandError { Empty, TooLong { bytes: usize }, ControlCharacter }
pub fn validate_startup_command(text: &str) -> Result<(), StartupCommandError>;
pub struct SshConfig { /* ... */ pub startup_command: Option<String> }

// oneterm-terminal (backend/state.rs, osc_router.rs ShellMark arm)
pub struct SessionState { /* ... */ pub prompt_end_count: usize }
impl SharedSessionState { pub fn prompt_end_count(&self) -> usize; }

// oneterm-ssh
pub(crate) struct PendingStartup {
    text: String,
    target: String,                      // "user@host:port", for the notification only
    deadline: tokio::time::Instant,
    first_a_at: Option<tokio::time::Instant>,
}
pub(crate) enum StartupOutcome { Sent, NoPrompt, TypedFirst, Refused(StartupCommandError) }
impl SshTransport { pub(crate) fn mark_user_input(&self); pub(crate) fn user_input_seen(&self) -> bool; }
const STARTUP_DEADLINE: Duration = Duration::from_secs(15);
const STARTUP_GRACE_AFTER_A: Duration = Duration::from_millis(250);
```

## Edge Cases and Failure Modes

- [ ] No mark (csh, tcsh, older fish, a Windows OpenSSH server's PowerShell or cmd, an
  expired password, an rc-file program, auto-attached `tmux`): dropped at 15 s with the toast.
- [ ] A user integration in the rc file emits `A`/`B` before the bootstrap line is read: sent
  early, but it still runs after the bootstrap (same channel, FIFO) and is displayed by the next
  prompt's line editor.
- [ ] Login slower than 15 s (a huge MOTD, a slow network filesystem home): dropped with the
  toast; the constant is the knob.
- [ ] Connection closes before ready: dropped, no toast (the tab already shows the close).
- [ ] The user resizes or scrolls: not input, the flag stays clear.
- [ ] Two `B`s in one chunk, or `A` and `B` together: sent once.
- [ ] Hand-edited value with a newline, an escape or more than 1024 bytes: refused at send
  with the toast; the dialog refuses it on Save.
- [ ] Hop session with a command used as a hop: ignored.
- [ ] Older OneTerm writes the document: the field is dropped (sessions kept), as for
  `sealed_password`.

## Verification

- [ ] Unit (`oneterm-core`): the validation table, including tab, ESC, U+0085, 1024 and 1025
  bytes, multi-byte UTF-8 at the boundary.
- [ ] Unit (`oneterm-terminal`): a `B` mark increments `prompt_end_count`; `A` still increments
  `prompt_count`.
- [ ] Integration (`crates/ssh`, the loopback-server harness of `task_tests.rs`, a server that
  records bytes and emits scripted marks): sent after the bootstrap bytes and only after the
  mark; at `B` immediately; at `A` alone only after 250 ms; once with several marks; dropped
  with exactly one `Notification` at the deadline (paused tokio clock); dropped when
  `mark_user_input` ran first; refused for a control character; a captured logger never sees
  the text.
- [ ] E2E on a real OpenSSH host (WSL or a VM): bash, zsh and dash each show the command once at
  the first prompt and run it; `cd /tmp` in the command moves the SFTP browser when it follows
  the terminal; a `tcsh` account gets the toast at 15 s and nothing typed; an account whose rc
  file runs `read -p "choice? "` gets the toast and the prompt receives nothing.
