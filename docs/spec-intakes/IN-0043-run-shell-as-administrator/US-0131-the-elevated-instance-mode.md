# Work: The elevated instance mode

ID: US-0131
Intake: IN-0043
Created: 2026-09-21

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability
- Risk lane: **high_risk**
- Risks, named so the review knows what to look at:
  1. **A marker that can lie.** If anything other than the process token feeds the
     "Administrator" marker, a window can claim an elevation it does not have, or hide one
     it does. M5 is the whole point of the packet and the first thing to check.
  2. **A gate that is missed.** The gates are spread over five crates. A write path left
     unguarded means an elevated process writing a configuration document the unelevated
     one owns (M4); a surface left ungated means SSH, SFTP or the updater running under an
     administrator token (M1, M2). Every guard below sits in the **shared** function rather
     than at its call sites, so a sixth caller added later is covered by construction.
  3. **Removing a validation.** This packet only adds gates, but two of them sit on
     existing safety code — the crash store's path validator
     (`crates/app/src/crash_report.rs:67-79`) and the `persist_blocked` refusal
     (`crates/settings/src/ui_config.rs:140-164`). Neither may be weakened to make the new
     behaviour fit.
  4. **An unelevated window that loses something.** Every gate must be a no-op when the
     token says not elevated. The regression to watch is a guard whose condition is wrong
     in the other direction.
- Spec Intake: `IN-0043` (Run a Windows shell as administrator)
- Decision inherited: `DEC-0019` (Accepted 2026-09-21, option A with M1-M7)
- Depends on: `US-0130`

## Outcome

An elevated OneTerm window is unmistakable, and it is a deliberately smaller application
than the ordinary one.

- **It says what it is, from the token.** `OneTerm (Administrator)` in the OS title bar and
  in the in-app title bar, plus a distinct title-bar border from a theme token. The source
  is `GetTokenInformation(TokenElevation)` and nothing else: the launch argument never
  contributes. A window elevated by any route says so; no window can be made to claim an
  elevation it does not have.
- **It runs local shells and nothing else.** No SSH Sessions panel, no SFTP, no Quick
  Connect or New Saved Session rows, no Agent panel, no right dock and no right-dock mode
  toggles. The "+" menu lists the three Windows shells.
- **It does not update itself.** No startup check, no manual check, no download, no
  install; the About/Updates surface says updates are done from the normal window.
- **It reads configuration and writes none.** Theme, font and key bindings are read;
  `docks.json`, `ui_config.json`, `ssh_session.json`, `terminal.json` and
  `update_config.json` are never written.
- **Its crash reports live in `crashes/elevated/`**, and it never shows the GitHub-draft
  crash dialog.

One switch drives all of it, and the switch is the token:

> The argument selects the shell. The token decides everything else.

## Scope

- [ ] In scope — one row per gate, each in the shared function rather than at its callers:

  | M | File | Change |
  | --- | --- | --- |
  | M5 | `crates/app/src/elevation.rs` | `process_is_elevated()` — `OpenProcessToken` + `GetTokenInformation(TokenElevation)`, `#[cfg(not(windows))]` a `const fn` returning `false` |
  | M5 | `crates/app/src/window.rs:66` | `set_window_title("OneTerm (Administrator)")` when elevated |
  | M5 | `crates/workspace/src/layout/workspace/mod.rs:262` | `AppTitleBar::new("OneTerm (Administrator)", ...)` when elevated |
  | M5 | `crates/workspace/src/layout/title_bar.rs:63` | `.border_color(cx.theme().warning)` when elevated |
  | M1 | `crates/terminal-view/src/panel/terminal_panel.rs:705-742` | the SSH block of the "+" menu is not emitted; `FIXED_ROWS` (`:763`) counts the rows actually emitted |
  | M1 | `crates/workspace/src/layout/workspace/mod.rs:204-214`, `layout.rs:38-41`, `:65-76` | `panel_names::SSH_CLIENT` is not built and `set_dock(DockPlacement::Right, ...)` is not called |
  | M1 | `crates/workspace/src/layout/workspace/mod.rs:263` | the title bar's `child` (the mode toggle group) is not attached |
  | M2 | `crates/app/src/window.rs:58` | `start_auto_check` not called |
  | M2 | `crates/settings-ui/src/updates/actions.rs:85`, `install.rs:12` | early return |
  | M2 | `crates/settings-ui/src/updates/groups.rs:22-36` | the Updates group becomes one line of text |
  | M4 | `crates/settings/src/ui_config.rs`, `crates/settings/src/terminal_settings/persist.rs` | `persist_blocked` set at load when elevated — **reuses the refusal that is already there** (`ui_config.rs:140-164`, `persist.rs:163-171`) |
  | M4 | `crates/workspace/src/layout/workspace/persistence.rs` | one guard in `save_state_logged`, covering all four call sites (`mod.rs:422`, `mod.rs:453`, `layout.rs:25`, `layout.rs:96`) |
  | M4 | `crates/session-ui/src/session_state.rs:483`, `crates/settings-ui/src/updates/config.rs:63` | guarded |
  | M7 | `crates/app/src/crash_report.rs:81-83` | `crashes_dir()` gains the `elevated` component — one function, so the writer, the reader and the path validator stay consistent |
  | M7 | `crates/app/src/window.rs:59-65` | `show_crash_reports` not called; `load_pending_reports()` (`lib.rs:56`) still runs so promotion and the newest-20 retention still happen |

- [ ] Out of scope:
  - **The "Run as administrator" menu rows** (`US-0132`). This packet makes an elevated
    window correct; the next one makes it reachable from the menu. The elevated instance's
    *suppression* of those rows lands with the rows themselves, in `US-0132`, because there
    is nothing to suppress until they exist.
  - The command line, the launch and the trusted paths (`US-0130`).
  - Any change to what an **unelevated** window does. Every gate is a no-op there, and the
    acceptance says so explicitly.
  - Weakening `oneterm.rc` (`crates/app/assets/oneterm.rc` has no `RT_MANIFEST` and gains
    none) or adding an auto-elevation path.
  - A theme change. `warning.background` / `warning.foreground` and `title_bar.border`
    already exist in every built-in theme; no theme JSON is edited and
    `scripts/check-theme-contrast.py` is untouched, because a **border** introduces no new
    text surface.

## Acceptance

- [ ] **The marker comes from the token.** An elevated OneTerm shows all three markers; an
      unelevated one shows none. A **non-elevated** process given
      `--elevated-shell cmd` opens Command Prompt and shows **no** marker and **no**
      restrictions — it logs one `warn` and is otherwise an ordinary window. (The test
      that proves the argument cannot forge the marker.)
