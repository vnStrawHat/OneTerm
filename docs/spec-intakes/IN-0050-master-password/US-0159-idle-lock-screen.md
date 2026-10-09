# Work: Idle lock screen

ID: US-0159
Intake: IN-0050
Created: 2026-10-09

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [ ] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability
- Risk lane: high-risk (screen protection of a running UI that holds the vault key; input
  blocking; a new persisted field in `ui_config.json`)
- Spec Intake, when required: [`IN-0050`](IN-0050.md); owner rulings Q1-Q10 accepted as
  recommended (2026-10-09; Q1: no lock at startup; Q2: 15 minutes, 1-480, no "Never"; Q5: the
  elevated window applies the lock read-only; Q6: quit allowed while locked; Q7: no reset on the
  lock screen, which explains how to reset);
  [`DEC-0021`](../../decisions/DEC-0021-secrets-at-rest-are-sealed-by-a-master-password-vault.md).
  Depends on `US-0157` (merged, main @ce80f17c); independent of `US-0158` (merged).

## Outcome

Once a master password exists, every OneTerm window (main, Settings, any other `Root`) shows only
a lock screen after `lock_after_idle_minutes` without keyboard or mouse input, or at once on the
`LockOneTerm` action (default `ctrl-shift-l`). The vault key is dropped at the lock; the
application content is not built, laid out or painted while locked, so no terminal pixel, host
name, dialog or toast is on screen and terminal output costs no frames. Only the master password
(checked against the header held in memory) brings the windows back, with focus where it was.
Quit stays possible. Terminal output never counts as activity. No vault means no lock.

## Scope

