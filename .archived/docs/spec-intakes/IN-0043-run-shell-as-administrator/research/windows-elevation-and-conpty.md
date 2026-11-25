# Research: Windows elevation and ConPTY

Intake: IN-0043
Date: 2026-09-21
Status: research input for the high-level design and `DEC-0019`. Not a contract.

Every claim below is tagged:

- **[code]** — read out of this repository at `main @a211335b`, with the file and line.
- **[api]** — a property of the documented Win32 surface that this repository's own
  spawn path already demonstrates; reasoned from the signatures we call.
- **[doc]** — vendor-documented behaviour, recalled and not re-verified against a live
  page in this session. Treat it as strong but citable-to-a-person, not to a URL fetch.
- **[unverified]** — not established here. Named so it is not mistaken for a finding.

---

## 1. How OneTerm spawns a shell today

### 1.1 The request: which program, which arguments, which environment

`ShellKind` (`crates/core/src/config/shell.rs:17-32`) is the closed set of local shells
the UI offers: `Cmd`, `PowerShell`, `Pwsh`, `Bash`, `Zsh`, `Sh`, `Custom`. Its
`display_name` (`crates/core/src/config/shell.rs:40-50`) is the single wording source for
the "+" menu row, the Settings dropdown and the tab label (`US-0114`).

`resolve_shell` (`crates/core/src/config/shell.rs:235-371`) turns a `LocalShellConfig`
into `ResolvedShell { program, args, env }`:

- **program** — `COMSPEC` for `Cmd` (`shell.rs:164-168`), a `PATH` scan for
  `powershell` / `pwsh` (`shell.rs:268-299`), `resolve_unix_shell` elsewhere.
- **args** — `cmd /K chcp 65001 >nul` for UTF-8 (`shell.rs:248-254`);
  `-NoLogo -NoExit -Command <prompt wrapper>` for both PowerShell kinds
  (`shell.rs:277-282`, `shell.rs:292-297`).
- **env** — the whole shell-integration mechanism. `PROMPT` carries the OSC 7 emitter for
  `cmd` (`shell.rs:343-345`), `PROMPT_COMMAND` for bash (`shell.rs:349-355`), `PS1` for
  zsh (`shell.rs:360-362`), on top of `base_env()`'s `TERM=xterm-256color`. The file says
  it in as many words: *"Shell integration is injected via env vars in resolve_shell() --
  fully silent, no temp file, no script written to the PTY"*
  (`crates/local-shell/src/session.rs:105-107`).

**This is the hinge for the whole intake.** OneTerm's shell integration is an
*environment block handed to `CreateProcessW`*. Any spawn path that cannot carry a custom
environment block loses OSC 7 (the cwd the SFTP panel follows), OSC 133 prompt marks, and
`TERM`.

### 1.2 The spawn: `CreateProcessW` with a pseudo-console attribute

`LocalSession::spawn` (`crates/local-shell/src/session.rs:37-120`) builds
`oneterm_vt::pty::Options` (`crates/vt/src/pty/mod.rs:141-164`) from the resolved shell and
hands it to the owner thread. On Windows the transport does exactly this
(`crates/vt/src/pty/windows/conpty.rs`):

| Step | Line | What it does |
| --- | --- | --- |
| `CreatePseudoConsole` | `conpty.rs:231-247` | creates the `HPCON` **in this process**, from two anonymous pipes |
| `STARTUPINFOEXW` | `conpty.rs:257-258` | extended startup info, `STARTF_USESTDHANDLES` with null handles so the child inherits nothing (`conpty.rs:261`) |
| `ProcThreadAttributeList::with_capacity(1)` | `conpty.rs:263` | `InitializeProcThreadAttributeList` |
| `set_pseudoconsole(handle)` | `conpty.rs:264`, `conpty.rs:384-404` | `UpdateProcThreadAttribute(..., PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, handle, ...)` |
| `CreateProcessW` | `conpty.rs:284-300` | `EXTENDED_STARTUPINFO_PRESENT` plus, when a custom env exists, `CREATE_UNICODE_ENVIRONMENT` (`conpty.rs:270-278`) |

So: **the pseudo-console reaches the child through one channel only — a process-thread
attribute on the `STARTUPINFOEXW` passed to `CreateProcessW`** [code]. The child's exit is
watched through its process handle (`conpty.rs:313-315`,
`crates/vt/src/pty/windows/child.rs:113-125`), and `DEC-0016` terminates it through that
same handle if it outlives its console.

`docs/terminal-backend.md` section 6.1 ("Configurable shell", line 390) and section 6.2
("Spawn via `oneterm_vt::pty`", line 453) are the owning prose for both halves.