- [ ] An OneTerm elevated by right-click -> Run as administrator, with **no** argument, is
      marked and fully restricted. There is no elevated-but-unrestricted state to reach.
- [ ] The elevated window's title reads `OneTerm (Administrator)` in the OS title bar (so
      the taskbar, Alt-Tab and every screenshot carry it) and in the in-app title bar, and
      the title bar's border is the theme's warning colour. The marker is readable in every
      built-in theme, light and dark.
- [ ] The elevated window has **no right dock**, no right-dock mode toggles in the title
      bar, no SSH Sessions separator or saved sessions in the "+" menu, no
      "Quick Connect..." row and no "New Saved Session..." row. The "+" menu shows the
      three Windows shells.
- [ ] The elevated window's "+" menu popup scrolls only when its own rows exceed the kit's
      height cap: `FIXED_ROWS` is right for the rows it actually emits, not for the
      unelevated menu's.
- [ ] The elevated window performs **no** update network request at start-up, and its
      manual check and install actions do nothing if reached by a key binding. Its
      About/Updates surface is one line saying updates are checked and installed from the
      normal OneTerm window, with no preferences, no status and no button.
- [ ] **Five hashes.** `ui_config.json`, `terminal.json`, `docks.json`,
      `ssh_session.json` and `update_config.json` are byte-identical before opening an
      elevated window and after closing it — including after the exit-time layout write
      (`crates/workspace/src/layout/workspace/mod.rs:253-259`), which is the write most
      likely to be missed.
- [ ] The elevated window still **reads** theme, font size and key-binding overrides from
      `ui_config.json`, so it looks like the user's OneTerm. Its right-dock mode is forced
      to `None` regardless of what is persisted.
- [ ] A panic in an elevated build writes its report under `<config>/crashes/elevated/`,
      and **no** crash dialog appears in the elevated window. The normal window's dialog on
      a later launch does not show that report. Promotion of stale `.native.tmp` files and
      the newest-20 retention still run in `crashes/elevated/`, so it does not grow without
      bound.
- [ ] `crash_report::delete_pending_report`'s guard (`crash_report.rs:67-79`) still rejects
      a path outside the managed store — in **both** modes. It must not have been loosened
      to accommodate the new subdirectory.
- [ ] **An unelevated window is unchanged in every respect**: title, title-bar border,
      right dock, menu, updater, config writes, crash dialog and crash path. This is the
      regression clause and it is checked deliberately, not assumed.
- [ ] An elevated instance whose configuration cannot be read (over-the-shoulder
      elevation, a different profile) starts on the defaults and says so **once**, as one
      info notification. The marker is still correct, because it comes from the token.
- [ ] `cargo test --workspace` green; Linux and macOS build and test green.

## Documentation

### Owning Docs Reviewed

- `docs/decisions/DEC-0019-elevated-shells-open-in-an-elevated-window.md` — rule 2 and
  mitigations M1, M2, M4, M5, M7, which this packet implements.
- `docs/spec-intakes/IN-0043-run-shell-as-administrator/low-level-design/elevated-instance.md`
  — section 7 (the switch and the seam table, row by row) and section 8 (error paths). The
  seam table is the checklist this packet is accepted against.
- `docs/gui-layout.md` — "Panel registration and presentation" (line 104, the "+" menu top
  to bottom), the right-dock mode toggles, and the persisted panel-name contract. The
  elevated window shows a **subset** of what this document describes; nothing in the
  contract changes, and the subset is written down in `US-0132`.
- `docs/auto-update.md` — "Update check flow" (line 275) and "Installation behavior"
  (line 307): the Windows helper replaces the whole distribution directory and keeps a
  rollback copy. Under an administrator token that helper can write `C:\Program Files` and
  leave files the ordinary instance cannot replace, which is why M2 removes the path
  entirely rather than warning about it. The paragraph lands in `US-0132`.
- `docs/crash-reporting.md` — "Capture boundary" (the `crashes/` store and the identity
  format), "Reconciliation and retention" (promotion skips staging owned by another live
  PID; each instance prunes to the newest 20), "Recovery lifecycle" (the Dismiss / Copy /
  Create Issue dialog). The split store and the suppressed dialog are exactly the two
  things this document currently states unconditionally; the paragraph lands in `US-0132`.
- `docs/agents/persistence.md` — storage and ownership rules for the five documents M4
  covers.
- `docs/agents/error-policy.md` — the "says so once" notification, and the rule that a
  best-effort step logs with its operation name and continues.
- `docs/PROJECT.md` — standing invariants and verification commands.
- `AGENTS.md` section 3.4 — theme tokens are read from `cx.theme()`, never hardcoded; the
  contrast gate governs text tokens and their surfaces.

### Documentation Action

**Update required, in `US-0132`, not here.** Three owning contracts state behaviour this
packet makes conditional — `docs/gui-layout.md:104` (what the "+" menu and the right dock
offer), `docs/auto-update.md` (what the updater does) and `docs/crash-reporting.md` (where
reports land and that a dialog shows them). All three need the same sentence: *"in an
elevated window, ..."*, and that sentence only makes sense to a reader once there is a
documented way to get an elevated window, which `US-0132` adds. Writing it here would
document a window no user can open.

The durable record in the meantime is `DEC-0019` (M1-M7, accepted) and the detail design's
seam table, both of which already name every line this packet touches.

Reason: the three owning contracts describe user-reachable behaviour, and the route to the
elevated window is `US-0132`.

### Reconciliation

Before completion: confirm the detail design's section 7 seam table still matches the code
line for line (if a seam moved, fix the table in the same commit), and record here that
`docs/gui-layout.md`, `docs/auto-update.md` and `docs/crash-reporting.md` are deliberately
deferred to `US-0132` with that packet named.

**Done.** The detail design's section 7 seam table matches the code, with the three
differences recorded in Evidence below and written back into the design in the same commit:
the M4 guard is a `write_refusal(elevated)` per document rather than a `persist_blocked` set
at load, the first-run default write is also suppressed, and M2 covers the About **dialog**
as well as the settings group. `docs/gui-layout.md`, `docs/auto-update.md` and
`docs/crash-reporting.md` are deliberately deferred to **`US-0132`**, which is the packet
that makes an elevated window reachable and therefore the first point at which a reader
could meet any of it.

## Context

- **`persist_blocked` already exists and means exactly this.** `ui_config.rs:133-145` and
  `terminal_settings/persist.rs:163-171` already carry a "the file could not be read;
  refusing to overwrite it" flag that both `save_to` and `persist` honour. M4 sets the same
  flag for a different reason. No new refusal mechanism is written.
