# Work: Idle lock screen

ID: US-0159
Intake: IN-0050
Created: 2026-10-09

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
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

- [x] In scope:
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
- [x] Out of scope (LLD § 7 follow-ups): OS session lock / sleep hooks (`WTSRegisterSessionNotification`);
  swapping the macOS native menu while locked; re-running `use_saved(true)` in an open connect
  dialog after an unlock (`US-0158` note; the dialog falls back to inputs, accepted); a
  "Set up" button inside the no-vault notification.

## Acceptance

From the `IN-0050` US-0159 candidate, made concrete:

- [x] With a vault, after the timeout both the main and the Settings window show only the lock
  screen; a screenshot taken by another process shows no terminal pixels.
- [x] Terminal output alone (`ping -t` / a `cat` loop) never prevents the lock; a key or a mouse
  move does (unit test with a fake clock + GUI walk).
- [x] A tab producing output while locked costs no frames (lock-screen frame count with and
  without output is the same within noise).
- [x] No workspace action is reachable while locked: a test enumerates every registered key
  binding and asserts none dispatches outside the lock view; `ctrl-shift-q` does nothing.
- [x] `ctrl-shift-l` (and the app-menu item, and Settings > Security > Lock Now) locks at once;
  the binding is listed and rebindable on the Key Bindings page.
