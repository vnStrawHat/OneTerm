# DEC-0019 Elevated shells open in an elevated window

Date: 2026-09-21

## Status

**Accepted 2026-09-21 by the owner (option A with M1-M7).**

The owner reviewed the four options and selected **A**: a "Run as administrator" entry per
Windows shell in the "+" menu launches a **new elevated OneTerm window** through
`ShellExecuteExW` with the `runas` verb on `current_exe()`, carrying an explicit argument
naming the shell to open. All seven mitigations proposed with the option were accepted and
are inherited rules; they are listed in [Inherited rules (M1-M7)](#inherited-rules-m1-m7)
below.

The detail design required by the high-risk lane is
`docs/spec-intakes/IN-0043-run-shell-as-administrator/low-level-design/elevated-instance.md`.
Work packets `US-0130`, `US-0131` and `US-0132` implement it, in that order.

## Context

The owner asked (2026-09-21) that the "+" (New Terminal) menu be able to run the Windows
shells as administrator.

Windows does not allow the obvious implementation. OneTerm attaches its shell to a
pseudo-console it creates itself, and the attachment happens through exactly one channel:
`UpdateProcThreadAttribute(..., PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, ...)` on the
`STARTUPINFOEXW` handed to `CreateProcessW`
(`crates/vt/src/pty/windows/conpty.rs:263-300`). `CreateProcessW` creates the child with a
copy of the caller's token and has no flag that means "elevate"; a process also cannot
raise its own token. The supported path from an unelevated process is `ShellExecuteEx`
with the `runas` verb, which routes through the AppInfo service and the UAC consent UI —
and `SHELLEXECUTEINFOW` carries no attribute list, no `STARTUPINFOEX` and no environment
block.

So an elevated child cannot join a pseudo-console this process owns, and it cannot receive
the environment that carries OneTerm's entire shell integration (`PROMPT`,
`PROMPT_COMMAND`, `PS1`, `TERM` — `crates/core/src/config/shell.rs:338-365`). The full
argument, with citations, is `docs/spec-intakes/IN-0043-run-shell-as-administrator/research/windows-elevation-and-conpty.md`.

Two further facts about this repository shape the choice. OneTerm **has no command line**
(`crates/app/src/lib.rs:32-125` never reads `std::env::args`) and **no single-instance
guard**. Both would be new, and the second is security-relevant rather than cosmetic.

Elevation is a property of a process. Since a OneTerm window belongs to exactly one
process, it is a property of a window. The decision is what to do with that fact.

## Decision

Future work inherits four rules.

1. **An elevated shell runs in a separate, elevated OneTerm process and therefore in its
   own window.** The "+" menu's elevated rows relaunch `oneterm.exe` through
   `ShellExecuteExW` with the `runas` verb, passing `std::env::current_exe()` as the file,
   an explicit working directory, and an argument naming the shell to open. The new
   process opens that shell itself, so `resolve_shell` runs under the elevated token and
   the shell integration is intact.

2. **Elevation is always visible, and the marker is derived from the process token.** An
   elevated window is marked in the OS window title
   (`crates/app/src/window.rs:66`), in the in-app title bar
   (`crates/workspace/src/layout/workspace/mod.rs:262`) and inside the window. The marker
   is read from the process's own token at start-up — never from the launch argument — so
   a window elevated by any route says so, and no window can be made to claim an elevation
   it does not have. Colour alone is not a marker: themes are user-editable.

3. **An elevation request is served by the process that received it.** If OneTerm ever
   gains single-instance coalescing, an "open this shell" argument carrying an elevation
   request must never be forwarded to an existing instance. Forwarding would answer an
   elevation request without elevating, which is a privilege bug wearing a UX bug's
   clothes.

4. **No channel is opened from the unelevated process into the elevated one, and nothing
   the unelevated process writes may direct what the elevated process executes.** The two
   processes share the system clipboard and, when the elevated token belongs to the same
   account, the config directory — nothing else. `terminal.json` holds
   `LocalShellConfig::program` and `args` (`crates/core/src/config/shell.rs:68-81`), so
   this rule has to be defended by the detail design, not assumed.