- **`crashes_dir()` is one function** (`crash_report.rs:81-83`) and the writer
  (`prepare_capture_paths`, `:31-39`), the reader (`load_pending_reports`, `:59-64`) and
  the path validator (`delete_pending_report`, `:67-79`) all route through it. Changing it
  changes all three consistently; guarding them separately would let them drift.
- **`save_state_logged` is one function** with four callers. One guard, four call sites
  covered, and a fifth caller added later covered for free.
- **The right dock needs no extra guards.** `sync_right_dock_mode` (`mod.rs:317-319`) and
  `apply_right_dock_width` (`mod.rs:403`) already early-return on `!has_dock(Right)`, so
  not building the dock is sufficient.
- **Feature `init`s must all still run.** `crates/app/src/init.rs:57-61` records the order
  invariant: a command callback may read its feature's global the moment the
  `AppServices` bundle exists, and `saved_ssh_sessions` reads `SshSessionStore::global`,
  which panics when absent. M1 therefore gates **surfaces**, not `init` calls — smaller
  diff, no panic risk.
- **Theme tokens.** `title_bar.border`, `title_bar.background`, `warning.background` and
  `warning.foreground` are present in every theme under `crates/theme/themes/`. A border is
  used rather than a background so `scripts/check-theme-contrast.py`'s `SURFACES` table
  needs no new entry; if a later change tints `title_bar.background` instead, that surface
  must be added to `SURFACES` and `PARENTS` in the same commit.
- **Why the token and not the argument.** `DEC-0019` rule 2 and M5. A marker derived from
  the argument could be forged by anyone who can start the process, which is anyone running
  as the same user.

## Plan

- [ ] `crates/app/src/elevation.rs`: `process_is_elevated()`, called as the first statement
      of `run()` into `oneterm_core::elevation::set_elevated` (the call site landed with
      `US-0130`; this packet supplies the real implementation behind it).
- [ ] M5 first, and verify it against a real elevated process before anything else: a gate
      driven by a wrong switch is worse than no gate.
- [ ] M7 next (it is `run()`-time and must be right before any elevated build is run for
      long), then M4, then M2, then M1.
- [ ] Unit tests for every gating decision that is a pure function, and a workspace-layout
      test asserting the elevated start-up path produces a dock state with no right dock.
- [ ] Manual Windows run-through of the acceptance list; `pwsh scripts/ci-local.ps1`.

## Decisions

- `DEC-0019` — rule 2 and mitigations M1, M2, M4, M5, M7. Not repeated here.

## Verification Plan

Focused / unit (`cargo test -p oneterm-core`, `-p oneterm-workspace`,
`-p oneterm-settings`, `-p oneterm-app`):

- [ ] The gate helper reports "restricted" for an elevated process **whether or not** an
      argument was given, and "unrestricted" for a non-elevated process **even when
      `--elevated-shell` was given**. The M5 test.
- [ ] The elevated start-up path yields a dock state with no right dock and does not name
      `panel_names::SSH_CLIENT` (in the style of
      `crates/workspace/src/layout/workspace/layout_tests.rs:151-180`).
- [ ] The title text helper returns `"OneTerm (Administrator)"` when elevated and
      `"OneTerm"` otherwise — one function, used by both `window.rs:66` and `mod.rs:262`,
      so the two markers cannot disagree.
- [ ] `crashes_dir()` yields `<config>/crashes` unelevated and `<config>/crashes/elevated`
      elevated, and `delete_pending_report` still rejects a path outside whichever store is
      current.
- [ ] `save_state_logged` writes nothing when elevated and writes as before otherwise;
      `UiConfig::persist` / `save_to` and the terminal-settings equivalents refuse when
      elevated, through the existing `persist_blocked` path.
- [ ] The "+" menu row count (`FIXED_ROWS`) matches the rows actually emitted in both
      modes.

Integration:

- [ ] `cargo test --workspace`.

E2E (manual, Windows interactive desktop; the consent prompt cannot be automated).
Evidence in `evidence/`:

- [ ] **E2** — screenshot of the elevated window showing all three markers at once.
- [ ] **E3** — `whoami /groups` in the elevated tab showing
      `Mandatory Label\High Mandatory Level`. The marker alone is not proof that the
      process is elevated; this is.
- [ ] **E4** — screenshots of the elevated window's "+" menu (three shells, nothing else)
      and of its title bar (no mode toggles, no right dock).
- [ ] **E5** — the normal window in the same session: tabs alive, a live SSH connection
      still responding, its own unmarked title bar.
- [ ] **E7** — `Get-FileHash` over the five configuration documents before opening the
      elevated window and after closing it; all five identical.
- [ ] **E8** — screenshot of the elevated About dialog showing the one-line updates text
      and no controls.
- [ ] **E9** — a panic in an elevated debug build lands in `crashes/elevated/`, no dialog
      in the elevated window, and the normal window's next-launch dialog does not show it.
- [ ] The marker read in a light theme and a dark theme, to confirm the border is visible
      in both.

Platform:

- [ ] `pwsh scripts/ci-local.ps1` green, including
      `python scripts/check-theme-contrast.py` (which must still pass untouched — if it
      does not, the marker was implemented as a text-on-new-surface change and the
      `SURFACES` table was not updated).

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### The seam table as built

