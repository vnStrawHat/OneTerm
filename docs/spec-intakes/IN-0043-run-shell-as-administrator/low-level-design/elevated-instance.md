# Low-Level Design: The elevated OneTerm instance

Intake: IN-0043
HLD: `docs/spec-intakes/IN-0043-run-shell-as-administrator/high-level-design.md`
Topic: elevated-instance process model, command line, trusted shell paths, mode gating
Date: 2026-09-21

> Written against `DEC-0019` as **accepted on 2026-09-21** (option A with mitigations
> M1-M7). Every rule below traces to a clause of that decision; where this file makes a
> choice the decision left open, it says so and gives the reason.

## Concern

One concern, because the three parts cannot be designed apart: **the second process**.
How it is started (the `runas` call and its arguments), how it decides what it is (the
command line and the process token), and what it is then allowed to do (the mode gates,
the config matrix, the crash store).

The "+" menu rows that *invoke* this are `US-0132` and are deliberately not detailed
here beyond the one fn pointer they call: the menu is presentation, this file is the
security boundary.

## Design

### 1. The two processes

```text
  medium integrity                                   high integrity
+----------------------------------+              +----------------------------------+
| oneterm.exe (the user's window)  |              | oneterm.exe --elevated-shell pwsh|
|                                  |              |                                  |
| "+" menu row                     |              | run():                           |
|   -> WorkspaceCommands           |              |  1 set_elevation(token query)    |
|      .launch_elevated_shell(kind)|              |  2 parse argv -> ElevatedShell   |
|         (crates/state)           |              |  3 set_initial_shell(kind)       |
|   -> crate::elevation (app)      |              |  4 crash paths (crashes/elevated)|
|                                  |              |  5 gpui app + window             |
| ShellExecuteExW {                |              |  6 TerminalPanel opens the shell |
|   lpVerb  = "runas"              |   AppInfo    |     -> resolve_shell runs HERE   |
|   lpFile  = current_exe()        | ----------->  |     -> CreatePseudoConsole HERE  |
|   lpParams= "--elevated-shell .."|   + UAC on   |                                  |
|   lpDir   = current_dir()        |  secure      | title: "OneTerm (Administrator)" |
|   fMask   = SEE_MASK_NOASYNC     |  desktop     | (title text is the whole marker) |
| }                                |              | right dock: absent               |
+----------------------------------+              +----------------------------------+
```

Nothing connects the two at run time. No pipe, no socket, no shared memory, no window
message. They share the system clipboard and — only when the elevated token belongs to
the same account — the configuration directory, which the elevated side **reads and never
writes** (section 6).

The trust boundary rule that governs every choice below, from `DEC-0019` rule 4:

> **Nothing the unelevated process writes may direct what the elevated process executes.**

The unelevated process contributes exactly three things to the elevated one: the verb, the
executable (its own `current_exe()`), and one token from a closed three-value enum. That
is the whole channel.

### 2. Where the code lives

The HLD left the owning crate open. Split by what each half needs:

| Piece | Crate | Why |
| --- | --- | --- |
| `ElevatedShell` enum, argument parser, trusted-path resolution, the `Elevation` enum, the `is_restricted()` / `initial_shell()` process globals | `crates/core`, new `src/config/elevation.rs` | pure data and `std` only; `core` is the one crate `settings`, `workspace`, `terminal-view`, `state` and `app` can all name (`crates/state/src/panel_names.rs:9-12`), and it already owns `ShellKind` and `config_dir()` (`crates/core/src/config/shell.rs:17-32`, `:109-124`) |
| The token query, the `ShellExecuteExW` call, the fatal `MessageBoxW` | `crates/app`, new `src/elevation.rs`, **the FFI calls alone `#[cfg(windows)]`** — every decision taken around them (the console-ownership rule, the launch-outcome mapping, the notification, the in-flight debounce) is plain `std` beside them, so it compiles and is unit-tested on all three runners rather than becoming dead code off Windows (`US-0130`, rework 2026-09-22) | `crates/app` already owns every process-level concern — the allocator (`crates/app/src/lib.rs:28-29`), the crash handlers (`:34-72`), the console Ctrl handler (`:78-93`) — and is the composition root that installs `WorkspaceCommands` (`crates/app/src/init.rs:62-80`). It is also the only crate that already depends on `windows-sys` outside the VT engine (`crates/app/Cargo.toml:67`). |

This keeps `crates/core` free of a `windows-sys` dependency it does not have today, and
keeps the external effect in the crate whose job is external effects. `crates/app` sets
the process global that everyone else reads:

```rust
// crates/app/src/lib.rs, first statement of run()
oneterm_core::elevation::set_elevation(crate::elevation::process_elevation());
```

`crates/terminal-view` never performs the effect; it calls a new `WorkspaceCommands` fn
pointer (`crates/state/src/commands.rs:38-70`), the seam `IN-0033` established, because
`oneterm-session-ui` already depends on `oneterm-terminal-view` and the reverse edge would
be a cycle (R1/R5).

### 3. Start-up sequence

`run()` (`crates/app/src/lib.rs:32-125`) reads `argv` for the first time in this project's
history. Two statements move to the front of the function, **before**
`oom::init_ballast()` (`:33`) and `crash_report::prepare_capture_paths()` (`:34`), because
`crashes_dir()` (`crates/app/src/crash_report.rs:81-83`) must already know whether this
process is elevated (M7):

| Step | Call | Notes |
| --- | --- | --- |
| 1 | `oneterm_core::elevation::set_elevation(process_elevation())` | the token query, section 5. The only source of the marker and of every gate. Three-valued: a failed query is `Unknown`, which is **restricted** and claims no elevation. |
| 2 | `oneterm_core::elevation::parse(std::env::args_os())` | section 4. `Err` -> `fatal_message(...)` + `std::process::exit(2)`. `Ok(Some(shell))` -> when elevated, `trusted_program_for(shell)` or exit 3; then `set_initial_shell(shell)`. Steps 1 and 2 are one function, `read_process_identity()`, the first statement of `run()`. |
| 3 | `oom::init_ballast()` | unchanged (`lib.rs:33`) |
| 4 | `crash_report::prepare_capture_paths()` | unchanged call site (`lib.rs:34`); `crashes_dir()` now resolves to `<config>/crashes/elevated` when step 1 said elevated |
| 5 | logging, native crash handler, Ctrl handler | unchanged (`lib.rs:47-93`) |
| 6 | `crash_report::load_pending_reports()` | unchanged call (`lib.rs:56`). It still runs when elevated, so promotion and the 20-report retention still happen; the **result is discarded** instead of being handed to the dialog (M7, section 7). |
| 7 | `app.run(...)` -> `init::init(cx)` -> `window::open_window(...)` | unchanged shape; the gates of section 7 apply inside |

