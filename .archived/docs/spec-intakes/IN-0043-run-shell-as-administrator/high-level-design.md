# High-Level Design: Run a Windows shell as administrator

Intake: IN-0043
Lane: high_risk
Date: 2026-09-21

> **Status: proposed, pending the owner's choice between options A, B, C and D**
> (`research/windows-elevation-and-conpty.md` section 3, `DEC-0019`). This design
> describes **option A**, the recommendation. If the owner picks B, the whole design
> collapses to one function and one menu row and this file is rewritten to a page. No work
> packet exists yet and none may be created before the decision is accepted and the
> high-risk lane's detail design is written.

## Idea

The owner wants the "+" (New Terminal) menu's Windows shells to be openable as
administrator. Windows does not let a medium-integrity process attach an elevated child to
a pseudo-console it owns: `PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE` is a process-thread
attribute on `CreateProcessW` (`crates/vt/src/pty/windows/conpty.rs:263-300`), and
`CreateProcessW` cannot elevate — elevation goes through `ShellExecuteEx` with the `runas`
verb, which takes no attribute list and no environment block
(`research/windows-elevation-and-conpty.md` sections 2.1-2.2).

So the elevated shell cannot live in this window. It lives in **a second OneTerm window,
running elevated, that is visibly marked as such** — the same shape Windows Terminal
ships. The "+" menu's elevated rows relaunch `oneterm.exe` through `runas` with an
argument naming the shell to open; the new process opens its own window and that shell in
it. The unelevated window keeps its tabs, its connections and its transfers.

Elevation is a property of a **process**, so it is a property of a **window** in OneTerm.
That one sentence is the whole process model, and it is what `DEC-0019` fixes.

## Diagram

```text
  medium integrity (asInvoker)                       high integrity (elevated)
+------------------------------------+            +------------------------------------+
| oneterm.exe  (the user's window)   |            | oneterm.exe  (the admin window)    |
|                                    |            |                                    |
| TerminalPanel::title_suffix        |            |  run()                             |
|   "+" dropdown                     |            |    parse argv -> OpenShell(kind)   |
|    Command Prompt                  |            |    is_elevated() -> true           |
|    PowerShell                      |            |       |                            |
|    PowerShell 7                    |            |       v                            |
|    Run as administrator  >         |            |  window title: "Administrator -    |
|       Command Prompt --------+     |            |   OneTerm", marked title bar,      |
|       PowerShell             |     |            |   marked tab                       |
|       PowerShell 7           |     |            |       |                            |
+------------------------------|-----+            |       v                            |
                               |                  |  PanelSpec::Shell(kind)            |
                               |                  |  -> LocalSession::spawn            |
      WorkspaceCommands.       |                  |  -> CreatePseudoConsole (ITS own)  |
      launch_elevated_shell    |                  |  -> CreateProcessW + PSEUDOCONSOLE |
      (crates/state, fn ptr)   |                  +------------------------------------+
                               |                                   ^
                               v                                   |
              ShellExecuteExW {                                    |
                lpVerb      = "runas"    -----> AppInfo service ---+
                lpFile      = current_exe()     + UAC consent
                lpParameters= --shell=powershell    (secure desktop)
                lpDirectory = <explicit>
                fMask       = SEE_MASK_NOCLOSEPROCESS
              }
                               |
                     +---------+---------+
                     |                   |
              ERROR_CANCELLED      any other error
              (user declined)      -> one message, the
              -> silent no-op         reason named
```