| M | Seam | As built |
| --- | --- | --- |
| M5 | `crates/app/src/elevation.rs` | `process_is_elevated()` — `OpenProcessToken` + `GetTokenInformation(TokenElevation)`; a `const fn` returning `false` off Windows. A failed query is treated as **not** elevated and logged at `error`. Landed with `US-0130`, because `run()` calls it before the crash paths |
| M5 | `crates/core/src/config/elevation.rs` | `window_title(elevated)` — the one source both title bars read |
| M5 | `crates/app/src/window.rs` | `set_window_title(window_title(elevated))` |
| M5 | `crates/workspace/src/layout/workspace/mod.rs` | `AppTitleBar::new(window_title(elevated), ..)` |
| M5 | `crates/workspace/src/layout/title_bar.rs` | `.border_color(cx.theme().warning)` when elevated. No theme JSON touched, no new text surface |
| M1 | `crates/terminal-view/src/panel/terminal_panel.rs` | the menu returns after the three shells; `FIXED_ROWS` is gone, replaced by `menu_rows(elevated, session_rows)`, and `menu_scrolls(rows, window)` carries the estimate's existing `ponytail:` caveat |
| M1 | `crates/workspace/src/layout/workspace/layout.rs` | `right_dock()` returns "no dock" when elevated, so `SSH_CLIENT` is not built and `set_dock(Right, ..)` is not called by either layout builder |
| M1 | `crates/workspace/src/layout/workspace/actions.rs` | `switch_right_dock_mode` returns early — one guard in the shared function, so the startup apply *and* a key binding on `SetRightDockMode` are both covered |
| M1 | `crates/workspace/src/layout/workspace/mod.rs` | the title bar's `child` (the mode toggle group) is not attached |
| M2 | `crates/settings-ui/src/updates/mod.rs` | `elevated_never_updates()` — one helper, called by `start_auto_check`, `check_now` and `download_and_install_update` (before its confirm dialog) |
| M2 | `crates/settings-ui/src/updates/groups.rs`, `crates/settings-ui/src/about.rs` | the Updates group is one line, `ELEVATED_UPDATES_TEXT`; the same line replaces the About **dialog**'s controls and its "Check for Updates" footer button — the design named only the settings group, and the dialog is the surface a user actually reaches |
| M4 | `crates/settings/src/ui_config.rs`, `crates/settings/src/terminal_settings/persist.rs` | `write_refusal(elevated)` — one function per document, asked by both shared write entry points |
| M4 | `crates/settings/src/ui_config.rs`, `crates/settings/src/terminal_config/document.rs` | the **first-run default is no longer written** when elevated |
| M4 | `crates/workspace/src/layout/workspace/persistence.rs` | one guard in `save_state_logged`, covering all four call sites |
| M4 | `crates/session-ui/src/session_state.rs`, `crates/settings-ui/src/updates/config.rs` | one guard in `save` / `persist_update_config` |
| M7 | `crates/app/src/crash_report.rs` | `crashes_dir_in(config_dir, elevated)` behind `crashes_dir()`; the writer, the reader and the path validator all still go through the one function |
| M7 | `crates/app/src/window.rs` | `show_crash_reports` not called; `load_pending_reports()` still runs, so promotion and the newest-20 retention still happen in `crashes/elevated/` |

### Deviation worth the owner's attention

