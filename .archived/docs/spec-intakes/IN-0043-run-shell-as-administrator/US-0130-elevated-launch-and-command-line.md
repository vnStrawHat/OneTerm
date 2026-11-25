# Work: The elevated launch and the command line

ID: US-0130
Intake: IN-0043
Created: 2026-09-21

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [x] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability
- Risk lane: **high_risk**
- Risks, named so the review knows what to look at:
  1. **The trust boundary itself.** This packet builds the only channel from a
     medium-integrity process into a high-integrity one. The failure that matters is not a
     crash: it is a version of this code where something the unelevated process writes —
     `terminal.json`'s `program`, a `PATH` entry, `COMSPEC`, a resolved path passed as an
     argument — ends up naming the executable the elevated process runs. `DEC-0019` rule 4
     and M3 exist for exactly that, and the acceptance below tests it directly.
  2. **A tolerant parser.** An argument parser that "does its best" with input it does not
     recognize is how an elevated process ends up doing something nobody asked for. The
     parser here has no tolerant branch, and the tests enumerate every rejection.
  3. **A UAC prompt is a real consent step.** Declining is the normal case and must cost
     nothing; every other failure must be visible exactly once, not logged into silence.
  4. **`lpDirectory`.** Unset, a debug build silently lands on a different configuration
     root (`crates/core/src/config/shell.rs:109-124`).
- Spec Intake: `IN-0043` (Run a Windows shell as administrator)
- Decision inherited: `DEC-0019` (Accepted 2026-09-21, option A with M1-M7)

## Outcome

OneTerm can start a second, elevated copy of itself on a named Windows shell, and the only
thing that crosses from the old process to the new one is a value from a closed
three-token enum.

Concretely, when this packet is done:

- `oneterm.exe --elevated-shell cmd|powershell|pwsh` starts OneTerm with that shell as its
  one initial shell. `oneterm.exe` with no arguments behaves exactly as it does today.
  Anything else shows one message and exits without opening a window.
- The shell is resolved to an absolute path OneTerm trusts — `%SystemRoot%\System32\...`
  or `%ProgramFiles%\PowerShell\...` — inside the elevated process, never through `PATH`,
  `COMSPEC` or user configuration, and `ShellKind::Custom` cannot be elevated at all.
- A new `WorkspaceCommands` fn pointer launches that command line through
  `ShellExecuteExW` with the `runas` verb on `current_exe()`, with an explicit
  `lpDirectory`. A declined consent prompt does nothing; every other failure produces one
  notification.

The visible marker, the restricted elevated mode and the menu rows are **not** in this
packet. This is the mechanism; `US-0131` makes it safe to be in and `US-0132` makes it
reachable.

## Scope

- [ ] In scope:
  - `crates/core/src/config/elevation.rs` (new): `ElevatedShell`, `CliError`,
    `ELEVATED_SHELL_FLAG`, `parse`, `trusted_program`, and the `initial_shell()` /
    `set_initial_shell()` process global. Pure `std`, no new dependency, no `windows-sys`
    in `crates/core`.
  - `crates/core/src/config/shell.rs` — `resolve_shell` gains the path that honours a
    trusted program override, so the elevated instance's spawn keeps the kind's own args
    and the shell-integration environment (`shell.rs:242-299`, `:338-365`) while taking its
    program from `trusted_program` instead of `COMSPEC` / `find_in_path`.
  - `crates/core/src/lib.rs` — re-export, following the shape at `lib.rs:24`.
  - `crates/app/src/elevation.rs` (new, `#[cfg(windows)]`): `launch_elevated_shell` and
    `fatal_message`. The token query lands here too but is used by `US-0131`; this packet
    adds the file and the `ShellExecuteExW` half.
  - `crates/app/src/lib.rs:32` — `run()` reads `argv` for the first time (steps 2 and 3 of
    `low-level-design/elevated-instance.md` section 3).
  - `crates/terminal-view/src/panel/terminal_panel.rs:182-194` — the
    `PanelSpec::DefaultShell` arm consults `initial_shell()`.
  - `crates/state/src/commands.rs:38-70` — one fn pointer added to `WorkspaceCommands`.
  - `crates/app/src/init.rs:65-76` — the composition root registers it.
  - `Cargo.toml:125-135` — `Win32_UI_Shell` and `Win32_UI_WindowsAndMessaging` added to
    the workspace `windows-sys` feature list.
- [ ] Out of scope:
  - **The "+" menu rows** (`US-0132`). The fn pointer is registered and reachable, but no
    menu row calls it yet; the packet is exercised from a command line and a temporary
    debug entry point.
  - **The elevation marker, the restricted mode, the crash-store split, the updater
    shutdown** (`US-0131`). This packet can launch an elevated OneTerm that still looks and
    behaves like an ordinary one. That is deliberate and is why `US-0131` is the next
    packet and not a later one.
  - Any single-instance guard. M6 is recorded here as a rule in the parser's doc comment
    and in `DEC-0019`; no coalescing code is written, because none exists to change
    (`research/windows-elevation-and-conpty.md` section 1.4).
  - A `RT_MANIFEST` requesting elevation. `crates/app/assets/oneterm.rc` stays manifest-free
    and OneTerm keeps running `asInvoker`; an auto-elevating OneTerm is the opposite of
    what this intake asked for.
  - Non-Windows behaviour. `parse` and `trusted_program` compile and are tested everywhere
    because they are pure; nothing else changes off Windows.