Step 2 runs before the ballast on purpose: a malformed command line must produce a message
and an exit, not a 64 MiB allocation first. The parse allocates a handful of small strings
and cannot itself be the thing that exhausts memory.

The shell requested on the command line reaches the first terminal tab through the
`initial_shell()` global rather than through a new parameter threaded
`open_window` -> `OneTermWorkspace::new` -> `build_named_panel` -> `PanelRegistry` -> `PanelSpec`:
the registry builds panels **by name** (R4, `crates/state/src/panel_names.rs:1-12`) and has
no place to carry a payload. `TerminalPanel`'s `PanelSpec::DefaultShell` arm
(`crates/terminal-view/src/panel/terminal_panel.rs:182-194`) reads the global instead of
passing `None`:

```rust
PanelSpec::DefaultShell { workspace } => {
    // `initial_shell()` is set once in run(), before any UI exists (IN-0043).
    let requested = oneterm_core::elevation::initial_shell();
    ...
}
```

This preserves `DEC-0016`'s rule that OneTerm opens **exactly one** initial shell
(`IN-0031`): the argument *replaces* the default shell, it never adds a tab beside it.

> ponytail: the global is read on every `PanelSpec::DefaultShell` build, not only the
> first, so a later "reset layout" in an elevated window re-opens the requested kind
> rather than the configured default. In an elevated window that is the desired answer
> (the configured default may be `Custom`, which M3 forbids); in an unelevated window the
> global is `None` and nothing changes. Ceiling: if a "reset to the configured default
> shell" action is ever wanted in an elevated window, the global has to become
> take-once.

### 4. The command-line contract

The whole grammar, in five lines:

```text
oneterm.exe                                  -> normal start, default shell (UNCHANGED)
oneterm.exe --elevated-shell cmd             -> start with Command Prompt as the one shell
oneterm.exe --elevated-shell powershell      -> start with Windows PowerShell 5.1
oneterm.exe --elevated-shell pwsh            -> start with PowerShell 7+
anything else                                -> one message box, exit code 2, no window
```

"Anything else" is every other input, named so no case is left to interpretation: an
unknown flag; `--elevated-shell` with no value; `--elevated-shell` with a value outside the
three tokens (including `bash`, `zsh`, `sh` and `custom`, which M3 forbids); the flag given
twice; a bare positional argument; anything after a valid pair. **The parser has no
tolerant branch.** The HLD's earlier "start normally and log" was superseded by the owner's
2026-09-21 ruling: an unrecognized argument means the caller is not the caller we think it
is, and starting an *elevated* window anyway is the one outcome that must not happen.

The parser is hand-written — three tokens and a flag do not justify a CLI dependency, and
`docs/agents/dependencies.md` asks for the smallest surface that works:

```rust
// crates/core/src/config/elevation.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElevatedShell { Cmd, PowerShell, Pwsh }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    /// The whole argument list, joined for the message. Never interpreted.
    Unrecognized(String),
}

pub const ELEVATED_SHELL_FLAG: &str = "--elevated-shell";

/// Parse the process arguments. `args` includes argv[0], which is skipped.
pub fn parse<I, S>(args: I) -> Result<Option<ElevatedShell>, CliError>
where I: IntoIterator<Item = S>, S: AsRef<std::ffi::OsStr>;
```

`ElevatedShell` is a separate closed enum rather than `ShellKind` on purpose: `ShellKind`
has seven variants including `Custom`, and `Custom` is exactly the one M3 forbids. A type
that cannot represent `Custom` cannot be talked into launching it. The mapping to
`ShellKind` is one `match` (`Cmd -> ShellKind::Cmd`, `PowerShell -> ShellKind::PowerShell`,
`Pwsh -> ShellKind::Pwsh`) and the reverse (`ShellKind -> Option<ElevatedShell>`) is what
the "+" menu uses to decide which rows may exist at all.

Exit codes, for the manual E2E and for anyone scripting the binary:

| Code | Meaning |
| --- | --- |
| 0 | normal exit |
| 2 | the command line was not understood; one message box, no window |
| 3 | the command line was understood but the requested shell has no trusted path on this machine (section 5); one message box, no window. **Elevated processes only** — a process that is not elevated is unrestricted, resolves the shell the ordinary way and opens a window, because the token decides everything else and no consent prompt was paid for (`US-0130`, as built) |

The message box is `MessageBoxW` and not `eprintln!`, because a release build is linked
with `windows_subsystem = "windows"` (`crates/app/src/bin/oneterm.rs:7-10`) and has no
console to print to. It runs before `gpui_platform::application()` (`lib.rs:95`), so there
is no OneTerm window and no theme yet; a native message box is the only surface that
exists. This adds `Win32_UI_WindowsAndMessaging` to the workspace `windows-sys` feature
list (`Cargo.toml:125-135`) alongside `Win32_UI_Shell` — and, as built,
`Win32_System_Registry`: windows-sys 0.59 gates the whole `SHELLEXECUTEINFOW` struct
behind it because the struct carries an `HKEY` field. The feature buys the struct
definition, nothing more; OneTerm still reads no registry key (section 5).

Non-Windows builds are unaffected: `parse` compiles everywhere (it is pure `std`) and the
non-Windows arm of `run()` never calls it, so `oneterm` on Linux and macOS still ignores
`argv` exactly as it does today.

### 5. Trusted shell-path resolution

M3: the elevated instance does **not** read `terminal.json`'s `program` / `args` to decide
what to run. It resolves the enum to an absolute path itself, and the resolution never
consults `PATH` and never consults user configuration.

| `ElevatedShell` | Resolved to | The Windows fact this relies on |
| --- | --- | --- |
| `Cmd` | `%SystemRoot%\System32\cmd.exe` | `SystemRoot` is set by the session manager from the registry for every process; `%SystemRoot%\System32` is writable only by `TrustedInstaller` and the Administrators group, so nothing a standard user writes can land there. **Not** `COMSPEC` — `crates/core/src/config/shell.rs:164-168` reads `COMSPEC` for an ordinary tab, and `COMSPEC` is a plain environment variable a parent process can set to anything. |
| `PowerShell` | `%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe` | Windows PowerShell 5.1 ships in the OS at this fixed path on every supported Windows version. The `v1.0` directory name is historical and has never changed across 2.0-5.1. Same ACL as above. |
| `Pwsh` | the highest `<N>` for which `%ProgramFiles%\PowerShell\<N>\pwsh.exe` exists and `<N>` parses as an integer; if none does, `%ProgramFiles%\PowerShell\7\pwsh.exe` is reported as the missing file so the message can name it | The PowerShell 7 MSI installs to `%ProgramFiles%\PowerShell\<major>` by default. `%ProgramFiles%` is admin-writable only, and in a 64-bit process it is the native `C:\Program Files` (OneTerm ships x64 — `crates/app/build.rs` copies the x64 ConPTY binaries only for `x86_64`). |
| anything else | nothing — unreachable, the enum has three variants | — |