- [ ] In scope:
  - `crates/state/src/vault_state.rs`: the screen-locked fact beside the key fact
    (`lock_screen`, `unlock_screen`, `is_screen_locked`, `VaultEvent::ScreenLocked`); a screen lock
    sets `locked_meanwhile` like `lock()` so no running operation puts a key back; set-up, unlock
    (dialog), change and reset refused while the screen is locked; a successful reset clears it.
  - `crates/state/src/vault_dialogs.rs`: R2 of `evidence/US-0157-verify.md` (after
    `LockedMeanwhile` the dialog's inputs are cleared).
  - `crates/workspace/src/lock_screen.rs` (new): `IdleClock` global, the keystroke interceptor,
    the deadline timer, the `LockScreen` `RootPlugin` (activity canvas, `decorate` replacement),
    the `LockView` (own title bar, password input, back-off, Quit, reset explanation), the
    `LockOneTerm` handler; the app-menu item in `layout/app_menus.rs`.
  - `crates/actions`: `LockOneTerm`; `elevated_policy.rs` classifies `lock_oneterm` allowed.
  - `crates/settings`: `lock_after_idle_minutes` in `ui_config.json` (optional, default 15,
    clamped to 1..=480 on read, lenient parse), `UiConfig::set_lock_after_idle_minutes`.
  - `crates/settings-ui`: Security page "Screen Lock" group (timeout number input 1-480, "Lock
    Now" locks the screen); Key Bindings row "Lock OneTerm" (`ctrl-shift-l`).
  - `crates/app/src/init.rs`: `oneterm_workspace::lock_screen::init` after every feature init.
  - Docs listed under Documentation.
- [ ] Out of scope (LLD § 7 follow-ups): OS session lock / sleep hooks (`WTSRegisterSessionNotification`);
  swapping the macOS native menu while locked; re-running `use_saved(true)` in an open connect
  dialog after an unlock (`US-0158` note; the dialog falls back to inputs, accepted); a
  "Set up" button inside the no-vault notification.

## Acceptance

From the `IN-0050` US-0159 candidate, made concrete:

- [ ] With a vault, after the timeout both the main and the Settings window show only the lock
  screen; a screenshot taken by another process shows no terminal pixels.
- [ ] Terminal output alone (`ping -t` / a `cat` loop) never prevents the lock; a key or a mouse
  move does (unit test with a fake clock + GUI walk).
- [ ] A tab producing output while locked costs no frames (lock-screen frame count with and
  without output is the same within noise).
- [ ] No workspace action is reachable while locked: a test enumerates every registered key
  binding and asserts none dispatches outside the lock view; `ctrl-shift-q` does nothing.
- [ ] `ctrl-shift-l` (and the app-menu item, and Settings > Security > Lock Now) locks at once;
  the binding is listed and rebindable on the Key Bindings page.
- [ ] Unlock with the master password restores each window's previous focus (a dead handle is
  tolerated); a wrong password shows the inline error, clears the input, and backs off from the
  5th failure (`VaultState`'s count, shared with the dialogs).
- [ ] Quit (button, window close, Alt+F4) works while locked.
- [ ] No vault (Uninitialised / Unavailable): no timer, no lock; the action says why.
- [ ] Sleep longer than the timeout locks on resume: the wall-clock term of the idle time
  (unit-tested decision; a real sleep is walked if possible, otherwise recorded as a gap).
- [ ] `lock_after_idle_minutes`: default 15, round trip, out-of-range values clamp to 1..=480,
  a non-integer value loads as the default without quarantining the file.
- [ ] `VaultState`: a screen lock during an unlock / change / set-up wins (`locked_meanwhile`);
  R2 fixed; the lock screen never re-reads `vault.json`; elevated windows lock and unlock
  read-only.
- [ ] Unit tests leave no temporary directory behind and assert timing only as ratios.

## Documentation

### Owning Docs Reviewed

- `IN-0050.md` — US-0159 candidate, acceptance, rulings Q1-Q11.
- `high-level-design.md` — state machine (IdleLocked), data flow 4-5, lock-screen wireframe.
- `low-level-design/idle-lock-overlay.md` — the contract this packet owns.
- `US-0157-vault-lifecycle-ui.md` (Plan, notes for `US-0159`: `locked_meanwhile`) and
  `evidence/US-0157-verify.md` (R2).
- `US-0158-ssh-saved-passwords.md` (notes for `US-0159`: a refused seal emits `Locked`) and
  `evidence/US-0158-verify.md`.
- `docs/gui-layout.md` — windows, Root, Settings window, Frames and re-rendering.
- `docs/agents/persistence.md` — `ui_config.json` row, elevation table.
- `docs/agents/structure.md`, `docs/agents/crate-dependency-rules.md` — no new crate edge
  (`workspace` already depends on `state`, `settings`, `actions`).
- `docs/agents/code-style.md`, `docs/agents/error-policy.md`.
- `docs/spec-intakes/IN-0048-fps-hud/US-0151-fps-hud.md` — setting + action + Settings row +
  Key Bindings row pattern.

### Documentation Action

- Update required: `idle-lock-overlay.md` (as built), `high-level-design.md` (state machine as
  built, Security wireframe), `docs/gui-layout.md` (lock screen section, Security page),
  `docs/agents/persistence.md` (`ui_config.json` row: `lock_after_idle_minutes`),
  `docs/agents/structure.md` (workspace file list), `IN-0050.md` (US-0159 created).

Reason: a new window-level surface, a new persisted field and a new action.

### Reconciliation

(Filled at completion.)

## Context

- gpui-pre 0.3.7 `Window::dispatch_key_event` runs the interceptors first; when one stops
  propagation no binding is matched, but `finish_dispatch_key_event` still offers the event to
  the focused path's raw key listeners (only the lock view is in the tree while locked).
- `App::notify` invalidates only windows whose last frame rendered the entity
  (`tracked_entities`), which is what makes hidden terminal output free.
- `Root::render` folds `decorate` in registration order; `gpui_component::init` registers
  `WindowState` first, so a plugin registered from `app::init` decorates last.

## Plan

- [ ] Records first (commit 1, docs only): this packet, `IN-0050.md` candidate ticked.
- [ ] Spike in the real build: `decorate` replacement (focus restore, input / IME, title bar
  drag / close, DWM thumbnail of a minimised window); result recorded below before the rest.
- [ ] `VaultState` + R2 + tests; `UiConfig` field + tests; action, binding row, policy;
  `lock_screen.rs` + tests; Security page; app init; docs.
- [ ] Gate; GUI walk on Windows with a private profile; evidence here.

### Choices the LLD leaves open (simplest taken)

- **Two facts, not a fifth phase.** `VaultPhase` keeps its four values; `VaultState` gains a
  `screen_locked` flag (`is_screen_locked()`), true only beside a held header (Locked). The HLD
  states the machine as "is the key in memory, is the screen covered"; a fifth `VaultPhase`
  value would change every exhaustive match in `session-ui` for a state those dialogs can never
  see (they are not rendered while locked), and they already treat Locked correctly.
- **Interceptor rule by binding, not by key list.** While locked, a keystroke is let through
  only when the focus is inside the lock view (`OneTermLock` in the context stack) and every
  binding the keymap resolves for it there belongs to the text input or the root's focus
  navigation (`input::*`, `root::*`), with no pending chord; text with no binding passes (it
  reaches the input handler, so AltGr and IME keep working). Everything else is stopped. This is
  the LLD's intent (editing keys and text only) computed from the keymap itself, so a binding
  added later is covered without editing a list.
- **Timeout clamp on read.** `lock_after_idle_minutes` is clamped to 1..=480 when read, the file
  is never rewritten for it; a non-integer value deserializes as absent (default 15) with a warn.
- **Security page "Lock Now"** now locks the screen (HLD: "from US-0159 on, the Lock action goes
  to IdleLocked") and is shown whenever a vault exists (Locked or Unlocked).
- **Events.** `lock_screen` from Unlocked emits `Locked` (the key went) then `ScreenLocked`;
  `unlock_screen` emits `Unlocked`. The plugins never react to `Locked`: they compare
  `is_screen_locked()` on every `VaultState` notification (a refused seal also emits `Locked`,
  `US-0158`).
- **No-vault notice** is a plain notification naming Settings > Security (no button).

## Decisions

- [`DEC-0021`](../../decisions/DEC-0021-secrets-at-rest-are-sealed-by-a-master-password-vault.md)
  (inherited); no new decision expected.

## Verification Plan

- Unit: `cargo test -p oneterm-state` (screen lock transitions, `locked_meanwhile` during an
  unlock / change / set-up, unlock_screen against the in-memory header after the file is
  replaced, back-off shared, elevated, R2 helper), `-p oneterm-settings` (field round trip,
  clamp, lenient parse), `-p oneterm-workspace` (idle-time decision incl. wall-clock jump,
  activity stamps vs a session event, the surface replaced and not rendered while locked, every
  binding blocked, focus restored / dead handle), `-p oneterm-settings-ui` (Security rows, key
  binding classification), `-p oneterm-actions`.
- Gate: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `python scripts/verify-dependency-graph.py`, `check-doc-paths.py`, `check-english.py`,
  `check-ignored-tests.py` if disk allows, `check-theme-contrast.py` if new token surfaces.
- E2E on Windows (debug `oneterm.exe` from this worktree, private `USERPROFILE` + working
  directory, timeout 1 minute): main + Settings lock after the timeout; screenshot from another
  process; `ping -t` alone does not keep it awake, a key / mouse move does; frame count while
  locked with and without output; `ctrl-shift-l`; `ctrl-shift-q` ignored while locked; unlock
  restores focus to the terminal; wrong password back-off; quit while locked; no vault, no lock;
  minimised-window taskbar thumbnail. Screenshots `us0159-*.png` in the session scratchpad.
- Owner's steps if not verifiable here: real sleep / resume, the elevated window, Linux / macOS.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

(Filled after implementation.)

## Handoff

Implementer: a subagent in an isolated worktree; the integrating actor verifies independently
before merging into `main`.