## Acceptance

- [ ] `oneterm.exe` with no arguments starts exactly as it does today: one initial shell,
      the configured kind, no message, no behaviour change. (The regression that matters
      most — every existing user is in this case.)
- [ ] `oneterm.exe --elevated-shell cmd` opens with Command Prompt as its **one** initial
      shell; `--elevated-shell powershell` and `--elevated-shell pwsh` likewise. `DEC-0016`'s
      rule that OneTerm opens exactly one initial shell (`IN-0031`) still holds — the
      argument replaces the default shell, it does not add a tab beside it.
- [ ] Every other command line shows **one** message box naming the flag and its three
      accepted values, exits with code 2, and opens no window: an unknown flag; the flag
      with no value; `bash`, `zsh`, `sh`, `custom`; the flag twice; a bare positional; a
      trailing extra argument.
- [ ] `ElevatedShell::from_shell_kind(ShellKind::Custom)` is `None`, and there is no path
      through this code by which a `Custom` shell can be launched elevated (M3).
- [ ] The resolved program for each token is the trusted absolute path and nothing else:
      `%SystemRoot%\System32\cmd.exe` (**not** `COMSPEC`),
      `%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe`, and
      `%ProgramFiles%\PowerShell\<highest numeric>\pwsh.exe`. `PATH` is not consulted on
      this path, and neither is `terminal.json`.
- [ ] A shell with no trusted path on this machine shows one message box naming the path
      that was looked for and exits with code 3. No window opens.
- [ ] The elevated instance's shell keeps OneTerm's shell integration: OSC 7 sets the
      breadcrumb and OSC 133 prompt marks work in the elevated tab, because `resolve_shell`
      and `CreatePseudoConsole` both run in that process
      (`crates/core/src/config/shell.rs:338-365`, `crates/vt/src/pty/windows/conpty.rs:231-300`).
- [ ] `launch_elevated_shell` calls `ShellExecuteExW` with `lpVerb = "runas"`,
      `lpFile = current_exe()`, `lpParameters` built from the enum, and a **non-null
      explicit `lpDirectory`**. It never passes a resolved program path, and never passes
      any string derived from user configuration or user input.
- [ ] Declining the consent prompt (`ERROR_CANCELLED`, 1223) does nothing at all: no
      window, no notification, no change to the launching window's tabs, SSH connections or
      transfers. One `log::info` line.
- [ ] Any other `ShellExecuteExW` failure produces exactly **one** error notification
      naming the OS error, plus one `log::warn`. Never a dialog
      (`docs/agents/error-policy.md`: dialogs are for decisions).
- [ ] `current_exe()` failing aborts the launch with one notification. Nothing is guessed
      from `PATH` or `argv[0]`.
- [ ] `cargo test --workspace` is green, which includes every test double that builds a
      `WorkspaceCommands` (the struct gained a field).
- [ ] Linux and macOS still build and test green: every new Win32 surface is
      `#[cfg(windows)]`, and the pure functions are tested on all three runners.

## Documentation

### Owning Docs Reviewed

- `docs/decisions/DEC-0019-elevated-shells-open-in-an-elevated-window.md` — rules 1, 3 and
  4, and mitigations M3 and M6, which this packet implements.
- `docs/spec-intakes/IN-0043-run-shell-as-administrator/low-level-design/elevated-instance.md`
  — sections 2 (crate split), 3 (start-up sequence), 4 (command-line contract), 5 (trusted
  paths) and 6 (the `ShellExecuteExW` call). The contract this packet is built against.
- `docs/spec-intakes/IN-0043-run-shell-as-administrator/research/windows-elevation-and-conpty.md`
  — sections 1.1, 1.2, 1.4 and 2.1: why `CreateProcessW` cannot elevate, why the
  pseudo-console cannot follow, and why OneTerm relaunches itself rather than the shell.
- `docs/terminal-backend.md` sections 6.1 (Configurable shell, line 390) and 6.2 (Spawn via
  `oneterm_vt::pty`, line 453) — the owning prose for shell resolution and the ConPTY
  spawn, both of which this packet adds a path to.
- `docs/agents/crate-dependency-rules.md` R1, R4, R5, R10 — why the menu reaches the effect
  through `WorkspaceCommands` and why the shared types go in `crates/core`.
- `docs/agents/error-policy.md` — notification versus dialog, and logging a best-effort
  step with its operation name.
- `docs/agents/dependencies.md` — no CLI dependency is added for a flag and three tokens.
- `docs/spec-intakes/IN-0031-startup-spawns-two-local-shells/BUG-0054-startup-spawns-exactly-one-local-shell.md`
  and `docs/decisions/DEC-0016-terminate-a-shell-that-outlives-its-pseudo-console.md` —
  "exactly one initial shell", which the argument must not break.

### Documentation Action