### 1.3 The "+" menu

`TerminalPanel::title_suffix` (`crates/terminal-view/src/panel/terminal_panel.rs:681-767`)
builds the dropdown on every open. The Windows arm is a three-element constant,
`crates/terminal-view/src/panel/terminal_panel.rs:697-699`:

```rust
#[cfg(windows)]
const SHELLS: [ShellKind; 3] = [ShellKind::Cmd, ShellKind::PowerShell, ShellKind::Pwsh];
```

Each row dispatches `AddPanelWithShell(kind)`
(`crates/terminal-view/src/panel/terminal_panel.rs:703`), which lands in
`PanelSpec::Shell(kind)` (`crates/terminal-view/src/panel/terminal_panel.rs:195-200`) and
spawns a local view. The menu's row order below the shells is owner-fixed
(`docs/gui-layout.md:104`, and the "settled by owner decision" list in `IN-0042.md`). The
row-count estimate that decides whether the popup scrolls counts `FIXED_ROWS = 7`
(`terminal_panel.rs:763`) — adding rows means updating that constant.

Cross-feature calls leave `crates/terminal-view` through
`oneterm_state::commands::WorkspaceCommands` (`crates/state/src/commands.rs:38-70`), the
fn-pointer registry installed at the composition root
(`crates/app/src/init.rs:62-80`). `IN-0033` established that seam and the reason for it
(R1/R5: `oneterm-session-ui` already depends on `oneterm-terminal-view`, so the reverse
edge is a cycle).

### 1.4 How OneTerm itself is launched

- The binary is a four-line shim: `crates/app/src/bin/oneterm.rs:12-14` calls
  `oneterm_app::run()`.
- **`run()` never reads `std::env::args`.** `crates/app/src/lib.rs:32-125` goes straight
  from the allocator ballast to logging to `app.run(...)`. A repository-wide search for
  `std::env::args` finds it only in `crates/tools/src/bin/*` (the diagnostic binaries) —
  `pty-throughput.rs:27`, `vt-esctest.rs:83`, `vt-corpus.rs:50`, `vt-bench.rs:76`,
  `sftp-dev-server.rs:488`. **OneTerm has no command line at all** [code].
- **There is no single-instance guard.** No `CreateMutex`, no named pipe, no
  `single_instance` crate anywhere in `crates/` [code]. Two `oneterm.exe` processes today
  are two independent applications that happen to share a config directory.
- The window opens once at `crates/app/src/window.rs:50-53`, and its OS title is set
  once, literally: `window.set_window_title("OneTerm")`
  (`crates/app/src/window.rs:66`). The in-app title bar text is the constant handed to
  `AppTitleBar::new("OneTerm", window, cx)`
  (`crates/workspace/src/layout/workspace/mod.rs:262`).
- The executable carries no hand-written application manifest. `crates/app/build.rs`
  compiles `crates/app/assets/oneterm.rc` (icon + `VS_VERSION_INFO` only — the file has no
  `RT_MANIFEST` entry) through `embed_resource::compile(...).manifest_required()`, so the
  manifest in the shipped exe is the tool's default. Consequence: the requested execution
  level is the default `asInvoker`, and nothing in this repository asks for elevation
  today [code].
- `config_dir()` is `target/` in debug and `<home>/.OneTerm` in release
  (`crates/core/src/config/shell.rs:109-124`), where `<home>` is `USERPROFILE` then `HOME`
  (`crates/core/src/config/shell.rs:91-96`). **The debug path is relative to the process's
  working directory.**

---

## 2. The Windows constraint

### 2.1 A non-elevated process cannot `CreateProcess` an elevated child

`CreateProcessW` creates the child with a copy of the calling process's primary token, or
with a token the caller supplies (`CreateProcessAsUserW` / `CreateProcessWithTokenW`, both
of which require privileges a medium-integrity interactive process does not hold). There
is no creation flag that means "elevate". A process cannot raise its own token either:
elevation is not a state a running process enters [api][doc].

The supported path from an unelevated process is `ShellExecuteEx` with
`lpVerb = "runas"`. The shell hands the request to the **AppInfo** service, which shows
the consent or credential prompt on the secure desktop and then creates the process
itself with the elevated token [doc]. (The other documented path, the COM elevation
moniker `Elevation:Administrator!new:{CLSID}`, elevates a registered COM server, not an
arbitrary executable, and needs a registered, ideally signed server — it does not apply to
"start this shell".)

