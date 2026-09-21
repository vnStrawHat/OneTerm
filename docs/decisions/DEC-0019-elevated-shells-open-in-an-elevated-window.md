# DEC-0019 Elevated shells open in an elevated window

Date: 2026-09-21

## Status

**Proposed** (draft for the owner, `IN-0043`). Nothing may be implemented against it until
it is accepted.

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
- [ ] **Tradeoff:** an elevated OneTerm runs *everything* elevated. SSH and SFTP, key and
      `known_hosts` handling, config writes and the auto-updater all run with an
      administrator token in that window. Nothing about the feature needs that; it is what
      per-process elevation means. It must be stated in `docs/gui-layout.md`,
      `docs/terminal-backend.md`, `docs/auto-update.md` and `docs/crash-reporting.md`.
- [ ] **Tradeoff:** drag-and-drop upload from Explorer does not work in the elevated
      window. `crates/sftp-ui/src/render.rs:436-451` accepts `ExternalPaths` drops;
      Explorer is medium integrity and UIPI forbids the drop onto a high-integrity window.
      Inherent, not fixable, must be documented.
- [ ] **Tradeoff:** under over-the-shoulder elevation (a standard user typing an
      administrator's credentials) the elevated process runs as that other account, so
      `USERPROFILE` and therefore `~/.OneTerm` differ
      (`crates/core/src/config/shell.rs:91-96`, `:109-124`). The elevated window then has
      no saved sessions, no theme, no dock layout and its own crash store. Whether that is
      explained in place or only documented is an open question in `IN-0043`.
- [ ] **Follow-up:** OneTerm gains a public command line where it had none. The argument
      vocabulary is new public surface for the binary and is the first thing a future
      single-instance feature will collide with — rule 3 exists for that collision.
- [ ] **Follow-up:** decide whether the in-app updater should refuse to run from an
      elevated window. An elevated update can write `C:\Program Files` and can leave files
      the ordinary instance cannot replace.