**Update required, in `US-0132`, not here.** `docs/terminal-backend.md` sections 6.1/6.2
gain the paragraph on the trusted-path resolution and on why an elevated child cannot join
this process's pseudo-console, and `docs/gui-layout.md:104` gains the menu rows. Both
changes describe behaviour a *user* can reach, and no user can reach any of it until
`US-0132` puts the rows in the menu: documenting a command line nobody is told about, one
packet before the feature exists, would put a paragraph in the owning contract describing
something that cannot yet happen.

The durable record of what this packet builds is `DEC-0019` (accepted, unchanged by this
packet) and `low-level-design/elevated-instance.md` (written, unchanged by this packet), so
nothing is undocumented in the meantime.

Reason: the owning contracts describe user-reachable behaviour; this packet adds mechanism
behind a seam that no user-facing surface calls yet.

### Reconciliation

Before completion, confirm: `DEC-0019` and `low-level-design/elevated-instance.md` still
describe what was built (in particular the `SHELLEXECUTEINFOW` field list and the trusted
path table — if the implementation deviates, the detail design is wrong and gets fixed in
the same commit), and record here that `docs/terminal-backend.md` and
`docs/gui-layout.md` are deliberately left to `US-0132`.

## Context

- **OneTerm has no command line today.** `run()` (`crates/app/src/lib.rs:32-125`) never
  reads `std::env::args`; the only `std::env::args` in the repository are in the
  diagnostic binaries under `crates/tools/src/bin/`. This packet creates that surface.
- **There is no single-instance guard**: no `CreateMutex`, no named pipe, nothing. So M6 is
  free today and is recorded rather than implemented.
- **The `WorkspaceCommands` seam** (`crates/state/src/commands.rs:38-70`) is the
  fn-pointer registry `IN-0033` established, installed at `crates/app/src/init.rs:62-80`.
  `crates/terminal-view` must not perform an external effect itself, and
  `oneterm-session-ui` already depends on `oneterm-terminal-view`, so the reverse edge
  would be a cycle (R5).
- **`crates/core` has no `windows-sys` dependency** (`crates/app/Cargo.toml:67` and
  `crates/vt/Cargo.toml:57` are the only ones). Keeping the new `crates/core` module pure
  `std` keeps it that way, and keeps `parse` and `trusted_program` testable on the Linux
  and macOS CI runners.
- **Why the parameterized resolution signature.** `resolve_unix_shell`
  (`crates/core/src/config/shell.rs:203-222`) already takes its `lookup` and `exists` as
  parameters for exactly this reason; `trusted_program` copies that shape rather than
  inventing one.
- **A release build has no console.** `crates/app/src/bin/oneterm.rs:7-10` sets
  `windows_subsystem = "windows"` in release, so the command-line error has to be a
  `MessageBoxW`; `eprintln!` would go nowhere.

## Plan

- [ ] `crates/core/src/config/elevation.rs`: the enum, `CliError`, `parse`,
      `trusted_program`, the `initial_shell` global, and the unit tests. Land this first —
      it is the whole security argument and it needs no Windows to test.
- [ ] `crates/core/src/config/shell.rs`: the trusted-program override path through
      `resolve_shell`, keeping the kind's args and the integration environment.
- [ ] `crates/app/src/elevation.rs`: `fatal_message` (`MessageBoxW`) and
      `launch_elevated_shell` (`ShellExecuteExW`), with the error map.
- [ ] `Cargo.toml:125-135`: `Win32_UI_Shell`, `Win32_UI_WindowsAndMessaging`.
- [ ] `crates/app/src/lib.rs`: the two new first statements of `run()`; the parse failure
      path. (`set_elevated` is called here too, but its consumer arrives in `US-0131`.)
- [ ] `crates/state/src/commands.rs` + `crates/app/src/init.rs`: the fn pointer.
- [ ] `crates/terminal-view/src/panel/terminal_panel.rs`: `PanelSpec::DefaultShell` reads
      `initial_shell()`.
- [ ] Manual Windows run-through of the acceptance list; `pwsh scripts/ci-local.ps1`.

## Decisions

- `DEC-0019` — option A, rules 1/3/4, mitigations M3 and M6. Not repeated here.

## Verification Plan

Focused (`cargo test -p oneterm-core`, runs on all three CI runner OSes because the code
under test is pure):

- [ ] `parse` accepts each of the three tokens; `parse([])` is `Ok(None)`.
- [ ] `parse` rejects, one test per way in: unknown flag; flag with no value; `bash`;
      `zsh`; `sh`; `custom`; the flag twice; a bare positional; a trailing extra argument.
- [ ] `ElevatedShell::from_shell_kind` is `None` for `Bash`, `Zsh`, `Sh`, **`Custom`**.
- [ ] `token()` / `shell_kind()` round-trip for all three variants.
- [ ] `trusted_program` from injected roots: the two `%SystemRoot%` paths exactly; `Pwsh`
      picks the highest numeric directory from an injected listing (`7`, `8`, `preview` ->
      `8`), ignores non-numeric entries, and returns `Err(<the 7 path>)` on an empty
      listing. No test may pass by consulting the host `PATH`.
- [ ] `resolve_shell` with a trusted program override keeps the kind's own args and the
      integration environment, and ignores `LocalShellConfig::program` and `args`.

