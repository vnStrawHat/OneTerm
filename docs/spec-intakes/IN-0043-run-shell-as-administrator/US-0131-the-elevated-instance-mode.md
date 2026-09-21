# Work: The elevated instance mode

ID: US-0131
Intake: IN-0043
Created: 2026-09-21

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [x] In progress
- [ ] Implemented
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
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

After implementation, record commands, results and gaps. Known in advance:

- The consent prompt cannot be automated; E2-E9 are manual with screenshots.
- Over-the-shoulder elevation needs a second account. If one is not available, the
  "starts on defaults and says so once" clause is verified by pointing the configuration
  directory elsewhere, and the difference from a real second account is stated rather than
  glossed over.
- UIPI drag-and-drop into an elevated window is not exercised: under M1 there is no SFTP
  panel there to drop onto.
- `cfg(unix)` gates are compile-and-unit-tested only.

## Handoff

Next: `US-0132` (the menu rows and the documentation). It depends on this packet because
the rows must already be suppressible in an elevated window before they are shown in a
normal one.
