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
- [x] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability
- Risk lane: high-risk (screen protection of a running UI that holds the vault key; input
  blocking; a new persisted field in `ui_config.json`)
- Spec Intake, when required: [`IN-0050`](IN-0050.md); owner rulings Q1-Q10 accepted as
  recommended (2026-10-09; Q1: no lock at startup; Q2: 15 minutes, 1-480, no "Never"; Q5: the
  elevated window applies the lock read-only; Q6: quit allowed while locked; Q7: no reset on the
  lock screen, which explains how to reset); **Q1 re-ruled 2026-10-13 (owner): lock at startup
  = yes** when a usable vault exists (acceptance rework below);
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
Since the acceptance rework (2026-10-13), whenever OneTerm starts with a usable vault every
window starts covered by the same lock screen, from its first frame.

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
- [x] Quit (button, window close) works while locked (button and `WM_CLOSE` walked). Alt+F4:
  **not walked** (a posted Alt+F4 closes nothing in either state, verify D4); the owner presses a
  physical Alt+F4. It is not an OS bypass: it passes the keystroke filter as an unbound key.
- [x] No vault (Uninitialised / Unavailable): no timer, no lock; the action says why.
- [ ] Sleep longer than the timeout locks on resume: the wall-clock term of the idle time
  (unit-tested decision; a real sleep is walked if possible, otherwise recorded as a gap).
- [x] `lock_after_idle_minutes`: default 15, round trip, out-of-range values clamp to 1..=480,
  a non-integer value loads as the default without quarantining the file.
- [x] `VaultState`: a screen lock during an unlock / change / set-up wins (`locked_meanwhile`);
  R2 fixed; the lock screen never re-reads `vault.json`; elevated windows lock and unlock
  read-only (elevated: unit test only).
- [x] Unit tests leave no temporary directory behind and assert timing only as ratios.

Acceptance rework 2026-10-13 (owner re-ruled Q1: lock at startup):

- [x] With a usable vault (Ready), every window starts locked: the first frame of the main
  window is the lock screen, the application view is never rendered before the first unlock.
  Elevated instance: the same, read-only.
- [x] No vault (Uninitialised) or an unusable one (Damaged / newer / unreadable): no startup
  lock; Settings > Security shows the state as before.
- [x] The lock screen names the config folder and says how to recover a forgotten password
  (quit, delete `vault.json`, every saved password lost); no Reset on it (Q7); Quit works.
- [x] The idle clock starts at the first unlock (no timer while the startup lock is up); the
  back-off applies at startup as after an idle lock.
- [x] Nothing stacks a second unlock surface on the lock screen (`ensure_unlocked` while the
  screen is locked opens nothing), and a dialog opened under the lock (a crash report at start)
  does not take the keyboard away from the lock input.
- [x] A session whose `sealed_password` outlived a deleted `vault.json` asks for the password
  ("the master password was reset"), keeps the envelope until replaced, never panics.
- [x] Correct password at startup: the application appears with the focus in the terminal.

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

Acceptance rework 2026-10-13: `IN-0050.md` (Q1 re-ruled, Q7 note, threat-model bullet, US-0159
candidate), `high-level-design.md` (state machine: start -> IdleLocked when Ready; wireframe
reset text), `low-level-design/idle-lock-overlay.md` (§ startup, the reset paragraph, as built),
`docs/gui-layout.md` (lock section). No persisted field changes (no new setting); `DEC-0021`
unchanged (its rule 2, reset loses every saved password, is what the lock screen now cites).

### Reconciliation

Docs changed, as planned: `low-level-design/idle-lock-overlay.md` (new "As built (`US-0159`)"
section with the spike result; § 7 sleep row), `high-level-design.md` (state machine as built),
`docs/gui-layout.md` (new § Lock screen; Security page row and paragraph),
`docs/agents/persistence.md` (`ui_config.json` row: `lock_after_idle_minutes`),
`docs/agents/structure.md` (workspace tree: `lock_screen.rs`), `IN-0050.md` (US-0159 created;
Q11 findings). Not changed: `crate-dependency-rules.md` / `scripts/dependency-graph-policy.json`
(no new edge: `workspace` already depends on `state`, `settings`, `actions`, `gpui-base`),
`docs/PROJECT.md`, `DEC-0021` (nothing here is a rule future work inherits beyond the LLD).