Unit / integration:

- [ ] `cargo test --workspace` — the `WorkspaceCommands` field forces every test double to
      be updated, which is the cheapest proof no seam was missed.

E2E (manual, Windows interactive desktop — the consent prompt is drawn by the AppInfo
service on the secure desktop and cannot be driven by this project's posted-message GUI
walks). Evidence in `evidence/`:

- [ ] **E10** from the detail design: `oneterm.exe --elevated-shell bash` and
      `oneterm.exe --wat`; screenshot each message box; exit code 2; no window.
- [ ] `oneterm.exe --elevated-shell pwsh` from an ordinary prompt opens PowerShell 7 as the
      one initial tab.
- [ ] From a temporary debug entry point calling `launch_elevated_shell`: the consent
      prompt appears naming `oneterm.exe`; **decline** it and screenshot that nothing
      happened (**E6**); accept it and confirm a second OneTerm window opens with that
      shell and `whoami /groups` reports `Mandatory Label\High Mandatory Level` (**E3**).
- [ ] Debug build, launched from a directory other than the repository root: both
      instances resolve `config_dir()` to the same `target/` — the `lpDirectory` proof.

Platform:

- [ ] `pwsh scripts/ci-local.ps1` green, including `cargo clippy --workspace --all-targets
      -- -D warnings` and the Linux/macOS compile of every `#[cfg(windows)]` seam.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### What was built

| Piece | File |
| --- | --- |
| `ElevatedShell`, `CliError`, `ELEVATED_SHELL_FLAG`, `parse`, `trusted_program`, `trusted_program_for`, `trusted_shell_config`, `window_title`, the `is_elevated` / `initial_shell` globals | `crates/core/src/config/elevation.rs` (new, pure `std`, 24 unit tests) |
| The M3 guard: one `if` at the top of `resolve_shell` that replaces the config with `trusted_shell_config`'s, so every local spawn in an elevated process takes its program from the trusted table and its args from the kind's own arm | `crates/core/src/config/shell.rs` |
| `process_is_elevated` (`GetTokenInformation(TokenElevation)`), `fatal_message` (`MessageBoxW`), `launch_elevated_shell` (`ShellExecuteExW`, `runas`, explicit `lpDirectory`, `SEE_MASK_FLAG_NO_UI`, no `SEE_MASK_NOCLOSEPROCESS`) | `crates/app/src/elevation.rs` (new) |
| `read_process_identity()` — the token, then the parse, as the first statement of `run()`; the "not elevated, opening anyway" `warn` once the logger exists | `crates/app/src/lib.rs` |
| `launch_elevated_shell` fn pointer + its two test doubles | `crates/state/src/commands.rs`, `crates/app/src/init.rs`, `crates/state/src/services.rs`, `crates/terminal-view/src/panel/tests.rs` |
| `PanelSpec::DefaultShell` reads `initial_shell()` — the argument *replaces* the default shell (`DEC-0016`) | `crates/terminal-view/src/panel/terminal_panel.rs` |
| `Win32_UI_Shell`, `Win32_UI_WindowsAndMessaging`, `Win32_System_Registry` | `Cargo.toml` |

### Commands

- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo test --workspace` — green; `oneterm-core` 75 tests (was 52), including the parser's
  ten rejection cases, the three trusted-path cases and the two `trusted_shell_config` ones.
- `python scripts/verify-dependency-graph.py` — *"Dependency graph policy passed for 20
  workspace packages and 20 explicit members"*. `crates/core` still has no `windows-sys`.
- `pwsh scripts/ci-local.ps1` — run at the end of `US-0132`, green (see that packet).

### E2E actually performed here (non-elevated side only)

- `oneterm.exe --elevated-shell zsh` → one message box naming the flag and its three accepted
  values, **exit code 2**, no window.
  `evidence/US-0130-bad-argument-message.png`.
- `oneterm.exe --elevated-shell cmd` from an **un**elevated prompt → one Command Prompt tab
  and nothing else added; title bar reads `OneTerm` with no marker and the ordinary border;
  the right dock and its mode toggles are present, i.e. fully unrestricted. One log line:
  *"--elevated-shell was given to a process that is not elevated: opening Command Prompt with
  no elevation and no restrictions"*. `evidence/US-0130-nonelevated-argument-opens-cmd.png`.
  This is the M5 proof from the other side: the argument cannot forge the marker.

### Deviations from the detail design, all reconciled into it in the same commit

1. `set_initial_shell` takes an `ElevatedShell`, not a `ShellKind`, so the global cannot hold
   `Custom` even by mistake. `initial_shell()` still returns `Option<ShellKind>`.
2. `Win32_System_Registry` joins the `windows-sys` feature list: windows-sys 0.59 gates the
   whole `SHELLEXECUTEINFOW` struct behind it because the struct carries an `HKEY` field. No
   registry key is read; pwsh is still resolved by scanning `%ProgramFiles%\PowerShell`.
3. **Exit 3 is checked only when the process is elevated.** A process that is not elevated is
   unrestricted by definition — it paid no consent prompt and resolves the shell the ordinary
   way — so refusing to open a window there would contradict *the token decides everything
   else*. Recorded in the detail design's exit-code table.
4. `trusted_shell_config` is a named function taking an injected resolver, rather than
   inline code in `resolve_shell`, so M3 has a unit test that needs neither Windows nor the
   process global.

### Reconciliation

`docs/terminal-backend.md` and `docs/gui-layout.md` were **deliberately left to `US-0132`**,
as this packet's Documentation Action says: nothing here is user-reachable until the menu
rows exist. `DEC-0019` is unchanged; `low-level-design/elevated-instance.md` was corrected
for the four deviations above in the implementation commit.

### Acceptance rework, 2026-09-21 — MAJ-1

Independent verification (`evidence/IN-0043-verify.md`) found this packet's own security
deliverable incomplete and marked it **FAIL**. Reworked rather than opened as a new BUG:
the behaviour was never accepted.

**What was wrong.** M3 dropped `program` and `args` and kept everything else, because the
trusted config was built with `..cfg.clone()`. Two fields of `terminal.json`'s shell block
still directed what the elevated process executed:

- `shell.env` — the ConPTY environment block writes custom entries **ahead** of the
  inherited ones, so `"env": { "PATH": "C:\\Users\\me\\bin" }` handed the elevated
  `cmd.exe` an attacker-chosen `PATH`; the first `net`, `sc`, `reg` or `icacls` the user
  typed would have run an attacker binary with a high-integrity token. For PowerShell the
  same field reaches `PSModulePath`.
- `shell.cwd` — and not even through the trusted config: `LocalSession::spawn` read
  `cfg.cwd` from the **original** config, so clearing it in `trusted_shell_config` would not
  have helped. `cmd.exe` searches the working directory before `PATH`.

A same-user process with no privilege writes one JSON file and waits for the user to open an
administrator shell. The user sees a correct UAC prompt for OneTerm, consents to OneTerm, and
gets an elevated shell whose command resolution someone else owns. That is the failure mode
option D was rejected for, reached by the back door.

**What changed.**

- `trusted_shell_config` builds the config **field by field**, deliberately not with
  struct-update syntax: `env` empty, `cwd` `None`, `args` empty, `program` trusted, and only
  `utf8` (the console codepage) crosses. The next field added to `LocalShellConfig` now
  fails the build until somebody decides which side of the boundary it is on.
- `ResolvedShell` gained `cwd`, and `LocalSession::spawn` takes it from there. Everything a
  spawn needs now comes out of `resolve_shell` — which is what makes the single guard at the
  top of that function complete, instead of a guard a sibling field walks around.
- `is_elevated()` became `is_restricted()` everywhere (see `US-0131`, MIN-3).

**Test:** `config::elevation::tests::an_elevated_config_keeps_no_field_of_the_configured_one`
(`oneterm-core`). It asserts *emptiness*, not equality, on every field. Mutation-checked:
restoring `env: cfg.env.clone()` fails it with *"no configured environment entry may survive:
PATH and PSModulePath decide what runs"*.

### Acceptance rework, 2026-09-21 (second) — the click crashed the app

The owner ran the branch and clicked `Run as administrator › PowerShell`. OneTerm logged
ten `ERROR RefCell already borrowed` lines inside a second and then aborted with
`0xc0000409` at
`gpui-pre-0.3.3/src/app/async_context.rs:65:27: RefCell already borrowed`, **at the moment
the UAC prompt appeared**. Reopened rather than filed as a new BUG: the packet was never
accepted.

**The backtrace, which names the whole mechanism** (frames as reported):

```text
48-61  App::update                      <- the mouse-up dispatch: the App RefCell is
                                           borrowed for the whole click
  42   terminal_panel.rs:779            <- the menu row's on_click
  41   elevation::launch_elevated_shell (elevation.rs:133)
  40   elevation::launch               (elevation.rs:214)
  39   ShellExecuteExW
  38   CFileOperationRecorder_CreateInstance
  37   SetAppStartingCursor
  36   RealShellExecuteW
  35   SHChangeNotifyRegister
  34   IsWindowUnicode
  33   CallWindowProcW                  <- the shell pumps the calling thread's messages
  32   gpui_windows::platform::window_procedure
  26   handle_msg
  25   handle_gpui_events
  24   run_foreground_task              <- gpui runs a queued foreground task, re-entrantly
  17   gpui::app::context::spawn
  16   oneterm_terminal_view::terminal_view::view::new  (view.rs:249)
  15   WeakEntity::update
  14   AsyncApp::update_entity          (async_context.rs:65)  -> borrow_mut() -> PANIC
