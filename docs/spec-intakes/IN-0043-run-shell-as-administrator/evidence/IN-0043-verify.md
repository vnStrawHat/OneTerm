# Independent verification — US-0130, US-0131, US-0132 (IN-0043)

Verifier: an independent session, adversarial and security-minded, with no part in the
implementation. Target: `feat/elevated-shell` @ `0ac3f8dc`, six commits on `main`
@ `590dd23e`. Read: `AGENTS.md`, `docs/HARNESS.md`, the whole intake folder
(`IN-0043.md`, `high-level-design.md`, `research/windows-elevation-and-conpty.md`,
`low-level-design/elevated-instance.md`, the three packets),
`docs/decisions/DEC-0019-elevated-shells-open-in-an-elevated-window.md`, and
`git diff main...HEAD` in full.

**The elevated side was not run.** The consent prompt is drawn by the AppInfo service on
the secure desktop; this session is forbidden to raise one. Everything below about an
elevated process is derived from reading, from unit tests, and from one **probe** that sets
the process global in a test binary and exercises the real code path. Where a claim rests
only on reading, it says so.

## Verdicts

| Packet | Verdict |
| --- | --- |
| **US-0130** — the launcher and the command line | **FAIL** — the launcher, the parser and the trusted-path table are all correct and well tested; the M3 guard this packet owns is not complete (MAJ-1), and M3 is the packet's own security deliverable |
| **US-0131** — the elevated instance mode | **FAIL** (MAJ-2, MAJ-3, MAJ-4: M1 and M4 are each breached on a default path) |
| **US-0132** — the menu rows and the docs | **PASS** (the rows and the docs are correct for what they claim; `docs/gui-layout.md`'s right-dock sentence inherits MAJ-2) |
| **Overall (IN-0043)** | **FAIL — do not accept.** Four majors, all of them holes in the two mitigations the decision exists to enforce (M3/rule 4, M1/M4). Three are exploitable as medium-integrity → high-integrity privilege escalation by a same-user process. |

The design is sound and the code is careful. What is wrong is not the mechanism but its
**coverage**: `DEC-0019` rule 4 is defended against `LocalShellConfig::program` and `args`
and against nothing else in the same document, and M1/M4 are defended against the paths
that *build* or *write* and not against the path that *restores*.

---

## 1. Trust boundary (DEC-0019 rule 4 / M3)

Every input the elevated process consumes that the unelevated process — or any other
process running as the same user — can write.

Threat model used: **split-token elevation**, the common case the LLD names
(`low-level-design/elevated-instance.md` § Edge Cases). Same SID, same `USERPROFILE`, therefore the
same `~/.OneTerm`. A medium-integrity process running as the user writes those files; the
high-integrity process reads them. That is exactly the channel rule 4 forbids.

| Input | Reached by the unelevated side? | Read by the elevated process? | Acted on? | Elevated branch tested? |
| --- | --- | --- | --- | --- |
| Command line (`argv`) | yes — it is the whole channel | yes, `crates/app/src/lib.rs:53` | closed 3-value enum, no tolerant branch (`crates/core/src/config/elevation.rs:109-134`) | **yes**, 10 rejection tests + `a_custom_shell_token_is_rejected` |
| Process environment block | **no** — `SHELLEXECUTEINFOW` carries none; AppInfo builds it from the elevated token's profile | `%SystemRoot%`, `%ProgramFiles%` at `elevation.rs:213-217` | trusted-path resolution | resolution tested with injected roots; the *absence of the channel* is a Win32 fact, read not run |
| `lpDirectory` → process cwd | **yes** — the launcher passes `current_dir()` (`crates/app/src/elevation.rs:166-169`); any same-user process can call `ShellExecuteExW` itself with any directory | **debug/fast-dev only**: `config_dir()` is the relative `target/` (`crates/core/src/config/shell.rs:109-112`) | selects the whole configuration root | no — see MIN-1 |
| `terminal.json` → `shell.program` | yes | yes | **dropped** (`elevation.rs:256`) | **yes** (`an_elevated_config_takes_the_trusted_program_and_drops_the_configured_one`) |
| `terminal.json` → `shell.args` | yes | yes | **dropped** (`elevation.rs:257`, and `shell.rs:381` then extends an empty vec) | **yes**, same test |
| `terminal.json` → `shell.env` | yes | yes | **APPLIED** — `..cfg.clone()` keeps it (`elevation.rs:258`), `shell.rs:251` merges it over `base_env()`, and the ConPTY block puts custom entries **before** the parent so they win (`crates/vt/src/pty/windows/conpty.rs:437-450`) | **no — MAJ-1** |
| `terminal.json` → `shell.cwd` | yes | yes | **APPLIED** — and not even through the trusted config: `crates/local-shell/src/session.rs:53` reads `cfg.cwd` from the **original** config, so clearing it in `trusted_shell_config` would not help | **no — MAJ-1** |
| `terminal.json` → `logging.{local,directory,write_mode}` | yes | yes | **APPLIED** — `crates/terminal-view/src/panel/terminal_panel.rs:364` → `LocalSession::spawn(.., logging)`; `LogWriteMode::Overwrite` truncates | **no — MAJ-4** |
| `terminal.json` → theme name, font family/size, scrollback, OSC security policy | yes | yes | presentation and policy only; fonts are family **names**, never paths (`crates/settings/src/terminal_config/font.rs:9-25`); themes are embedded, no user theme file is read (`crates/theme/src/theme.rs:28,89`) | n/a — no code-execution surface |
| `terminal.json` (corrupt) | yes | yes | **quarantine renames the file** (`crates/settings/src/terminal_config/document.rs:146`) — outside every M4 guard | no — MIN-2 |
| `ui_config.json` | yes | yes (`crates/settings/src/ui_config.rs:93`) | theme, `right_dock_mode`, key-binding overrides. Bindings map to a closed `BINDABLE_ACTIONS` table, so the worst case is *reaching an action that should be unreachable* — see MAJ-3 | write side tested (`write_refusal`); the **read** side is not gated |
| `ui_config.json` (corrupt) | yes | yes | **quarantine renames the file** (`ui_config.rs:108`) | no — MIN-2 |
| `docks.json` | yes | **yes** — `read_dock_document()` then `load_layout()` (`crates/workspace/src/layout/workspace/mod.rs:183-192`) | **BUILDS THE SIDE DOCKS BY NAME** — SSH Client / SFTP / Agent panels are constructed in the elevated window | **no — MAJ-2**, and the packet's own verification-plan item for this was never written |
| `ssh_session.json` | yes | **yes** — `oneterm_session_ui::init` runs unconditionally (`crates/app/src/init.rs:42`) and installs `SshSessionStore::global` | the "+" menu does not list it when elevated; but the store is live and `saved_ssh_sessions` works, and a restored session panel (MAJ-2) renders it | write guarded (`crates/session-ui/src/session_state.rs:453`), untested |
| `update_config.json` | yes | yes | M2 removes every check/download/install path (`crates/settings-ui/src/updates/mod.rs:23-28`, `actions.rs:12,90`, `install.rs:14`). **No network call is reachable**: `start_auto_check` is not called at all (`crates/app/src/window.rs:64`), `check_now` and `download_and_install_update` early-return. The crash dialog — the one surface with a GitHub link — is not shown (`crates/app/src/window.rs:68`) | write guarded (`updates/config.rs:64`), untested |
| `~/.ssh/known_hosts` | yes | only if an SSH connection is attempted — which MAJ-3 and MAJ-2 both make possible | read + (on approval) append, under the administrator token | no |
| `crashes/elevated/**` | separate store (M7) | yes | own subdirectory, one `join` (`crates/app/src/crash_report.rs:98-104`) | **yes**, both directions + the path validator |

### Confirmed for the record

- **M3 covers every local spawn path.** There is exactly one production call site of
  `LocalSession::spawn` (`crates/app/src/session_factory.rs:24`), reached by every route —
  the startup default shell, `AddPanelWithShell`, the "+" menu rows, split, duplicate tab,
  `new_terminal_here`. All of them go through `resolve_shell`, and the guard is at the top
  of that one function (`crates/core/src/config/shell.rs:241-248`). The *placement* of the
  guard is right. Its *contents* are not (MAJ-1).
- **No registry key is read anywhere in the workspace** (grep for `HKEY`, `RegGetValue`,
  `RegOpenKey`, `System::Registry`: zero hits outside the `Cargo.toml` comment), so
  `Win32_System_Registry` is justified by the `SHELLEXECUTEINFOW` struct alone, as claimed.
- **No user theme file and no font path** is read from the config directory, so the two
  obvious "config → code execution" routes a terminal usually has are genuinely absent.

---

## 2. Findings

### MAJ-1 — M3 drops `program` and `args` and keeps `env`, `cwd` and the log destination

`crates/core/src/config/elevation.rs:255-259`

```rust
Ok(LocalShellConfig {
    program: Some(program),
    args: Vec::new(),
    ..cfg.clone()          // <- env, cwd and utf8 survive
})
```

`DEC-0019` rule 4 is *"nothing the unelevated process writes may direct what the elevated
process executes"*. `program` and `args` are not the only fields of `terminal.json` that
direct what executes.

1. **`shell.env`.** `crates/core/src/config/shell.rs:251` merges `cfg.env` over
   `base_env()`, and `crates/vt/src/pty/windows/conpty.rs:437-450` writes the custom
   entries into the child's environment block **before** the inherited ones and skips any
   inherited duplicate. A `terminal.json` containing
   `"shell": { "kind": "cmd", "env": { "PATH": "C:\\Users\\me\\bin" } }` therefore gives the
   elevated `cmd.exe` an attacker-chosen `PATH`. The first `net`, `sc`, `reg` or `icacls`
   the user types in that administrator window runs the attacker's binary with a high
   integrity token. For PowerShell the same field reaches `PSModulePath`, which autoloads
   on first use of any unresolved command name.
2. **`shell.cwd`.** `crates/local-shell/src/session.rs:53` —
   `working_directory: cfg.cwd.clone().or_else(home_dir)` — reads the **untrusted original**
   `cfg`, not the value `resolve_shell` substituted. Even clearing `cwd` in
   `trusted_shell_config` would not close this; the guard is bypassed by construction.
   `cmd.exe` searches the current directory before `PATH`, so a `cwd` alone is the same
   escalation with one fewer field.

Severity: a same-user, no-privilege process writes one JSON file and waits for the user to
open an administrator shell from the "+" menu. The user is shown a correct UAC prompt for
OneTerm, consents to OneTerm, and gets an elevated shell whose command resolution the
attacker owns. This is the exact failure mode option D was rejected for.

Suggested fix, in the shape the module already uses — replace the struct-update syntax
with an explicit construction, so the next field added to `LocalShellConfig` fails the
build rather than silently crossing the boundary:

```rust
Ok(LocalShellConfig {
    kind: cfg.kind,
    program: Some(program),
    args: Vec::new(),
    env: HashMap::new(),   // or an allowlist of the OSC-integration keys
    cwd: None,             // and fix session.rs:53 to read the trusted cfg
    utf8: cfg.utf8,
})
```

and a test in the style of the existing one, asserting emptiness rather than equality.
Note that `session.rs:53` must be fixed in the same change or the `cwd` half stays open.

### MAJ-2 — an elevated window **does** get a right dock, restored from `docks.json`

`crates/workspace/src/layout/workspace/mod.rs:183-205`,
`crates/workspace/src/layout/workspace/persistence.rs:42-84`,
`crates/workspace/src/layout/workspace/layout.rs:38,70-78`

The M1 gate was placed on the two *builders*. It was not placed on the *restore*:

1. `read_dock_document()` reads `docks.json` (`mod.rs:183`).
2. `load_layout()` calls `dock_area.load(state, ..)` (`persistence.rs:82`). Its own doc
   comment states the contract: the **center** is blanked deliberately, and *"Side docks,
   sizes and open state load as before"* (`persistence.rs:40-41`). The SSH Client panel —
   and therefore the Session tree and the SFTP browser, or the Agent panel — is built here,
   by name, with no elevation check.
3. `loaded == true`, so `reset_center_only` → `apply_center_reset` runs. When elevated,
   `right_dock()` returns `Some(None)` and the `if let Some(right)` at `layout.rs:50` is
   skipped — so `set_dock(Right, ..)` is **not called**, which is what the design asked
   for, and which is exactly why the dock loaded at step 2 is **left in place**.
4. `switch_right_dock_mode` then early-returns when elevated (`actions.rs:196`), so even a
   persisted `RightDockMode::None` cannot hide it afterwards.

`right_dock()`'s own doc comment says *"`sync_right_dock_mode` and `apply_right_dock_width`
both early-return on `!has_dock(Right)`, so no further guard is needed"* (`layout.rs:63-65`).
That is true of those two functions (`mod.rs:332-334`) and false as an argument, because the
premise it rests on — that an elevated window has no right dock — is the thing being proved.

`reset_default_layout` — the path the design and the packet reason about — only runs on a
**first ever launch**. Every user who has run OneTerm once has a `docks.json` with a right
dock, so the *default* path in practice is the broken one.

**Probe (run here).** Temporarily added to
`crates/workspace/src/layout/workspace/layout_tests.rs`, run with `--exact` so the process
global did not touch any other test, then removed:

```rust
let (dock_area, cx) = dock_area(cx);
set_right_dock(&dock_area, panel_names::SSH_CLIENT, px(333.), true, cx);  // what load_layout leaves
oneterm_core::elevation::set_elevated(true);
cx.update(|window, cx| { layout::apply_center_reset(dock_area.downgrade(), window, cx); });
let observed = right_dock(&dock_area, cx);
oneterm_core::elevation::set_elevated(false);
assert_eq!((observed.0, observed.1, observed.2.as_str()),
           (333., true, panel_names::SSH_CLIENT));
```

```
test layout::workspace::layout_tests::verify_elevated_center_reset_leaves_a_loaded_right_dock_in_place ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 36 filtered out
```

The assertion that **passed** is the one that says M1 is broken: after the elevated center
reset the right dock is still there, still 333px, still open, still `ssh-client`.

Consequences: SSH, SFTP and the Agent panel are all present in an elevated OneTerm window.
That makes SSH connections, SFTP transfers and `known_hosts` writes run under an
administrator token — the three things M1 exists to remove — and it re-opens the UIPI
drag-and-drop tradeoff `DEC-0019` said "under M1 there is no SFTP panel in the elevated
instance to drop onto".

`docs/gui-layout.md:38` states this as fact — *"`ssh_client_panel` is never built,
`set_dock(Right, ..)` is never called"* — and is wrong for the same reason.

`US-0131`'s own verification plan has the item that would have caught this: *"The elevated
start-up path yields a dock state with no right dock and does not name
`panel_names::SSH_CLIENT`"*. No such test exists in `layout_tests.rs`, yet the packet's
proof block is marked `[x] Unit proof`.

Suggested fix: one guard where the restore happens, not at the builders — in `load_layout`,
drop the side docks when elevated the same way the center is already dropped, or in
`OneTermWorkspace::new` force `reset_default_layout` when elevated. Then add the test the
plan already asks for, driving the **whole** startup path and not just `apply_center_reset`.

### MAJ-3 — `ctrl-shift-n` opens Quick Connect in an elevated window

`crates/workspace/src/layout/workspace/actions.rs:121-128`,
`crates/settings-ui/src/key_bindings/key_bindings_actions.rs:88-96`

`new_ssh_session` ships bound to `ctrl-shift-n` with `context: None`, so it is a global
binding, installed by `setup_key_bindings` in every process including the elevated one. The
handler has no elevation guard:

```rust
pub(crate) fn on_action_new_session(&mut self, _: &NewSession, window: &mut Window, cx: &mut Context<Self>) {
    (commands(cx).open_quick_connect_dialog)(window, cx);
}
```

`open_quick_connect_dialog` (`crates/session-ui/src/quick_connect_dialog.rs:160`) has no
guard either. The dialog opens and connects. M1 names Quick Connect explicitly as absent
from the elevated instance; the *row* is absent, the *action* is not.

This is the sibling case the packet's own risk list calls "a surface left ungated means
SSH … running under an administrator token", and it is precisely the case the
`switch_right_dock_mode` guard was placed in the shared function to prevent — the same
reasoning was not applied one function further down the file.

Suggested fix: one `if oneterm_core::elevation::is_elevated() { return; }` at the top of
`on_action_new_session`, matching `on_action_set_right_dock_mode`'s guard four lines above
it; plus a unit test over the handler-visible predicate.

### MAJ-4 — terminal logging gives an elevated window an arbitrary file write

`crates/terminal-view/src/panel/terminal_panel.rs:359-366`,
`crates/settings/src/terminal_config/logging.rs:11-23`,
`crates/core/src/terminal_logging.rs:17-34`

`terminal.json` carries `logging.local` (bool), `logging.directory` (an arbitrary
`PathBuf`) and `logging.write_mode` (`Append` | **`Overwrite`**, "truncate the file once
when logging starts"). Nothing in this branch gates any of them. An elevated OneTerm with
`logging.local = true` opens a file at an attacker-chosen path **as Administrator** on the
first tab, and with `Overwrite` truncates it.

This is two findings in one:

- **M4.** "The elevated instance reads configuration and writes none of it" — it writes a
  log file, in a location the unelevated side chose, on a default-shaped path.
- **Privilege escalation.** Create-or-truncate at an arbitrary path with a high integrity
  token is a destructive primitive on its own (truncate a service binary, a driver, a
  signed catalog), and an append primitive whose content is terminal output is enough to
  reach an administrator-run `.ps1` profile or a `.bat`.

Suggested fix: force `TerminalLogConfig::enabled = false` when elevated, in
`LoggingConfig::runtime_config` — one function, both the local and the SSH caller — and
say so in `docs/terminal-backend.md` beside the M3 table.

### MIN-1 — `lpDirectory` chooses the elevated instance's configuration root in debug builds

`crates/app/src/elevation.rs:163-172`, `crates/core/src/config/shell.rs:109-112`

The launcher passes its own `current_dir()`, which is correct and load-bearing for the
reason the comment gives. But the field is attacker-chosen in the general case: any
same-user process may call `ShellExecuteExW(runas, oneterm.exe, "--elevated-shell cmd")`
with any `lpDirectory`. In a debug or `fast-dev` build `config_dir()` is the relative
`target/`, so the elevated instance is pointed at `<attacker dir>/target/terminal.json` —
and then MAJ-1 and MAJ-4 apply without needing write access to the user's real profile.
Release builds resolve from `USERPROFILE` and are unaffected, which is why this is ranked
minor on its own; it is a severity multiplier for MAJ-1/MAJ-4 rather than a finding that
stands alone.

Worth one sentence in the LLD: the `lpDirectory` argument is inside the trust boundary, not
outside it, and only the release `config_dir()` makes that harmless.

### MIN-2 — a corrupt config document is quarantined (renamed) by the elevated process

`crates/settings/src/ui_config.rs:105-111`, `crates/settings/src/terminal_config/document.rs:144-148`

The M4 guards sit on the *write* entry points and on the *missing-file default*. The
quarantine rename on a parse failure sits on neither. An elevated window that meets a
corrupt `ui_config.json` renames the user's file to its quarantine sibling. Under
over-the-shoulder elevation that is a modification in the other account's profile, which is
the exact trace M4 says is never left. Small blast radius, needs a corrupt file — but it
contradicts a stated invariant and the manual checklist's step 9 would not catch it (all
five hashes would be unchanged; a *sixth*, renamed file appeared).

### MIN-3 — the token query's failure branch is the unsafe direction, not the safe one

`crates/app/src/elevation.rs:28-67`

The mechanics are correct: `OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, ..)`, a
correctly sized `TOKEN_ELEVATION`, `CloseHandle` on every path, `returned` ignored (fine),
`TokenIsElevated != 0`. The initialisation order is correct: `read_process_identity()` is
the first statement of `run()` (`crates/app/src/lib.rs:86`), `main()` does nothing before
it (`crates/app/src/bin/oneterm.rs:12-14`), and the first reader — `crashes_dir()` via
`prepare_capture_paths()` — runs after. `Relaxed` ordering is adequate: one flag, written
once on the main thread before any task is spawned, and every background reader is reached
through a task submission that already carries the happens-before.

The argument to make both ways, as asked:

- **For "false is safe":** the code's own reason — no window should claim an elevation it
  cannot prove. A window wrongly marked "(Administrator)" trains users to ignore the marker.
- **Against:** a process that **is** elevated and fails the query gets `is_elevated() ==
  false`, and therefore **none of M1, M2, M3, M4 or M7 apply**. It runs SSH, SFTP, the
  updater and `terminal.json`'s `program`/`args` under an administrator token, unmarked.
  The LLD's corollary — *"there is no elevated-but-unrestricted state to reach"*
  (section 7) — is false in this branch. Fail-open on a security switch is the wrong
  default even when the branch is theoretical.

Risk accepted here as **minor** only because `OpenProcessToken` on one's own process with
`TOKEN_QUERY` cannot be denied. If it is ever reachable, the honest answer is to **exit**
with a message rather than to pick a direction: a process that cannot tell what it is
should not open a terminal. Recommend recording that in the LLD's error table rather than
changing code now.

### MIN-4 — an elevated window's Settings page accepts edits and silently discards them

`crates/settings/src/terminal_settings/persist.rs:185-190`, `crates/settings/src/ui_config.rs:254-259`

`persist_global` / `persist` log a `warn` and return. There is no user-visible feedback, and
Settings is fully present in an elevated window (`open_settings` is ungated). The user
changes a font size in an administrator window, it applies for the session, and it is gone
next time with no explanation. Not a security issue; it is the honesty half of M4. One
notification, or a disabled Settings surface, closes it.

The same page carries the other half: `network_page` — *"GitHub Connection: how OneTerm
reaches GitHub Releases to check for and download updates"*, with proxy and
certificate-verification controls — is **not** gated
(`crates/settings-ui/src/updates/groups.rs:60-78`), so an elevated window still offers a
settings page configuring a feature M2 removed, whose edits are then discarded. M2 replaced
the Updates group with one line; the Network page should get the same treatment or be
hidden.

### MIN-5 — three of the five M4 guards have no test, and one of them can be tested

`US-0131`'s Gaps section says the `docks.json`, `ssh_session.json` and `update_config.json`
guards are untestable because "flipping the global in a test would race every other test in
the same binary". That is true for a shared test binary and **not** true in general: the
probe in MAJ-2 above flipped the global, ran the real code path and restored it, using
`cargo test -- --exact` so it was the only test in the process. The same technique gives
`save_state_logged` a real elevated-branch test in `oneterm-workspace` at the cost of one
`--exact` invocation, or a dedicated `tests/` integration binary at the cost of none.
The claim as written is stronger than the evidence for it.

---

## 3. What was verified as correct

Recorded so a re-review does not re-derive it.

**The launcher (US-0130), `crates/app/src/elevation.rs:152-211`.**

- `lpVerb = "runas"` — the only supported medium→high route; correct.
- `fMask = SEE_MASK_FLAG_NO_UI` and nothing else. `SEE_MASK_FLAG_NO_UI` (0x400) suppresses
  the **shell's own error message box** only. It does **not** suppress the UAC consent
  prompt, which is drawn by `consent.exe` on the secure desktop under the AppInfo service
  and which no caller flag can suppress. The code comment says exactly this and is right.
- `SEE_MASK_NOCLOSEPROCESS` absent → `hProcess` is not returned → no handle to leak.
  Verified by reading the single `fMask` assignment; nothing else sets a bit.
- `lpFile = std::env::current_exe()`, never a name, never `argv[0]`. Spaces in the path are
  a non-issue: `lpFile` is its own field, not a command line, so no quoting is involved. A
  `current_exe()` failure aborts the launch with one notification and guesses nothing
  (`crates/app/src/elevation.rs:160-162`).
- `lpParameters` is `format!("{FLAG} {token}")` where `token` is a `&'static str` from a
  `match` on a three-variant enum. No user text can reach it, so there is no quoting hazard
  and no quoting code.
- `lpDirectory = current_dir()` with `current_exe().parent()` as fallback — matches the
  design. See MIN-1 for the boundary note.
- `ERROR_CANCELLED` mapping: `GetLastError()` is called inside the same `unsafe` block
  immediately after `ShellExecuteExW` with no intervening call that could clobber it
  (`crates/app/src/elevation.rs:194-200`). 1223 → silent `Declined`; everything else → one notification.
  `GetLastError` (not `hInstApp`) is the documented accessor for `ShellExecuteEx`. Correct.
- `Win32_System_Registry` is pulled in for the `HKEY` field of `SHELLEXECUTEINFOW` and
  nothing else; zero registry calls exist in the workspace.

**The command line (US-0130).**

- The grammar has no tolerant branch. Every rejection path returns
  `CliError::Unrecognized` → `fatal_message` → `exit(2)` (`crates/app/src/lib.rs:56-59`).
  Ten rejection tests, including `custom`, `bash`/`zsh`/`sh`, the flag twice, the joined
  `--elevated-shell=cmd` form, a bare positional and a trailing extra.
- The argument list is joined for the message and **never interpreted**
  (`elevation.rs:82-96`, `:118-127`).
- Exit 3 is checked only when elevated (`lib.rs:67-76`) — a deliberate, recorded deviation,
  and the right one: a non-elevated process paid no consent prompt.
- Trusted-path resolution consults neither `PATH` nor `COMSPEC` nor `terminal.json`;
  `%SystemRoot%` / `%ProgramFiles%` are read in the elevated process only
  (`elevation.rs:201-229`). pwsh takes the highest numeric directory under
  `%ProgramFiles%\PowerShell`, re-deriving the directory name from the parsed integer so a
  `07` cannot be smuggled through, and reports `…\7\pwsh.exe` when none exists. Six
  resolution tests, all against injected roots — a test that passed by reading the host
  would fail there. In a 64-bit elevated process `%ProgramFiles%` is the native
  `C:\Program Files`, and OneTerm ships x64.

**The token switch (US-0131, M5).**

- One source: `process_is_elevated()`. One title helper: `window_title(elevated)`, used by
  both the OS title (`crates/app/src/window.rs:78`) and the in-app bar
  (`crates/workspace/src/layout/workspace/mod.rs:270`), which also feeds
  `app_menus::init` (`title_bar.rs:42`), so all three strings come from one function.
- The argument cannot set the flag — `parse` touches no global; test
  `the_argument_never_makes_the_process_elevated`.
- The warning border reads `cx.theme().warning` (`title_bar.rs:68`) — a border, not a new
  text surface, so `scripts/check-theme-contrast.py` is untouched. Confirmed: the script is
  absent from `git diff main...HEAD`, and the gate passes it unchanged.
- Mutation-tested: see §4.

**M2 (US-0131).** No network call is reachable in an elevated instance. `start_auto_check`
is not called at all (`crates/app/src/window.rs:58-70`), `check_now` and `download_and_install_update`
early-return through the single `elevated_never_updates()` helper, `persist_update_config`
returns before touching the queue, the Updates group is one label, and the About **dialog**
drops its footer button too — a correct extension beyond what the design named. The crash
dialog, the only other route to a browser, is not shown.

**M7 (US-0131).** `crashes_dir_in(config_dir, elevated)` behind one `crashes_dir()`, so the
writer, the reader and the path validator cannot drift; the validator was **not** loosened
to fit (new test `deleting_a_report_outside_the_current_store_is_refused`). `load_pending_reports`
still runs when elevated, so promotion and the newest-20 retention still happen — only the
dialog is suppressed.

**M1, the parts that hold.** The "+" menu returns after the three shells
(`terminal_panel.rs:761-763`) — no submenu, no SSH block, no Quick Connect row, no New
Saved Session row — and `menu_rows` counts what each mode emits rather than a literal. The
mode-toggle group is not attached (`mod.rs:270-278`). `switch_right_dock_mode` is guarded in
the shared function, so a key binding on `SetRightDockMode` cannot build a dock either.
`oneterm_agent_ui::init` still runs, correctly, for the install-order invariant.

**M4, the parts that hold.** `write_refusal(elevated)` is one function per document asked
by both shared write entry points, and it keeps `persist_blocked`'s own meaning rather than
overloading it — the deviation from the design is an improvement, and the packet's
reasoning for it is correct: setting `persist_blocked` at load would **not** have stopped
the first-run default write, which happens *during* the load. That catch is the best thing
in this branch. `save_state_logged` is guarded once for all four callers.

---

## 4. Commands

Run in this worktree at `0ac3f8dc`, `$env:CARGO_BUILD_JOBS=6`.

```
cargo test -p oneterm-core -p oneterm-app -p oneterm-settings -p oneterm-settings-ui -p oneterm-terminal-view -p oneterm-workspace
  -> exit 0. Counts confirmed against the mutation runs below: oneterm-core 75,
     oneterm-terminal-view 353, oneterm-workspace 36; oneterm-settings, oneterm-settings-ui
     and oneterm-app all green. 0 failed in every binary, doc-tests included.
```

```
python scripts/verify-dependency-graph.py
  -> Dependency graph policy passed for 20 workspace packages and 20 explicit members,
     and no tracked path is over 150 characters.
```

**Mutation testing** — four decisions mutated, each caught by the test that owns it, all
restored with `git checkout --` afterwards (`git status` clean).

| Mutation | File | Test that failed |
| --- | --- | --- |
| the parser accepts `custom` (`from_token` returns `Cmd`) | `crates/core/src/config/elevation.rs` | `config::elevation::tests::a_custom_shell_token_is_rejected` |
| `trusted_shell_config` keeps `cfg.args` | same | `…::an_elevated_config_takes_the_trusted_program_and_drops_the_configured_one` |
| `window_title` ignores elevation | same | `…::only_an_elevated_window_is_marked` |
| `menu_rows` shows the submenu when elevated | `crates/terminal-view/src/panel/terminal_panel.rs` | `panel::tests::the_menu_row_count_matches_the_rows_each_mode_emits` |

```
cargo test -p oneterm-core          (3 mutations live)
  -> test result: FAILED. 72 passed; 3 failed
cargo test -p oneterm-terminal-view (1 mutation live)
  -> test result: FAILED. 352 passed; 1 failed
```

The four guards are real. Note what the mutation set also shows: there is **no** test that
fails when M1's right dock, M3's `env`/`cwd`, M4's logging write or the `NewSession`
handler are wrong, because no test covers them.

```
pwsh scripts/ci-local.ps1
  -> exit 0, final line: ci-local: all checks passed.
  Notable lines from that run:
    check-theme-contrast: 1365 foreground/surface pairings across 351 token/variant rows,
      all >= 4.5:1; primary text out-reads muted.foreground on all 585 shared-surface comparisons
    Doc path check passed for 203 current paths in 11 documents.
    English contributor-text check passed for 980 files.
    Dependency graph policy passed for 20 workspace packages and 20 explicit members.
    THIRD-PARTY-NOTICES.md is up to date.
```

---

## 5. Non-elevated walk

Own `fast-dev` build, own pids, `PrintWindow` for the captures, posted messages only. No
UAC prompt was raised and **no submenu row was clicked**. `target/fast-dev` was deleted
afterwards; the owner's own OneTerm (pid 21060, `dist\oneterm-x86_64-pc-windows-msvc`) was
never touched — every process acted on here was started by this session and addressed by
its own pid.

**A — a command line OneTerm does not understand.** `oneterm.exe --elevated-shell zsh`:

```
cls=#32770 title='OneTerm'          <- a real MessageBoxW dialog, not a OneTerm window
cls=PseudoConsoleWindow title=''
EXITCODE = 2
```

`evidence/IN-0043-verify-bad-argument.png` — the box names the rejected arguments verbatim
(`--elevated-shell zsh`) and the three accepted forms, and nothing was interpreted. **Exit
code 2**, no OneTerm window. This capture is byte-for-byte the same size (7670 bytes) as the
implementer's `US-0130-bad-argument-message.png`, i.e. independently reproduced pixel for
pixel.

**B — the same flag in a process that is not elevated.**
`oneterm.exe --elevated-shell cmd` from an unelevated session:

```
TITLE = 'OneTerm'  cls=Zed::Window
client = 1280x831
```

`evidence/IN-0043-verify-nonelevated-cmd.png`. The window title is `OneTerm` with **no**
`(Administrator)` suffix and no warning border; there is exactly one tab, `Command Prompt`,
so the argument *replaced* the default shell rather than adding a tab (`DEC-0016` holds);
and the right dock (Session + SFTP Browser) and all three mode toggles are present, i.e.
the process is fully unrestricted. This is M5 proven from the side that can be tested here:
**the argument cannot forge the marker, and it cannot impose the restrictions either.**

**C — the "+" menu and the `Run as administrator ›` submenu.**
`evidence/IN-0043-verify-plus-menu.png` and `evidence/IN-0043-verify-plus-submenu.png`.
Top to bottom: `Command Prompt`, `PowerShell`, `PowerShell 7`, `Run as administrator ›`
(opening onto the same three names, as a separate hit target with no modifier and no
overlap), the `SSH Sessions` separator, `No saved sessions`, a plain separator,
`Quick Connect...  Ctrl+Shift+N`, `New Saved Session...`. The owner-fixed order below the
shell block is undisturbed and no scrollbar appears at this length. Reproduced
independently of the implementer's `US-0132-plus-menu-run-as-admin.png` and matching it.

Note what the same screenshot shows for MAJ-2: this is the right dock a normal session
leaves in `docks.json`, and it is the dock an elevated window then restores.

---

## 6. The manual checklist in US-0131 — review and corrections

The checklist (US-0131 § *Manual acceptance checklist*) is unusually good: it is ordered,
it names the artefact for each step, and it separates "the marker says so" from "the token
says so". Four corrections, one addition, before it goes to the owner.

1. **Step 9 is not falsifiable as written.** "Also confirm no new file appeared in the
   config directory" **will fail every time**: `prepare_capture_paths()` calls
   `create_private_dir(crashes_dir())` on every start (`crates/app/src/crash_report.rs:33-35`),
   so an elevated run always creates `<config>\crashes\elevated\`. That is M7 working, not
   M4 breaking. Reword to: *"the five documents are byte-identical, and the only new path
   under the config directory is `crashes\elevated\`."* Add the sibling check MIN-2 needs:
   *"and no `*.bak` / quarantine sibling of any of the five appeared."*
2. **"Unchanged" needs to be said per document, and the five hashes are not enough.**
   `docks.json` is the one most likely to move and the one the current step already flags —
   but `ui_config.json` can be *renamed* rather than rewritten (MIN-2), which a hash
   comparison of the same path reports as "file not found", not as a difference. Say
   explicitly: *for each of the five, the file must still exist, at the same path, with the
   same hash, and with no sibling that did not exist before.*
3. **Step 4 must include the right dock as a hash-level check, not only a screenshot.**
   Given MAJ-2, "no right dock in the window at all" is the single most important line in
   the whole checklist and it currently rests on one screenshot of a machine whose
   `docks.json` state is unstated. Make the precondition explicit: *"before step 2, open the
   normal window, ensure the right dock is open on SSH Client, close it so `docks.json`
   records that, then elevate."* That is the state every real user is in, and it is the
   state the bug needs.
4. **Step 11 (declined UAC) should also record the exit path.** "Nothing at all" is right
   but unverifiable from a screenshot; add *"and `Get-Process oneterm` shows no new
   process"*, so a window that opened and closed quickly cannot pass as "nothing".
5. **Add a step between 3 and 4, for MAJ-1 and MAJ-3** — the two things the current
   checklist cannot catch:
   - *In the elevated tab, run `where.exe cmd` and `$env:PATH` (or `echo %PATH%`) and
     confirm `PATH` is the machine's and not one from `terminal.json`.*
   - *Press `Ctrl+Shift+N` in the elevated window. Nothing must open.*

   Both are one keystroke and both currently fail.

Also, three wording notes:

- Step 1 names `target\*.json` for the fast-dev build. Correct, and worth adding *why*:
  a debug build's `config_dir()` is the relative `target/`, so the hashes must be taken
  from the directory the elevated process will actually resolve — which is the launcher's
  `current_dir()`, not necessarily the repo root (MIN-1).
- Step 12's condition, `elevated && persist_blocked`, only fires when `ui_config.json` is
  **absent** in the other profile. If that account has ever run OneTerm, the notification
  will correctly not appear; say so, or the step reads as a failure.
- The screenshot names are consistent and unambiguous
  (`evidence/US-0131-E<n>-<slug>.png`) and match the three already in `evidence/`. No
  change needed.

---

## 7. Gaps in this verification

- **The elevated side was never run.** Every claim about a high-integrity process is from
  reading, from unit tests, or from the MAJ-2 probe, which sets the process global in a
  test binary — real code path, real assertion, but not a real elevated token.
- **MAJ-1 and MAJ-4 are not demonstrated end to end.** Demonstrating them needs an elevated
  window, which this session may not open. They are derived from a complete read of the
  chain (`terminal.json` → `TerminalSettings` → `resolve_shell` / `LocalSession::spawn` →
  `environment_block`), each link cited above, with the decisive one — custom env entries
  beating the inherited block — confirmed by that function's own unit tests
  (`conpty.rs:547`: *"the parent environment must be appended after the custom entries"*).
- **`cfg(unix)` paths** are compile-and-unit-tested only, as in the packets.
- **Over-the-shoulder elevation** needs a second administrator account; none here.
- **Group Policy "deny elevation"** and any real `ShellExecuteExW` failure remain
  unexercised, so the non-`ERROR_CANCELLED` notification branch is read, not run.
- No attempt was made to audit `gpui-component`'s `DockArea::load` beyond its behaviour as
  documented in `load_layout`'s own comment and as observed by the probe.

---

# Re-verification of 24226d4c — 2026-09-21

Target: `feat/elevated-shell` @ `24226d4c`, three commits on `ac6f2928` (the commit that
carried the findings above). Scope: the four majors and the five minors only, plus the
question of whether the new security tests are actually run. Same constraints: no UAC
prompt, the elevated side remains the owner's manual acceptance.

## Verdicts

| Packet | Verdict |
| --- | --- |
| **US-0130** | **PASS** |
| **US-0131** | **PASS** |
| **US-0132** | **PASS** |
| **Overall (IN-0043)** | **PASS — accept, subject to the owner's 15-step manual checklist.** All four majors are closed at the root rather than at the reported symptom, and three of the five minors were fixed beyond what was recommended. Six new **minor** items are recorded below; none blocks acceptance, and one of them (NEW-4) is a question the coordinator asked rather than a defect. |

## Per-finding status

| # | Status | Evidence |
| --- | --- | --- |
| **MAJ-1** env / cwd crossed the trust boundary | **FIXED** | `trusted_shell_config` builds `LocalShellConfig` field by field with no `..cfg.clone()` (`crates/core/src/config/elevation.rs:256-279`): `env: HashMap::new()`, `cwd: None`, only `kind` and `utf8` cross. `ResolvedShell` gained `cwd` (`shell.rs:144-154`) and `LocalSession::spawn` reads `resolved.cwd`, not `cfg.cwd` (`crates/local-shell/src/session.rs:53-56`). Independently re-probed — see below. Mutation-checked. |
| **MAJ-2** the right dock was restored from `docks.json` | **FIXED, at the root** | `startup_dock_document` (`mod.rs:60-66`) returns `None` when restricted, so the document is never read; `right_dock`'s `None` arm now calls `remove_dock` (`layout.rs:50-60`, `:115-121`) instead of skipping `set_dock`. Two locks, and the outer one is the one that mattered. Independently re-probed. Mutation-checked. |
| **MAJ-3** `ctrl-shift-n` opened Quick Connect | **FIXED, and generalised** | `oneterm_actions::elevated_policy`, an exhaustive table with unclassified = denied, consulted at four layers: `apply_key_bindings` (denied actions are **not bound at all**, `state.rs:111-117`), `on_action_new_session` (`actions.rs:127-132`), `session-ui`'s public entry points (`lib.rs:54-90`), and `open_quick_connect_dialog` itself (`quick_connect_dialog.rs:161-167`). Classification audited independently — see below. |
| **MAJ-4** terminal logging was an elevated arbitrary file write | **FIXED** | `LoggingConfig::gated` forces `enabled = false` when restricted, inside `runtime_config`, the one function both `local_config` and `ssh_config` resolve through (`crates/settings/src/terminal_config/logging.rs:47-73`). The test drives it with `directory = C:\Windows\System32\drivers\etc` and `LogWriteMode::Overwrite`. |
| **MIN-1** `lpDirectory` is inside the trust boundary | **RESOLVED as documented** | Not changed, which is right; the LLD now states it explicitly as a developer-build-only exposure and a severity multiplier, and adds the standing rule *"a release build must never gain a cwd-relative `config_dir()`"*. |
| **MIN-2** quarantine renamed the user's file | **FIXED** | One guard in `oneterm_core::persistence::quarantine_file` (`crates/core/src/persistence.rs:143-158`) — the shared function, so `ui_config.json`, `terminal.json` and `docks.json` are covered at once rather than three times. |
| **MIN-3** the token query failed open | **FIXED, beyond the recommendation** | `Elevation` is three-valued. `Unknown` **is restricted** and its marker reads `OneTerm (elevation unknown)` — the recommendation was only to record the risk; the rework split the two questions correctly, because fail-closed on the restrictions and fail-honest on the marker are not the same answer. `is_elevated()` was **removed** so no seam can be guarded with the weaker predicate: grep confirms zero Rust call sites of `is_elevated` / `set_elevated` remain, and all 17 gate sites read `is_restricted()`. |
| **MIN-4** settings discarded edits silently; Network page survived M2 | **FIXED** | The Network page is not built when restricted (`crates/settings-ui/src/panel.rs:115-120`) and the settings window carries one warning-coloured line (`:125-133`, `:150-160`). |
| **MIN-5** three guards called "untestable" | **FIXED, claim retracted** | Five `#[ignore]`d elevated-branch tests now exist, each restoring the global on every exit path including a panic. All five run and pass — see Commands. |

## MAJ-1 re-probe — attacking `Unknown`, not `Elevated`

Every test in the rework flips the switch to `Elevation::Elevated`. `Elevation::Unknown` is
the new fail-closed branch and is covered only by a unit test of `is_restricted()` itself, so
this probe drove the **whole spawn path** under `Unknown`. Added temporarily to
`crates/core/src/config/shell.rs`, run with `--exact --ignored`, then removed.

A `LocalShellConfig` poisoned in every field — `program` = `C:\Users\attacker\evil.exe`,
`args` = `/c calc`, `env` = `PATH` plus `PSModulePath` plus `COMSPEC` all pointing at the
attacker, `cwd` = `C:\Users\attacker\stage` — was resolved twice. Unrestricted first, and the
probe asserts the poison **does** get through there, so a pass cannot come from an inert
fixture. Then under `Elevation::Unknown`:

```
test config::shell::verify_probe::probe_no_terminal_json_field_survives_under_unknown_elevation ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 76 filtered out
```

The assertion is an **exact key-set** equality, not a list of absences: the elevated spawn's
environment is precisely

```
["COLORTERM", "LANG", "PROMPT", "TERM", "TERM_PROGRAM", "WSLENV"]
```

— `base_env()` plus the `cmd` OSC 7 prompt, and nothing else. `program` ends in
`\System32\cmd.exe`, no configured argument survives, and `cwd` is `None`.

**Every spawn path is covered by that one guard**, confirmed by tracing rather than assumed:
the five routes — startup default shell (`terminal_panel.rs:241`), `AddPanelWithShell` and
the "+" menu row (`:247`), `new_terminal_here` (`spaces.rs:113`), split (which creates an
*empty* Space and fills it through `new_terminal_here`), and duplicate tab
(`duplicate.rs:124`) — all converge on `TerminalPanel::spawn_local_session`, then the single
production `LocalSession::spawn` (`crates/app/src/session_factory.rs:24`), then
`resolve_shell`. A grep for `cfg.` inside `LocalSession::spawn` now returns **nothing**: the
spawner reads only `resolved`, which is what makes the one guard complete.

## MAJ-2 re-probe — also under `Unknown`

The same attack as the original probe, under `Elevation::Unknown`: a pre-built `ssh_client`
right dock standing in for what `load_layout` used to leave behind, then the restore gate and
the center reset.

```
test layout::workspace::layout_tests::probe_unknown_elevation_also_has_no_right_dock_and_reads_no_layout ... ok
```

The injected reader panics if called and was not called; `has_dock(Right)` is false
afterwards; the dumped `DockAreaState` carries no right dock and its JSON names neither
`ssh_client` nor `sftp`. The assertion that used to pass — *the dock is still there* — now
fails, which is the point.

## MAJ-3 — the classification, audited independently

I extracted both policy tables and the full `BINDABLE_ACTIONS` id list and compared them as
sets rather than trusting the exhaustiveness test:

- **38 bindable ids, 13 denied, 25 allowed, 38 classified.** No id unclassified, no id in
  both tables, no id in a table that is not bindable.
- **Denied**, and I agree with every one: `new_ssh_session`, `open_session`,
  `delete_session`, `session_property`, and all nine `sftp_*`.
- **Allowed**, and I agree with every one: the four `split_*`, `new_terminal_tab`,
  `close_panel`, `close_space`, `toggle_zoom`, `find`, the four `terminal_*`
  (`clear`, `copy`, `paste`, `select_all`), the seven input-channel ids, `toggle_gutter`,
  `open_settings`, `about`, `quit`, `duplicate_session`.
- **`duplicate_session` is the only one worth arguing**, and allowing it is right: it reopens
  the *active tab's* live session, every tab in an elevated window is a local shell, and the
  local branch spawns through `resolve_shell` (`duplicate.rs:121-126`) and is therefore
  covered by MAJ-1's guard.
- **No Agent action is bindable at all**, so there is nothing to deny there; the Agent panel
  is unreachable through the right dock, which no longer exists.

**Rows as well as keys.** The "+" menu returns after the three shells when restricted
(`terminal_panel.rs:761-763`), so Quick Connect and New Saved Session have no row; the
session tree's and the SFTP browser's context menus live on panels that are not built. So for
every denied action both the row and the key are gone, not just the key.

**`apply_key_bindings` is the strongest of the four layers** and it is correct in a way worth
recording: it first strips *every* rebindable action's default from the kit snapshot
(`state.rs:104-109`) and then re-adds only the allowed ones, so a denied action ends with
**no binding at all** — including a user's own `ui_config.json` override, which the handler
guards alone would never have seen.

## New findings — all minor, none blocking

- **NEW-1 (doc, normative).** The LLD's **Interfaces** block still declares the removed API:
  `pub fn set_elevated(value: bool)`, `pub fn is_elevated() -> bool` and
  `pub(crate) fn process_is_elevated() -> bool`
  (`low-level-design/elevated-instance.md:500-517`), with `bool` signatures that contradict
  the three-valued `Elevation` the same document now describes in its error table. The
  start-up sequence (`:33`, `:76`, `:94`), section 7's prose (`:373`, `:395`) and one seam row
  (`:417`) name `is_elevated()` too. The behavioural prose was reconciled thoroughly; the
  interface contract was not. Worth one pass before the intake closes.
- **NEW-2 (doc, user-facing).** `README.md:126-128` says `docks.json` is among the files
  "read and never written". It is no longer **read** in an elevated window, and the visible
  consequence — an administrator window never restores your saved layout, every time, by
  design — is the load-bearing half of the MAJ-2 fix and is not stated for users.
- **NEW-3 (defence in depth).** `open_duplicate_ssh_dialog` is the one `session-ui` public
  entry point the policy does not consult; the other three do. It is unreachable today (no
  SSH tab can exist in an elevated window), but the policy table's own comment rejects
  exactly that reasoning for the `sftp_*` ids — *"'unreachable' is a property of the current
  layout and this is a property of the action"*. One `if`, for consistency with the rule the
  table states about itself.
- **NEW-4 (the coordinator's question).** The five security tests are `#[ignore]`d and neither
  `ci-local` nor CI runs them. Assessed below.
- **NEW-5 (behaviour, undocumented).** Because `trusted_shell_config` sets `cwd: None`,
  Duplicate-tab and `new_terminal_here` in an elevated window no longer inherit the live
  OSC 7 working directory (`duplicate.rs:121` sets it, the guard discards it). That is the
  correct security answer and a real behaviour change a user will notice; it is documented
  nowhere.
- **NEW-6 (doc).** `docs/agents/persistence.md` — the document `AGENTS.md` tells every agent
  to read *before changing persisted schemas or storage mechanics* — has **no** mention of
  elevation, although this branch made a whole class of writes conditional, suppressed
  quarantine, and stopped one document being read. One paragraph there would put the rule
  where the next agent will look for it.

## NEW-4 — should CI run the five ignored tests? Yes. Recommendation below

**The problem is real.** The five tests are the only automated proof of the MAJ-2 restore
gate and of three M4 guards. `cargo test --workspace` skips them, so today a regression in
any of the five is caught by **nothing**. That is worse than before the rework in one narrow
sense: previously the guards were untested and the packet said so; now they are tested and a
reader will reasonably assume CI covers them.

**A blanket `cargo test --workspace -- --ignored` is not the answer.** Measured here: the
workspace has **17** ignored tests, and the set is heterogeneous by design — benchmarks,
release-only measurements, visual-review-only tables, and
`session_orphan_tests::orphan_liveness_table`, whose own reason says it *"spawns real cmd.exe
processes and can take minutes"*. Running all of them in CI would be slow and noisy.

**A dedicated test binary with a process-per-test harness (the second option asked about) is
not worth it.** The five tests assert against `save_state_to`, `persist_update_config`,
`startup_dock_document` and `apply_center_reset`, all of which are private or `pub(crate)`.
Moving them to `tests/` means widening visibility purely to accommodate a test — which is
precisely the *"removing a validation"* risk this packet's own risk list names — plus a custom
`harness = false` re-exec shim to maintain. Reach for it only if the count grows past a
handful.

**Recommended, and measured here: three scoped invocations, not five.** The `#[ignore]`
reason says *"run alone with `--exact`"*, which is stronger than the tests actually need.
`--ignored` already selects only the ignored tests, and `--test-threads=1` makes them
sequential in one process; each restores the global on `Drop` before the next starts. I ran
it:

```
cargo test -p oneterm-workspace   --lib -- --ignored --test-threads=1
  -> test result: ok. 3 passed; 0 failed; 36 filtered out
cargo test -p oneterm-session-ui  --lib -- --ignored --test-threads=1
  -> test result: ok. 1 passed; 0 failed; 77 filtered out
cargo test -p oneterm-settings-ui --lib -- --ignored --test-threads=1
  -> test result: ok. 1 passed; 0 failed; 54 filtered out
elapsed: 15.4s wall for all three, of which the tests themselves are 0.01s
```

All five, in three lines, for fifteen seconds of a gate that already takes minutes. Three
lines beat five because a *new* elevation test added to one of those crates is picked up
automatically, where a hand-written list of five test names silently would not be. The three
crates contain no other ignored tests today; if one later gains an unrelated ignored
benchmark it would join the run, which fails in the "CI got slower" direction rather than the
"a security check stopped running" direction — the right way round.

**Add one census line to close the rot.** The scoping above still assumes somebody puts a new
elevation test in one of those three crates. Make that assumption checkable:

```
cargo test --workspace -- --ignored --list   # 17 today
```

diffed against a checked-in list, so **any** new ignored test anywhere fails the gate until
someone records it and decides whether it belongs in the elevation run. That is the same
"exhaustive table, unclassified fails the build" pattern this rework just used for
`elevated_policy`, applied to the ignore list — and it is one line plus one small text file.

**Where to put it.** `scripts/ci-local.ps1` and `scripts/ci-local.sh`, immediately after
`cargo test --workspace`; and in `.github/workflows/ci.yml` in the **Full workspace quality
gate** job after its `cargo test --workspace` (line 122). No Windows job is needed: all five
tests are platform-independent — they drive pure logic and gpui's `TestAppContext`, with no
`cfg(windows)` in them — and that job already installs the GPUI Linux dependencies.

**Also worth doing, since it is free:** relax the `#[ignore]` reason text from *"run alone
with `--exact`"* to *"run with `--ignored --test-threads=1`"*, so the next maintainer reads
the convention that matches what the gate does.

## Commands

All at `24226d4c`, `$env:CARGO_BUILD_JOBS=6`.

```
cargo test -p oneterm-core -p oneterm-app -p oneterm-actions -p oneterm-settings \
           -p oneterm-settings-ui -p oneterm-session-ui -p oneterm-terminal-view \
           -p oneterm-workspace -p oneterm-local-shell
  -> exit 0. oneterm-core 76, oneterm-actions 20, oneterm-session-ui 77 (+1 ignored),
     oneterm-settings-ui 54 (+1 ignored), oneterm-terminal-view 353 (+3 ignored),
     oneterm-workspace 36 (+3 ignored), oneterm-settings 49, oneterm-app 33 (+2 ignored).
     0 failed in every binary.
```

The five ignored security tests, each run alone with `--exact --ignored`:

```
oneterm-workspace   an_elevated_window_never_reads_the_saved_layout                  -> ok. 1 passed
oneterm-workspace   the_elevated_startup_path_yields_a_dock_state_with_no_right_dock -> ok. 1 passed
oneterm-workspace   an_elevated_window_writes_no_dock_layout                         -> ok. 1 passed
oneterm-session-ui  an_elevated_window_writes_no_saved_sessions                      -> ok. 1 passed
oneterm-settings-ui an_elevated_window_queues_no_update_config_write                 -> ok. 1 passed
```

Mutation testing — one per major fix, each caught by the named test, both restored
(`git status` clean afterwards):

| Mutation | Test that failed | Assertion |
| --- | --- | --- |
| `trusted_shell_config` keeps `env: cfg.env.clone()` | `config::elevation::tests::an_elevated_config_keeps_no_field_of_the_configured_one` | *"no configured environment entry may survive: PATH and PSModulePath decide what runs"* |
| `right_dock`'s `None` arm skips `remove_dock` | `layout_tests::the_elevated_startup_path_yields_a_dock_state_with_no_right_dock` | *"an elevated window must have no right dock"* |

```
pwsh scripts/ci-local.ps1
  -> exit 0, final line: ci-local: all checks passed.
     English contributor-text check passed for 981 files.
     check-theme-contrast: 1365 pairings, all >= 4.5:1 (untouched, as before).
     THIRD-PARTY-NOTICES.md is up to date.
```

## The 15-step manual checklist — re-reviewed

Every correction from section 6 above was taken, and two of them were taken further than
suggested: step 1 now **creates** the `docks.json` precondition instead of merely asserting
one, and step 2 records `before-names.txt` alongside the hashes so a quarantine **rename** is
visible where a same-path hash comparison would have reported only "file not found". Step
12's "only `crashes\elevated\` may be new" is now correct and falsifiable, where the old step
9 would have failed every run. Steps 5 and 8 are new and directly exercise MAJ-1, MAJ-3 and
MAJ-4 — all three would have failed on the previous branch, which is the property a checklist
step should have.

Three small additions, none blocking:

1. **Nothing exercises the *allowed* side of the policy table.** The packet's own risk 4 is
   *"a guard whose condition is wrong in the other direction"*, and `apply_key_bindings`
   filtering is exactly where that could happen: a table typo would silently unbind `Ctrl+T`,
   copy and paste, or the splits in the elevated window, and no manual step would notice. Add
   one line to step 6: *"in the elevated window confirm `Ctrl+T` opens a tab,
   `Ctrl+Shift+C` / `Ctrl+Shift+V` copy and paste, and a split works."*
2. **Step 5's MAJ-1 check should name `PSModulePath`.** It checks `PATH` and `cwd`; the
   PowerShell-specific vector is `PSModulePath`, which autoloads a module on the first
   unresolved command. Put it in the `terminal.json` fixture alongside `PATH` and check it in
   the elevated PowerShell tab.
3. **Step 8 would be sharper against a file that already exists.** "The directory must
   contain no new file" catches creation; pointing `logging.directory` at a directory holding
   a file with known content, and re-hashing it afterwards, also catches the `Overwrite`
   truncation, which is the destructive half of MAJ-4.

And one note rather than a change: **`Elevation::Unknown` has no manual step and should not
have one** — the token query cannot be made to fail by hand. It is covered by
`an_unknown_token_is_restricted_but_claims_no_elevation` and, end to end, by the two probes
above. Worth one sentence in the checklist's preamble so its absence reads as deliberate.

## Gaps in this re-verification

- **The elevated side is still unrun**, for the same reason. Everything about a
  high-integrity process is reading, unit tests, and probes that set the process global in a
  test binary.
- **No GUI walk this round.** The non-elevated walk in section 5 was performed against
  `0ac3f8dc`; nothing in this rework changes the unelevated rendering path, and `ci-local`
  covers the build. `target/fast-dev` was not rebuilt.
- **MAJ-4 is proven at the gate, not at the file system.** The test asserts
  `enabled == false`; nobody watched a log file fail to appear, because that needs an
  elevated window. Checklist step 8 is the closing evidence.
- Over-the-shoulder elevation, Group Policy denial and a real `ShellExecuteExW` failure
  remain unexercised, as before.

---

# Re-verification of 16d32217 — 2026-09-21

Target: `feat/elevated-shell` @ `16d32217`, the four commits added after the last
independent PASS (`c135b425`): `21597918` (the `runas` launch moved off the gpui thread),
`d67ee77c` (a merge of `main`), `8113440e` (the title-bar border removed, and the elevated
instance's console released) and `16d32217` (the `(Administrator)` suffix highlighted).
Scope: those four deltas only. Same constraints as the two sections above: no UAC prompt is
raised, no elevated process is started, and the elevated side remains the owner's manual
acceptance — which he has already passed on `8113440e` for steps 0 and 3–15.

## Verdicts

| Delta | Verdict |
| --- | --- |
| **1 — the `runas` launch on its own thread** (`21597918`) | **PASS.** The security shape of the call is byte-for-byte what it was, the only mask change is the one the new thread requires, and every failure path is a logged best effort rather than a panic. Two minors (NEW-8, NEW-10). |
| **2 — the warning-coloured border removed** (`8113440e`) | **PASS.** No conditional colour is left in `title_bar.rs`, and `DEC-0019` M5 carries the amendment. |
| **3 — the elevated instance releases its own console** (`8113440e`) | **PASS.** The sole-owner test is the right test, the buffer probe cannot misread, an inherited console is kept, and nothing reaches the console before it goes. One minor (NEW-9). |
| **4 — the highlighted suffix** (`16d32217`) | **FAIL — MAJ-5.** The `"warning"` key the 39 themes gained is not a key `gpui-component` reads. Nothing about the rendering changed, the marker is still below 4.5:1 in **16 of 39** variants (1.64:1 on Ayu Light), and the gate's 39 new pairings measure a value the application never draws. |
| **Overall** | **FAIL — do not accept `16d32217`.** Deltas 1–3 are sound and stand on their own. Delta 4 is one wrong JSON key away from being correct; until it is fixed, the legibility claim in `US-0131`'s second acceptance tweak and in `DEC-0019` M5's second amendment is not true of the running application. |

---

## MAJ-5 — the 39 `warning` entries are inert, and the gate is a false green

`crates/theme/themes/*.json` (all 24 files, 39 variants),
`scripts/check-theme-contrast.py:208-219`,
`crates/workspace/src/layout/title_bar.rs:116-126`

**The key does not exist.** In `gpui-component` 0.6.0 — the published crate this workspace
builds against (`Cargo.lock`: `gpui-component 0.6.0`, `registry+…crates.io-index`), and the
pinned `reference/gpui-kit` agrees — the theme-colour struct spells the token
**`warning.background`**:

```rust
// gpui-component-0.6.0/src/theme/schema.rs:616-618
/// Warning background color.
#[serde(rename = "warning.background")]
pub warning: Option<SharedString>,
```

`ThemeConfigColors` has no `deny_unknown_fields`, so a `"warning"` key inside `colors` is
read by nobody and dropped in silence. A plain `"warning"` **is** a valid key — but only in
`HighlightThemeStyle`, the syntax-highlighting block, which these themes already fill in
(`crates/theme/themes/ayu.json:97-99, 322-324`). `reference/gpui-kit/.theme-schema.json`
says exactly that:

```
ThemeConfigColors    ['warning.background', 'warning.active.background',
                      'warning.hover.background', 'warning.foreground']
HighlightThemeStyle  ['warning', 'warning.background', 'warning.border']
```

**Probed, twice, rather than read.** Both probes were added temporarily, run, and removed;
`git status` is clean.

1. *Does the key survive deserialisation?* A probe in `crates/theme/src/theme.rs` loaded
   every embedded theme through the kit's own `ThemeRegistry::load_themes_from_str` and
   printed the parsed field:

   ```
   PROBE light.colors.warning = None; dark.colors.warning = None
   ```

   — for `Ayu Light` and `Ayu Dark`, whose JSON now carries `"warning": "#854d0e"` and
   `"warning": "#facc15"`. The value never reaches the kit.

2. *What does the application actually draw, then?* A probe in
   `crates/workspace/src/layout/workspace/layout_tests.rs`, under `gpui::TestAppContext`,
   registered all 39 variants and called the real `Theme::apply_config` on each, then read
   `Theme::global(cx).warning` and `.title_bar` — the two values
   `elevation_suffix()` composites. Every one of them is the theme's own `base.yellow`,
   because the kit's fallback is `apply_background_color!(warning, fallback = self.yellow)`
   (`schema.rs:878`). A sample against what the JSON claims:

   | Variant | JSON now says | Actually rendered |
   | --- | --- | --- |
   | Ayu Dark | `#facc15` | `#feb454` |
   | Ayu Light | `#854d0e` | `#f1ad49` |
   | Hybrid Light | `#713f12` | `#948000` |
   | Aurora Light | `#a16207` | `#eab308` |
   | Matrix | `#facc15` | `#ffea00` |

**The marker is illegible in 16 of 39 variants.** Feeding the probe's 39 measured
`(warning, title_bar.background)` pairs through a WCAG 2.1 relative-luminance ratio written
for this review — independent of `check-theme-contrast.py`, and cross-checked against that
script's own numbers on the JSON values, which it reproduces to the hundredth:

```
1.64 Ayu Light          1.67 Everforest Light   1.74 Molokai Light
1.92 Aurora Light       1.98 Catppuccin Latte   2.07 Flexoki Light
2.09 Mellifluous Light  2.44 Solarized DARK     2.55 Hybrid Light
2.74 macOS Classic Light 2.75 Gruvbox Light     2.81 Zed One Light
3.22 Fahrenheit (DARK)  3.43 Molokai Dark       3.59 Hybrid Dark
4.32 Solarized Light
```

All twelve light variants, **and four dark ones**. The lowest, Ayu Light at 1.64:1, is
amber-on-off-white: the word `(Administrator)` is close to invisible in exactly the place
the decision put it. `US-0131`'s rework asserts the opposite — *"the 27 dark variants take
exactly `#facc15`, the kit's own value — what they already rendered, so nothing changes
visually there"* — and no dark variant rendered `#facc15` before or after.

**Where the reasoning went wrong, precisely.** The rework's premise is *"no theme in
`crates/theme/themes/` defined it, so all 39 variants were inheriting the kit's own default
— an amber (`yellow-400` / `yellow-500`)"*. The first half is right; the second is not. The
kit's fallback for `warning` is not a fixed amber, it is **`self.yellow`** — and every one
of the 39 variants *does* define `base.yellow`, so each was already rendering its own. The
survey that followed ("fails on all 12 light themes, 1.24:1 on Hybrid Light") measured a
colour no theme uses. The real figure for Hybrid Light is 2.55:1 — still a failure, so the
problem was real; the cure was applied to the wrong key.

**The gate now certifies a value the renderer cannot see.** `check-theme-contrast.py`'s own
contract is stated in its header: *"a foreground paired with a surface the application never
paints under it proves nothing"*. Its 39 new `warning` rows read the same dead key, so
`1404 pairings … all >= 4.5:1` is true of the repository's JSON and false of the
application. This is worse than leaving `warning` out of `SURFACES`, because the gate now
asserts the property rather than being silent about it.

**The fix is one word, 39 times.** Rename the key to `"warning.background"` in the theme
files and in `SURFACES`'s comment. Re-derived with the same function, that lands the
27 dark variants on `#facc15` (worst 8.93:1, Solarized Dark) and the 12 light ones on the
values already chosen (worst 4.71:1, Aurora Light) — every variant over the floor, the four
failing dark themes fixed as a side effect, and the gate's number becomes a fact about the
window. Re-run both probes above afterwards: reading the JSON back is what missed this.

---

## Minor findings

- **NEW-7 (minor, and the other half of MAJ-5).** `warning` is drawn as **text** on four
  surfaces `SURFACES` does not list: the settings window's read-only note
  (`crates/settings-ui/src/panel.rs:157`) and its key-bindings note
  (`key_bindings/key_bindings_ui.rs:216`) on `background`; the forwarding row's warning
  (`crates/session-ui/src/forward_rows.rs:292`); the terminal's own warning line
  (`crates/terminal-view/src/terminal_view/render.rs:603`, and `Alert::warning` at `:510`);
  and a `Warning` notification's whole body (`crates/theme/src/notif_ext.rs:86`) on
  `popover.background`. The agent card's Blocked/Stale colour (`crates/agent-ui/src/card.rs:65,75`,
  `view.rs:410,641`) is a fifth. As rendered today, `warning` clears 4.5:1 on `background` in
  only 24 of 39 variants and on `popover.background` in 24 of 39. None of this is a
  regression — every one of those sites rendered the same colour before this branch — but
  the script's own rule is *"when a component starts drawing one of these tokens on a
  background that is not listed, add the surface"*, and this branch brought `warning` under
  the gate with one of its six surfaces. The settings read-only note is the one worth
  naming: it is `MIN-4`'s fix from the previous round, and in an elevated window on a light
  theme it is the sentence explaining why the page does nothing.
- **NEW-8 (minor, behaviour).** The launch is now asynchronous and nothing debounces it.
  `launch_elevated_shell` spawns a thread and returns; clicking `Run as administrator ›
  PowerShell` five times spawns five threads and five consent requests, and consenting five
  times opens five administrator windows. Before `21597918` the modal call made a second
  click impossible. Not a privilege problem — every window still costs its own consent — but
  one `AtomicBool` held for the lifetime of the outstanding request would close it.
- **NEW-9 (minor, diagnostics).** The two `log::error!` lines in `process_elevation()`
  (`crates/app/src/elevation.rs:49, 65`) can never be emitted: `read_process_identity()` is
  the first statement of `run()` (`lib.rs:86`) and `env_logger …init()` is at `:111`, so the
  `Elevation::Unknown` branch — the fail-closed one — is silent in every build. The window
  still says `(elevation unknown)`, so nothing is unsafe; but "why is this window
  restricted" has no answer in the log. It also happens to be *why* the claim "nothing is
  written to a console that is about to go away" holds, so the two are worth fixing
  together: a deferred buffer, or move the query's reporting after the logger exists.
- **NEW-10 (minor, undocumented).** The helper thread is detached, so quitting OneTerm while
  the consent prompt is up terminates it inside `ShellExecuteExW`: `CoUninitialize` never
  runs, and if the user then approves, an elevated OneTerm starts with no launcher left. It
  is harmless — the verb, the file and the parameters were fixed before the thread started —
  but it is neither exercised nor written down.

---

## What was verified as correct

**Delta 1 — the background-thread `runas`.**

- *The security shape did not move.* `git diff c135b425 21597918 -- crates/app/src/elevation.rs`
  restricted to the call's fields: `lpVerb = "runas"`, `lpFile = current_exe()`,
  `lpParameters = "--elevated-shell <token>"` from the closed three-variant enum,
  `lpDirectory = current_dir()`, `nShow = SW_SHOWNORMAL`, `hwnd` left null by
  `mem::zeroed()` — all unchanged. The **only** difference is
  `fMask = SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI` in place of `SEE_MASK_FLAG_NO_UI`, which
  is required rather than optional: the helper thread has no message loop and exits as soon
  as the call returns, and without `NOASYNC` `ShellExecuteEx` is allowed to complete the
  operation after that. `SEE_MASK_NOCLOSEPROCESS` is still unset and nothing reads
  `hProcess`. `SHELLEXECUTEINFOW` has no environment member, so there is still no way for an
  environment block to cross, by construction rather than by omission.
- *The receiver cannot outlive the window destructively.* `async_channel::bounded(1)`;
  the foreground half is a plain `window.spawn(cx, …).detach()`, so when the window is
  dropped the task is dropped with it and the receiver goes. `send_blocking` on a channel
  whose receivers are gone returns `Err` — it does not block and does not panic — and that
  `Err` goes through `oneterm_core::report_best_effort`, which is a `log::warn!` and nothing
  else (`crates/core/src/lib.rs:59-66`). On the foreground side `receiver.recv().await`
  returning `Err` is an early `return`, and the notification's `cx.update(…)` result is
  likewise a best effort. There is no `unwrap`, no `expect`, and no weak handle to upgrade
  anywhere on the path, so "the window closed before the prompt was answered" costs one log
  line.
- *`remember_ui_thread()` runs before any possible launch.* It is called at
  `crates/app/src/lib.rs:89`, inside `run()`, before `oom::init_ballast()` and long before a
  window, a menu or the `commands` service exists. The only production route to
  `launch_elevated_shell` is `crates/app/src/init.rs:76` → the `commands` service →
  `crates/terminal-view/src/panel/terminal_panel.rs:780`, the "+" menu row.
- *The `debug_assert` is fired by no test path.* `ElevationRequest::execute` has exactly one
  call site in the workspace — the closure in `launch_elevated_shell`. The one test that
  touches this seam substitutes a fake for the service function
  (`crates/terminal-view/src/panel/tests.rs:499`). And a test binary that *did* reach it
  would pass rather than false-fail: `UI_THREAD` is a `OnceLock` nobody sets outside `run()`,
  so `debug_assert_ne!(Some(&current), None)` holds.
- `outcome_of` / `notification_for` are pure and tested, including that `ERROR_CANCELLED`
  is silent and that the code number reaches the user on any other failure; and
  `error_cancelled_matches_windows` pins the spelled-out `1223` to `windows_sys`' own
  constant on Windows.

**Delta 2 — the border.** `crates/workspace/src/layout/title_bar.rs:70` is now an
unconditional `.border_color(cx.theme().border)`. A grep of that file for `is_restricted`
and `elevation` returns only the new suffix element (`:85`, `:116-118`) — no conditional
colour anywhere. `DEC-0019` M5 carries **both** amendments, newest first, and the earlier
one keeps the reason the colour cost nothing to lose (*"colour alone is not a marker: themes
are user-editable"*).

**Delta 3 — the console.**

- *The buffer probe cannot misread a count.* `GetConsoleProcessList` is documented to return
  the **required** element count when the buffer is too small, storing nothing. With
  `[0u32; 2]`: one owner returns 1 → `Free`; any larger set returns its true count, which is
  > 1 → `Keep`. A count of exactly 1 from a too-small buffer is not a state the API can
  produce. A failed call returns 0, which `console_action(true, 0)` maps to `Keep` — asserted
  in the test, with the reason in the comment.
- *A legitimately inherited console is kept.* The case the finding asks about — an
  administrator prompt running `oneterm.exe --elevated-shell cmd` by hand — is `owners >= 2`
  and therefore `Keep`, asserted at `(true, 2)` and `(true, 8)`. Confirmed for the ConPTY
  case too: a debug build started from a shell inside OneTerm or Windows Terminal counts the
  host and the shell as well, so it is never 1.
- *Nothing logs before the release.* The only output reachable before
  `release_own_console()` is `fatal_message`'s `eprintln!` — and every one of its call sites
  is followed immediately by `process::exit`, so the release is not reached on that path and
  the message stays on screen. `process_elevation`'s two `log::error!` calls run before
  `env_logger` is installed and are discarded (NEW-9). *After* `FreeConsole`, `env_logger`
  writes to a handle that is no longer valid; it ignores write errors, so there is no panic
  and no second failure — the elevated instance simply has no live log, which the packet
  states and justifies against M4.
- *The gate is `is_restricted()` **and** sole ownership*, in that order
  (`crates/app/src/lib.rs:90-96`), so an ordinary developer build never loses its console,
  and a release build stops at `GetConsoleWindow()` being null.
- *The `SW_HIDE` probe is recorded honestly.* `US-0130`'s third rework prints the real
  output of both runs, including that `SW_HIDE` hid `Zed::Window` as well as
  `ConsoleWindowClass`, names the benign verb it used (`open`, never `runas`), and the
  conclusion it draws — `SW_SHOWNORMAL` stays — is what the code does.

**Delta 4 — the half that is right.** `window_title_parts` is a *view* of `window_title`,
not a copy: `the_two_spans_are_exactly_the_window_title` concatenates the two spans and
asserts the result **is** `window_title(elevation)` for all three states, and
`an_unrestricted_window_has_no_suffix_span` checks the other direction while asserting both
restricted states do have a suffix, so it cannot pass vacuously. The suffix is a sibling
`div` after `self.app_menu_bar` (`title_bar.rs:85`), so `gpui-component` is not patched.
`Option<AnyElement>` plus `.children(…)` means an ordinary window renders no element at all
rather than an empty one.

**And one question answered in the negative.** *Does adding an explicit `warning` change any
existing rendering — the danger/warning toasts, the git status widget, the agent card — in
any theme?* **No, in none of them, in any of the 39 variants.** Because the key is inert,
`Warning` notifications (`notif_ext.rs:86`), the agent card's Blocked/Stale colour and its
context-usage `success → warning → danger` tint (`card.rs:65,75,582`), the terminal's paused
progress and `Alert::warning` (`render.rs:510,533,603`), the forwarding-row warning and the
two settings notes all render exactly what they rendered at `c135b425`. (The git status
widget does not read `warning` at all: a grep of `crates/workspace/src/widgets/` for it
returns nothing.) That total absence of
any visual change is the tell: a token that 12 light themes were supposed to have restyled
should not be invisible in a diff of the rendered output.

---

## Mutation checks

One per delta that owns a test, each reverted immediately (`git status` clean afterwards).

| Mutation | Test | How it failed |
| --- | --- | --- |
| `console_action`: `owners == 1` → `owners >= 1` | `elevation::tests::only_a_console_of_our_own_is_released` | `assertion left == right failed  left: Free  right: Keep` — the inherited-console case, which is the one that matters |
| `window_title_parts`: `Elevation::Elevated => ("OneTerm", None)` | `config::elevation::tests::the_two_spans_are_exactly_the_window_title` | `the spans must concatenate to exactly the OS title for Elevated  left: "OneTerm"  right: "OneTerm (Administrator)"` |

Both tests are load-bearing rather than decorative. Note what the second one does **not**
catch, and cannot: it pins the *words*, and MAJ-5 is about the *colour*. There is no test
anywhere that reads the colour the suffix is drawn in — which is why a dead JSON key
survived a green gate.

## Commands

All at `16d32217`, `$env:CARGO_BUILD_JOBS=6`.

```
cargo test -p oneterm-app -p oneterm-core -p oneterm-workspace -p oneterm-theme
  -> exit 0. oneterm-app 24, oneterm-core 78, oneterm-theme 2,
     oneterm-workspace 36 (+3 ignored, the elevation ones). 0 failed in every binary.

python scripts/check-theme-contrast.py
  -> check-theme-contrast: 1404 foreground/surface pairings across 390 token/variant rows,
     all >= 4.5:1; primary text out-reads muted.foreground on all 585 shared-surface
     comparisons
     (exit 0 -- and 39 of those 1404 measure a key gpui-component does not read: MAJ-5)

pwsh scripts/ci-local.ps1
  -> exit 0, final line: ci-local: all checks passed.
```

Probes, both added temporarily and removed (`git status` clean):

```
crates/theme/src/theme.rs                                -> parsed colors.warning is None
crates/workspace/src/layout/workspace/layout_tests.rs    -> Theme::warning for all 39
                                                            variants after apply_config
```

`target/fast-dev` was not built, so there is nothing to delete.

## Gaps in this re-verification

- **The elevated side is still unrun**, for the same reason as both sections above: the
  consent prompt is drawn on the secure desktop and this session may not raise one. No
  elevated process was started, and no `oneterm.exe` was enumerated or signalled.
- **`release_own_console()` was never executed.** Its decision half is asserted
  (`console_action`) and its query half is asserted to be self-consistent
  (`the_console_queries_agree_with_each_other`), but `FreeConsole()` itself is only reached
  in an elevated, console-subsystem process. The owner's step-0 run on `8113440e` is the
  only evidence that the console actually disappears, and the *kept* branch — starting the
  elevated exe from an administrator console by hand — has no evidence at all.
- **MAJ-5 is proven at the token, not at the pixel.** The probe reads the `Hsla` the
  renderer would use; nobody photographed an illegible title bar. A screenshot of an
  elevated Ayu Light window would close it, and needs an elevated window.
- **No GUI walk.** Nothing in these four deltas changes the unelevated rendering path except
  the suffix element, which renders nothing at all when the window is not restricted.
- Over-the-shoulder elevation, Group Policy denial and a real `ShellExecuteExW` failure
  remain unexercised, as in both earlier sections.