Nothing is shared between the two processes at run time: no pipe, no socket, no shared
memory, no window message. The only thing they have in common is the config directory on
disk, and only when the elevated token belongs to the same account (see
[What is and is not shared](#what-is-and-is-not-shared)).

## UI Wireframe

### The "+" menu

The menu's row order below the shells is owner-fixed (`docs/gui-layout.md:104`,
`IN-0042.md` "settled by owner decision"), so the elevated rows go **with the shells, at
the top**, as one submenu that does not disturb anything below it. Non-Windows builds do
not render it at all.

```text
   before (Windows)                        after (Windows)
+------------------------------+     +------------------------------------+
| Command Prompt               |     | Command Prompt                     |
| PowerShell                   |     | PowerShell                         |
| PowerShell 7                 |     | PowerShell 7                       |
|------ SSH Sessions ----------|     | [shield] Run as administrator    > |  <- new, submenu
| [#] prod-web                 |     |------ SSH Sessions ----------------|
|- - - - - infra - - - - - - - |     | [#] prod-web                       |
| [#] db-01                    |     |- - - - - infra - - - - - - - - - - |
|------------------------------|     | [#] db-01                          |
| Quick Connect...             |     |------------------------------------|
| New Saved Session...         |     | Quick Connect...                   |
+------------------------------+     | New Saved Session...               |
                                     +------------------------------------+
                                                     |
                                     +---------------+
                                     v
                       +--------------------------------+
                       | Command Prompt                 |   <- the same three kinds,
                       | PowerShell                     |      the same display_name
                       | PowerShell 7                   |      wording (US-0114)
                       +--------------------------------+
```

A submenu rather than a modifier click: a Shift-click is invisible, undiscoverable and
untestable through the posted-message GUI walks this project verifies UI with, and it
would make "open a shell" and "open an elevated shell" the same hit target. Three extra
top-level rows were the other candidate and were rejected for doubling the menu's shell
block.

Inside an **already elevated** window the submenu is absent: every shell there is already
administrator, and a `runas` from an elevated process would be a second identical window.

### The elevated window is unmistakable

Three markers, because each is the one a different user notices first:

```text
+-----------------------------------------------------------------------+
| [shield] Administrator - OneTerm            [SSH][SFTP][Agent]  _ o x  |  <- 1. OS title
+-----------------------------------------------------------------------+     2. in-app
| [shield] PowerShell x | Command Prompt x |                  [ + ] [^]  |        title bar
+-----------------------------------------------------------------------+     3. tab badge
|                                                                       |
| PS C:\Windows\system32>                                               |
|                                                                       |
+-----------------------------------------------------------------------+
| ...                                          [shield] Administrator   |  <- status bar
+-----------------------------------------------------------------------+
```

1. **OS window title** — `window.set_window_title(...)` at `crates/app/src/window.rs:66`
   is the constant `"OneTerm"` today. Elevated, it reads `Administrator - OneTerm`, so
   the taskbar, Alt-Tab and every screenshot carry it.
2. **The in-app title bar** — `AppTitleBar::new("OneTerm", window, cx)` at
   `crates/workspace/src/layout/workspace/mod.rs:262`, plus a shield icon.
3. **A status-bar or tab marker** inside the window, for the user who is looking at the
   terminal and not at the chrome.

The marker is **derived from the process token at start-up, never from the launch
argument**. A window that is elevated says so even if it was elevated by some other route
(the user right-clicked the exe, a policy auto-elevated it); a window that is not elevated
cannot be made to claim it is. Colour alone is not enough — themes are user-editable
(`crates/theme/themes/`) and the contrast gate (`scripts/check-theme-contrast.py`) governs
text, not semantics.

## Data Flow

1. The user opens the "+" menu and picks `Run as administrator > PowerShell`. The
   dropdown builder in `TerminalPanel::title_suffix`
   (`crates/terminal-view/src/panel/terminal_panel.rs:681-767`) runs on every open, so no
   state is cached.
2. The row calls a new fn pointer on `WorkspaceCommands`
   (`crates/state/src/commands.rs:38-70`) — `crates/terminal-view` must not perform an
   external effect itself, and the registry is the seam `IN-0033` already established for
   exactly this direction. The argument is a `ShellKind`, a `crates/core` type
   `crates/state` already names (`crates/state/src/commands.rs:15`).
3. The implementation (owner: a Windows-only module; the crate is a detail-design
   question, see below) resolves:
   - `lpFile` = `std::env::current_exe()`. Never a name, never `PATH`.
   - `lpParameters` = the "open this shell" argument for that kind.
   - `lpDirectory` = an explicit directory. This is load-bearing: `config_dir()` is the
     relative path `target/` in debug builds
     (`crates/core/src/config/shell.rs:109-124`), and `runas` does not inherit the
     caller's working directory.
   - `lpVerb` = `runas`, `fMask` includes `SEE_MASK_NOCLOSEPROCESS`.
4. `ShellExecuteExW` hands the request to the AppInfo service, which prompts on the secure
   desktop. Three outcomes, all handled at this step (see [Failure paths](#failure-paths)).
5. The new process starts elevated. `oneterm_app::run()`
   (`crates/app/src/lib.rs:32-125`) — which reads no arguments at all today — parses
   `argv` before the GPUI application starts.
6. The window opens with the elevation markers applied, derived from the **token**, and
   the requested shell is opened as the initial tab through the existing
   `PanelSpec::Shell(kind)` path
   (`crates/terminal-view/src/panel/terminal_panel.rs:195-200`). `DEC-0016`'s startup rule
   that OneTerm opens exactly one initial shell (`IN-0031`) governs: the argument
   *replaces* the default shell, it does not add a tab beside it.
7. `LocalSession::spawn` (`crates/local-shell/src/session.rs:37-120`) runs **inside the
   elevated process**, so `resolve_shell`'s environment injection
   (`crates/core/src/config/shell.rs:338-365`) works exactly as it does today: OSC 7 cwd,
   OSC 133 prompt marks and `TERM` all survive. That is the reason option A relaunches
   OneTerm rather than the shell.

## The seams

| Seam | File | What changes | Why here |
| --- | --- | --- | --- |
| The "+" menu rows | `crates/terminal-view/src/panel/terminal_panel.rs:681-767` | a `#[cfg(windows)]` submenu; `FIXED_ROWS` (line 763) grows by one | the menu is this panel's `title_suffix` |
| The command seam | `crates/state/src/commands.rs` | one fn pointer taking a `ShellKind` | R1/R5: a feature crate must not reach another feature or perform the effect itself |
| The composition root | `crates/app/src/init.rs:62-80` | registers the pointer | the only crate that knows every feature |
| The command line | `crates/app/src/lib.rs:32` | `run()` reads `argv` for the first time | there is no CLI today (`research/...` section 1.4) |
| Elevation detection + markers | `crates/app/src/window.rs:66`, `crates/workspace/src/layout/workspace/mod.rs:262` | both are hard-coded `"OneTerm"` constants | the window's identity is the shell crate's |
| Win32 surface | `Cargo.toml:125-135` | add `Win32_UI_Shell` (and `Win32_Security` is already present for the token query) | the workspace declares every dependency once |

**Open boundary question for the detail design:** which crate owns the `ShellExecuteExW`
call and the `is_elevated()` token query. `crates/core` is gpui-free and already holds the
Windows FFI for file replacement; `crates/app` is the composition root and is where the
process-level concerns (crash handling, the Ctrl handler at
`crates/app/src/lib.rs:78-93`, the allocator) already live. The rules R1-R12 permit
either; the detail design picks one and states why.

## What is and is not shared

| | Shared with the unelevated window |
| --- | --- |
| Terminal tabs, Spaces, scrollback | **No.** Separate process, separate memory. |
| SSH connections, SFTP transfers | **No.** Nothing is migrated or reconnected. |
| Clipboard | Yes — the system clipboard is per user, not per integrity level. |
| Input channels (`DEC-0009`) | **No.** Membership is in-memory and per Space. |
| `ssh_session.json`, `ui_config.json`, `terminal.json`, `docks.json` | **Only when the elevated token belongs to the same account.** |
| `crashes/`, `edit-cache/<pid>/` | Same rule. `edit-cache` is already per-PID. |

The config directory is `<home>/.OneTerm` in release builds, from `USERPROFILE` then
`HOME` (`crates/core/src/config/shell.rs:91-96`, `:109-124`). Two cases:

- **Split-token elevation** (the user is in Administrators and consents): same SID, same
  `USERPROFILE`, same config directory. The elevated window has the user's saved sessions,
  theme and layout — and both processes now persist layout and settings on exit. Two
  concurrent writers are already possible today with two ordinary instances
  (`crates/core/src/persistence.rs` does the atomic replace), so this is a pre-existing
  property the feature makes routine rather than a new one.
- **Over-the-shoulder elevation** (a standard user types an administrator's credentials):
  the process runs as that other account, so `USERPROFILE` and therefore `~/.OneTerm`
  differ. The elevated window has **no saved sessions, no theme, no dock layout and its
  own crash store** — it looks like a first run. Whether that is explained in place or
  merely documented is an open question for the owner, recorded in `IN-0043.md`.

The intake deliberately does **not** propose sharing state across the boundary. Any
channel from the medium-integrity process into the elevated one — a pipe, a socket, a
shared file the elevated side acts on — is the option D attack surface
(`research/windows-elevation-and-conpty.md` section 2.2) reintroduced by the back door.
**One rule: nothing the unelevated process writes may direct what the elevated process
executes.** Reading shared *settings* (a theme, a font size) is not the same as taking
instructions, but the line has to be drawn and defended in the detail design, because
`terminal.json` holds `LocalShellConfig::program` and `args`
(`crates/core/src/config/shell.rs:68-81`) — a config file the unelevated instance can
write that names an executable the elevated instance would run.

## Failure paths

| Path | Behaviour |
| --- | --- |
| User declines the UAC prompt | `ShellExecuteExW` fails with `ERROR_CANCELLED` (1223). **Silent no-op.** The user said no; telling them so is noise. |
| No administrator available, or policy denies elevation | One message naming the reason. The menu row is not hidden in advance: the policy state is not reliably knowable before asking, and a missing row is worse than a clear refusal. |
| `ShellExecuteExW` fails for any other reason | One message with the OS error, and a log line. The existing window is untouched. |
| `current_exe()` fails | The row cannot act; message and log. Nothing is guessed from `PATH` or from `argv[0]` — resolving the executable by name is how the wrong binary gets elevated. |
| The elevated process starts but its window fails to open | It is a normal OneTerm start-up failure (`crates/app/src/window.rs`, CORR-63): logged, and with no window the process has nothing to show. The launching window has already returned and shows nothing; the user sees a UAC prompt followed by nothing. Acceptable, and the same as any failed launch. |
| The elevated process cannot read the config (different profile) | It starts with defaults, as a first run does. The elevation marker is still correct, because it comes from the token. |
| An unknown or malformed argument | The process starts normally with the default shell and logs the argument it did not understand. It never fails closed into a window-less elevated process. |
| A future single-instance guard routes the argument to an existing instance | **Forbidden.** An elevation request is always served by the process that received it. Recorded in `DEC-0019` so this cannot be lost. |

## Detail Design

- [ ] Detail design: **required (high-risk)** — not yet written.
- Reason: the intake launches an elevated process. That is an external effect and a
  security boundary, so `docs/HARNESS.md` blocks `story create` until at least one file
  exists under `low-level-design/`. It is deliberately not written yet: the owner chooses
  between options A, B, C and D first (`DEC-0019`), and the detail design of the option
  that is not chosen is waste. Expected concerns once A is accepted:
  - `low-level-design/elevated-launch.md` — the `SHELLEXECUTEINFOW` fields verbatim, the
    argument vocabulary and its parser, the `is_elevated()` token query, the error map,
    and the crate that owns them.
  - `low-level-design/elevation-trust-boundary.md` — what the elevated process is allowed
    to read from disk and what it must re-derive, with `LocalShellConfig::program` as the
    worked example.