```

**Cause.** `ShellExecuteExW` is modal while the AppInfo consent request is outstanding, and
it pumps the calling thread's message queue to stay responsive — frames 39→33 are the shell
doing exactly that. The call was made synchronously from the menu's `on_click`, which gpui
invokes from inside `App::update` with the `App` `RefCell` **already mutably borrowed**
(frames 48–61). So the pump re-entered OneTerm's own window procedure (frame 32), gpui
dispatched a queued foreground task (frame 24), and that task's `AsyncApp::update_entity`
asked for a second mutable borrow (frame 14) and aborted the process. The ten `ERROR` lines
before it are the same re-entrancy hitting gpui's non-fatal borrow diagnostics once per
pumped message.

Nothing about elevation is special here. Any blocking Win32 call that pumps messages, made
from a gpui callback, does this. What made it certain rather than occasional is that the
consent prompt takes hundreds of milliseconds, so there is always a queued task to hit — in
the owner's trace, a terminal view's own start-up closure.

**The fix, at the root.** `ShellExecuteExW` now runs on a thread of its own and never
touches gpui:

- `ElevationRequest` owns everything the call needs (`current_exe`, the
  `--elevated-shell <token>` string, `current_dir`). Built on the UI thread — it only reads
  process state and formats a string, neither of which pumps — so a failure is still
  reported without a round trip, and the construction of what crosses the trust boundary
  stays next to the security argument for it.
- `ElevationRequest::execute` runs on `std::thread::Builder::new().name("oneterm-elevate")`,
  initialises **its own** COM apartment (`CoInitializeEx`, `COINIT_APARTMENTTHREADED` —
  `ShellExecuteEx` delegates to COM shell extensions) and uninitialises it before returning.
  `fMask` gains `SEE_MASK_NOASYNC` beside `SEE_MASK_FLAG_NO_UI`: that thread has no message
  loop and exits as soon as the call returns, so the operation must complete before it does.
  `runas`, `lpDirectory` and the absence of `SEE_MASK_NOCLOSEPROCESS` are unchanged — the
  security shape of the call did not move, only the thread it runs on.
- The outcome comes back over an `async_channel` and is consumed by a plain foreground
  `window.spawn(cx, ..)`; the notification is pushed from inside that task's `cx.update`.
  **The menu handler returns immediately after spawning**, so the `App` borrow from the
  click is released long before the consent prompt appears.
- `LaunchOutcome` gained `Failed(String)` so "what happened" is one value, and the two pure
  halves — `outcome_of(started, last_error)` and `notification_for(&outcome)` — are
  testable without launching anything.

**Re-entrancy audit of the rest of the path.** `commands(cx)` at the call site reads a
global and returns; `notify` / `push_notification` build and enqueue; `ElevationRequest::build`
does `current_exe`, `current_dir` and a `format!`. None pumps messages. `fatal_message`'s
`MessageBoxW` **is** modal and pumping, and is fine: it runs in `read_process_identity()`,
the first statement of `run()`, before gpui exists at all — there is no `App` to borrow. The
trusted-path resolution's `read_dir` / `is_file` block but do not pump. `ShellExecuteExW` was
the only message-pumping call reachable from a gpui callback.

**The guard against a refactor undoing this.** `run()` records the gpui thread's id
(`remember_ui_thread()`), and `execute()` opens with
`debug_assert_ne!(current().id(), UI_THREAD)`. A future change that "simplifies" the launch
back onto the calling thread trips it in every dev build, on the first click, with the
reason in the message. That is the cheap structural check the rework asked for; the
alternative — asserting "no `App` borrow is held" — is not expressible, because **every**
gpui callback holds one. The borrow was never the bug; a pumping call underneath it was.

**Verification.**

- `cargo test -p oneterm-app -p oneterm-core` — green. New:
  `elevation::tests::a_declined_prompt_says_nothing_and_a_failure_says_why` (the outcome
  mapping: `ERROR_CANCELLED` → silent, anything else → one notification naming the code) and
  `elevation::tests::error_cancelled_matches_windows` (the cross-platform constant against
  `windows_sys`' own).
- **Threading probe, with a benign verb.** A temporary test drove the same mechanism — worker
  thread, `CoInitializeEx`, `SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI`, `async_channel` hop —
  with verb **`open`** on `cmd.exe /c exit`, never `runas`, so no consent prompt was raised:

  ```text
  PROBE: ui=ThreadId(2) worker=ThreadId(3) outcome=Started
  test elevation::tests::probe_worker_thread_and_channel_hop ... ok
  ```

  The FFI ran on a different thread from the caller and the outcome came back through the
  channel. Probe removed afterwards.
- **Not verified here:** that the real `runas` path no longer crashes. That needs a consent
  prompt, which this session must not raise. `US-0131`'s checklist **step 0** is the
  closing evidence.

### Acceptance rework, 2026-09-21 (third) — a console window came with the elevated window

Owner acceptance: **step 0 passed** — the click no longer logs `RefCell already borrowed`
and the launching window stays alive, so the re-entrancy fix is accepted. The elevated
window then opened **with a separate console window beside it**.

**Cause, confirmed by reading.** `crates/app/src/bin/oneterm.rs:7-10` sets
`windows_subsystem = "windows"` only under `not(debug_assertions)`:

```rust
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]
```

So a **release** build is a GUI-subsystem binary and gets no console at all — nothing to fix
there, and the packet says so rather than leaving it to be assumed. A **debug or `fast-dev`**
build is a console-subsystem binary, which is deliberate (it is how a developer reads
`log::info!` output). Windows allocates a console for such a process when it has none to
inherit, and a process started through `runas` never inherits one: the launcher's console
belongs to a different integrity level. Hence a fresh, visible console for the elevated
instance, every time, in exactly the build the owner runs.

**`nShow = SW_HIDE` was tried first, and does not work.** It was the cheap fix, and it hides
the wrong window as well. Probed against the real `fast-dev` binary with verb **`open`**
(never `runas`, so no consent prompt), enumerating the launched process's own top-level
windows:

```text
nShow = SW_HIDE (0):
  ShellExecuteExW ok=True lastError=0
  hidden  cls=Zed::Window        title='OneTerm'          <- the app window, hidden
  hidden  cls=ConsoleWindowClass title='...\oneterm.exe'