`SHELLEXECUTEINFOW` carries `hwnd`, `lpVerb`, `lpFile`, `lpParameters`, `lpDirectory`,
`nShow`, `fMask`, and — with `SEE_MASK_NOCLOSEPROCESS` — returns `hProcess`. It has
**no `lpAttributeList`, no `STARTUPINFOEX`, no environment block, and no handle
inheritance** [api][doc]. Two consequences follow directly from section 1.2:

1. `PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE` cannot be delivered. It exists only as a
   process-thread attribute on `CreateProcessW`, and `ShellExecuteEx` does not take one.
   **An elevated child cannot be attached to a pseudo-console this process created.**
2. A custom environment block cannot be delivered. The elevated process is built from the
   elevated token's profile environment, so `PROMPT`, `PROMPT_COMMAND`, `PS1` and `TERM`
   from `resolve_shell` (section 1.1) do not reach it.

Failure modes worth naming now, because the design must handle each: the user declines
the prompt (`ERROR_CANCELLED`, 1223); the machine has no path to an administrator
(standard user, no admin credentials); Group Policy disables the prompt or forces
`Automatically deny elevation requests`; the verb is unavailable for the target file
type.

### 2.2 UIPI, integrity levels, and whether a medium-IL host could carry a high-IL shell

State it precisely, because the popular answer ("UIPI forbids it") is the wrong reason.

- **UIPI** blocks window messages, hooks and drag-and-drop from a lower-integrity process
  to a higher-integrity window [doc]. The ConPTY data path has no window: the console host
  is windowless and the traffic is anonymous pipes. **UIPI is not what stops option D.**
- What actually stops the straightforward version is section 2.1: the attribute rides on
  `CreateProcessW`, and `CreateProcessW` cannot elevate. Full stop [api].
- **Could a high-IL helper create the ConPTY instead, and feed a medium-IL OneTerm?**
  Mandatory Integrity Control is a *no-write-up / no-read-up-for-process-objects* policy,
  but the higher side may always weaken its own objects: a high-IL helper can create a
  named pipe whose security descriptor and mandatory label admit medium integrity, and
  then relay conin/conout bytes and resize requests. So the mechanism is **not
  impossible** — the pipes exist, the helper owns the `HPCON`, and OneTerm keeps only
  bytes. That is the honest answer, and it is why option D is evaluated rather than
  dismissed.
- **What is not established here [unverified]:** whether `CreatePseudoConsole` inside a
  high-IL helper behaves identically when its console host is high-IL and its client is
  high-IL while the *reader* is medium-IL. Nothing in the API suggests it would not (the
  reader holds an ordinary pipe handle the helper duplicated to it), but this has not been
  tested on this machine and must not be recorded as a finding.
- **What is certain about option D is its security shape, not its plumbing.** A permanent
  high-IL helper that accepts "run this command line elevated" from a medium-IL client
  turns every medium-IL code path in OneTerm into an elevation path. Any process running
  as the same user — which needs no elevation at all — can inject into, debug, or simply
  drive that medium-IL OneTerm and reach the helper. A single UAC prompt at helper
  start-up would buy a standing elevation service for the rest of the session. That is a
  security boundary this project would be creating, owning and maintaining.

### 2.3 What Windows Terminal does

Windows Terminal exposes a per-profile boolean `elevate`. Documented behaviour: when
`true`, the profile always opens in an elevated context; if the current window is not
elevated, **a new elevated window is created and the profile opens there** [doc]. An
already-elevated window simply opens the profile as a tab.

Windows Terminal does **not** mix elevated and unelevated tabs or panes in one window.
Mixed-elevation designs (a de-elevated or re-elevated pane inside one window) were
explored publicly by that team and not shipped; the shipped model is one window, one
elevation level [doc]. An elevated Windows Terminal window is visually distinguished —
current builds mark the elevated window and tabs rather than leaving them identical to an
unelevated window [doc], though the exact present-day marker (shield glyph, title prefix,
theme) is **[unverified]** here and should not be copied blind.

The point for OneTerm is the *shape*, and it is the shape a reading of section 2.1
predicts: the most-used Windows terminal, with far more engineering behind it, landed on
"a separate elevated window, launched through the elevation service" because the platform
leaves no cheaper correct option.

### 2.4 What an elevated OneTerm process implies

Elevation is per process, so "elevate OneTerm to get an elevated shell" elevates
everything OneTerm does. Concretely, in this repository:

| Surface | Under an elevated process |
| --- | --- |
| Every terminal tab | elevated, including ones the user opened for ordinary work |
| SSH / SFTP | `russh` network code, key files and `known_hosts` handling run with an admin token. Nothing needs it. |
| Config writes | `terminal.json`, `ui_config.json`, `docks.json`, `ssh_session.json` written by an elevated process (`crates/core/src/persistence.rs` owns the atomic replace) |
| Explorer drag-and-drop | **broken.** `crates/sftp-ui/src/render.rs:436-451` accepts `ExternalPaths` drops to upload files. Explorer runs at medium integrity and UIPI forbids a drop onto a high-integrity window [doc][code]. This is a real, user-visible regression inside an elevated window. |
| Auto-update | `docs/auto-update.md` (Windows section, lines ~313-324) spawns a helper that replaces the whole distribution directory and relaunches, keeping a rollback copy under `<config>/updates/backup-<pid>-<ts>`. Under elevation that helper is elevated: it can now write `C:\Program Files`, and it can leave files whose owner or ACL the ordinary instance cannot replace. An update triggered from an elevated window therefore changes the install for the non-elevated one too. |
| Crash reporting | `docs/crash-reporting.md`: reports land in `<config>/crashes/`, and start-up promotion skips staging owned by another **live** PID. A normal and an elevated instance are two PIDs sharing (or not sharing, see below) one store, and each prunes to the newest 20. |

**The config-directory split is the sharp edge.** `config_dir()` derives from `USERPROFILE`
(`crates/core/src/config/shell.rs:91-96`, `:109-124`):

- *Split-token elevation* — the user is themselves in Administrators and consents. Same
  SID, same `USERPROFILE`, **same `~/.OneTerm`**. Sessions, themes and layout are all
  there; two instances now write the same files.
- *Over-the-shoulder elevation* — the user is a standard user and types an administrator's
  credentials. The process runs as **that** account: different `USERPROFILE`, therefore a
  different `~/.OneTerm`, therefore **no saved SSH sessions, no theme, no dock layout, and
  a separate crash store**. The elevated window looks like a fresh install.