The design said M4 would "reuse the mechanism that is already there" by setting
`persist_blocked` at load. As built the guard is a separate reason inside each document's
shared write function, and `persist_blocked` keeps its own meaning ("the file could not be
read; it may still be the user's"). Two things fall out of that, both wanted:

1. The **first-run default write** is now covered. Setting `persist_blocked` after load
   would not have stopped it, because `UiConfig::load_from` and `TerminalConfig::load_from`
   write the default file *during* the load. Under over-the-shoulder elevation that is a
   file created in the administrator's profile — exactly the trace M4 forbids.
2. `persist_blocked` stays available as the "this window is on the defaults" signal, which
   is what drives the one info notification M4's failure path asks for.

### Commands

- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo test --workspace` — green. New tests: `window_title` both ways and *the argument
  never makes the process elevated* (`oneterm-core`); `crashes_dir_in` both ways and *a
  delete outside the current store is still refused* (`oneterm-app`); `write_refusal` for
  `ui_config.json` and `terminal.json`, each proving the elevated refusal **and** that the
  unreadable-file refusal still stands on its own reason (`oneterm-settings`);
  `menu_rows` in both modes (`oneterm-terminal-view`).
- `python scripts/check-theme-contrast.py` — passes **untouched**, which is the proof the
  marker was done as a border and not as text on a new surface.
- `pwsh scripts/ci-local.ps1` — run at the end of `US-0132`, green.

### E2E actually performed here

Only the **unelevated** side, which is this packet's regression clause: with
`--elevated-shell cmd` given to a non-elevated process the window is titled `OneTerm`, has
the ordinary border, the right dock and all three mode toggles
(`evidence/US-0130-nonelevated-argument-opens-cmd.png`), and with the "+" menu open it shows
the full unelevated row set (`evidence/US-0132-plus-menu-run-as-admin.png`). Every gate is a
no-op there, which is the direction most likely to be got backwards.

### Manual acceptance checklist — for the owner, on an interactive desktop

This session cannot run any of it: the consent prompt is drawn by the AppInfo service on the
secure desktop, and this session is forbidden to raise one. Build with
`cargo build -p oneterm-app --profile fast-dev`; the binary is `target/fast-dev/oneterm.exe`.

Rewritten after the independent verification (`evidence/IN-0043-verify.md` §6), which found
the first version's step 9 not falsifiable and steps 2-4 unable to catch three of the four
majors. Two of the steps below are one keystroke each and both **failed** on the branch the
verifier read.

**`Elevation::Unknown` has no step here, deliberately.** The token query cannot be made to
fail by hand, so there is nothing for a person to do; it is covered by
`an_unknown_token_is_restricted_but_claims_no_elevation` and end to end by the two
re-verification probes, which drove the whole spawn and layout paths under `Unknown`.

**Which directory to hash.** A `fast-dev` build resolves `config_dir()` to the *relative*
`target/`, so the five documents live under the **launcher's working directory**, not
necessarily the repository root. Start the normal window from the repository root and hash
`<repo>\target\*.json`. A release build resolves from `%USERPROFILE%` and the files are
under `~\.OneTerm\`.

0. **The click must not crash the window it was made from.** Start the **normal** window
   from a console (`cargo run -p oneterm-app --profile fast-dev`) so its log is visible, open
   the "+" menu, and click `Run as administrator › Command Prompt`. While the consent prompt
   is up and after it is answered **either way**, the log must contain **no**
   `RefCell already borrowed` line and the window must still be alive and responsive. This
   is the first thing to check because it is what failed: the launch used to run
   `ShellExecuteExW` on the gpui thread, whose message pump re-entered the window procedure
   while the click still held the `App` borrow, and the process aborted at
   `async_context.rs:65` the moment a queued task ran (`US-0130`, second rework). Decline the
   prompt for this step; steps 3 onward consent.
   `evidence/US-0131-E0-no-reentrancy.png` — the console, showing the click and a clean log.

1. **Precondition — put `docks.json` in the state every real user is in.** This is the step
   the old checklist lacked, and without it step 5 proves nothing: `reset_default_layout`
   only runs on a *first ever* launch, so a machine with no `docks.json` would pass a broken
   build. Open the **normal** window, make sure the right dock is open on **SSH Client**, add
   a saved SSH session if there is none, then close the window so the layout is written.
   Confirm `docks.json` now contains `ssh_client_panel`.
2. **E7 setup — record the before state, not only hashes.**
   `Get-ChildItem target\*.json | Get-FileHash | Export-Csv before.csv`, and separately
   `Get-ChildItem target -Force | Select-Object Name > before-names.txt`. The second file is
   the point: a quarantine **renames** a document, which a hash of the same path reports as
   "file not found" rather than as a difference, and the old wording would have missed it.
3. **E2 — the marked window.** Right-click `oneterm.exe` → *Run as administrator*, consent.
   Screenshot showing **both** markers at once: the taskbar / OS title reading
   `OneTerm (Administrator)` **plain**, and the in-app title bar reading the same words with
   **`(Administrator)` highlighted** beside the application name — the theme's warning
   colour, bold. There is deliberately **no** coloured title-bar *border* (owner ruling,
   `DEC-0019` M5 as amended twice), so a coloured border appearing here is a failure, not a
   pass; and the OS title bar must stay plain, because a window title cannot carry a colour.
   `evidence/US-0131-E2-marked-window.png`.

3b. **No console window.** The elevated window must open **alone** — no console window
   beside it, and none behind it in Alt-Tab or on the taskbar. Check in the **debug /
   `fast-dev`** build specifically: that is the console-subsystem one, and `runas` gives such
   a process a console of its own, which is what the owner saw. A release build is
   GUI-subsystem and never had a console.
   `evidence/US-0131-E3b-no-console.png`.

    Consequence to expect, and not a failure: the elevated instance has **no live log**. It
    gives the console up rather than redirecting to a file, which an elevated window has no
    business writing (M4, and checklist step 12 checks exactly that). To read its log, start
    it from an already-elevated prompt with `oneterm.exe --elevated-shell cmd` — that console
    is inherited rather than allocated, so it is kept and written to.
4. **E3 — really elevated.** `whoami /groups` in the elevated tab; screenshot the line
   `Mandatory Label\High Mandatory Level`. `evidence/US-0131-E3-whoami-groups.png`. The
   marker alone is not proof; this is.
5. **The two one-keystroke checks the old checklist could not catch.** Both failed before
   this rework.
   - **MAJ-1.** Before step 3, poison `terminal.json` so the check can fail:
     `"shell": { "env": { "PATH": "C:\\Users\\<you>\\bin", "PSModulePath": "C:\\Users\\<you>\\modules" }, "cwd": "C:\\Users\\<you>\\stage" }`.
     Then in the elevated **Command Prompt** tab run `echo %PATH%` and `where.exe cmd`:
     `PATH` must be the machine's with **no** entry from that file, and `cmd` must resolve
     under `%SystemRoot%\System32`. In an elevated **PowerShell** tab run `$env:PATH` and
     `$env:PSModulePath` — `PSModulePath` is the PowerShell-specific vector, because it
     autoloads a module on the first unresolved command name, so it is the one to check in
     the shell that has it. Finally `cd` with no argument: the working directory must be the
     profile's home, not the `shell.cwd` above.
     `evidence/US-0131-E11-path-and-cwd.png`.
   - **MAJ-3.** Press `Ctrl+Shift+N` in the elevated window. **Nothing must open** — no Quick
     Connect dialog, no session dialog. `evidence/US-0131-E12-ctrl-shift-n.png`.
6. **E4 — the smaller application, and the one that needed step 1.** Screenshot the elevated
   window's "+" menu: exactly three rows (Command Prompt, PowerShell, PowerShell 7), **no**
   `Run as administrator ›` row, no "SSH Sessions" separator, no saved sessions, no
   "Quick Connect...", no "New Saved Session...". Second screenshot of the whole window:
   **no right dock at all** — no Session tree, no SFTP Browser, no Agent panel — and no
   SSH Client / Agent / None segments in the title bar.
   `evidence/US-0131-E4-elevated-plus-menu.png`, `evidence/US-0131-E4-no-right-dock.png`.

   **Then the other direction**, which nothing else in this list checks: `apply_key_bindings`
   re-adds only the *allowed* actions, so a typo in the policy table would silently unbind
   the terminal itself and an elevated window that cannot open a tab would look like a build
   problem rather than a classification one. In the elevated window confirm **`Ctrl+T` opens
   a tab**, **`Ctrl+Shift+C` / `Ctrl+Shift+V` copy and paste**, **a split works** (context
   menu ▸ Split Right) and **`Ctrl+F` opens the search bar**. The packet's own risk 4 is a
   guard whose condition is wrong in the other direction; this is that check.
   `evidence/US-0131-E14-allowed-actions.png`.
7. **No-argument case.** That window was started with **no** argument, which is the proof
   there is no elevated-but-unrestricted state: it is marked and restricted anyway.
8. **MAJ-4 — no log file, and nothing truncated.** Before step 3, create a directory holding
   one file with known content (`"keep me" > <dir>\canary.log`), record its hash, and set
   `"logging": { "local": true, "directory": "<dir>", "write_mode": "overwrite" }` in
   `terminal.json`. After opening the elevated window that directory must contain **no new
   file** *and* `canary.log` must still hash the same. The existing file is the point:
   "no new file" catches creation, and `overwrite` — which truncates — is the destructive
   half of MAJ-4 and would leave the file count unchanged.
   `evidence/US-0131-E13-no-terminal-log.png`.
9. **E8 — the updater is absent.** OneTerm ▸ About in the elevated window: one line,
   *"Updates are checked and installed from the normal OneTerm window."*, no status, no
   preferences, and **no "Check for Updates" button in the dialog footer**. Open Settings:
   the **Network** page must be absent from the sidebar, and the window must carry the line
   saying changes here are not saved.
   `evidence/US-0131-E8-about-updates.png`, `evidence/US-0131-E8-settings-readonly.png`.
10. **Theme check.** Switch the elevated window between one light and one dark built-in theme
    and confirm **the title still reads `OneTerm (Administrator)` in both**, and that the
    highlighted suffix is still *legible* in both — not merely coloured. The words are what a
    theme cannot take away; the colour is checked by
    `python scripts/check-theme-contrast.py`, which now measures `warning` against
    `title_bar.background` in all 39 variants, so this step is confirming the gate rather
    than substituting for it. A light theme is the one worth looking at: the kit's own amber
    fails there, which is why every theme sets the token explicitly.
11. **E9 — the crash store.** In an elevated debug build, trigger a panic. The report must
    land in `<config>\crashes\elevated\`, **no** crash dialog may appear in the elevated
    window, and the normal window's dialog on its next launch must not show it.
12. **E7 — the after state.** Close the elevated window; this is the step that matters,
    because the exit-time layout write is the one most likely to be missed. Then:
    - `Get-ChildItem target\*.json | Get-FileHash` and diff against `before.csv`: for each of
      `ui_config.json`, `terminal.json`, `docks.json`, `ssh_session.json` and
      `update_config.json`, the file must **still exist, at the same path, with the same
      hash**.
    - `Get-ChildItem target -Force | Select-Object Name` and diff against
      `before-names.txt`: the **only** new path may be `crashes\elevated\`. That directory
      *is* M7 working — `prepare_capture_paths` creates it on every elevated start — so the
      old wording "no new file appeared" would have failed every single run. No `*.bak`, no
      `.invalid-*` quarantine sibling, and no new `*.json` may appear.
13. **E5 — the normal window is untouched.** In the same session, the unelevated window still
    has its tabs, a live SSH connection still responding, its right dock, and its own
    unmarked title bar.
14. **E6 — declined.** From the unelevated window, "+" ▸ *Run as administrator* ▸ any shell,
    then **decline** the prompt. Nothing at all: no window, no notification, no change to the
    launching window's tabs, connections or transfers; one `info` log line. A screenshot
    cannot prove "nothing", so also run `Get-Process oneterm` and confirm **no new process**
    exists — a window that opened and closed quickly must not pass as "nothing".
    `evidence/US-0131-E6-declined.png`.
15. **Over-the-shoulder, only if a second administrator account exists.** Elevate with that
    account's credentials and confirm the window starts on the defaults and leaves **no** file
    behind in that account's `~\.OneTerm`. The "says so once" notification fires only when
    `ui_config.json` is **absent** there — if that account has ever run OneTerm, its correct
    behaviour is to stay silent, which is a pass and not a failure.

### Acceptance rework, 2026-09-21 — MAJ-2, MAJ-3, MAJ-4 and four minors

Independent verification (`evidence/IN-0043-verify.md`) marked this packet **FAIL** on three
majors, each a breach of M1 or M4 on a **default** path. Reworked rather than opened as new
BUGs: none of it had been accepted.

**MAJ-2 — an elevated window did get a right dock.** The M1 gate was on the two layout
*builders*. It was not on the *restore*: `load_layout` rebuilds the side docks **by name**,
so `ssh_client` (Session tree + SFTP browser) or `agent` was constructed before either
builder ran — and declining to call `set_dock(Right, ..)` does not remove a dock that is
already there, it leaves it. `reset_default_layout`, the path the design reasoned about, only
runs on a *first ever* launch, so for every user who had run OneTerm once the default path
was the broken one. SSH connections, SFTP transfers and `known_hosts` writes were all
available under an administrator token.
*Fix:* an elevated window **does not read `docks.json` at all** — `startup_dock_document`
returns `None` and the fixed default layout is built every time. Second lock: `right_dock`'s
`None` arm now calls `remove_dock` instead of skipping `set_dock`.
*Tests:* `layout_tests::an_elevated_window_never_reads_the_saved_layout` (the injected reader
**panics** if it is called, so the assertion is "not read", not "read and ignored") and
`layout_tests::the_elevated_startup_path_yields_a_dock_state_with_no_right_dock` — the test
this packet's own verification plan named and the branch did not have. Mutation-checked:
reverting `remove_dock` fails the second with *"an elevated window must have no right dock"*.

**MAJ-3 — `ctrl-shift-n` opened Quick Connect in an elevated window.** Removing a *row* is
not removing an *action*. `new_ssh_session` ships bound globally, and neither
`on_action_new_session` nor `open_quick_connect_dialog` had a guard. The same reasoning that
put the `switch_right_dock_mode` guard in the shared function was not applied one function
further down the file.
*Fix:* one exhaustive table, `oneterm_actions::elevated_policy`, classifying **every**
`BINDABLE_ACTIONS` id; an unclassified id is **denied**. Consulted by
`on_action_new_session`, by `session-ui`'s three public entry points (the
`WorkspaceCommands` seam), by `open_quick_connect_dialog` itself, and — the one that closes
every keystroke route at once, including a user's own override — by `apply_key_bindings`,
which does not bind a denied action at all.
*Test:* `key_bindings_actions::tests::every_bindable_action_is_classified_for_an_elevated_window`
(`oneterm-settings-ui`). A new action added and forgotten fails it, which is the point.

**MAJ-4 — terminal logging was an arbitrary elevated file write.** `logging.local` plus
`logging.directory` from `terminal.json`, ungated, created a file at a config-chosen path
under an administrator token; `LogWriteMode::Overwrite` made it create-or-**truncate**.
*Fix:* `LoggingConfig::runtime_config` — the one function both the local and the SSH caller
resolve through — forces `enabled = false` when restricted.
*Test:* `terminal_config::logging::tests::an_elevated_window_never_opens_a_terminal_log_file`
(`oneterm-settings`).

**MIN-3 — the token query failed open.** A process that *was* elevated and failed the query
got `is_elevated() == false`, so **none** of M1/M2/M4/M7 applied: SSH, SFTP, the updater and
`terminal.json`'s program, all under an administrator token, unmarked. The corollary "there
is no elevated-but-unrestricted state to reach" was false in exactly that branch.
*Fix:* `Elevation` is now three-valued. `Unknown` is **restricted** (fail closed) and its
marker reads `OneTerm (elevation unknown)` with the warning border — claiming
"(Administrator)" would assert an elevation the token never confirmed, which `DEC-0019`
rule 2 forbids. `is_elevated()` is gone; there is one predicate, `is_restricted()`, because
two that differ only in the `Unknown` case invite guarding a security seam with the wrong one.
*Test:* `config::elevation::tests::an_unknown_token_is_restricted_but_claims_no_elevation`.

**MIN-2 — a corrupt document was quarantined (renamed) by the elevated process.** The M4
guards sat on the write entry points; a quarantine is a rename and is not one of them.
*Fix:* one guard in `oneterm_core::persistence::quarantine_file`, covering
`ui_config.json`, `terminal.json` and `docks.json` at once. The elevated window starts on the
defaults, says so once, and leaves the file for the ordinary window.

**MIN-4 — settings accepted edits and discarded them, and the Network page survived M2.**
*Fix:* the Network page is not built when restricted, and the settings window carries one
line: *"This is an administrator window: settings changed here apply until it closes and are
not saved."*
*Test:* `panel::tests::the_elevated_settings_note_says_the_edits_are_not_saved`.

**MIN-5 — the "untestable" claim is retracted.** The previous Gaps section said the
`docks.json`, `ssh_session.json` and `update_config.json` guards could not be tested because
flipping the process global would race the other tests in the binary. The verifier showed the
technique that works, and all three now have real elevated-branch tests. Each flips the
global, restores it on every exit path including a panic, and is marked
`#[ignore = "flips the process-global elevation switch; run alone with --exact"]` — the
convention this repository already uses for tests that cannot share a process
(`session_orphan_tests::orphan_liveness_table`). Five such tests exist; each was run alone:

```text
cargo test -p oneterm-workspace   -- --exact --ignored layout::workspace::layout_tests::an_elevated_window_never_reads_the_saved_layout
cargo test -p oneterm-workspace   -- --exact --ignored layout::workspace::layout_tests::the_elevated_startup_path_yields_a_dock_state_with_no_right_dock
cargo test -p oneterm-workspace   -- --exact --ignored layout::workspace::persistence::tests::an_elevated_window_writes_no_dock_layout
cargo test -p oneterm-session-ui  -- --exact --ignored session_state::tests::an_elevated_window_writes_no_saved_sessions
cargo test -p oneterm-settings-ui -- --exact --ignored updates::config::tests::an_elevated_window_queues_no_update_config_write
```

All five: `test result: ok. 1 passed`. The `docks.json` guard moved from `save_state_logged`
into `save_state_to`, which is both deeper (every dock write passes through it) and takes an
explicit path, so the test is not aimed at the developer's real configuration directory.
`update_config.json` asserts that **nothing reached the persist queue**, which is
deterministic where "no file appeared" would be racy.

**MIN-1 — `lpDirectory`.** Documented, not changed: it is *inside* the trust boundary, and
only the release `config_dir()` (from `%USERPROFILE%`) makes that harmless. A debug or
`fast-dev` build resolves `config_dir()` relative to the launcher's working directory, which
any same-user process chooses when it calls `ShellExecuteExW` itself. Recorded in the detail
design as a developer-build-only exposure and a severity multiplier, not a finding on its own.

### Final pass, 2026-09-21 — the re-verification's six new items

Re-verification (`evidence/IN-0043-verify.md`, "Re-verification of 24226d4c") returned
**PASS** on all three packets and recorded six new minors, none blocking. All six are closed.

**NEW-4 — the five security tests ran in no gate.** The one that mattered: they are the only
automated proof of the M1 restore gate and three M4 guards, and `cargo test --workspace`
skips an ignored test. A reader would reasonably have assumed CI covered them. Four lines
now, added to `scripts/ci-local.ps1`, `scripts/ci-local.sh`, `AGENTS.md` §4 and the **Full
workspace quality gate** job of `.github/workflows/ci.yml`, immediately after
`cargo test --workspace`:

```text
cargo test -p oneterm-workspace   --lib -- --ignored --test-threads=1   # 3 tests
cargo test -p oneterm-session-ui  --lib -- --ignored --test-threads=1   # 1
cargo test -p oneterm-settings-ui --lib -- --ignored --test-threads=1   # 1
python scripts/check-ignored-tests.py                                   # the census
```

Per **crate**, not per test name, so a new elevation test in one of the three is picked up
for free where a hand-written list of five names would silently not be. `--ignored` already
selects only the ignored tests and `--test-threads=1` makes them sequential in one process,
each restoring the global on `Drop` before the next starts — so the `#[ignore]` reason text
relaxed from *"run alone with `--exact`"* to *"run with `--test-threads=1`"*, which is what
the gate actually does. Linux-only in CI: all five drive pure logic and gpui's
`TestAppContext`, with no `cfg(windows)` site in them.

The census (`scripts/check-ignored-tests.py` + `scripts/ignored-tests.txt`, **17** entries
today) is what stops the scoping rotting: it diffs the full `--ignored --list` against a
checked-in file, so **any** new ignored test anywhere fails the gate until someone records it
and decides whether it belongs in the elevation run. Same pattern as `elevated_policy` — an
exhaustive table where unclassified fails the build — applied to the ignore list. Re-record
with `--write` after a deliberate change.

**NEW-1 — the LLD's Interfaces block still declared the removed API.** The behavioural prose
was reconciled thoroughly and the interface contract was not: `set_elevated(bool)`,
`is_elevated()` and `process_is_elevated() -> bool` were still declared, with `bool`
signatures contradicting the three-valued `Elevation` the same document describes. Fixed,
along with the diagram, the start-up sequence, the token-query snippet and the two prose
mentions. The snippet now shows the real three-valued body and the paragraph beneath it says
why a `bool` was the wrong shape.

**NEW-3 — `open_duplicate_ssh_dialog` consults the policy**, like the other three
`session-ui` entry points. Unreachable today, and that is precisely the reasoning the policy
table rejects for the `sftp_*` ids: *"unreachable is a property of the current layout and
this is a property of the action."* One `if`.

**NEW-2, NEW-5, NEW-6 — three documentation gaps, all real.**

- The README said `docks.json` was "read and never written". It is not **read** either, and
  the visible consequence — an administrator window never restores your saved layout — is
  the load-bearing half of the MAJ-2 fix and was not stated for users. Now it is, with the
  reason.
- `cwd: None` means Duplicate tab and New Terminal Here no longer inherit the live OSC 7
  working directory in an elevated window. Correct security answer, real behaviour change,
  documented nowhere. Now in `docs/terminal-backend.md` §6.1.1, the LLD and the README.
- `docs/agents/persistence.md` — the document `AGENTS.md` tells every agent to read *before
  changing persisted schemas or storage mechanics* — had no mention of elevation, although
  this branch made a whole class of writes conditional. It now carries the matrix and the
  three rules that were each missed once: a first-run default write is a write, a quarantine
  is a write, and the guard belongs in the shared function — the one place gated at a
  *builder* instead of at the *restore* is the one that shipped a hole.

**Checklist, three additions.** A step exercising the **allowed** side (`Ctrl+T`, copy/paste,
split, `Ctrl+F`), because `apply_key_bindings` re-adds only allowed actions and a table typo
would silently unbind the terminal itself — the packet's own risk 4, and nothing else checked
it. Step 5 now names `PSModulePath` beside `PATH` and checks it in a PowerShell tab, where it
autoloads on the first unresolved command. Step 8 points `logging.directory` at a directory
holding a known file, so `Overwrite` **truncation** is caught and not only creation. Plus one
preamble sentence saying `Elevation::Unknown` has no manual step deliberately — the query
cannot be made to fail by hand.

**New test:** `elevated_policy::tests::the_local_terminal_stays_usable_in_an_elevated_window`
(`oneterm-actions`) — the allowed side, asserted rather than assumed.

### Acceptance rework, 2026-09-21 — M5 is the title text, and nothing else

Owner ruling on the built work: **remove the warning-coloured title-bar border entirely.**
The marker is the title text alone — `OneTerm (Administrator)` (or
`OneTerm (elevation unknown)`) in the OS title bar and in the in-app title bar.

`DEC-0019`'s M5 is amended in place rather than reinterpreted, because it is an inherited
rule and the next reader must not find the old wording and re-add the colour: *"Amended
2026-09-21 by the owner: title text only, no colour."* The detail design's marker section,
this packet's acceptance, `docs/gui-layout.md`, the README and checklist steps 3 and 10
follow it. Step 10 stops being "is the border visible in both themes" and becomes **"the
title reads the same in a light and a dark theme"**, which is the thing that still has to be
true.

What this does **not** change: the marker still derives from the process token and never
from the launch argument, and the two title surfaces still come from one
`window_title(elevation)` so they cannot disagree. Those tests stay. The border code and
the theme assertion that went with it are deleted.

Worth recording why the colour was there and why losing it costs nothing the decision
relied on: it was chosen as a *border* specifically so it introduced no new text surface and
left `scripts/check-theme-contrast.py`'s `SURFACES` table untouched. Removing it therefore
removes a marker, not a constraint — the contrast gate is unaffected in both directions, and
`DEC-0019`'s own rule that "colour alone is not a marker" always meant the text was carrying
the weight.

### Acceptance tweak, 2026-09-21 — the suffix is highlighted in the app title bar

Owner acceptance: **checklist steps 3-15 all passed**. One tweak on top: in the **app**
title bar the suffix must stand out in a prominent colour. The **OS** title bar stays plain.

**Shape.** Two spans, and the second one is ours. The app title bar's title is the app
*menu's* name, which the kit renders as a single string with no place for a second colour —
and `docs/PROJECT.md` forbids patching `gpui-component`. So the menu keeps the plain
`OneTerm` and `AppTitleBar::render` draws ` (Administrator)` beside it, in
`cx.theme().warning`, bold.

`window_title()` stays the single source: `window_title_parts()` is a **view** of it, not a
second copy, and `the_two_spans_are_exactly_the_window_title` concatenates the spans and
asserts the result *is* `window_title(elevation)` for all three states. That is what stops
the coloured form and the plain OS form drifting apart.
`an_unrestricted_window_has_no_suffix_span` checks the other direction, and asserts both
restricted states *do* have one so it cannot pass vacuously.

**The token, and why this was not one line.** `warning` looked like the natural choice and
turned out not to be a per-theme token at all: **no theme in `crates/theme/themes/` defined
it**, so all 39 variants were inheriting the kit's own default — an amber (`yellow-400` /
`yellow-500`) that the contrast gate could not even see, because it is not in the JSON the
gate reads.