nShow = SW_SHOWNORMAL (1):
  ShellExecuteExW ok=True lastError=0
  VISIBLE cls=Zed::Window        title='OneTerm'
```

`nShow` becomes the process's default show command, and gpui's window inherits it — the
elevated window would simply never appear. `SW_SHOWNORMAL` stays.

**The fix: the elevated process disowns its console.** In `run()`, before logging is
initialised and only when `is_restricted()`:

- `GetConsoleWindow()` is null in a release build (GUI subsystem, no console) — nothing to
  do, so the code is a no-op there rather than being `cfg`-gated on the build profile.
- Otherwise `GetConsoleProcessList` says how many processes are attached. **One** means the
  console was allocated for this process alone, which is the `runas` case — `FreeConsole()`
  disowns it and the window goes with it. **More than one** means the console was inherited
  from whoever started us, most likely an administrator prompt running
  `oneterm.exe --elevated-shell cmd` by hand; that window is not ours to close, no
  unexpected window appeared, and the log stays where the person who started it can read it.

That distinction is the whole reason this is six lines rather than one. `FreeConsole()`
unconditionally would detach from a console the user opened themselves and silently throw
away the output they started the process to see.

**What the elevated instance's logging costs.** After `FreeConsole` its `stderr` goes
nowhere, so the `runas`-launched instance has no live log. It is **not** redirected to a
file: a log under the configuration directory is a write, and M4's promise is that an
elevated window leaves nothing behind — under over-the-shoulder elevation, nothing in the
administrator's profile — which the manual checklist's step 12 checks literally ("the only
new path may be `crashes\elevated\`"). Diagnosing an elevated instance therefore goes
through the crash store, which M7 already keeps in `crashes/elevated/`, or by starting it
from an administrator console by hand, which is the case the process-count check
deliberately preserves.

### Acceptance rework, 2026-09-22 (fourth) — the push turned two CI jobs red

Both failures are the same root cause wearing two hats: **this packet's code was written on a
Windows host and reasoned about as though Windows were the only host**, while `crates/core`'s
half exists precisely so the security argument is verified on the Linux and macOS runners too.

**1. "Full workspace quality gate" (ubuntu) — `-D warnings`, dead code in
`crates/app/src/elevation.rs`:**

```
error: enum `ConsoleAction` is never used                     (elevation.rs:181)
error: function `console_action` is never used                (elevation.rs:194)
error: constant `ERROR_CANCELLED_CODE` is never used          (elevation.rs:237)
error: variants `Started` and `Declined` are never constructed (elevation.rs:242-244)
error: function `outcome_of` is never used                    (elevation.rs:251)
error: fields `executable`, `parameters` and `directory` are never read (elevation.rs:283-292)
```

**2. "Backend tests" (macOS) — six tests in `crates/core/src/config/elevation.rs`:**

```
cmd_resolves_under_system32_and_never_through_comspec        (:669)
windows_powershell_resolves_under_the_v1_0_directory         (:678)
a_missing_system_shell_reports_the_path_it_looked_for        (:690)
pwsh_takes_the_highest_numeric_directory                     (:698)
pwsh_skips_a_numeric_directory_that_holds_no_executable      (:713)
pwsh_with_no_install_reports_the_version_7_path              (:725)

left:  Err("X:\\Windows/System32/cmd.exe")
right: Err("X:\\Windows\\System32\\cmd.exe")
```

**Cause (1): the platform split was drawn one level too high.** `console_action`,
`outcome_of`, `LaunchOutcome` and `ElevationRequest`'s fields are pure decisions with no Win32
in them, but each was reached *only* from inside a `#[cfg(windows)]` body — `release_own_console`
and `ElevationRequest::execute` were each written twice, once per platform, with the whole
decision inside the Windows copy. Off Windows the decisions were therefore unreachable, which is
exactly what `dead_code` says. They were not unused: they were unused **on two of the three OSes
this code is compiled for**, and the tests that prove the rules ran only on the third.

**Fix (1): the `cfg` moved down onto the FFI calls, and nothing else** — no `#[allow(dead_code)]`,
which would have preserved the mistake and silenced the report of it.

| Was | Is |
| --- | --- |
| `release_own_console` written twice: a no-op, and a Windows copy holding the queries, the decision and `FreeConsole` | one `release_own_console` — `console_owners()` (`cfg`, the two Win32 queries / `(false, 0)`), then `console_action`, then `free_console()` (`cfg`, `FreeConsole` / unreachable no-op) |
| `ElevationRequest::execute` written twice: a `Failed("…is a Windows feature")` stub, and a Windows copy holding the strings, the call and the mapping | one `execute` — the debug assertion, then `request_elevation(&executable, &parameters, &directory)` (`cfg`, `ShellExecuteExW` / `(false, 50)`), then `outcome_of` |