- [x] Unlock with the master password restores each window's previous focus (a dead handle is
  tolerated); a wrong password shows the inline error, clears the input, and backs off from the
  5th failure (`VaultState`'s count, shared with the dialogs).
- [x] Quit (button, window close, Alt+F4) works while locked (button and `WM_CLOSE` walked;
  Alt+F4 sends the same `WM_CLOSE`).
- [x] No vault (Uninitialised / Unavailable): no timer, no lock; the action says why.
- [ ] Sleep longer than the timeout locks on resume: the wall-clock term of the idle time
  (unit-tested decision; a real sleep is walked if possible, otherwise recorded as a gap).
- [x] `lock_after_idle_minutes`: default 15, round trip, out-of-range values clamp to 1..=480,
  a non-integer value loads as the default without quarantining the file.
- [x] `VaultState`: a screen lock during an unlock / change / set-up wins (`locked_meanwhile`);
  R2 fixed; the lock screen never re-reads `vault.json`; elevated windows lock and unlock
  read-only (elevated: unit test only).
- [x] Unit tests leave no temporary directory behind and assert timing only as ratios.

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

Docs changed, as planned: `low-level-design/idle-lock-overlay.md` (new "As built (`US-0159`)"
section with the spike result; § 7 sleep row), `high-level-design.md` (state machine as built),
`docs/gui-layout.md` (new § Lock screen; Security page row and paragraph),
`docs/agents/persistence.md` (`ui_config.json` row: `lock_after_idle_minutes`),
`docs/agents/structure.md` (workspace tree: `lock_screen.rs`), `IN-0050.md` (US-0159 created;
Q11 findings). Not changed: `crate-dependency-rules.md` / `scripts/dependency-graph-policy.json`
(no new edge: `workspace` already depends on `state`, `settings`, `actions`, `gpui-base`),
`docs/PROJECT.md`, `DEC-0021` (nothing here is a rule future work inherits beyond the LLD).

## Context

- gpui-pre 0.3.7 `Window::dispatch_key_event` runs the interceptors first; when one stops
  propagation no binding is matched, but `finish_dispatch_key_event` still offers the event to
  the focused path's raw key listeners (only the lock view is in the tree while locked).
- `App::notify` invalidates only windows whose last frame rendered the entity
  (`tracked_entities`), which is what makes hidden terminal output free.
- `Root::render` folds `decorate` in registration order; `gpui_component::init` registers
  `WindowState` first, so a plugin registered from `app::init` decorates last.

## Plan

- [x] Records first (commit 1, docs only): this packet, `IN-0050.md` candidate ticked.
- [x] Spike in the real build: `decorate` replacement (focus restore, input / IME, title bar
  drag / close, DWM thumbnail of a minimised window). Run on the real plugin rather than a
  throwaway one (the plugin is the spike's whole subject); result under Evidence and in the
  LLD's As built section.
- [x] `VaultState` + R2 + tests; `UiConfig` field + tests; action, binding row, policy;
  `lock_screen.rs` + tests; Security page; app init; docs.
- [x] Gate; GUI walk on Windows with a private profile; evidence here.

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
- **Late input locks first** (not in the LLD): each stamp first checks the idle time, and input
  arriving after more than the timeout locks and is stopped (keys in the interceptor, mouse in
  the capture phase). Without it the first key after a resume from sleep would reset the clock
  and reach the terminal.
- **Timer recheck** at most every 60 s (LLD § 3 said one wake per deadline): Windows relative
  timers may not count time asleep, so a passed deadline is noticed within a minute of a resume
  even without input. It draws no frame.
- **Reset text always shown**, no "Forgot it?" disclosure, no eye toggle on the input.
- **Frame diagnostics**: each window logs `lock screen: N frames in S s` (debug) at unlock; the
  walk measures with it that a hidden terminal costs no frame.
- **The binding-enumeration test** runs in `oneterm-workspace` over every binding of the keymap
  the test builds (all of `gpui_component::init`'s, plus globals shaped like OneTerm's, including
  `ctrl-shift-left`, which the input also binds, and a bare `z`); OneTerm's own table lives in
  `oneterm-settings-ui`, which `workspace` cannot reach. The rule is keymap-driven, so it does
  not depend on which actions the table holds; the GUI walk checked the real table's
  `ctrl-shift-q` and `ctrl-shift-l`.

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
- [x] Unit proof
- [ ] Integration proof
- [x] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Worktree gate, 2026-10-09, Windows 11, `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2` (the app
build needed `CARGO_BUILD_JOBS=1`: rustc crashed on `windows` at 2, the known memory limit):

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo test -p oneterm-state` | 84 passed (9 new: a screen lock drops the key and only `unlock_screen` lifts it; from Locked; no / damaged vault, no lock; a screen lock during a dialog unlock and during a change wins; a vault replaced on disk does not open it; elevated read-only; back-off shared with the dialogs; a reset landing after the lock) |
| `cargo test -p oneterm-workspace` | 58 passed, 3 ignored (8 new in `lock_screen_tests.rs`: idle time takes the larger clock; timer decision; the application is not rendered while locked and is again after; every keymap binding blocked plus live keys; focus restored and a wrong password; late input locks first while on-time input and a notify do not; the timer locks an idle window; no vault, no lock) |
| `cargo test -p oneterm-settings` | 50 passed (1 new: default, clamp, lenient parse; round trip extended) |
| `cargo test -p oneterm-settings-ui` | 58 passed, 1 ignored (Security rows: Lock Now whenever a vault exists, elevated too; the elevated-classification exhaustiveness test covers `lock_oneterm`) |
| `cargo test -p oneterm-actions` | 4 passed |
| `cargo test -p oneterm-workspace --lib -- --ignored --test-threads=1` | 3 passed |
| `cargo test -p oneterm-settings-ui --lib -- --ignored --test-threads=1` | 1 passed |
| `python scripts/verify-dependency-graph.py` | pass (21 packages, no new edge) |
| `python scripts/check-doc-paths.py` | pass (223 paths) |
| `python scripts/check-english.py` | pass (1119 files) |
| `python scripts/check-theme-contrast.py` | pass (no new token; the lock view uses `background`, `foreground`, `muted.foreground`, `danger`) |
| `python scripts/check-ignored-tests.py` | **not run**: it builds every crate's tests (12.7 GB free); no `#[ignore]` was added |
| `cargo test -p oneterm-session-ui` / `-p oneterm-app` | not run: session-ui untouched; the app's one-call `init.rs` change is compiled by clippy `--all-targets` |

Mutation check: with the interceptor's `stop_propagation` commented out, the two
input-blocking tests fail (a global action fires; the key that locked is delivered).
`%TEMP%` after the runs: no `oneterm-lock-screen-*`, `oneterm-vault-state-*`,
`oneterm-security-page-*` or `oneterm-ui-config-*` directory.

**GUI walk** (debug `oneterm.exe` from this worktree; private profile: `USERPROFILE` and the
working directory in scratch, so `config_dir()` = `<scratch>/cwd-<label>/target`; the floor
fixture `vault-v1.json` as `vault.json`; `lock_after_idle_minutes: 1`; driven by posted window
messages to the launched pid only, modifier chords by attaching to that window's input thread;
screenshots `us0159-*.png` in the session scratchpad):

| Step | Result |
| --- | --- |
| Start (`01`), `ping -t 127.0.0.1` in the tab, Settings opened from the app menu (`02`, `03`) | the app menu shows "Lock OneTerm  Ctrl+Shift+L" |
| 45 s idle with ping output (`04b`) | unlocked, ping scrolling |
| 75 s idle with ping output (`05`, `06`) | **both** windows show only the lock screen (own title bar, icon, "OneTerm is locked", focused empty input, Unlock, the reset paragraph, Quit OneTerm); captured with `PrintWindow` from another process: no terminal pixel |
| `ctrl-shift-q` while locked | process still alive |
| Wrong password (`08`) | "Wrong master password.", input cleared |
| 6th wrong (`10`) | "Too many wrong attempts. Try again in 3 s.", Unlock disabled |
| Right password (`11`, `12`) | both windows back; ping kept running underneath (219 replies); `ctrl-c` then reached the terminal (`14`): focus restored |
| App menu > Lock OneTerm (`13`) | locks at once |
| `ctrl-shift-l` (the real key table) | locks at once (each frame run below) |
| Frames while locked, 20 s plus typing the password (debug log) | main window: 43 and 40 frames idle, 26 and 10 with `for /L ... echo` flooding the tab; Settings window: 2 and 2 idle, 12 and 2 flooding. The flood adds no frames (unlocked, the same flood draws at the frame cap); the one Settings count of 12 did not repeat and is not explained (the window's input is not touched by the run) |
| Mouse move at 40 s, key at 80 s, no input after (`19`, `20`) | unlocked at 130 s, locked at 152 s |
| Unlock from the Settings window (`26`) | both windows unlock |
| Security page (`28`) | Lock Now with the shortcut text; Screen Lock group "Lock After Idle (minutes)" = 1; `+` writes `"lock_after_idle_minutes": 2` to `ui_config.json` |
| Key Bindings (`29`) | App Menu row "Lock OneTerm  Ctrl+Shift+L" |
| Quit OneTerm on the lock screen (`30`) | process exits |
| Lock, then `WM_CLOSE` to the main window (`33`) | process exits in 0.07 s |
| No vault, 75 s idle (`31`), `ctrl-shift-l` (`32`) | no lock; notification "Set up a master password in Settings > Security first: the lock screen asks for it." |
| Logs | no panic; only the pre-existing DirectX debug-layer lines |

Gaps:

- **DWM thumbnail of a window minimised before the lock: not verified.** A
  `DwmRegisterThumbnail` capture from this session drew nothing even for a visible window, and
  hovering the taskbar needs the owner's real mouse. A minimised window's thumbnail is the frame
  from before the minimise, so the open risk stands; follow-up:
  `DWMWA_FORCE_ICONIC_REPRESENTATION` while locked (needs the raw `HWND`). The minimised window
  itself (`24`) has no client area to capture.
- **Sleep / resume not walked**: covered by the wall-clock term, the late-input rule and the
  one-minute recheck, each unit-tested on the decision.
- **IME composition** and **dragging / the caption buttons** of the lock title bar not exercised
  (posted `WM_CHAR` only; non-client hit-testing needs the real mouse).
- **Elevated window** not walked (UAC); unit test `the_elevated_window_locks_and_unlocks_read_only`.
- Linux / macOS not run (CI). The full `cargo test --workspace` runs on `main` after independent
  verification.

## Handoff

Implementer: a subagent in an isolated worktree; the integrating actor verifies independently
before merging into `main`.