After resolution the path is checked with `Path::is_file` and nothing else. No reparse-point
check, no owner check, no signature check: all three paths sit under a directory a standard
user cannot write, and a machine where they do not is already lost before OneTerm starts.

> ponytail: existence check only. Ceiling — this trusts the ACLs of `%SystemRoot%\System32`
> and `%ProgramFiles%`. Upgrade path if that is ever not enough: `GetFileSecurityW` +
> an owner check against `BUILTIN\Administrators` / `NT SERVICE\TrustedInstaller`.

**Deliberate deviation from the owner's wording.** `DEC-0019`'s M3 offers "pwsh via its
registry install path **or** `%ProgramFiles%\PowerShell\7\pwsh.exe`". This design takes the
second and does not read
`HKLM\SOFTWARE\Microsoft\PowerShellCore\InstalledVersions\<GUID>\InstallLocation`, for two
reasons. First, that value is admin-written but it *points* anywhere the installer was told
to install: an MSI run with `INSTALLFOLDER=D:\tools\pwsh` records a path under a directory
a standard user may be able to write, which is the exact hole M3 exists to close — so
honouring the registry would need a second check that the target is under a protected
directory, at which point the directory scan alone is the same answer with less code.
Second, the scan needs no registry FFI and it picks up a
side-by-side PowerShell 8 for free. The cost is real and stated: **a pwsh installed outside
`%ProgramFiles%\PowerShell` cannot be elevated from the menu** — the row is offered, the
UAC prompt is answered, and the elevated process exits with code 3 and one message naming
the path it looked for. **Open for the owner:** if a custom-location pwsh must be
elevatable, the registry lookup comes back *plus* a check that `InstallLocation` resolves
under a protected directory, plus `RegGetValueW` from the `Win32_System_Registry` feature
the `SHELLEXECUTEINFOW` struct already pulls in (section 4). Until then this is the
recorded behaviour, not an oversight.

Resolution lives in `crates/core` and is parameterized the way `resolve_unix_shell`
(`crates/core/src/config/shell.rs:203-222`) already is, so it can be unit-tested on the
Linux and macOS CI runners without touching the host:

```rust
pub fn trusted_program(
    shell: ElevatedShell,
    system_root: &str,
    program_files: &str,
    versions: impl Fn(&str) -> Vec<String>, // read_dir in production
    exists: impl Fn(&str) -> bool,          // Path::new(..).is_file() in production
) -> Result<String, String>; // Err carries the path that was looked for, for the message
```

**Strings, not `PathBuf`, and a private `win_join(&[&str]) -> String`.** `std::path` is
host-flavoured: `PathBuf::join` separates with `/` on the Linux and macOS runners, so the
resolution built `X:\Windows/System32/cmd.exe` there and the tests that assert the real
Windows path went red on two of the three OSes (`US-0130`, rework 2026-09-22 — the same
lesson as `US-0114`). These paths are Windows paths by definition, so they are built as
strings with `\` spelled out and the injected lookup/existence seams stay string-based.
The result is turned into a `PathBuf` once, where `LocalShellConfig::program` needs one.

**Why the resolution runs in the elevated process and not in the launching one.** Resolving
first and passing the resolved path as an argument would make the unelevated process the
thing that names the executable the elevated process runs — `DEC-0019` rule 4, violated in
one line. The launching process therefore sends a token from a closed enum and nothing
else; the elevated process resolves. The consequence is a UAC prompt that can be answered
and then produce no window, which is worse UX than checking first and is the correct
trade: a pre-flight check in the unelevated process could only ever be advisory, and an
advisory check that the elevated side then trusts is the hole with extra steps.

**Why the environment variables are trustworthy here.** `%SystemRoot%` and `%ProgramFiles%`
are read in the *elevated* process, whose environment block was built by the AppInfo
service from the elevated token's profile. `SHELLEXECUTEINFOW` carries no environment block
at all (`research/windows-elevation-and-conpty.md` section 2.1) — the very limitation that
forced option A is what makes these variables unreachable from the launching process.

**Every field, not two.** `trusted_shell_config` builds the elevated
`LocalShellConfig` field by field and deliberately **not** with `..cfg.clone()`
(`IN-0043` MAJ-1, as built). `program` and `args` are not the only fields of
`terminal.json`'s shell block that direct what executes:

| Field | Why it cannot cross |
| --- | --- |
| `program`, `args` | names the executable and its command line |
| `env` | the ConPTY environment block writes custom entries **ahead** of the inherited ones, so a `PATH` or `PSModulePath` here decides what the elevated shell runs on the first unqualified command — the first `net`, `sc`, `reg` or `icacls` the user types |
| `cwd` | `cmd.exe` searches the working directory before `PATH`: the same escalation with one fewer field |
| `utf8` | presentation only (the console codepage); the one field that crosses |

Struct-update syntax would carry the next field added to `LocalShellConfig` across the
boundary silently; listing every field means the next one fails the build until somebody
decides which side it is on.

`ResolvedShell` carries `cwd` for the same reason: `LocalSession::spawn` used to read
`cfg.cwd` from the **original** config, so the guard at the top of `resolve_shell` was
bypassed by construction. Everything a spawn needs now comes out of `resolve_shell`, which
is what makes that one guard complete.

**The behaviour change this costs, stated rather than discovered.** `cwd: None` is
unconditional in an elevated window, so **Duplicate tab** and **New Terminal Here** open at
the home directory instead of inheriting the tab's live OSC 7 working directory
(`crates/terminal-view/src/panel/duplicate.rs` sets it; the guard discards it). That is the
correct security answer — the cwd of a live shell is not a value this process may take
instructions from once it is elevated, and `cmd.exe` searches the working directory before
`PATH` — and it is a real difference a user will notice between an ordinary window and an
administrator one. `docs/terminal-backend.md` §6.1.1 says so for users.

Once resolved, the shell is spawned through the ordinary path:
`PanelSpec::Shell`-equivalent -> `LocalSession::spawn`
(`crates/local-shell/src/session.rs:37-120`) -> `CreatePseudoConsole` +
`CreateProcessW` in **this** process (`crates/vt/src/pty/windows/conpty.rs:231-300`), with
`resolve_shell`'s environment injection (`crates/core/src/config/shell.rs:338-365`) intact.
That is the entire reason option A relaunches OneTerm instead of relaunching the shell:
OSC 7 cwd, OSC 133 prompt marks and `TERM` all survive. `LocalShellConfig::program` is
overridden with the trusted path and `args` is taken from `resolve_shell`'s own kind arm
(`shell.rs:242-299`), never from `cfg.args` (`shell.rs:368`).

### 6. The launch: `ShellExecuteExW`

```rust
// crates/app/src/elevation.rs   #[cfg(windows)]
let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
info.cbSize      = size_of::<SHELLEXECUTEINFOW>() as u32;
info.fMask       = SEE_MASK_FLAG_NO_UI;
info.hwnd        = std::ptr::null_mut();
info.lpVerb      = w!("runas");
info.lpFile      = exe.as_ptr();        // std::env::current_exe(), wide + NUL
info.lpParameters= params.as_ptr();     // "--elevated-shell <token>", built from the enum
info.lpDirectory = dir.as_ptr();        // std::env::current_dir(), wide + NUL
info.nShow       = SW_SHOWNORMAL;
if ShellExecuteExW(&mut info) == FALSE { /* GetLastError(), section 8 */ }
```

Field by field, and why each value:

- **`lpVerb = "runas"`.** The only path from a medium-integrity process to an elevated
  child. `CreateProcessW` creates the child with a copy of the caller's token and has no
  "elevate" flag; a process cannot raise its own token
  (`research/windows-elevation-and-conpty.md` section 2.1).
- **`lpFile = std::env::current_exe()`.** Never a name, never `PATH`, never `argv[0]`.
  Resolving the executable by name is how the wrong binary gets elevated. A failure of
  `current_exe()` aborts the launch with one notification and logs; nothing is guessed.
- **`lpParameters`.** Built by `format!("{ELEVATED_SHELL_FLAG} {token}")` where `token` is
  a `&'static str` from a `match` on the enum. It contains no user-supplied text, ever, so
  Windows command-line quoting is not a hazard on this path and there is no quoting code
  to get wrong.