Measured against every variant's `title_bar.background`, that kit amber **fails on all 12
light themes** (1.24:1 on Hybrid Light, 1.40 on Gruvbox Light — yellow on a pale title bar).
It passes comfortably on all 27 dark ones. Surveying every token the themes *do* define, only
four clear the floor there at all — `accent.foreground` (5.92), `foreground` (5.65),
`muted.foreground` (5.00), `secondary.foreground` (4.88) — and not one of them is a
highlight: three are the ordinary text colours and the fourth is "text on an accent fill".
There was no existing token that was both prominent and legible everywhere.

So **`warning` is now an explicit OneTerm theme token**, set by all 39 variants:

- the **27 dark** variants take exactly `#facc15`, the kit's own value — what they already
  rendered, so nothing changes visually there;
- the **12 light** variants take a darker shade of the kit's own yellow ramp. Lightness only,
  same hue family, the smallest move that clears the floor with a little margin:

  | Variant | Value | Ratio on its title bar |
  | --- | --- | --- |
  | Aurora Light | `#a16207` | 4.71 |
  | macOS Classic Light | `#a16207` | 4.88 |
  | Gruvbox Light | `#854d0e` | 4.99 |
  | Mellifluous Light | `#854d0e` | 5.14 |
  | Catppuccin Latte | `#854d0e` | 5.18 |
  | Molokai Light | `#854d0e` | 5.38 |
  | Hybrid Light | `#713f12` | 5.62 |
  | Ayu Light | `#854d0e` | 5.80 |
  | Solarized Light | `#854d0e` | 5.82 |
  | Flexoki Light | `#854d0e` | 5.99 |
  | Zed One Light | `#854d0e` | 6.02 |
  | Everforest Light | `#854d0e` | 6.27 |