Acceptance rework 2026-10-13, as planned: `IN-0050.md`, `high-level-design.md` (state machine
edge start -> IdleLocked, as-built paragraph, data flow 5, wireframe), `idle-lock-overlay.md`
(§ 4 "At start", § 7 rows, As built: lock view text, lock at startup with its two general
rules), `docs/gui-layout.md` (lock section). Not changed: `persistence.md` (no new field),
`DEC-0021`, `structure.md` (no new file).

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

### Rework after independent verification (2026-10-09)

[`evidence/US-0159-verify.md`](evidence/US-0159-verify.md): PASS with one Medium and three Low
defects, fixed on top of the verifier's commit (`dd57f82f`, 7 tests).

| Defect | Fix |
| --- | --- |
| D2 (Medium): a native OS file dialog open when the lock comes stays usable over it, and its result was applied while locked (walked: the log folder was saved to `terminal.json` under the lock; the same path starts an SFTP upload or saves a download) | No shared picker wrapper exists, so the rule lives in one helper, `VaultState::screen_locked(cx)` (`crates/state/src/vault_state.rs:265`), checked by each continuation before it applies anything, with a debug log: `crates/session-ui/src/auth_form.rs:611` (SSH key), `crates/settings-ui/src/terminal/logging.rs:193` (log folder), `crates/sftp-ui/src/transfer.rs:621` (upload) and `:721` (download Save; a window that is gone counts as locked). The rule for future pickers is in `idle-lock-overlay.md` § 7. Tests: `a_key_picked_while_the_screen_is_locked_is_dropped` (session-ui; the test platform's path prompt answered while locked and again unlocked; fails with the guard removed) and `the_app_wide_screen_lock_check_follows_the_global` (state). |
| D1 (Low): gpui runs every keystroke interceptor, so the Key Bindings recorder could record the key that locked (late input) | `crates/settings-ui/src/key_bindings/key_bindings_ui.rs:293` `recorder_armed` returns false while `VaultState::screen_locked`; the lock's interceptor is installed at init, before any recorder, so it has locked by then. Test `the_recorder_takes_no_key_while_the_screen_is_locked`; the verifier's `verifier_d1_a_later_interceptor_still_sees_the_key_that_locked` is flipped to the rule (0 keys taken). The rule for future interceptors is in § 7. |
| D3 (Low): a lock drops element-keyed state; the Settings window returns to General | **Known limit, recorded** (LLD § 7, `docs/gui-layout.md` § Lock screen): the kit's `Settings` keeps the selected page in a private keyed `SettingsState` with no setter or change callback (`gpui-component 0.7.0` `setting/settings.rs`), so holding it in the panel would mean forking the widget. Dialogs, inputs, terminals and the dock are entities and come back intact (verifier). |
| D4 (docs) | LLD § 6.4: Alt+F4 is not an OS bypass; the Alt+F4 acceptance tick says "not walked"; `docs/gui-layout.md`: the password is checked against the header held in memory (read at start, replaced by set-up, change or rehash). |
| I4 (note, taken) | An empty Enter on the lock input is no longer an attempt (`crates/workspace/src/lock_screen.rs:485`): no Argon2id run, no failure counted; tested in `unlocking_restores_the_focus_and_counts_a_wrong_password`. |

The verifier's other notes, recorded (not defects):

- **I1:** a stopped keystroke is still offered to the first capture-phase key listener on the
  focus path (gpui quirk); no terminal registers one, pinned by a verifier test.
- **I2:** text without a keystroke (`VK_PACKET` auto-type, IME composition) skips the key filter
  and the idle clock; while locked it can only reach the lock input, the one painted input
  handler. Unlocked, such typing does not postpone the lock.
- **I3:** window deactivation and system-generated mouse moves (z-order changes under a resting
  cursor) postpone the lock; they never open it.
- **I5:** `is_screen_locked()` is `screen_locked && Locked`, so it fails open by construction if
  a future transition leaves Locked without clearing the flag; today only a reset does, on
  purpose.
- **I6:** a reset that lands after a lock lifts it, by design (nothing is left to unlock with).
- **I7:** an unlock during the post-unlock rehash answers Busy, counts nothing, and the lock
  holds until the rehash lands.
- **I9:** OSC 9;4 taskbar progress and OSC 52 clipboard writes still leave a locked OneTerm.
- **Follow-up:** the minimised-window DWM thumbnail stays an open, unverified risk; the fix is
  `DWMWA_FORCE_ICONIC_REPRESENTATION` while locked (needs the raw `HWND`).

### Acceptance rework 2026-10-13: lock at startup

**What changed and why.** The owner re-ruled intake Q1 (2026-10-13): whenever OneTerm starts and
a master-password vault exists, the app must ask for the master password first. Every window
starts covered by this packet's lock screen, exactly as after an idle lock. No new setting: it
is always on while a vault exists. Reopened as acceptance rework of this packet (not a new BUG:
the lock screen was built but not yet accepted by the owner).

Coordinator rulings recorded here:

- `VaultStatus::Ready` at start -> screen locked from the first frame (no flash of the
  application). Uninitialised -> no lock (unchanged). Unavailable (Damaged / newer /
  unreadable) -> no startup lock (nothing to unlock with); the UI opens and Settings > Security
  shows the state as today. Elevated instance: the same rule, read-only (Q5); an elevated
  profile without a vault does not lock.
- Q7 stays: no Reset on the lock screen. Its paragraph is rewritten for the new rule: unlocking
  needs the master password; if it is forgotten, quit, delete `vault.json` in the OneTerm config
  folder (the lock screen prints the absolute path: a path is not a secret), and every saved
  password is lost (`DEC-0021` rule 2). Quit stays (Q6).
- Stale envelopes after a manual `vault.json` deletion are made clean (or recorded as a
  follow-up BUG candidate if the fix exceeds about 60 lines).
- The idle clock starts at the first unlock; no timer while the startup lock is up.
- The lock screen is the only unlock surface while it is up: no `ensure_unlocked` caller may
  stack an Unlock dialog on it.

Plan:

- [x] `VaultState::load_at_start` (`init` uses it): `load` plus the screen lock when a usable
  vault was read; no event (nothing subscribes yet). Tests: Ready -> locked, Uninitialised and
  Unavailable -> not, elevated Ready -> locked and unlocks read-only, back-off at startup.
- [x] `ensure_unlocked` opens nothing while the screen is locked (one guard in the shared
  function, every caller covered).
- [x] Lock view: keep the keyboard on the lock view while locked (a dialog opened under the
  lock, such as a crash report at start, focuses a view that is not drawn); the reset paragraph
  rewritten with the config folder path.
- [x] Workspace tests: a window built over a startup-locked vault never renders the application,
  the lock input has the focus and takes typing, the correct password brings the application
  and the focus that the application set at construction; the idle timer starts at that unlock
  even when the lock screen was up longer than the timeout.
- [x] Stale envelope: check `US-0158`'s connect and session dialogs with a vault deleted by hand;
  add the test.
- [x] Docs as built: HLD state machine, `idle-lock-overlay.md` (startup, reset paragraph),
  `docs/gui-layout.md` lock section.
- [x] Gate and GUI walk (first frame captured from another process): Evidence.

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

**Rework gate** (worktree, `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`, 11.7 GB free):

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo test -p oneterm-state` | 86 passed (85 after the verifier + 1 new) |
| `cargo test -p oneterm-workspace` | 64 passed, 3 ignored (verifier D1 test flipped; empty-Enter check added) |
| `cargo test -p oneterm-settings` / `-p oneterm-actions` | 50 / 4 passed |
| `cargo test -p oneterm-settings-ui` | 59 passed, 1 ignored (1 new: recorder while locked) |
| `cargo test -p oneterm-session-ui` | 100 passed, 1 ignored (1 new: key picked while locked) |
| `cargo test -p oneterm-sftp-ui` | 75 passed |
| `--ignored --test-threads=1` for workspace / session-ui / settings-ui | 3 / 1 / 1 passed |
| `python scripts/verify-dependency-graph.py` | pass |
| `python scripts/check-doc-paths.py` | pass |
| `python scripts/check-english.py` | pass |
| `python scripts/check-theme-contrast.py` | pass |

Mutation: with the key-picker guard disabled, `a_key_picked_while_the_screen_is_locked_is_dropped`
fails ("a result applied under the lock"). The rework changes no visible layout (the native
dialog itself was not re-walked); the GUI walk was not repeated. `%TEMP%`: no
`oneterm-key-picker-lock-*` or `oneterm-recorder-lock-*` directory left.

### Acceptance rework 2026-10-13: lock at startup (evidence)

What was built (shortest form; the LLD's As built section has the detail):

- `crates/state/src/vault_state.rs`: `VaultState::load_at_start` (= `load` + the screen lock
  when the vault is Ready; no event), used by `VaultState::init`.
- `crates/state/src/vault_dialogs.rs`: `ensure_unlocked` opens nothing while the screen is
  locked (one guard; every caller covered).
- `crates/workspace/src/lock_screen.rs`: the reset paragraph names the absolute config folder
  (`forgot_text`); `LockView` tracks a focus handle on its whole view and gives the input the
  focus back on render when the focus is outside it (a dialog opened under the lock).
- `crates/settings-ui/src/security.rs`: the Screen Lock description says OneTerm starts locked.
- Stale envelopes: no code change. `US-0158` already shows the input with "Saved password
  unavailable: the master password was reset. Type the password." and keeps the envelope; a
  test pins it (`connect_dialog.rs`, `#[cfg(test)] SshAuthForm::note`).
- Startup callers of `ensure_unlocked`: none. The workspace always resets the center to one
  local shell (`reset_center_only` / `reset_default_layout`); SSH tabs are never restored;
  the connect and session dialogs are user-initiated. The guard makes the rule structural.
- First-frame ordering (read, then walked): `app::init` runs `VaultState::init` before
  `lock_screen::init`; `window::open_window` builds the workspace and then `Root::new`, which
  builds the plugins, so `LockScreen::new` -> `follow_vault` sets `locked` before the first
  `Root::render`. The workspace focused its terminal inside its constructor, so that focus is
  what the plugin saves and the unlock restores.

New tests (Argon2id at `KdfParams::FLOOR`, RAII temp dirs; the stale-envelope test uses a path
that is never created):

| Crate | Test | Pins |
| --- | --- | --- |
| state | `a_usable_vault_at_start_locks_the_screen_before_any_input` | Ready -> locked, not busy, `locked_meanwhile` false; dialog unlock / change / reset refused; `unlock_screen` -> Unlocked, events `[Unlocked]` |
| state | `no_or_an_unusable_vault_at_start_does_not_lock` | Uninitialised, damaged, newer, unreadable (a directory named `vault.json`), each normal and elevated |
| state | `the_elevated_window_starts_locked_and_unlocks_read_only` | Q5: no rehash, no `vault.bak` |
| state | `wrong_passwords_at_start_back_off` | 4 free, 5th backs off, right password refused while waiting |
| workspace | `a_vault_at_start_locks_the_window_before_its_first_frame` | the application view renders 0 times before the unlock; bound keys stopped; the input takes typing; unlock restores the focus the view gave itself while built |
| workspace | `the_idle_clock_starts_at_the_first_unlock` | a lock screen up 20 min does not re-lock at the unlock; the timer armed by it locks after the timeout |
| workspace | `nothing_under_the_startup_lock_takes_the_keyboard` | `ensure_unlocked` opens no dialog and runs nothing while locked, runs at once after; a dialog opened under the lock leaves the focus on the input |
| session-ui | `a_saved_password_outliving_its_vault_asks_for_the_password` | no vault: no unlock asked, input shown with the reset note, envelope kept, `take_auth` errs without panicking |

Changed test: `while_locked_no_binding_fires_and_only_the_input_takes_keys` blurred the window
and expected typing to go nowhere; with the focus reclaim the next frame gives the input its
focus back, so it now checks the out-of-view rule on `allowed_while_locked` directly and that a
blur does not last.

Mutation checks: with the render-time focus reclaim disabled,
`nothing_under_the_startup_lock_takes_the_keyboard` and the changed test fail; with the
`ensure_unlocked` guard disabled, `nothing_under_the_startup_lock_takes_the_keyboard` fails
("an Unlock dialog was stacked under the lock screen").

GUI walk (debug `oneterm.exe` from this worktree, `CARGO_BUILD_JOBS=1`; private profile:
`USERPROFILE` and the working directory in scratch, so `config_dir()` = `<scratch>\us0159r-cwd-a\target`;
`vault-v1.json` as `vault.json`, `ssh-session-v2-sealed.json` as `ssh_session.json`; posted
window messages to the launched pid only; screenshots `us0159r-*.png` in the session
scratchpad):

| Step | Result |
| --- | --- |
| Launch with a vault; capture from another process the moment the window is visible (`01-launch-a`), then +100 ms (`01-launch-b`) and +500 ms | the first capture (taken 420 ms after the window appeared; `PrintWindow` was slow on its first call) is all black: nothing drawn yet. The next capture is the lock screen, the reset paragraph naming the absolute folder. No capture ever showed application content. On the relaunch the first capture was done 88 ms after the window appeared: black again, then the lock screen |
| `ctrl-,` and `ctrl-shift-q` while locked (`02`) | still 1 top-level window, process alive; no Settings window |
| Typing `abc` with no click (`02`) | 3 dots in the input: the input has the keyboard from start |
| Enter (`03`) | "Wrong master password.", input cleared |
| Right password (`04`), then `echo after-unlock` + Enter (`05`) | the application; the echo ran in the terminal (focus restored to the terminal); no stray characters from the keys typed while locked |
| Close, relaunch (`06-relaunch-*`) | locked again from the first drawn frame |
| Close, delete `vault.json`, relaunch (`07`) | no lock |
| Connect to `prod` (stale envelope) (`08`) | Password input, "Update saved password" box, note "Saved password unavailable: the master password was reset. Type the password."; `ssh_session.json` still holds both envelopes |
| Settings (`ctrl-,`) > Security (`09`, `10`) | "Not set up." with Set Up...; the Screen Lock description says OneTerm starts locked |
| `vault.json` = `not json`, relaunch (`11`) | no lock (Unavailable); log: "vault.json is damaged and left in place" |
| Fresh profile with a vault, Quit OneTerm on the startup lock (`12`) | process exits |
| Logs | no panic, no ERROR; only the DirectX debug-layer warning |

Rework gate (worktree, Windows 11, `CARGO_BUILD_JOBS=1`, `CARGO_INCREMENTAL=0`: the box was
near its commit limit from other workloads, so rustc ran out of memory on `windows` and
`gpui-component` at 2 jobs and sometimes at 1; each such step was retried, never a test failure):

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo clippy --workspace --all-targets --features oneterm-app/terminal-diagnostics -- -D warnings` | pass |
| `cargo test -p oneterm-state` | 90 passed (4 new) |
| `cargo test -p oneterm-workspace` | 67 passed, 3 ignored (3 new, 1 changed) |
| `cargo test -p oneterm-session-ui` | 101 passed, 1 ignored (1 new) |
| `cargo test -p oneterm-settings-ui` | 59 passed, 1 ignored |
| `cargo test -p oneterm-app` | 26 passed |
| `--ignored --test-threads=1` for workspace / session-ui / settings-ui | 3 / 1 / 1 passed |
| `python scripts/check-ignored-tests.py` | pass (19 ignored tests, all recorded) |
| `python scripts/verify-dependency-graph.py` | pass (no new edge) |
| `python scripts/check-doc-paths.py` / `check-english.py` | pass / pass |
| `python scripts/check-theme-contrast.py` | pass (no new token) |

Gaps: the elevated instance at startup is unit-tested only (UAC); a crash-report dialog at
start was not staged in the walk (the gpui test opens a dialog under the lock instead);
Linux / macOS not run. The full `cargo test --workspace` runs on `main` after independent
verification.

### Acceptance rework 2026-10-13, round 2 (after the verification of `6be2402a`)

[`evidence/US-0159-verify.md`](evidence/US-0159-verify.md) § "Acceptance rework (lock at
startup)": FAIL with two Medium defects, both fixed on top of the verifier's commit (its six
tests cherry-picked; the two that pinned V1 and V2 are flipped).

| Defect | Fix |
| --- | --- |
| V1 (Medium, regression of `6be2402a`): a click on the lock background focused the view's own tracked handle; the reclaim fired only when the focus was outside the view, so typing and Enter went nowhere until the field was clicked | `crates/workspace/src/lock_screen.rs:605`: while locked the input gets the focus back whenever it does not have it. A focus from outside the view (a dialog) is remembered (`LockView::taken`). Test `verifier_startup_a_background_click_takes_the_keyboard_from_the_input` now asserts the input keeps the keyboard ("abc" typed after the click) |
| V2 (Medium): after the unlock, a dialog opened under the lock (the crash report at start, a host-key prompt under an idle lock) was on top with the focus restored behind it, so keys reached the shell | `lock_screen.rs:380`: the unlock focuses the remembered handle, else the focus saved when the lock came. `lock_screen.rs:397`: the dialog saved the lock input as the focus to restore when it closes; `LockScreen::render` (unlocked only) hands a focus left on the hidden lock view back to the saved one. gpui-component exposes no accessor for an active dialog's focus handle (`WindowState::active_dialogs` is `pub(crate)`), so the handle is captured when it takes the focus rather than looked up. Test `verifier_startup_after_the_unlock_a_dialog_opened_under_the_lock_has_no_focus` now asserts the dialog has the focus after the unlock and, once it closes, the application view |
| Cosmetic: the colon after the printed path read as part of it | `lock_screen.rs:77`: `...delete vault.json in the folder "<path>". OneTerm then starts...` |

Mutation checks (each reverted): the round-1 reclaim condition fails the V1 test; the unlock
ignoring the remembered handle fails the V2 test; no hand-back after the dialog closes fails the
V2 test (its second half).

The verifier's notes, recorded (not defects):

- Tab on the lock screen never leaves the password input: `root::Tab` is dropped with the
  surface, so Unlock and Quit are reached by Enter, the mouse, or a window close (Q6).
  Pre-existing since `US-0159`.
- `vault.bak` is never read on load (`crates/vault/src/document.rs:94`); deleting
  `vault.json` alone starts OneTerm without a lock. `vault.bak` keeps the old wrapped key under
  the forgotten password until the next set-up overwrites it; it exposes nothing `vault.json`
  did not.
- `ensure_unlocked` while screen-locked returns silently (debug log) and runs no continuation;
  every caller is user-initiated and unreachable under the lock.
- A Quick Connect hop with a stale envelope shows a plain input with no "master password was
  reset" note (`quick_connect_dialog.rs:97`); pre-existing `US-0158` behaviour, identical with
  a Locked vault, no panic.

Round-2 gate (worktree, `CARGO_BUILD_JOBS=2`, one crate per command; one out-of-memory
compiler crash on `cargo test -p oneterm-workspace` was retried at 1 job):

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo test -p oneterm-workspace` | 73 passed, 3 ignored (the verifier's 6 included, 2 flipped) |
| `cargo test -p oneterm-state` | 90 passed |
| `cargo test -p oneterm-session-ui` | 101 passed, 1 ignored |
| `cargo test -p oneterm-settings-ui` | 59 passed, 1 ignored |
| `cargo test -p oneterm-app` | 26 passed |
| `--ignored --test-threads=1` for workspace / session-ui / settings-ui | 3 / 1 / 1 passed |
| `verify-dependency-graph.py` / `check-doc-paths.py` / `check-english.py` | pass |
| `check-ignored-tests.py` | pass (19 ignored tests, all recorded) |

Round-2 walk (rebuilt debug binary, private profile, own pid; `us0159r-r2-*.png`):

| Step | Result |
| --- | --- |
| Startup lock, type `xy`, click the empty background left of the card, type `abc` (`r2-01`) | 5 dots: the input kept the keyboard (V1 fixed); the paragraph prints the folder in quotes, the sentence ends after it |
| Crash report staged in `target\crashes`, start, type `xy` (`r2-02`) | 2 dots: the dialog under the lock takes no key |
| Clear, right password, Enter (`r2-03`), type `echo behind` (`r2-04`) | the crash dialog is on top and has the keyboard; nothing reached the shell (V2 fixed) |
| Dismiss, type `echo after-dismiss` + Enter (`r2-05`, `r2-06`) | the shell shows only `echo after-dismiss` and its output: the keyboard went back to the terminal, `echo behind` never reached it |

## Handoff

Implementer: a subagent in an isolated worktree; the integrating actor verifies independently
before merging into `main`.