- **`lpDirectory = std::env::current_dir()`** (falling back to `current_exe()`'s parent).
  **It is inside the trust boundary, not outside it** (`IN-0043` MIN-1, as built): any
  same-user process may call `ShellExecuteExW(runas, oneterm.exe, "--elevated-shell cmd")`
  with any `lpDirectory` of its own, and in a **debug or `fast-dev`** build `config_dir()`
  is the relative `target/`, so that directory chooses the elevated instance's whole
  configuration root. Release builds resolve `config_dir()` from `USERPROFILE` and are
  unaffected, which is the only reason this is acceptable: it is a **developer-build-only**
  exposure, and it is a severity multiplier for anything else that reads configuration
  rather than a hole on its own. A release build must never gain a cwd-relative
  `config_dir()`.
  Load-bearing: `runas` does not inherit the caller's working directory, and in **debug**
  builds `config_dir()` is the relative path `target/`
  (`crates/core/src/config/shell.rs:109-124`), so an unset `lpDirectory` silently puts the
  elevated instance on a different configuration root. Release builds resolve
  `config_dir()` from `USERPROFILE` and are unaffected either way.
- **`fMask = SEE_MASK_FLAG_NO_UI`.** Suppresses the shell's *own* error dialog — not the
  consent prompt, which the AppInfo service owns and which no flag can suppress — so every
  failure is reported by OneTerm in one place, in OneTerm's own notification style.
- **`SEE_MASK_NOCLOSEPROCESS` is deliberately NOT set**, against the HLD's sketch. It
  exists to return `hProcess`, nothing here consumes that handle, and a returned handle
  that is never closed is a leak. Not asking for it is one fewer thing to get wrong.
- **`hwnd = null`.** gpui does not expose the platform window handle, and the consent
  prompt is drawn on the secure desktop regardless; a null owner is documented as allowed.
- **No environment block, no attribute list.** `SHELLEXECUTEINFOW` has neither
  (`research/...` section 2.1). This is why the elevated process re-derives everything.

`Win32_UI_Shell` is added to the workspace `windows-sys` feature list (`Cargo.toml:125-135`)
for `SHELLEXECUTEINFOW` / `ShellExecuteExW`; `Win32_Foundation` (`GetLastError`,
`CloseHandle`), `Win32_Security` (`TOKEN_QUERY`, `TOKEN_ELEVATION`, `GetTokenInformation`)
and `Win32_System_Threading` (`GetCurrentProcess`, `OpenProcessToken`) are already listed.

**M6, no forwarding.** OneTerm has no single-instance guard today — no `CreateMutex`, no
named pipe, nothing (`research/...` section 1.4) — so every launch is a new process and
the rule is satisfied by construction. It is written down here and in `DEC-0019` rule 3
because it stops being free the moment instance coalescing is added: forwarding an
`--elevated-shell` argument to an existing instance would answer an elevation request
without elevating, which is a privilege bug wearing a UX bug's clothes. The parser's doc
comment carries the rule so the next person to add coalescing reads it.

### 7. The elevated-mode switch and every seam it gates

**One switch, and it is the token.** `oneterm_core::elevation::is_restricted()` reads the
value set in step 1 of section 3. The command-line argument selects *which shell opens* and
nothing else; it never contributes to the marker or to any gate. Stated as one sentence,
because it is the rule that makes M5 true:

> The argument selects the shell. The token decides everything else.

Two corollaries fall out and are both intended:

- A OneTerm the user elevated by some other route (right-click -> Run as administrator, a
  policy auto-elevation) is marked and restricted exactly like one launched from the menu.
  That is fail-safe: there is no elevated-but-unrestricted state to reach — **including
  when the token query fails**, which is why `Elevation` has three values and not two, and
  why `is_restricted()` is the only predicate the gates read (`IN-0043` MIN-3).
- A **non**-elevated process given `--elevated-shell cmd` opens Command Prompt, unmarked
  and unrestricted, and logs one `warn`. It cannot claim an elevation it does not have, so
  there is nothing to be fooled by, and it does not dead-end a window over a token race.

The token query:

```rust
// crates/app/src/elevation.rs   #[cfg(windows)]
pub(crate) fn process_elevation() -> Elevation {
    let mut token = std::ptr::null_mut();
    if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == FALSE {
        return Elevation::Unknown;              // restricted, and claims nothing
    }
    let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
    let mut returned = 0u32;
    let ok = GetTokenInformation(token, TokenElevation, (&mut elevation as *mut _).cast(),
                                 size_of::<TOKEN_ELEVATION>() as u32, &mut returned);
    CloseHandle(token);
    if ok == FALSE { return Elevation::Unknown; }
    if elevation.TokenIsElevated != 0 { Elevation::Elevated } else { Elevation::NotElevated }
}
```

`#[cfg(not(windows))]` it is a `const fn` returning `Elevation::NotElevated`, so every gate
below compiles away to the current behaviour on Linux and macOS and no surface is
`#[cfg]`-duplicated.

**Why three values and not a `bool`.** A failed query has two honest answers and they are
different ones. On the restrictions, fail **closed**: a process that *is* elevated and
cannot prove it would otherwise run SSH, SFTP, the updater and `terminal.json`'s program
under an administrator token with none of M1-M7 applied, and the corollary below — *there is
no elevated-but-unrestricted state to reach* — would be false in exactly that branch. On the
marker, fail **honest**: `DEC-0019` rule 2 forbids a window claiming an elevation the token
did not confirm, so `Unknown` reads `OneTerm (elevation unknown)` rather than
`(Administrator)`. `is_restricted()` is the only predicate the gates read; there is
deliberately no `is_elevated()` beside it, because two predicates differing only in this
case is an invitation to guard a security seam with the weaker one.

Seams, one row per gate, with the line each one sits on today:

| M | Seam | File:line today | Elevated behaviour |
| --- | --- | --- | --- |
| M5 | OS window title | `crates/app/src/window.rs:66` — `window.set_window_title("OneTerm")` | `"OneTerm (Administrator)"`, so the taskbar, Alt-Tab and every screenshot carry it |
| M5 | in-app title bar text | `crates/workspace/src/layout/workspace/mod.rs` — `AppTitleBar::new("OneTerm", window, cx)` | **Two spans** (owner ruling 2026-09-21): the app menu bar keeps the plain `OneTerm`, and `AppTitleBar::render` draws the suffix beside it — ` (Administrator)` or ` (elevation unknown)` — in `cx.theme().warning`, bold. A sibling element rather than part of the menu name, because the kit renders a menu name as one string and `docs/PROJECT.md` forbids patching it. `window_title_parts` is a **view** of `window_title`, not a second copy, and a test concatenates the spans and asserts the result is the OS title, so the plain and the coloured forms cannot drift. |
| M5 | `warning` on `title_bar.background` | `scripts/check-theme-contrast.py` `SURFACES` | A new text-on-surface pairing, so it is checked: every theme in `crates/theme/themes/` now sets `warning` explicitly — the 27 dark variants keep exactly the kit's `#facc15`, the 12 light ones take a darker shade of the kit's own yellow ramp (lightness only, same hue). The kit's default amber reads on a dark title bar and fails on all 12 light ones, which is why the token could not simply be left to the kit. |
| M5 | ~~title-bar border~~ | `crates/workspace/src/layout/title_bar.rs` | **Removed** (owner ruling 2026-09-21, `DEC-0019` M5 as amended): the marker is the **title text only**. The border briefly carried `cx.theme().warning`; it was chosen as a border precisely so it added no text surface, so removing it leaves `scripts/check-theme-contrast.py` untouched in both directions. If a later change ever tints `title_bar.background`, that surface must be added to `SURFACES` (and to `PARENTS`) in the same commit — the note outlives the border it was written for. |
| M1 | the "+" menu shell list | `crates/terminal-view/src/panel/terminal_panel.rs:697-704` | the three Windows kinds only, and **no "Run as administrator" rows** — `is_restricted()` suppresses them, so an elevated window cannot spawn a second identical one |
| M1 | the "+" menu SSH block | `terminal_panel.rs:705-742` — "SSH Sessions" separator, saved rows, "Quick Connect...", "New Saved Session..." | absent in full. `FIXED_ROWS` (`:763`) is computed for the rows actually emitted, so the scroll estimate stays honest in both modes |
| M1 | right dock content | `crates/workspace/src/layout/workspace/mod.rs` (`startup_dock_document`), `layout.rs` (`right_dock`) | **`docks.json` is not read at all** (as built, `IN-0043` MAJ-2). Gating the two layout *builders* was necessary and not sufficient: `load_layout` restores the side docks **by name**, so a saved `ssh_client` or `agent` dock is built before either builder runs, and declining to call `set_dock(Right, ..)` does not remove a dock that is already there — it leaves it. An elevated window therefore starts from the fixed default layout, every time, and `right_dock`'s `None` arm now calls `remove_dock` rather than merely skipping `set_dock`. `sync_right_dock_mode` and `apply_right_dock_width` early-return on `!has_dock(Right)` as before |
| M1 | right-dock mode toggles | `crates/workspace/src/layout/workspace/mod.rs:263` — `.child(|_, cx| title_bar::mode_toggle_group(cx))` | the child is not attached. The three segments ("SSH Client" / "Agent" / "None", `title_bar.rs:110-152`) are absent rather than disabled: there is nothing to switch to |
| M1 | Agent panel | registered at `crates/app/src/init.rs:42-44` | `oneterm_agent_ui::init(cx)` still runs — the `AppServices` bundle's install order invariant (`init.rs:57-61`) depends on every feature `init` having run — but the panel has no route into the layout because the right dock does not exist and the toggle is gone |
| M1 | **actions**, not only rows | `crates/actions/src/elevated_policy.rs` (new), consulted by `workspace/.../actions.rs`, `session-ui/src/lib.rs`, `session-ui/src/quick_connect_dialog.rs` and `settings-ui/.../key_bindings/state.rs` | Removing a *row* is not removing an *action*: `new_ssh_session` ships bound to `ctrl-shift-n` with a global context, so Quick Connect opened in an elevated window with no row involved (`IN-0043` MAJ-3). One exhaustive table classifies every `BINDABLE_ACTIONS` id; an unclassified id is **denied**, and `every_bindable_action_is_classified_for_an_elevated_window` fails until somebody classifies it. `apply_key_bindings` does not bind a denied action at all, which closes every keystroke route including a user's own override |
| M4 | terminal logging | `crates/settings/src/terminal_config/logging.rs` — `LoggingConfig::runtime_config` | **off regardless of config** (`IN-0043` MAJ-4). `logging.local` plus `logging.directory` is a file created at a config-chosen path under an administrator token, and `LogWriteMode::Overwrite` makes it a create-or-**truncate** primitive. One gate in the function both the local and the SSH caller resolve through |
| M4 | quarantine renames | `crates/core/src/persistence.rs` — `quarantine_file` | **no rename** (`IN-0043` MIN-2). The M4 guards sit on the write entry points; a quarantine is a rename of the user's file and is not one of them. An elevated window that meets a corrupt document starts on the defaults, says so once, and leaves the file for the ordinary window to quarantine. One guard covers `ui_config.json`, `terminal.json` and `docks.json` |
| M2/M4 | the Network settings page | `crates/settings-ui/src/panel.rs` — `pages` | **hidden** (`IN-0043` MIN-4). It configures how the updater reaches GitHub, and M2 removed the updater; a page whose edits are silently discarded is worse than one that is not there. The settings window also carries one line saying this is an administrator window and its changes are not saved |
| M2 | startup update check | `crates/app/src/window.rs:58` — `start_auto_check(window, cx)` | not called |
| M2 | manual check | `crates/settings-ui/src/updates/actions.rs:85` — `check_now` | early return |
| M2 | download and install | `crates/settings-ui/src/updates/install.rs:12` — `download_and_install_update` | early return, before the confirm dialog |
| M2 | the About/Updates surface | `crates/settings-ui/src/updates/groups.rs:22-36` — `group(cx)`, rendered by `crates/settings-ui/src/about.rs:137` and by the About dialog at `about.rs:82` | the group's items are replaced by one line: *"Updates are checked and installed from the normal OneTerm window."* No preferences, no status, no button |
| M4 | `ui_config.json` write | `crates/settings/src/ui_config.rs:224-237` (`persist`) and `:154-164` (`save_to`) | **reuse the mechanism that is already there**: `persist_blocked` is set at load when elevated, and both functions already refuse with `persist_blocked_error()` (`:140-145`). No new guard is written |
| M4 | `terminal.json` write | `crates/settings/src/terminal_settings/persist.rs:163-171`, `crates/settings/src/terminal_config/document.rs:181` | same `persist_blocked` mechanism, already present at `persist.rs:163` |
| M4 | `docks.json` write | `crates/workspace/src/layout/workspace/persistence.rs` — `save_state_logged`, called from `mod.rs:422`, `mod.rs:453`, `layout.rs:25`, `layout.rs:96` | one guard inside `save_state_logged`, not four at the call sites: every writer routes through it, so one `if` closes all of them and a fifth caller added later is covered for free |
| M4 | `ssh_session.json` write | `crates/session-ui/src/session_state.rs:483` | guarded. Unreachable under M1 (no surface can edit a session), so the guard is belt-and-braces and costs one `if` |
| M4 | `update_config.json` write | `crates/settings-ui/src/updates/config.rs:63` (`persist_update_config`) and `crates/update/src/manager.rs:297` (`persist_config`, unreachable under M2) | guarded at `config.rs:63` |
| M7 | crash store root | `crates/app/src/crash_report.rs:81-83` — `crashes_dir()` | `config_dir().join("crashes").join("elevated")`. One function; the writer (`prepare_capture_paths`, `:31-39`), the reader (`load_pending_reports`, `:59-64`) and the path validator (`delete_pending_report`, `:67-79`) all go through it, so they stay consistent with no further edits |
| M7 | the crash dialog | `crates/app/src/window.rs:59-65` — `show_crash_reports(...)` | not called. `load_pending_reports()` (`lib.rs:56`) still runs, so promotion of stale `.native.tmp` files and the newest-20 retention (`docs/crash-reporting.md` section "Reconciliation and retention") still happen in `crashes/elevated/`; only the GitHub-draft dialog is suppressed |

Why the crash store is split at all: two processes at different integrity levels sharing
one directory is the awkward case `docs/crash-reporting.md` already worries about — the
promotion logic skips staging owned by another *live* PID, and each instance prunes to the
newest 20. A file written by the elevated process can carry an ACL the medium-integrity
process cannot delete, which would wedge that pruning permanently. A separate subdirectory
costs one `join` and removes the whole class.

### 8. Error paths

| Path | Behaviour | Where |
| --- | --- | --- |
| User declines the UAC prompt | `ShellExecuteExW` fails, `GetLastError() == ERROR_CANCELLED` (1223). **Silent no-op**, one `log::info`. The user said no; telling them so is noise | `crates/app/src/elevation.rs` |
| Policy denies elevation, or no administrator is reachable | one error notification naming the OS error. The row is **not** hidden in advance: the policy state is not reliably knowable before asking, and a missing row is worse than a clear refusal | same |
| Any other `ShellExecuteExW` failure | one error notification with the OS error, one `log::warn`. The existing window is untouched | same |
| `current_exe()` fails | one error notification, one `log::warn`, no launch. Nothing is guessed from `PATH` or `argv[0]` | same |
| The notification itself | `window.push_notification(notify(NotificationType::Error, msg, cx), cx)` — the shape `crates/settings-ui/src/updates/notify.rs:21-28` already uses, through `oneterm_theme::notif_ext::notify`. One message, never a dialog: `docs/agents/error-policy.md` reserves dialogs for decisions | `crates/app/src/elevation.rs` |
| Unrecognized command line | `MessageBoxW` naming the flag and the three accepted values, exit code 2, no window | `run()`, step 2 |
| Requested shell has no trusted path | `MessageBoxW` naming the path that was looked for, exit code 3, no window. An elevated window with no shell in it is worse than none | `run()`, before the gpui app starts |
| The elevated process cannot read the configuration (different profile, over-the-shoulder elevation) | it starts with defaults and says so **once**, as one info notification after the window opens: *"Settings could not be read for this account; this window is using the defaults."* The marker is still correct, because it comes from the token, not from the configuration | `crates/app/src/window.rs` after `open_window` |
| The token query itself fails | **fail closed on the restrictions, honest on the marker** (`Elevation::Unknown`, as built). M1-M7 all apply, and the title reads `OneTerm (elevation unknown)` with the warning border. The first draft of this table said "treated as not elevated", which was fail-**open** on a security switch: a process that *is* elevated and fails the query would have run SSH, SFTP, the updater and `terminal.json`'s program under an administrator token, unmarked, and the corollary in section 7 — *there is no elevated-but-unrestricted state to reach* — would have been false in exactly that branch. Claiming "(Administrator)" instead is not the answer either: `DEC-0019` rule 2 forbids a marker asserting an elevation the token never confirmed. `OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY)` on one's own process is always granted, so the branch stays theoretical; the direction it fails in is not. | section 7 |
| The elevated window fails to open | an ordinary OneTerm start-up failure (CORR-63): logged, no window. The launching window has already returned; the user sees a consent prompt followed by nothing. Same as any failed launch | `crates/app/src/window.rs` |

## Interfaces

```rust
// ── crates/core/src/config/elevation.rs (new, pure std) ─────────────────
pub const ELEVATED_SHELL_FLAG: &str = "--elevated-shell";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElevatedShell { Cmd, PowerShell, Pwsh }

impl ElevatedShell {
    /// The command-line token for this shell ("cmd", "powershell", "pwsh").
    pub fn token(self) -> &'static str;
    /// The ShellKind this opens. Total: every variant maps.
    pub fn shell_kind(self) -> crate::ShellKind;
    /// The elevatable form of a kind, or None (Bash/Zsh/Sh/Custom -- M3).
    pub fn from_shell_kind(kind: crate::ShellKind) -> Option<Self>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError { Unrecognized(String) }

/// Parse process arguments (argv[0] skipped). See section 4 for the grammar.
pub fn parse<I, S>(args: I) -> Result<Option<ElevatedShell>, CliError>
where I: IntoIterator<Item = S>, S: AsRef<std::ffi::OsStr>;

/// Resolve to an absolute path OneTerm trusts. Err carries the path looked for.
/// Strings, never `PathBuf`: these are Windows paths on every host (section 5).
pub fn trusted_program(
    shell: ElevatedShell,
    system_root: &str,
    program_files: &str,
    versions: impl Fn(&str) -> Vec<String>,
    exists: impl Fn(&str) -> bool,
) -> Result<String, String>;
/// The same, against this machine (`%SystemRoot%`, `%ProgramFiles%`, read_dir, is_file).
pub fn trusted_program_for(shell: ElevatedShell) -> Result<String, String>;

/// M3 as one function: the config an elevated process may spawn for `cfg.kind` --
/// the trusted program, the kind's own args, and nothing from `terminal.json`.
/// `resolve` is `trusted_program_for` in production and injected in tests, so the
/// rule is unit-tested without the process global and without a Windows host.
/// `resolve_shell` calls this behind `if is_restricted()`; that one guard covers
/// every local spawn, because every one of them resolves through it.
pub fn trusted_shell_config(
    cfg: &LocalShellConfig,
    resolve: impl Fn(ElevatedShell) -> Result<String, String>,
) -> Result<LocalShellConfig, AppError>;

/// What the process token said. Three-valued: see section 7.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Elevation { #[default] NotElevated, Elevated, Unknown }
impl Elevation { pub fn is_restricted(self) -> bool; }   // true for Elevated AND Unknown

/// The window title, for the OS title bar and the in-app one -- one function for
/// both, so the two markers cannot disagree. Takes the value rather than reading
/// the global, so the rule is testable without mutating process state.
pub fn window_title(elevation: Elevation) -> &'static str;

/// Process globals, set exactly once in run() before any UI exists.
pub fn set_elevation(value: Elevation);
pub fn elevation() -> Elevation;           // NotElevated until set, and on non-Windows
pub fn is_restricted() -> bool;            // the ONE predicate every gate reads
/// Takes an `ElevatedShell`, not a `ShellKind`, as built: the global then cannot
/// hold `Custom` even by mistake, which is the same argument the enum itself makes.
pub fn set_initial_shell(shell: ElevatedShell);
pub fn initial_shell() -> Option<crate::ShellKind>;   // what the panel needs

// ── crates/app/src/elevation.rs (new; the Win32 bodies are #[cfg(windows)],
//    the module is not, because the fn pointer field exists on every platform) ──
pub(crate) fn process_elevation() -> Elevation; // a const fn returning NotElevated off Windows
pub(crate) fn fatal_message(text: &str);   // MessageBoxW; used before the gpui app starts
pub(crate) fn launch_elevated_shell(kind: oneterm_core::ShellKind, window: &mut Window, cx: &mut App);

// ── crates/state/src/commands.rs (one field added to WorkspaceCommands) ──
/// Launch a NEW elevated OneTerm on this shell (Windows only; a no-op with one
/// log line elsewhere). The kind is passed as a value, never a path: nothing the
/// unelevated process writes may direct what the elevated process executes
/// (`DEC-0019` rule 4). An elevation request is always served by the process that
/// received it and must never be forwarded to another instance (`DEC-0019` rule 3).
pub launch_elevated_shell: fn(ShellKind, &mut Window, &mut App),
```

`WorkspaceCommands` is `#[derive(Clone, Copy)]` and built in exactly two places — the
composition root (`crates/app/src/init.rs:65-76`) and test doubles — so the added field is
a compile-time-checked change with no runtime surface.

## Edge Cases and Failure Modes

- [ ] **No arguments.** Normal start, default shell, unmarked, unrestricted. Byte-for-byte
      today's behaviour; this is the case every existing user is in.
- [ ] **`--elevated-shell` with a valid token in a process that is not elevated.** Opens
      that shell; no marker, no restrictions, one `warn`. The argument never implies
      elevation.
- [ ] **Elevated with no argument** (right-click -> Run as administrator). Marked and fully
      restricted; the default shell opens. No elevated-but-unrestricted state exists.
- [ ] **`--elevated-shell bash` / `custom` / `--elevated-shell` alone / `--foo` / a bare
      path / the flag twice.** All identical: one message box, exit 2, no window.
- [ ] **UAC declined.** `ERROR_CANCELLED`; silent no-op; the launching window is untouched,
      its tabs, SSH connections and SFTP transfers all still running.
- [ ] **pwsh not installed, or installed outside `%ProgramFiles%\PowerShell`.** Consent
      prompt is answered, then one message box naming
      `%ProgramFiles%\PowerShell\7\pwsh.exe` and exit 3. No window.
- [ ] **Over-the-shoulder elevation** (a standard user types an administrator's
      credentials). The elevated process runs as that other account, so `USERPROFILE` and
      therefore `~/.OneTerm` differ (`crates/core/src/config/shell.rs:91-96`, `:109-124`):
      no saved theme, no font override, no key-binding overrides, its own
      `crashes/elevated/`. It starts on defaults and says so once. Under M4 it writes
      nothing into that other profile either, so it leaves no trace there.
- [ ] **Split-token elevation** (the common case). Same SID, same `USERPROFILE`, same
      configuration directory — and because of M4 there is still exactly one writer, the
      unelevated instance. The "two concurrent writers" worry the HLD raised does not
      arise for this feature.
- [ ] **Drag-and-drop upload from Explorer into the elevated window.** Does not work: UIPI
      forbids a drop from a medium-integrity Explorer onto a high-integrity window
      (`crates/sftp-ui/src/render.rs:436-451` accepts `ExternalPaths`). Inherent to
      elevation and not fixable — and under M1 there is no SFTP panel in that window to
      drop onto, so the regression is moot in practice. Still documented, because a user
      who elevates OneTerm by hand will meet it.
- [ ] **Debug build launched from a different working directory.** `lpDirectory` is the
      launching process's `current_dir()`, so both instances resolve `config_dir()` to the
      same `target/`. Ceiling: in a debug build the configuration root of the elevated
      process is therefore influenced by the launching process's cwd. Release builds
      resolve it from `USERPROFILE` and are unaffected. Developer-build-only, accepted.
- [ ] **A future single-instance guard.** Forbidden to forward an `--elevated-shell`
      argument. `DEC-0019` rule 3 / M6, repeated in the parser's doc comment.
- [ ] **Token query failure.** `Elevation::Unknown`, which is **restricted** — every gate of
      M1-M4 and M7 applies — while the marker reads `OneTerm (elevation unknown)` rather than
      claiming an elevation the token never confirmed (`DEC-0019` rule 2). Fail closed on the
      restrictions, fail honest on the marker; an earlier draft of this line said "treated as
      not elevated", which was fail-**open** and would have left a genuinely elevated process
      running SSH, SFTP, the updater and `terminal.json`'s program unmarked. The variant
      carries a `&'static str` naming which call failed, because the query runs before
      `env_logger` exists; `run()` logs it once the logger is up (`IN-0043` MIN-3, NEW-9).
- [ ] **A second elevation request while an elevated window is open.** Served: a second
      consent prompt, a second elevated window. No coalescing, by M6.
- [ ] **A second *click* while the first consent prompt is still up.** Ignored. The launch
      became asynchronous when it moved off the gpui thread, and with it went the accidental
      debounce the modal call gave for free; one flag, held for the lifetime of the
      outstanding request, restores it (`IN-0043` NEW-8). This is about clicks, not about
      requests: M6 is untouched, and two *separate* elevation requests still get two prompts.
- [ ] **Quitting OneTerm while the consent prompt is up.** The helper thread is detached, so
      the process exits with that thread inside `ShellExecuteExW`: its `CoUninitialize` never
      runs, and if the user then approves the prompt an elevated OneTerm starts with no
      launcher left to tell. Harmless, and worth writing down rather than discovering
      (`IN-0043` NEW-10). Nothing the elevated process needs came from the launcher: the
      verb, the executable and the one-token parameter were all fixed before the thread
      started, and the COM apartment dies with the process. The window that opens is a
      normal elevated OneTerm; only the `log::info!` saying it started is lost.

## Verification

Unit, and these are the whole of the focused proof because everything they cover is pure
data (`cargo test -p oneterm-core`, runs on all three CI runner OSes):

- [ ] `parse` accepts each of the three tokens and yields the matching `ElevatedShell`.
- [ ] `parse` with no arguments yields `Ok(None)` — the normal start must not regress.
- [ ] `parse` rejects, with `CliError::Unrecognized`: an unknown flag; the flag with no
      value; `bash`, `zsh`, `sh`, `custom`; the flag given twice; a bare positional; a
      trailing extra argument. One test per case, because each is a distinct way in.
- [ ] `ElevatedShell::from_shell_kind` returns `None` for `Bash`, `Zsh`, `Sh` and
      **`Custom`** — the M3 test, and the one that fails loudest if someone widens the enum.
- [ ] `token()` and `shell_kind()` round-trip for all three variants.
- [ ] `trusted_program` returns `%SystemRoot%\System32\cmd.exe` and the `v1.0` PowerShell
      path from injected roots, never consulting `PATH`.
- [ ] `trusted_program` for `Pwsh` picks the **highest** numeric directory from an injected
      listing (`7`, `8`, `preview` -> `8`), ignores non-numeric entries, and returns
      `Err(<the 7 path>)` when the listing is empty.

Unit, gating decisions (pure functions, no gpui): `cargo test -p oneterm-core`,
`cargo test -p oneterm-workspace`.

- [ ] The gate helper reports "restricted" for an elevated process regardless of whether an
      argument was given, and "unrestricted" for a non-elevated process **even when
      `--elevated-shell` was given** — the M5 "the token decides" test.
- [ ] The elevated start-up path does not name `panel_names::SSH_CLIENT`
      (`crates/workspace/src/layout/workspace/layout_tests.rs` style, asserting the dock
      state has no right dock).

Integration: `cargo test --workspace` — `WorkspaceCommands` gains a field, so every test
double that builds one must still compile. That is the cheapest proof that no seam was
missed.

Platform: `pwsh scripts/ci-local.ps1`. Linux and macOS must still build: every new Win32
surface is `#[cfg(windows)]` and every new pure function is not.

**Manual E2E, on an interactive Windows desktop. It cannot be automated**: the consent
prompt is drawn by the AppInfo service on the secure desktop, where the posted-message GUI
walks this project uses cannot reach it, and no test double can stand in for a real token.
Every step keeps its artefact in the intake's `evidence/` folder, created by the packet
that first produces one:

- [ ] **E1 — the row and the prompt.** Screenshot of the "+" menu in a normal window
      showing the "Run as administrator" rows in place, with the rows below them
      (SSH Sessions, Quick Connect..., New Saved Session...) in their owner-fixed order.
      Click one; screenshot the UAC prompt naming `oneterm.exe`.
- [ ] **E2 — the marked window.** Screenshot of the elevated window showing all three
      markers at once: the OS title bar / taskbar reading `OneTerm (Administrator)`, the
      in-app title bar reading the same. The title text is the whole marker: the warning-
      coloured border was removed on the owner's ruling (`DEC-0019` M5 as amended).
- [ ] **E3 — really elevated.** `whoami /groups` in the elevated tab, screenshot showing
      `Mandatory Label\High Mandatory Level`. This is the proof that the feature did what
      it claims; the marker alone is not.
- [ ] **E4 — the elevated "+" menu.** Screenshot showing exactly the three Windows shells:
      no "Run as administrator" rows, no SSH Sessions separator, no saved sessions, no
      Quick Connect..., no New Saved Session... And a screenshot of the elevated window's
      title bar with **no** right-dock mode toggles and no right dock.
- [ ] **E5 — the normal window is untouched.** Same screenshot session: the original window
      still has its tabs, a live SSH connection still responding, and its own unmarked
      title bar.
- [ ] **E6 — declined.** Click the row, decline the prompt. Screenshot showing no new
      window, no notification, no change; plus the log line at `info`.
- [ ] **E7 — configuration unchanged.** `Get-FileHash` over `ui_config.json`,
      `terminal.json`, `docks.json`, `ssh_session.json` and `update_config.json` before
      opening the elevated window and after closing it. **All five hashes identical.** This
      is the M4 proof and the one an automated test cannot give, because it has to survive
      a real window close and its exit-time layout write
      (`crates/workspace/src/layout/workspace/mod.rs:253-259`).
- [ ] **E8 — the updater is absent.** Screenshot of the elevated window's About dialog
      showing the one-line "updates are installed from the normal OneTerm window" text and
      no check button, no preferences, no status.
- [ ] **E9 — the crash store is split.** Trigger a panic in an elevated debug build;
      confirm the report lands in `<config>/crashes/elevated/` and that **no** dialog
      appears in the elevated window; then confirm the normal window's crash dialog on the
      next launch does not show it.
- [ ] **E10 — a bad command line.** Run `oneterm.exe --elevated-shell bash` and
      `oneterm.exe --wat` from a prompt. Screenshot each message box; confirm exit codes 2
      and 2 and that no window opened.