**12 variants changed appearance; 39 gained the explicit token.** Making it explicit
everywhere — rather than only in the 12 that needed it — is what lets the gate measure it the
same way it measures every other token, from the repository's own JSON. The alternative was
teaching `check-theme-contrast.py` a literal copied out of `gpui-component`'s default theme,
which would silently go stale the next time the kit is bumped.

`warning` on `title_bar.background` joined `SURFACES` with its draw site, as the script's own
contract requires. The gate now reports **1404 pairings across 390 rows** (was 1365 / 351),
all at or above 4.5:1.

**Tests:** `config::elevation::tests::the_two_spans_are_exactly_the_window_title` and
`::an_unrestricted_window_has_no_suffix_span` (`oneterm-core`), plus
`python scripts/check-theme-contrast.py` for the colour.

**Frame:** `evidence/US-0132-title-bar-unelevated.png` — the non-elevated title bar,
unchanged, with no suffix span at all. The elevated form cannot be framed from this session
(no consent prompt), which is why the split is proven at unit level and the colour by the
gate; checklist steps 3 and 10 are the closing evidence.

### Gaps

- **The whole elevated side is unverified here**, for the reason above. Items 2-12 are the
  owner's. Nothing in this packet's acceptance may be read as proven until they are run.
- Over-the-shoulder elevation needs a second account; none is available here, so the
  different-profile behaviour — including the "says so once" notification, whose condition
  is `elevated && persist_blocked` — is **not exercised at run time**.
- UIPI drag-and-drop into an elevated window is not exercised: under M1 there is no SFTP
  panel there to drop onto.
- `cfg(unix)` gates are compile-and-unit-tested only.
- ~~The `docks.json`, `ssh_session.json` and `update_config.json` guards have no unit test
  because flipping the global would race the other tests in the binary.~~ **Retracted.** The
  claim was stronger than the evidence for it: `--exact` plus a restore-on-drop guard plus
  `#[ignore]` gives each of the three a real elevated-branch test, and all three now have
  one (MIN-5 above). What remains true is narrower — those tests do not run inside
  `cargo test --workspace` and must be invoked individually, which is why the five commands
  are written out above rather than left to be rediscovered.

## Handoff

Next: `US-0132` (the menu rows and the documentation). It depends on this packet because
the rows must already be suppressible in an elevated window before they are shown in a
normal one.