So `ConsoleAction`, `console_action`, `LaunchOutcome`, `ERROR_CANCELLED_CODE`, `outcome_of` and
all three `ElevationRequest` fields are now read on every OS, and their tests
(`only_a_console_of_our_own_is_released`, `a_declined_prompt_says_nothing_and_a_failure_says_why`,
`a_second_click_is_ignored_while_a_prompt_is_outstanding`, plus a new
`the_request_carries_the_flag_and_one_token` asserting `--elevated-shell pwsh` and an absolute
executable) run on all three runners rather than on Windows alone. The module header now states
where the split is and that an `#[allow(dead_code)]` there would mean it is in the wrong place.

**Verified without a Linux toolchain** (only the msvc target is installed here): the module's
`#[cfg(windows)]` / `#[cfg(not(windows))]` attributes were temporarily rewritten to
`#[cfg(all(windows, not(fakeunix)))]` / `#[cfg(any(not(windows), fakeunix))]` and the crate
compiled with `cargo rustc -p oneterm-app --lib -- --cfg fakeunix`, i.e. the non-Windows half of
this file against a Windows toolchain. Result: the non-Windows arms type-check and the build
produces **no `dead_code` diagnostic at all** (ten `unexpected cfg` warnings for `fakeunix`, and
nothing else). The attributes were restored immediately afterwards.

**Cause (2): `std::path` is host-flavoured.** `trusted_program` built its three paths with
`PathBuf::join`, which separates with the *host's* separator — `/` on Linux and macOS. The
trusted paths are Windows paths by definition (they are resolved in the elevated Windows process
and executed nowhere else), so on the Unix runners the function produced
`X:\Windows/System32/cmd.exe`: a string that is neither a Windows path nor what the tests assert.
**The tests were right and the function was wrong on two of the three OSes** — the same lesson as
`US-0114`'s 2026-09-18 rework, where `Path::file_stem` read a Windows program path as one
component on Linux.

**Fix (2): build them as strings.** A private `win_join(&[&str]) -> String` joins with `\`
explicitly (and trims a trailing `\` off each part, so a `SystemRoot` of `X:\` does not double
it). `trusted_program`, `trusted_program_for` and `trusted_shell_config`'s injected `resolve` are
now `String`-based end to end — `versions: impl Fn(&str) -> Vec<String>`,
`exists: impl Fn(&str) -> bool`, `Result<String, String>` — so the seams cannot reintroduce a
host-flavoured join either. The one `PathBuf` left is where `LocalShellConfig::program` needs one.
The six tests keep their expected values unchanged and now hold on every OS by construction; a
new `windows_components_are_joined_with_a_backslash_on_every_os` pins `win_join` itself. None
were `cfg(windows)`-ed away: the resolution rules are the M3 argument, and the crate split exists
so they are verified on the Linux and macOS runners.

**The rule, now in `docs/agents/code-style.md`:** *Windows-shaped strings are never built with
`std::path`* — `Path`/`PathBuf` use the host's separator and the host's component rules, so a
Windows path constructed or split on a Linux or macOS runner is wrong; build and read it as a
string with `\` (and `/`) spelled out.

**Checks.** `cargo test -p oneterm-core -p oneterm-app` — green (`oneterm-core` lib 80,
`oneterm-app` lib 26). `cargo clippy --workspace --all-targets -- -D warnings` — clean.
`pwsh scripts/ci-local.ps1` — *"ci-local: all checks passed"*.

### Gaps

- **The elevated side is unverified in this environment.** The consent prompt is drawn by the
  AppInfo service on the secure desktop, which this project's posted-message GUI walks cannot
  reach, and this session must not raise a UAC prompt at all. E3, E6 and the accept path of
  the launch are therefore **not run**: they are the owner's manual checklist in `US-0131`.
- Over-the-shoulder elevation needs a second account; none is available here, so it was **not
  exercised**.
- `cfg(unix)` behaviour is compile-and-unit-tested only; no Linux or macOS desktop run is
  available in this environment. The 2026-09-22 rework's non-Windows compile is a
  **simulation** — the module's `cfg` attributes flipped by a `--cfg fakeunix` predicate and
  built against the msvc toolchain, which proves the non-Windows arms type-check and are
  reachable, not that the crate links on a Unix host. Only the CI runners prove that.
- `launch_elevated_shell`'s own error map (`ERROR_CANCELLED` vs everything else) is
  **unexercised at run time**: reaching it needs a real `ShellExecuteExW` failure. The
  mapping is now unit-tested as pure data, but no real failure has been observed.
- **The crash fix is unproven against `runas`.** The mechanism is verified (probe above,
  benign verb) and the cause is confirmed by the owner's backtrace, but nobody has clicked
  the row since the fix — that raises a consent prompt. Checklist step 0 is what proves it.

## Handoff

Next: `US-0131` (the elevated instance mode). It depends on `set_elevated` and on there
being an elevated process to look at, both of which this packet provides.