- *Debug builds* — `config_dir()` is the relative path `target/`, resolved against the
  process working directory. A relaunch that does not set `lpDirectory` lands somewhere
  else entirely (the `runas` verb does not inherit the caller's cwd) and silently uses a
  different config root.

---

## 3. Options

### A. "Run as administrator" rows open a new elevated OneTerm window

The "+" menu gains, per Windows shell, a way to say "elevated" (a submenu, a modifier
click, or three extra rows). Choosing it calls `ShellExecuteExW` with `runas` on
**OneTerm's own executable**, passing an argument that means "open this shell kind". The
new process is elevated, opens its own window, opens that shell, and is marked as
elevated everywhere the user can see it.

- **What the user sees:** a UAC prompt, then a second OneTerm window that is unmistakably
  the administrator one, with the requested shell already open in it. The original window
  is untouched; its tabs, connections and transfers keep running unelevated.
- **Cost:** three things this repository does not have. (1) A command line — `run()`
  ignores `argv` entirely (section 1.4), so an argument vocabulary, its parsing and its
  failure behaviour are all new. (2) A single-instance rule — with no guard today, the
  argument must mean "this *new* process opens that shell", which is simple, but the
  decision has to be recorded so a later single-instance feature does not silently route
  an elevated request into the unelevated instance. That would be a privilege-escalation
  bug, not a UX bug. (3) A visible elevation marker in the window title, the in-app title
  bar and the tab, all of which are hard-coded constants today
  (`crates/app/src/window.rs:66`, `crates/workspace/src/layout/workspace/mod.rs:262`).
  Plus `Win32_UI_Shell` added to the workspace `windows-sys` feature list
  (`Cargo.toml:125-135` does not include it today) and documentation updates to
  `docs/gui-layout.md` and `docs/terminal-backend.md`.
- **Risk:** everything in section 2.4 applies to the elevated window — that is inherent,
  not a defect, but it must be documented and visible. The specific risks are the UAC
  prompt itself, running `russh` under an admin token if the user opens SSH there, the
  over-the-shoulder config split, and the `runas` working directory and environment.
- **What it does not do:** it does not put an elevated shell in the existing window. The
  owner asked for "the Windows shells can run as admin", and this answers it with a second
  window. That gap is the substance of the decision.

### B. The row runs the shell in an external console window

`ShellExecuteExW("runas", <the resolved shell program>, <args>, ...)` with no
pseudo-console at all. Windows gives the elevated shell its own `conhost` window.

- **What the user sees:** a UAC prompt, then a normal Windows console window — not a
  OneTerm tab, not OneTerm's theme, font, keybindings, search, scrollback, Sixel or agent
  panel.
- **Cost:** one function and one menu row. No command line, no single-instance question,
  no marker, no process model. Genuinely a day's work including the docs.
- **Risk:** low, and confined. Nothing in OneTerm becomes elevated. The honest caveats
  are that the shell-integration environment is lost (section 2.1: OSC 7 cwd, OSC 133
  marks, `TERM`), so it is a plain console, and that a row in OneTerm's menu that opens a
  window OneTerm does not own will be read by some users as a bug.
- Worth keeping as the fallback precisely because it is the only option whose risk
  section is short.

### C. Relaunch the current OneTerm elevated ("Restart as administrator")

One menu row; `ShellExecuteExW("runas", <own exe>)`, then quit.

- **What the user sees:** a UAC prompt, then every tab, every SSH connection and every
  in-flight SFTP transfer gone, replaced by a fresh elevated window.
- **Cost:** smaller than A (no per-shell argument needed, though the same marker and the
  same config-split handling are), but it destroys session state, and OneTerm has no
  session restore for live connections — a terminal's scrollback and a live SSH channel
  cannot be handed to another process.
- **Risk:** the highest user-visible cost of the four, for the least benefit. It also
  makes the elevated state sticky and easy to forget, which is the state section 2.4 says
  is dangerous.
- A "Restart as administrator" row could still be a reasonable *addition* later; it is a
  poor answer to "I want an admin shell".

### D. Host an elevated shell inside the non-elevated window

A high-integrity helper process, started once through `runas`, creates the pseudo-console
and the shell and relays conin/conout plus resize over a named pipe whose label admits
medium integrity. OneTerm keeps its tab, its renderer and its theme; only the bytes cross.

- **Feasible?** Mechanically, probably yes (section 2.2), with one untested assumption
  named there. Nothing in the API says the pipes cannot be shared downward.
- **Secure?** No, and not fixably so within this project's budget. It converts every
  medium-integrity weakness in a large GUI application — including a malicious process
  running as the same user, which needs no privilege at all — into arbitrary elevated
  execution. The helper would need its own authenticated protocol, its own argument
  validation, its own lifetime management, code signing to be trustworthy at all, and a
  security review per release. This is the reason Microsoft states that UAC is not a
  security boundary, turned into a feature.
- **Worth it?** No. It is the largest option by an order of magnitude, it is the only one
  that creates a new attack surface OneTerm must then own forever, and the outcome it buys
  over option A is "the elevated shell is a tab in this window instead of a tab in a
  second window".
- **Why Windows Terminal did not do it:** the shipped answer there is a separate elevated
  window (section 2.3), by a team that investigated the alternatives in public. Adopting
  the same shape is the cheap way to inherit that analysis.

### Recommendation

**A, with B as the cheap fallback**, and `DEC-0019` records the choice.

A is the only option that answers the request (an admin shell, inside OneTerm, usable
like every other tab) without creating a new security boundary, and it matches the shape
the platform and the reference implementation both push towards. B is kept named in the
records so that, if the owner decides A's cost is not worth it, the fallback is a
decision and not a rewrite.

### High-risk points to carry into the design

1. **The UAC prompt.** Not decorative: declining is the normal case and must land
   somewhere the user sees, not in a log line. `ERROR_CANCELLED` is a quiet no-op, every
   other failure is a message.
2. **Network code under an admin token.** The elevated window can open SSH and SFTP.
   Nothing prevents it and nothing should silently encourage it; the marker is what makes
   the user aware, and the docs must state it plainly.
3. **The config directory under a different profile.** Over-the-shoulder elevation gives
   the elevated instance a different `~/.OneTerm`: no sessions, no theme, a separate crash
   store, and a `docks.json` that is not the user's. The design must decide whether that
   is an accepted, explained outcome or a case to detect and warn about.
4. **The `runas` working directory and environment.** `lpDirectory` must be set
   explicitly (debug builds resolve `config_dir()` relative to it), and the elevated
   process gets the elevated token's environment, not OneTerm's — so anything the child
   shell needs must be re-derived inside the new process, which is precisely why option A
   relaunches *OneTerm* and lets `resolve_shell` run there, rather than relaunching the
   shell (option B).
5. **Single instance, if it ever arrives.** An "open this shell" argument routed to an
   existing unelevated instance would answer an elevation request without elevating. The
   rule that an elevation request is always served by the process that received it has to
   be written down before, not after, someone adds instance coalescing.
6. **Concurrent writers.** Two OneTerm processes sharing one config directory (the
   split-token case) both persist layout and settings on exit. That is already possible
   today with two normal instances, but this feature makes it the expected workflow.