### Inherited rules (M1-M7)

The seven mitigations the owner accepted with option A. They are not advice: each one is a
constraint future work inherits, and each is a seam the detail design names with a file and
line.

- **M1 — the elevated instance is local shells only.** No SSH Sessions panel, no SFTP, no
  Quick Connect and no New Saved Session rows, no Agent panel. The right dock is `None` and
  its title-bar mode toggles are absent, not disabled. The "+" menu lists only the Windows
  shells, and **no "Run as administrator" rows again**: an elevated window cannot spawn a
  second identical one. The elevated window answers the request that was made and offers no
  surface that nothing about elevation requires — the smaller the elevated application, the
  smaller the tradeoff below.
- **M2 — the elevated instance never checks, downloads or installs updates.** Its
  About/Updates surface says so in one line and offers no control. An elevated updater can
  write `C:\Program Files` and can leave files whose owner or ACL the ordinary instance
  cannot replace, which would silently change the install for the non-elevated window too.
  This settles the follow-up the first draft of this decision left open.
- **M3 — the elevated instance does not read `terminal.json`'s `program` / `args` to decide
  what to run.** The shell arrives as a value from a closed three-token enum
  (`--elevated-shell cmd|powershell|pwsh`) and OneTerm resolves it itself to an absolute
  path it trusts — `%SystemRoot%\System32\cmd.exe`,
  `%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe`, and pwsh from its
  install location under `%ProgramFiles%` — **never** through `PATH`, `COMSPEC` or user
  configuration. `ShellKind::Custom` cannot be elevated at all. An unknown or missing
  argument shows one message and exits without opening a window. This is rule 4 made
  concrete: `terminal.json` is a file the unelevated process can write, so the elevated
  process must not take instructions from it.
- **M4 — the elevated instance reads configuration and writes none of it.** Theme, font and
  key bindings are read; `docks.json`, `ui_config.json`, `ssh_session.json`, `terminal.json`
  and `update_config.json` are never written. One writer for those documents remains the
  unelevated instance, so the "two concurrent writers" hazard this feature would otherwise
  make routine does not arise, and an over-the-shoulder elevation leaves no trace in the
  administrator's profile.
- **M5 — the elevation marker derives from the process token, never from the argument.**
  `GetTokenInformation(TokenElevation)` is the only source. The title reads
  `OneTerm (Administrator)` in both the OS title bar and the in-app title bar, and the
  title bar carries a distinct background or border from a theme token. Stated as one
  sentence: *the argument selects the shell, the token decides everything else.* A window
  elevated by any route says so; no window can be made to claim an elevation it does not
  have.
- **M6 — no single-instance forwarding.** Every elevation request is served by the process
  that received it. OneTerm has no instance coalescing today, so this is free now and stops
  being free the moment someone adds it: forwarding an elevation request to an existing
  unelevated instance would answer it without elevating.
- **M7 — the elevated instance's crash reports go to `crashes/elevated/`, and the elevated
  window never shows the GitHub-draft dialog.** A separate subdirectory keeps a report
  written under an administrator token from wedging the ordinary instance's promotion and
  retention pass; suppressing the dialog keeps an elevated window from being a route to a
  browser and a prefilled issue.

Failure paths accepted with the mitigations: a declined UAC prompt (`ERROR_CANCELLED`,
1223) is a **silent no-op**; any other `runas` failure produces **one** notification; an
elevated instance that cannot read configuration falls back to defaults and **says so
once**. `lpDirectory` must be set explicitly, because debug builds resolve `config_dir()`
relative to the working directory (`crates/core/src/config/shell.rs:109-124`) and the
`runas` verb does not inherit the caller's.

## Alternatives

- [x] **A — a new elevated OneTerm window (selected above).** Answers the request with a
      real OneTerm tab: theme, font, keybindings, search, scrollback, shell integration,
      all of it. Costs a command line, a single-instance rule, an elevation marker and
      documentation. Matches Windows Terminal's shipped `elevate` profile setting, which
      opens an elevated profile in a new elevated window rather than as a tab in the
      current one.
- [ ] **B — the row runs the shell in an external console window.** `runas` on the shell
      program itself, no pseudo-console. One function and one menu row; nothing in OneTerm
      becomes elevated. Not selected because what opens is a plain `conhost` window with
      none of OneTerm's terminal, and the `runas` verb carries no environment block, so
      OSC 7 cwd reporting and OSC 133 prompt marks are lost. **Kept as the named
      fallback**: if the owner judges A's cost too high, B is a decision, not a rewrite.
- [ ] **C — restart the whole application elevated.** One row, `runas` on the own
      executable, then quit. Not selected: it destroys every open tab, every live SSH
      connection and every in-flight SFTP transfer, and OneTerm cannot hand those to
      another process. It also makes the elevated state sticky and easy to forget, which
      is exactly the state the consequences below say to avoid.
- [ ] **D — an elevated helper process feeds a ConPTY into this window.** A high-integrity
      helper owns the pseudo-console and relays bytes over a named pipe whose label admits
      medium integrity. Mechanically this is probably possible — the higher side may
      always weaken its own objects, and UIPI does not apply because no window is
      involved. Not selected on security, not on feasibility: it turns every
      medium-integrity weakness in a large GUI application into arbitrary elevated
      execution, reachable by any process running as the same user with no privilege at
      all. It would need an authenticated protocol, argument validation, lifetime
      management, code signing and a security review per release — a standing elevation
      service this project would own forever — to buy "the elevated tab is in this window
      instead of the next one".

## Consequences

- [ ] **Benefit to confirm:** a user can open an administrator shell from the "+" menu and
      get a real OneTerm terminal, while the window they were already working in keeps its
      tabs, connections and transfers unelevated.
- [ ] **Benefit to confirm:** nobody can be confused about what they are typing into. Every
      elevated window is marked, and the marker cannot lie because it comes from the token.
- [ ] **Tradeoff:** an elevated OneTerm runs *everything* it runs with an administrator
      token — that is what per-process elevation means, and no design can undo it. M1, M2
      and M4 shrink the surface instead: SSH, SFTP, the session store, the Agent panel and
      the updater are all absent from the elevated instance, and it writes no
      configuration. What remains elevated is the local shell the user asked for, which is
      the point. The shrinking must be stated in `docs/gui-layout.md`,
      `docs/terminal-backend.md`, `docs/auto-update.md` and `docs/crash-reporting.md`, so a
      user is not surprised that the elevated window is a smaller application.
- [ ] **Tradeoff:** drag-and-drop upload from Explorer does not work in any elevated
      window. `crates/sftp-ui/src/render.rs:436-451` accepts `ExternalPaths` drops;
      Explorer is medium integrity and UIPI forbids the drop onto a high-integrity window.
      Inherent, not fixable, must be documented. Under M1 there is no SFTP panel in the
      elevated instance to drop onto, so this bites only a OneTerm the user elevated by
      hand.
- [ ] **Tradeoff:** under over-the-shoulder elevation (a standard user typing an
      administrator's credentials) the elevated process runs as that other account, so
      `USERPROFILE` and therefore `~/.OneTerm` differ
      (`crates/core/src/config/shell.rs:91-96`, `:109-124`). The elevated window then has
      no theme, no font override and no key-binding overrides, and its own crash store. It
      starts on the defaults and **says so once** (accepted with the mitigations); under M4
      it writes nothing into that other profile, so it leaves no trace there.
- [ ] **Follow-up:** OneTerm gains a public command line where it had none. The argument
      vocabulary is new public surface for the binary and is the first thing a future
      single-instance feature will collide with — rule 3 / M6 exists for that collision.
- [x] **Settled by M2:** the in-app updater does not run from an elevated window at all.
      The About/Updates surface there says updates are done from the normal window.
