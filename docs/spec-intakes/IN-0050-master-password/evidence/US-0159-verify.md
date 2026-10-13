# US-0159 independent verification

Packet: [`US-0159`](../US-0159-idle-lock-screen.md) (high-risk lane: the lock screen is a
security boundary over a running UI that holds the vault key)
Verified head: `b82c3012` (implementer branch `feat/idle-lock-screen`, base `main` @ `ce80f17c`)
Verifier branch: `verify/US-0159`, in the implementer's worktree (12-13 GB free throughout)
Date: 2026-10-09, Windows 11, `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`, worktree-local `target`

## Verdict: PASS, with one Medium defect for the owner to fix or accept before merge

The boundary holds where it matters most. While locked, no workspace or global shortcut fires
in either window. That covers all 17 default App Menu, terminal, split and Space bindings,
`ctrl-shift-q`, `ctrl-,`, `shift-escape` and a global chord. No key, text, paste, click or wheel
reaches a terminal. The application view is not rendered. Every screenshot taken by another
process shows only the lock. Only the master password lifts the lock, and focus returns to the
terminal. Terminal output (`ping -t`) does not postpone the lock (a clean run locked at exactly
60 s). The "late input locks first" rule holds against a terminal-shaped view's raw key and
mouse listeners (new tests). The lock input's undo history cannot bring back a password. The
deadline timer re-arms after every unlock. A lock taken during the post-unlock rehash holds.

The one boundary gap (D2) is outside gpui. An OS file dialog that was open when the lock came
stays usable on top of the lock screen. OneTerm then applies its result while every window is
locked. This was walked: a log folder was chosen and `terminal.json` was saved behind the lock.
The same path starts an SFTP upload or download. D1, D3 and D4 are Low.

## Defects

| # | Severity | Where | What |
| --- | --- | --- | --- |
| D1 | Low | `crates/workspace/src/lock_screen.rs:75-83` with `crates/settings-ui/src/key_bindings/key_bindings_ui.rs:272-282` | **"Input after the timeout is not delivered" does not cover other keystroke interceptors.** gpui runs every interceptor, whatever an earlier one did (`dispatch_keystroke_interceptors` iterates the whole set; `propagate_event` is checked only afterwards). The Key Bindings recorder is one. It is armed while its capture element has focus, and on a late key that is still the case: the lock interceptor locks, then the recorder records the key and saves it as the row's binding. Repro: Settings > Key Bindings > Edit on a row, let the machine sleep past the timeout (the late path needs the deadline timer to be late), wake it and press a key. OneTerm locks and the key becomes that action's binding. When the timer locks on time the recorder is disarmed, because focus moves to the lock input. Pinned by `verifier_d1_a_later_interceptor_still_sees_the_key_that_locked` (inverted by a fix). Fix: the recorder returns early when `VaultState::is_screen_locked()`. |
| D2 | **Medium** | `crates/workspace/src/lock_screen.rs:395-410` (the lock covers gpui surfaces only) with every `prompt_for_paths` / `prompt_for_new_path` caller: `crates/settings-ui/src/terminal/logging.rs:170`, `crates/sftp-ui/src/transfer.rs:577` (upload), `:691` (download Save), `crates/session-ui/src/auth_form.rs:584` (key file) | **An OS file dialog open at lock time stays usable, and its result is applied while locked.** The dialogs are native Windows windows owned by the main window (`gpui-pre-windows` `show_dialog`). The lock neither covers nor closes them, and the callbacks do not check the lock. Walked (steps 10-13 below): the Settings > Terminal Logging > Log Folder picker was opened, the idle lock came under it (both windows locked, captures 12-13), a folder was chosen in the dialog, and `terminal.json` was saved with `"directory": "...\\us0159v-attacker-logs"` at 18:46:00 while both windows still showed the lock (capture 15). The same path lets the person at the keyboard start an SFTP upload of any local file to the connected host, or save a remote file to any local path. It needs the owner to leave a native file dialog open for the whole timeout. While such a modal dialog is up the main window is disabled, so the timeout runs out under it. Fix: one helper in `oneterm-state` (for example `VaultState::screen_locked(cx)`) checked by the four callbacks, which drop the result while locked. Alternatively, record that native dialogs are outside the lock (LLD § 7) and let the owner accept it. |
| D3 | Low | `crates/workspace/src/lock_screen.rs:395-410` (`decorate` drops the surface) | **Element-keyed UI state is discarded by a lock.** gpui keeps keyed element state only for elements rendered in the last frame, and a locked window renders none of the application. Walked: the Settings window was on Security (capture 18). After a lock and an unlock it showed General (capture 20). Earlier it had also been reset from Terminal Logging to General (captures 10, 16). OneTerm's own keyed state is only in `settings-ui` (`terminal/font.rs`, `terminal/logging.rs`). The gpui-component `Settings` page selection is keyed state too. In the main window, dialogs, inputs, terminals and the dock are entities, which survive (focus and the terminal came back intact in every run). `docs/gui-layout.md` § Lock screen says windows come back unchanged. Either note the Settings page reset there, or keep the page in an entity. |
| D4 | Low (docs) | `low-level-design/idle-lock-overlay.md:144` (§ 6.4), `US-0159-idle-lock-screen.md:84-85`, `docs/gui-layout.md:226-227` | (a) Alt+F4 is **not** an OS event that bypasses gpui. On Windows, `WM_SYSKEYDOWN` goes through `translate_accelerator` into gpui's keystroke dispatch first, so the lock interceptor sees it. It closes the window only because an unbound key passes inside the lock view. The packet ticks Alt+F4 as working ("sends the same `WM_CLOSE`"), but nobody walked it: a posted `WM_SYSKEYDOWN VK_F4` with Alt held closed neither a locked **nor an unlocked** window here, so the emulation proves nothing either way. Quit stays possible through the button and the caption close (Q6). Say "Alt+F4 not walked". (b) gui-layout says the lock checks "the vault header read at start". It checks the header held in memory, which a set-up, change or rehash in this process replaces. |

### Not defects (recorded)

- **I1, gpui offers a stopped keystroke to one capture listener.** When an interceptor stops a
  keystroke, `finish_dispatch_key_event` still calls the first capture-phase key listener on the
  focus path (the loop checks `propagate_event` after the call). For the late key that path is
  the old frame's, with the terminal focused. `TerminalView` registers `on_key_down` /
  `on_key_up` only (`crates/terminal-view/src/terminal_view/render.rs:311-313`), and the only
  `capture_key_down` listeners in the dependency tree are the kit's disabled-input guards. So no
  terminal sees it. While locked, the path is the lock view's. Pinned by
  `verifier_a_late_key_reaches_no_terminal_key_listener` (`quirk == 1`), so a future capture
  listener on a terminal path would show up there.
- **I2, text that arrives without a keystroke.** `VK_PACKET` text (`SendInput` Unicode:
  password-manager auto-type, AutoHotkey) produces no gpui `KeyDown`
  (`gpui-pre-windows` `handle_key_event`). It arrives as `WM_CHAR` straight to the input handler.
  IME composition goes to the input handler too. Neither passes the interceptor or stamps the
  idle clock. While locked, the only painted input handler is the lock input's, so the text
  can reach nothing else. Unlocked, such typing does not count as activity, and a late
  `VK_PACKET` key after a resume can reach the terminal before the next timer check (at most
  60 s).
- **I3, what counts as activity besides real input.** `observe_window_activation` fires on
  deactivation too. Synthetic `WM_MOUSEMOVE` / `WM_NCMOUSEMOVE` (Windows sends them when the
  z-order changes under a resting cursor) count as mouse moves. Both postpone the lock; neither
  opens it. One run here locked at 129 s instead of 60. The owner was using the desktop at the
  same time. A clean rerun locked at exactly 60 s, and a run with `PrintWindow` captures every
  20 s locked at 61 s, so other processes' screenshots do not keep OneTerm awake.
- **I4, an empty Enter is an attempt.** Enter on the empty lock input runs Argon2id and counts a
  failure (walked: two Enters, "Wrong master password."). Anyone at the keyboard can push the
  owner into the 30 s back-off this way, as with any wrong password (LLD § 5 accepts that).
- **I5, `is_screen_locked()` fails open by construction.** It is `screen_locked &&
  Phase::Locked` (`vault_state.rs:253`), so any future transition off Locked that forgets the
  flag lifts the lock silently. Today only `finish_reset` leaves Locked while the flag may be
  set, and it clears the flag on purpose. A `debug_assert!` in the phase setters would keep it
  that way.
- **I6, a reset that lands after a lock lifts it.** That is correct against Q1 / Q7. A reset can
  only start while unlocked (`check_write` refuses it while screen-locked). It removes
  `vault.json`, so there is nothing left to lock with. A failed removal returns before the flag
  is cleared and keeps the lock (`vault_state.rs:703`).
- **I7, unlocking during the rehash.** The first unlock below the default parameters starts a
  rehash. Locking right after and typing the password then answers Busy. No failure is counted,
  and the lock holds until the rehash lands (80 ms in an optimized build, `US-0156`). Pinned by
  `verifier_a_screen_lock_during_the_rehash_holds_and_unlocks_after`. Acceptable.
- **I8, a refused seal never covers the screen.** `seal` emits only `Locked`
  (`a_seal_after_a_reset_elsewhere_is_refused_and_drops_the_key` asserts the event list is
  exactly `[Locked]`, so no `ScreenLocked`). The plugins follow `is_screen_locked()`, never the
  event.
- **I9, what still leaves a locked OneTerm.** OSC 9;4 taskbar progress from a hidden terminal
  still updates the taskbar button. An OSC 52 write still reaches the clipboard (LLD § 7). The
  OS window titles are static ("OneTerm", "Settings"), so no host or tab name shows in the
  taskbar or Alt+Tab.
- **I10, minimised-window thumbnail: not verifiable here either.** The thumbnail needs a hover
  with the owner's real mouse, and `PrintWindow` on a minimised window has no client area. The
  open risk in the LLD stands.
- **Frames.** The Settings window's unexplained 12 frames in the implementer's walk cannot come
  from terminal output, because the Settings window has no terminal. They match its lock input's
  cursor blink while that window was active. Here the Settings lock view drew 0-1 frames in
  65-114 s while inactive. The main window drew 44-47 frames in 65-114 s while `ping -t` printed
  a line a second underneath: fewer frames than lines, and not proportional to time (the frames
  are the lock and unlock transitions, typing and the `PrintWindow` captures).

## Gate (verifier run, head `b82c3012` plus the verifier's tests)

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass (implementer head: cached, no rebuild needed; again with the verifier tests) |
| `cargo test -p oneterm-state -p oneterm-workspace -p oneterm-settings -p oneterm-settings-ui -p oneterm-actions -p oneterm-session-ui` | pass. Implementer head: state 84, workspace 58 + 3 ignored, settings 50, settings-ui 58 + 1 ignored, actions 4, session-ui 99 + 1 ignored. With the verifier tests: state 85, workspace 64 + 3 ignored |
| `cargo test -p oneterm-workspace --lib -- --ignored --test-threads=1` | 3 passed |
| `cargo test -p oneterm-settings-ui --lib -- --ignored --test-threads=1` | 1 passed |
| `cargo test -p oneterm-session-ui --lib -- --ignored --test-threads=1` | 1 passed |
| `python scripts/verify-dependency-graph.py` | pass (21 packages) |
| `python scripts/check-doc-paths.py` | pass (223 paths) |
| `python scripts/check-english.py` | pass (1119 files; 1120 with this file) |
| `python scripts/check-theme-contrast.py` | pass |
| `python scripts/third-party-notices.py --check` | pass |
| `python scripts/check-ignored-tests.py` | not run (it builds every crate's tests; 12-13 GB free). `git diff main..HEAD` adds no `#[ignore]`; the only match is the packet's own table row |
| `Cargo.lock` / `Cargo.toml` | unchanged by the branch (no new package) |
| `%TEMP%` | 42 `oneterm-*` entries before and after, newest 17:30 (before this run); no `oneterm-lock-screen-*` or `oneterm-vault-state-*` left by the runs |

## Tests added (all pass)

`crates/workspace/src/lock_screen_tests.rs`, section "Independent verification". The tests use
a `Probe` view with a raw `capture_key_down`, `on_key_down` and `on_mouse_down`, shaped like
`TerminalView`:

- `verifier_a_late_key_reaches_no_terminal_key_listener`: after 16 minutes idle the key locks
  and the probe's key listener never runs. Nothing reaches it while locked (`y ctrl-v enter`,
  typed text). Also pins the gpui capture quirk (I1).
- `verifier_a_late_click_reaches_no_terminal_mouse_listener`: the late click locks in the
  capture phase and is not delivered. No later click reaches the hidden view.
- `verifier_undo_in_the_lock_input_never_returns_a_password`: unlock with the right password,
  lock again, `ctrl-z` / `ctrl-shift-z` / `ctrl-y` leave the input empty (`set_value` clears the
  kit's undo history). The same holds after a wrong password.
- `verifier_after_an_unlock_the_timer_locks_again`: twice over, the timer locks and an unlock
  re-arms it. Only the re-armed task can lock, after `advance_clock(RECHECK + 1 s)`. This is the
  re-entrancy path (the timer replaces its own task from inside `cx.update`): no panic, no lost
  timer.
- `verifier_d1_a_later_interceptor_still_sees_the_key_that_locked`: pins D1.
- `verifier_chords_never_complete_and_paste_reaches_only_the_input`: a global `ctrl-k ctrl-q`
  never completes, neither from a prefix pending in the terminal at lock time nor typed in the
  lock input. `ctrl-v` pastes into the password field.

`crates/state/src/vault_state_tests.rs`:

- `verifier_a_screen_lock_during_the_rehash_holds_and_unlocks_after`: I7. Busy, no failure
  counted, the rehashed header is kept with no key, and then the password opens the screen.

## Attacks and results

### Code review against the pinned gpui (`gpui-pre 0.3.7`, `gpui-pre-windows 0.3.7`, `gpui-base 0.7.0`)

- **Interceptor placement.** `Window::dispatch_key_event` (`window.rs:5861`) runs the
  interceptors before any binding is matched. Its rule uses `Keymap::bindings_for_input` with
  the same context stack that `DispatchTree::dispatch_key` uses, so it cannot disagree with
  dispatch about which bindings apply. `.all()` over the resolved bindings is conservative: any
  non-`input::` / `root::` binding stops the key. The keymap test enumerates more than 50
  bindings. Pending chords are rejected, and a prefix pending before the lock is dropped
  because focus moves (`pending.focus != window.focus`). Tested.
- **No `WM_CHAR` after a stopped key.** `translate_accelerator` skips `TranslateMessage` when
  gpui handled the key down, so a stopped key produces no text. The platform input handler is
  taken out at the start of every draw and only re-registered by a painted, focused element.
  From the first locked frame the lock input is the only one.
- **Mouse.** The raw listeners live in the plugin overlay, which the locked `decorate` drops
  too. While locked nothing intercepts the lock view's own clicks, and the application has no
  hitboxes. While unlocked, the canvas paints after the application, so application
  capture-phase listeners run first. A capture listener that stops an event (a disabled input,
  a dismissing popover) hides that event from the idle clock. That fails safe: the lock comes
  sooner. External file drops turn into gpui mouse events and reach only the painted frame.
- **Other entry points.** No jump list, tray icon, single-instance forwarding, URL handler or
  global hotkey exists (searched `crates/`). The window titles are static. The in-window app
  menu is part of the dropped surface. Programmatic `activate_window` is called only at
  startup and when the Settings window opens.
- **State machine.** `lock_screen` with no vault (Uninitialised / Unavailable) does nothing.
  From Unlocked it emits `Locked` then `ScreenLocked`. While an operation runs it sets
  `locked_meanwhile`. Every `finish_*` keeps the lock: change and rehash keep only the new
  header, a dialog unlock answers `LockedMeanwhile`, and `unlock_screen` checks
  `is_screen_locked()` before counting. Set-up, the dialog unlock, change and reset are refused
  while locked. R2 clears the dialog inputs on `LockedMeanwhile`.
- **Idle clock.** `max(monotonic, wall)` with a wall clock set back counted as zero (unit
  test). The 60 s recheck is drawn as no frame. Each `VaultState` or `UiConfig` notify
  replaces the task (the old one is dropped, so there is never more than one). The clock is
  not reset by a restart. The timeout is clamped to 1..=480 on read, and the Settings number
  input has min 1, max 480 and rounds (a negative or NaN value saturates to 0 and clamps to 1).
  An elevated window keeps a changed timeout in memory only (`write_refusal`).
- **Dialogs under the lock.** gpui-component dialogs are entities in `WindowState` and come
  back unchanged. The Key Bindings recorder is disarmed while locked because focus moves (D1
  covers the late path). OS file dialogs are D2.
- **Elevated.** `lock_oneterm` is in `ALLOWED_WHEN_ELEVATED`, and the exhaustiveness test
  covers it. With no vault in the administrator's profile, `can_lock_screen()` is false: no
  timer, no lock. `unlock_screen` skips the rehash when read-only (implementer test
  `the_elevated_window_locks_and_unlocks_read_only`). Not walked (UAC).
- **Code style and docs.** Theme tokens only (`background`, `foreground`, `muted_foreground`,
  `danger`). English. The `ui_config.json` row in `persistence.md`, the Key Bindings row
  (`ctrl-shift-l`, rebindable), the app menu item and the gui-layout § Lock screen are all
  present. The `lock screen: N frames in S s` log carries counts only. The password text
  appears nowhere in the profile or the log.

### GUI walk

Setup: a fresh `oneterm.exe` debug build from this worktree (the old binary predated the last
source edits by 16 minutes, so it was rebuilt; `cargo build -p oneterm-app` took 14 s). The
profile was private: `USERPROFILE` and the working directory were scratch folders, with the
floor fixture `vault-v1.json` as `vault.json` and `lock_after_idle_minutes: 1`. The driver was
the implementer's posted-message script, copied as `us0159v-gui.ps1`, and it touched only the
pid it launched. The native dialog was driven through UI Automation, `WM_SETTEXT` and
`BM_CLICK` on that pid's windows, never with the owner's mouse. Screenshots are
`us0159v-*.png` in the session scratchpad.

| # | Step | Result |
| --- | --- | --- |
| 1 | Start; `echo before-lock` in the tab; `ctrl-,` | the Settings window opens (2 windows) |
| 2 | `ctrl-shift-l` | **both** windows show only the lock (captures 03, 04) |
| 3 | At **each** locked window: Escape, `shift-escape`, `ctrl-w`, `ctrl-t`, `ctrl-f`, `ctrl-v`, `ctrl-,`, `ctrl-k`, `ctrl-shift-n/q/c/v/a/right/left/up/down/x/l`, Alt+Space, right-click, 3 wheel notches, Enter | process alive, still 2 windows, still locked (05, 06). The Enter on the empty input answered "Wrong master password." (I4) |
| 4 | Right password in the main window | both windows unlock (07, 08) |
| 5 | `echo after-unlock` | lands in the terminal. Focus was restored, and nothing typed while locked reached it: no stray characters, no new tab, split or zoom (09) |
| 6-9 | Settings > Terminal Logging, click Log Folder | the native "Select Folder" dialog opens, owned by the main window (main window disabled) |
| 10-11 | No input for 75 s | both windows locked **under** the dialog (12, 13) |
| 12 | In the dialog: path `...\us0159v-attacker-logs`, Select Folder | **`terminal.json` saved with that `logging.directory` at 18:46:00 while both windows were locked (15) = D2** |
| 13 | Unlock from the Settings window | the page shows General, not Terminal Logging (16). Terminal Logging shows the new folder with the reset arrow (17) |
| 14 | Security page, `ctrl-shift-l`, unlock | back on General (18-20) = **D3** |
| 15 | `ping -t 127.0.0.1`, no input | unlocked at 45 s and 75 s (21, 22), then locked at 129 s (log). Clean rerun: unlocked 18:50:20, **locked 18:51:21** with ping running (25) (I3) |
| 16 | Unlock, then `PrintWindow` every 20 s | locked at 61 s: another process's screenshots do not postpone it |
| 17 | Posted Alt+F4 (`WM_SYSKEYDOWN VK_F4`, Alt held) at a locked and then at an unlocked window | neither closed, so the emulation is not faithful (D4a) |
| 18 | Lock, "Quit OneTerm" | process exits |
| Logs | | no panic; `lock screen: N frames` lines carry counts only (main 44-168, Settings 0-46); the password appears nowhere in the profile or the log; one pre-existing DirectX SDK-layer error at start |

## Deviations (packet § "Choices the LLD leaves open")

1. **Two facts, not a fifth phase: accept.** Every dialog already treats Locked correctly, and
   `is_screen_locked()` cannot be true without a held header. Note I5 (it fails open by
   construction).
2. **Interceptor rule by binding, not by key list: accept.** It is stronger than the LLD's list.
   It resolves exactly as dispatch does, covers future bindings, and the tests enumerate the
   keymap. The gaps are D1 (other interceptors) and I2 (text with no keystroke). Neither lets a
   key reach the application while locked.
3. **Timeout clamped on read, file never rewritten, lenient parse: accept.**
4. **Security page "Lock Now" covers the screen and shows whenever a vault exists: accept**
   (HLD). There is no "drop the key only" action any more, which nothing needs.
5. **Events (`Locked` then `ScreenLocked`; plugins follow the fact, not the event): accept.**
   This is the right answer to the `US-0158` note (I8).
6. **No-vault notice without a button: accept** (out-of-scope follow-up recorded).
7. **Late input locks first: accept.** It is proved here against raw listeners, not only
   bindings. D1 and I2 are its limits.
8. **Timer recheck at most every 60 s: accept.** One wake a minute, no frame. It bounds the
   late-input window after a resume.

The rest: the reset text always shown (accept: Q7 wording, nothing secret); frame diagnostics
at debug level (accept: counts only); the binding-enumeration test living in `workspace` over
the kit's keymap plus OneTerm-shaped globals (accept: the rule is keymap-driven, and this walk
fired all of OneTerm's real default chords at both windows).

## For the owner

- **D2:** decide fix or accept. The fix is a few lines: one `is_screen_locked` guard in the
  four native-dialog callbacks. Accepting means a line in LLD § 7: "OS dialogs open at lock time
  stay usable; their result applies".
- D1, D3 and D4 can ship and be fixed as follow-ups.
- Still unwalked, as in the packet: a real sleep and resume, the elevated window (UAC), the DWM
  thumbnail of a minimised window, a physical Alt+F4, IME composition, Linux and macOS.

## Re-verification of fedd8734

Rework head `fedd8734` on `feat/idle-lock-screen`, built on the verifier's `dd57f82f`. Checked
out as `verify/US-0159` in the same worktree (12 GB free throughout).

### Verdict: PASS, no open defect

D1 and D2 are fixed. Each fix is proved by a test that fails with the guard removed, and D2 also
by the GUI walk. D3 is recorded as a known limit. D4's wording is corrected. I4 is taken: an
empty Enter is not an attempt. What remains is the list of notes and unwalked items above
(I1-I3, I5-I10). It is unchanged by the rework.

### Attacks and results

- **D2: one helper, four continuations.** `VaultState::screen_locked(cx)`
  (`crates/state/src/vault_state.rs:265`) is `try_global` plus `is_screen_locked()`. It returns
  false before init.
  - Placement: each guard sits **inside the same UI-thread step** that applies the result.
    - `auth_form.rs:611`: before the key path is set.
    - `logging.rs:193`: before both the input and the `set(...)` that saves `terminal.json`.
    - `transfer.rs:621`: before `do_upload_paths`.
    - `transfer.rs:721`: download. A separate `cx.update` runs right before
      `Self::download_to(...)`, with no `.await` between them. A window that is gone counts as
      locked.
  - Lock between pick and continuation: a lock that fires after the picker returns but before
    the continuation runs is caught. The new SFTP download test locks in exactly that gap.
  - Nothing leaks before a guard. Before the upload guard, only the path count is logged. Before
    the download guard, nothing is logged or written. The OS Save dialog creates no file.
  - Coverage: `rtk proxy grep -rn "prompt_for" crates` finds exactly the four call sites
    (`auth_form.rs:584`, `logging.rs:170`, `transfer.rs:577`, `:696`). The other match is an
    OSC 133 test name. There is no `window.prompt` and no other file-dialog API in `crates/`.
- **D1.** `recorder_armed` (`key_bindings_ui.rs:293`) answers false while locked, before it
  reads its own state.
  - Ordering: gpui keeps interceptors in a `SubscriberSet`, a `BTreeMap` keyed by an increasing
    subscriber id, so they run in registration order (`gpui-pre 0.3.7` `subscription.rs:20, 54`).
    The lock's interceptor is registered once, in `app::init` (`crates/app/src/init.rs:93`,
    before any window). The recorder's interceptor is created only by a click on Edit
    (`key_bindings_ui.rs:272`). So the lock runs first and has locked by the time the recorder
    asks.
  - Tests: the flipped `verifier_d1_a_later_interceptor_still_sees_the_key_that_locked` asserts
    0 keys taken. Note: the settings-ui test fails under mutation only because no
    `KeyBindingsState` exists (a panic), not because an armed recorder took a key. The ordering
    proof is the workspace test.
- **I4.** `lock_screen.rs:485`: `submit` returns when `len_chars() == 0`, before
  `unlock_screen`. So there is no Argon2id run, no failure counted and no back-off change. The
  Unlock button goes through `submit` too. A space is one character and **is** an attempt.
  That matches Q8 (bytes as typed, never trimmed). The docs say "an empty Enter", which reads
  correctly. The button stays enabled on an empty field and a click does nothing: cosmetic.
- **D3, D4 docs.** LLD § 7 has the native-dialog and interceptor rules and the element-state
  limit. LLD § 6.4 now says Alt+F4 is not an OS bypass. The packet's Alt+F4 tick reads "not
  walked". `docs/gui-layout.md` names the header held in memory and records the Settings page
  reset.

### Mutation checks (each run, then reverted; `git diff` shows only the added tests)

| Guard disabled | Test | Result |
| --- | --- | --- |
| `auth_form.rs:611` | `a_key_picked_while_the_screen_is_locked_is_dropped` | FAILED ("a result applied under the lock", left `picked_key`) |
| `transfer.rs:621` and `:721` | the two new `reverify_*` tests below | both FAILED |
| `recorder_armed`'s lock check | `the_recorder_takes_no_key_while_the_screen_is_locked` | FAILED (panic: no `KeyBindingsState`) |
| the empty-Enter check | `unlocking_restores_the_focus_and_counts_a_wrong_password` | FAILED (`lock_screen_tests.rs:280`) |

### Tests added (pass)

`crates/sftp-ui/src/transfer.rs` tests, with a `locked_vault` helper (the floor fixture in a
`TempDir`):

- `reverify_a_save_path_answered_into_a_lock_downloads_nothing`: the Save dialog opens
  unlocked, the path is picked, and the lock fires before the continuation runs. The backend
  gets no transfer request, and no queue item appears.
- `reverify_an_upload_picked_under_the_lock_uploads_nothing`: a pick while locked starts no
  upload and no collision dialog. After an unlock, the same pick uploads `/home/u/a.txt`.

### GUI walk (rebuilt debug binary, `cargo build -p oneterm-app` 15 s, private profile, own pid)

| Step | Result |
| --- | --- |
| Settings > Terminal Logging > Log Folder: the native "Select Folder" dialog opens | as before |
| No input for 80 s | both windows locked under the dialog (`us0159v-r01`, `r02`) |
| In the dialog: path `...\us0159v-attacker-logs`, Select (`WM_SETTEXT` + `BM_CLICK` to the dialog's controls) | the dialog closes; `terminal.json` **unchanged** (last write 19:11:33, at startup; `directory` still the default); log line `log folder picker answered while locked; ignored`; both windows still locked (`r03`, `r04`) |
| Control: unlock, the same pick | `terminal.json` saved at 19:14:30 with `us0159v-attacker-logs`. The method works, so the drop under the lock is real |
| Logs | no panic; the password appears nowhere in the log or the profile's JSON |

### Gate (head `fedd8734` plus the two tests)

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo test -p oneterm-state -p oneterm-workspace -p oneterm-settings -p oneterm-settings-ui -p oneterm-actions -p oneterm-session-ui -p oneterm-sftp-ui` | pass: state 86, workspace 64 + 3 ignored, settings 50, settings-ui 59 + 1 ignored, actions 4, session-ui 100 + 1 ignored, sftp-ui 77 (75 + 2 new) |
| `--ignored --test-threads=1` for workspace / settings-ui / session-ui | 3 / 1 / 1 passed |
| `verify-dependency-graph.py`, `check-doc-paths.py`, `check-english.py`, `check-theme-contrast.py`, `third-party-notices.py --check` | pass |
| `Cargo.lock` | unchanged since `ce80f17c` |
| `%TEMP%` | 42 `oneterm-*` entries before and after; no `oneterm-key-picker-lock-*`, `oneterm-recorder-lock-*` or `oneterm-local-pane-*` left |

## Acceptance rework (lock at startup) — verification of 6be2402a

Independent verifier, 2026-10-13. Worktree reset to `6be2402a` (base `main` @ `b97196e1`),
branch `verify/US-0159-startup`, own `target`, debug only, `CARGO_BUILD_JOBS=2` then `1` (the
box sat at about 3.4 GB free commit: rustc ran out of memory on `windows` and `gpui-component`
several times; each step was retried, none was a test failure). Disk: 42 GB free at start.

### Verdict: FAIL — the security boundary holds; two Medium defects, one a regression of this commit

The ruling is met: with a Ready vault every window's first drawn frame is the lock screen
(walked twice; the first capture from another process already shows it, never the
application); a window opened under the startup lock is born locked; Uninitialised and
Unavailable start unlocked and the lock action explains itself; the elevated rule is the same
code path (unit-tested); no Reset on the lock screen; Quit works and exits cleanly. Every item
of the rework acceptance list passes. The verdict is FAIL because of V1, which the unrequested
focus change introduced (the fix is one condition), and V2, which the startup lock turns from a
rare idle-lock case into one met at every start after a crash.

### Defects

| # | Severity | Where | What | Repro |
| --- | --- | --- | --- | --- |
| V1 | Medium (regression of `6be2402a`) | `crates/workspace/src/lock_screen.rs:590` with `:574` | `.track_focus(&self.focus)` on the whole lock view makes gpui move the focus to that handle on any mouse down on the lock background (`gpui-pre 0.3.7` `div.rs:2768`). The reclaim at `:574` fires only when the focus is *outside* the view, so it never gives the input its focus back: the input loses its focus ring, and typing and Enter go nowhere, silently, until the user clicks the field. Before `6be2402a` the view tracked no focus and a background click left the input focused. | Startup lock, type `xy`, click the empty area left of the card, type `abc`: the input still shows 2 dots (`us0159rv-08`). Test `verifier_startup_a_background_click_takes_the_keyboard_from_the_input` pins it. Suggested fix: reclaim also when the view's own handle is the focused one (`!contains_focused \|\| self.focus.is_focused(window)`), or drop `track_focus`: mutation M2 below shows every other lock test still passes without it. |
| V2 | Medium | `crates/workspace/src/lock_screen.rs:375` | On unlock the plugin restores the focus saved when the lock began (at startup: the terminal the workspace focused while it was built). A dialog opened under the lock (the crash-report dialog `show_crash_reports` opens at start, a host-key prompt under an idle lock) is then on top with the focus behind it: keys typed "into the dialog" go to the shell under the modal, and Enter runs them there. Before the rework the startup crash dialog opened unlocked and took the focus. | Stage `target\crashes\<id>.crash.txt` in the profile, start, unlock, type `echo behind`, Dismiss: the shell prompt holds `echo behind` (`us0159rv-09` to `11`). Test `verifier_startup_after_the_unlock_a_dialog_opened_under_the_lock_has_no_focus` pins it. Suggested fix: on unlock, when the window has an active dialog, focus the dialog instead of the saved handle (or skip the restore). |

### Not defects (recorded)

- **Tab on the lock screen never leaves the input.** `Root`'s Tab handler (`root::Tab`, bound
  in the `Root` context) is dropped with the surface, so Unlock and Quit are reachable only by
  Enter, the mouse, or a window close (Q6). Pre-existing since `US-0159`. The gpui test
  platform delivers the unhandled tab as text; Windows filters control characters
  (`gpui-pre-windows 0.3.7` `events.rs:1641`): walked, Tab followed by the right password
  unlocks. Test `verifier_startup_tab_does_not_leave_the_lock_input`.
- **`vault.bak`.** `vault::document::load` (`crates/vault/src/document.rs:94`) reads only
  `vault.json`; nothing resurrects a vault from `vault.bak` (walked: `vault.json` deleted,
  `vault.bak` kept, OneTerm started with no lock). `vault.bak` keeps the old wrapped key, sealed
  under the forgotten password, until the next set-up overwrites it (`Create` goes through
  `update_secret_json_file`, which makes `.bak` a copy of the new document). It exposes nothing
  `vault.json` did not. Optional wording: "delete vault.json (and vault.bak)". Not a `DEC-0021`
  rule 2 breach: the rule governs the in-app Reset, and the paragraph tells the truth (every
  saved password is lost).
- **Recovery paragraph wording.** Text matches Q6/Q7 and `DEC-0021` rule 2, English, no secret.
  The printed folder is right for the private profile (`<cwd>\target`: the debug `config_dir()`
  made absolute against the process cwd, which nothing changes, no `set_current_dir` in
  `crates/`) and, by construction, for a release build (`home_dir()\.OneTerm`, the same function
  `vault.json` is read with). Cosmetic: the colon right after the path reads as part of it
  (`...\target:` on screen).
- **`ensure_unlocked` while screen-locked** returns silently (`log::debug!`) and runs no
  continuation. Every caller (`connect_dialog.rs:196`, `:309`, `session_dialog.rs:745`) is
  user-initiated and unreachable under the lock; the `:309` connect dialog falls back to inputs,
  the outcome `US-0158` already accepts. Acceptable.
- **No event at start.** `load_at_start` emits nothing; the only `VaultEvent` subscriber outside
  `oneterm-state` is `SshSessionStore` (`session_state.rs:874`), for `Reset` only. The plugin
  follows `is_screen_locked` through an observer and reads it in `LockScreen::new`.
- **Quick Connect hops with a stale envelope** show a plain input with no "master password was
  reset" note (`quick_connect_dialog.rs:97` sets `use_saved(false)` without a note).
  Pre-existing `US-0158` behaviour, identical with a Locked vault; no panic.
- **Quit on the startup lock** exits in under a second, no panic; the profile's only write is
  the workspace's normal layout save (`docks.json`, a complete document, `.bak` the previous
  generation); `vault.json`, `ssh_session.json`, `ui_config.json`, `terminal.json` untouched.
  The password appears in no log and no profile file.

### Attacks and results

| Attack | Result |
| --- | --- |
| (a) First frame, code | `app::init` runs `VaultState::init` (`load_at_start`) before `lock_screen::init`; `open_window` builds the workspace, then `Root::new` builds the plugins, so `LockScreen::new` -> `follow_vault` sets `locked` before the first `Root::render`; `decorate` drops the surface unbuilt. Holds. |
| (a) First frame, mutation M1 | `this.follow_vault(window, cx)` removed from `LockScreen::new`: 7 startup tests FAIL (`a_vault_at_start_locks_the_window_before_its_first_frame`: renders 1, expected 0; `verifier_startup_a_window_opened_under_the_startup_lock_never_renders`: 1 vs 0; others). The 0-render test does catch an ordering slip. |
| (a) Window opened under the startup lock | New test: a second `Root` window opened while startup-locked is born locked, renders its application 0 times, renders after the unlock. Walked with Settings and main both locked and unlocked from the Settings window (`us0159rv-18` to `20`). |
| (b) Focus fix vs buttons | Buttons are focusable and inside the tracked view, so a click on one is not undone. Tab cannot reach them (above). |
| (b) Background click | **V1.** |
| (b) Two windows | Focus is per window; each `LockView` reclaims only its own. Both locked: 359 ms CPU in 10 s (one window: 312 ms; unlocked idle baseline 203 ms). No ping-pong. |
| (b) Dialog under the lock | Real crash-report dialog staged: while locked typing reaches the lock input at once (`xy` before the click), the dialog takes no key. After the unlock: **V2**. |
| (b) Render loop | `focus()` during draw does not refresh (gpui `refresh` is a no-op while drawing) and re-focusing the focused handle returns early. Test: 0 extra frames in 10 s of fake time, with and without a dialog under the lock; GUI CPU above. |
| (c) `load_at_start` | Ready -> locked, not busy, `locked_meanwhile` false; Uninitialised / damaged / newer / unreadable -> not locked, normal and elevated; elevated Ready -> locked, unlock read-only (implementer's tests, all pass). Mutation M4 (`screen_locked = false`): 2 state tests FAIL. |
| (d) `ensure_unlocked` guard | Opens nothing, continuation not run, silent (above). |
| (e) Idle clock | No timer while locked (`restart_timer` returns at once). The timer observer is registered in `init` before any window's plugin observer, but it only spawns the task; the plugin's stamp runs in the same flush, before the task's first poll. Mutation M3 (stamp removed): 3 tests FAIL. New test: 9 h startup lock, unlock, the first key does not relock. |
| (f) Stale envelopes | Walked: `vault.json` deleted, the `prod` connect dialog shows the input, "Update saved password" and the reset note (`us0159rv-14`). Hops: above. Under a new vault the envelope fails as `OtherVault` with its own message (`auth_form.rs:75`, vault-level tests); not walked. |
| (g) Recovery paragraph | Above. |
| (h) Quit on the startup lock | Above (`us0159rv-12`). |
| (i) Unavailable at start | `vault.json` = `not json`: no lock; `ctrl-shift-l` -> "OneTerm cannot lock: vault.json is damaged (not JSON). Reset is the only way back." (`us0159rv-15`); Settings > Security shows the status, Reset..., and the new Screen Lock text (`us0159rv-17`). |

### GUI walk

Debug `oneterm.exe` built in this worktree; private profile (`USERPROFILE` and cwd in scratch,
so `config_dir()` = `<scratch>\us0159rv-cwd-a\target`); floor fixture `vault-v1.json` and
`ssh-session-v2-sealed.json`; posted window messages to the launched pid only; screenshots
`us0159rv-*.png` in the session scratchpad.

| Step | Result |
| --- | --- |
| Launch with a vault, capture loop every ~50 ms from another process (`PrintWindow`) | the first capture (260 ms after the window became visible; the first `PrintWindow` call is slow) is the lock screen; no capture shows application content. Relaunch: first capture at 154 ms, the lock screen again |
| `ctrl-,`, `ctrl-shift-q`, `ctrl-shift-l` while startup-locked | 1 top-level window, process alive, still locked |
| Typing `abc` without a click | 3 dots: the input has the keyboard from the start |
| Enter | "Wrong master password.", input cleared (`04`) |
| Tab, then the right password, Enter | unlocked (`05`) |
| `echo ok` + Enter | ran in the terminal (`06`): focus restored to the terminal |
| Relaunch with a crash report staged | locked; 10 s idle = 312 ms CPU; background click -> V1 (`08`); after the unlock -> V2 (`09` to `11`) |
| Quit OneTerm on the startup lock | exits; profile diff above |
| `vault.json` deleted, `vault.bak` kept | no lock (`13`); `prod` shows the reset note (`14`) |
| `vault.json` = `not json` | no lock; lock action notice (`15`); Settings > Security (`17`) |
| Settings and main, `ctrl-shift-l`, unlock from Settings | both lock, both unlock (`18` to `20`) |
| Logs | no panic; only the DirectX debug-layer line |

### Mutation checks (each reverted with `git checkout -- <file>`)

| Mutation | Tests that failed |
| --- | --- |
| M1 `LockScreen::new` no longer calls `follow_vault` | 7 startup tests incl. `a_vault_at_start_locks_the_window_before_its_first_frame` |
| M2 `.track_focus(&self.focus)` removed | only `verifier_startup_a_background_click_takes_the_keyboard_from_the_input` (the click no longer focuses the view): the root cause of V1; the implementer's dialog and blur tests still pass |
| M3 `IdleClock::stamp` removed from the unlock | `the_idle_clock_starts_at_the_first_unlock`, `verifier_startup_the_first_key_after_a_long_startup_lock_does_not_relock`, `verifier_after_an_unlock_the_timer_locks_again` |
| M4 `load_at_start` does not lock | `a_usable_vault_at_start_locks_the_screen_before_any_input`, `wrong_passwords_at_start_back_off` |

### Tests added (`crates/workspace/src/lock_screen_tests.rs`, all pass)

- `verifier_startup_a_background_click_takes_the_keyboard_from_the_input`: pins V1; flip
  when fixed.
- `verifier_startup_tab_does_not_leave_the_lock_input`
- `verifier_startup_an_idle_lock_screen_draws_no_frame_loop`
- `verifier_startup_a_window_opened_under_the_startup_lock_never_renders`
- `verifier_startup_after_the_unlock_a_dialog_opened_under_the_lock_has_no_focus`: pins V2;
  flip when fixed.
- `verifier_startup_the_first_key_after_a_long_startup_lock_does_not_relock`

### Gate (head `6be2402a` plus the six tests)

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo clippy --workspace --all-targets --features oneterm-app/terminal-diagnostics -- -D warnings` | pass |
| `cargo test -p oneterm-state` / `-p oneterm-workspace` / `-p oneterm-session-ui` / `-p oneterm-settings-ui` / `-p oneterm-app` | 90 / 73 + 3 ignored (67 + 6 new) / 101 + 1 ignored / 59 + 1 ignored / 26, all pass (run per crate: the combined command re-unifies `windows` features and ran out of memory) |
| `--ignored --test-threads=1` for workspace / session-ui / settings-ui | 3 / 1 / 1 passed |
| `python scripts/check-ignored-tests.py` | pass (19 ignored tests, all recorded) |
| `verify-dependency-graph.py`, `check-doc-paths.py`, `check-english.py`, `check-theme-contrast.py`, `third-party-notices.py --check` | pass |
| `Cargo.lock` | unchanged since `b97196e1` |
| `%TEMP%` | 36 directories before, 40 after: the 4 new ones are Edge/BITS, none ours; no `oneterm-*` directory left |

## Re-verification of 86af2afb (round 2)

Independent verifier, 2026-10-13. Branch `verify/US-0159-startup` reset to `86af2afb`
(contains the round-1 verification as `cbc860ed`); own `target`, debug, `CARGO_BUILD_JOBS=2`,
one crate per test command (one out-of-memory retry on `oneterm-app`, no test failure). Disk:
28.6 GB free at start.

### Verdict: PASS — V1 and V2 fixed; one new Low defect (V3) and two notes

### Fixes checked

- **V1** (`lock_screen.rs:605`): while locked the input takes the focus back whenever it lacks
  it. Walked: type `xy`, click the background, type `abc` -> 5 dots (`us0159rv2-01`). The
  pinned test is flipped and passes. Mouse-down on Unlock focuses the button and the next
  frame gives the input the focus back. That does not break the click: walked with the mouse
  held 1.5 s on Unlock, then released -> unlocked (`02`, `03`). Quit OneTerm clicked on the
  startup lock -> the process exits (`10`). No loop: 375 ms CPU in 10 s startup-locked (round
  1: 312 ms); each window reclaims only its own focus, so two windows cannot ping-pong.
- **V2** (`:380`, `:397`): the unlock focuses `taken` (the dialog opened under the lock), else
  the saved focus. `LockScreen::render`, which runs only while unlocked, hands a focus left
  on the hidden lock view back to the saved one. Walked with a **host-key prompt arriving under
  an idle lock**: `sftp-dev-server` (random host key per run) behind a loopback proxy that holds
  the TCP connection 4 s (`us0159rv2-delay-proxy.py`). Connect, then `ctrl-shift-l` at once.
  The prompt arrived under the lock, and the lock input kept the keyboard (`05`, `06`). After
  the unlock the prompt is on top with the keyboard (`07`). Typing `echo leak` + Enter reached
  the prompt (Enter = Trust and Connect), not the local shell, whose prompt stayed empty
  (`08`). Later typing went to the new SSH tab (`09`).
- **Path quoting** (`:77`): `in the folder "<path>".`, no trailing colon (`01`).

### Attacks (the implementer's list)

| # | Attack | Result |
| --- | --- | --- |
| 1 | Anything but a dialog landing focus on the hidden lock view while unlocked | Only a view that saved the lock input as its "focus to restore" can put it there: a dialog or popover opened under the lock. Popovers and menus cannot open under the lock (the surface is dropped, input is stopped); notifications take no focus. `LockScreen::render` hands any such focus back. Residual, by code reading only: if the saved handle has died meanwhile (its tab closed), `upgrade()` is `None`, the focus stays on the hidden view, and keys go nowhere until a click. That needs a tab closed under the lock, which no input can do. |
| 2 | Two dialogs opened under the lock | The unlock focuses the last one, which is the one on top. **V3:** closing it restores the lock input it saw when it opened, and the plugin hands that to the pre-lock focus, *behind* the first dialog, which is still open. Without the lock, closing B would focus A. Test `verifier_r2_two_dialogs_under_the_lock` pins it. |
| 3 | Host-key prompt during an idle lock | Walked (above): the prompt has the keyboard, Enter does not reach the shell. |
| 4 | Dialog closed while locked | No dialog in the app closes itself while locked that I could stage; tested with `close_dialog` under the lock instead. The lock input keeps the keyboard. |
| 5 | `taken` outliving its dialog | Same test: the closed dialog's handle is dead at the unlock (`upgrade()` -> `None`), no panic, and the focus goes to the pre-lock view (`verifier_r2_a_dialog_closed_under_the_lock_strands_nothing`). |
| - | Dialog open *before* an idle lock (user walked away from a connect dialog) | Keeps the keyboard after the unlock (`verifier_r2_a_dialog_open_before_an_idle_lock_keeps_the_keyboard`). |

### Defects and notes

| # | Severity | Where | What |
| --- | --- | --- | --- |
| V3 | Low (needs two dialogs opened under one lock; the startup crash reports open one at a time) | `crates/workspace/src/lock_screen.rs:397` | After the unlock, closing the top dialog of two opened under the lock gives the keyboard to the application behind the remaining dialog (keys go to the shell under a modal, the V2 symptom one level down). A fix could remember every `taken` handle, or hand the focus back to the active dialog when one is open. |
| N1 | Info | V2 design, `ssh` host-key prompt | The unlock now focuses a host-key prompt the user has not seen yet. Its Enter is "Trust and Connect" (walked), so an Enter typed ahead after the unlock trusts an unseen key. The unlock's own Enter is consumed while the lock is busy; the risk is a deliberate second Enter after the screen changed. The prompt's default action is `US-0158` / SSH territory, not this packet's. |
| N2 | Info | `:397` | Residual stranding when the saved focus died under the lock (attack 1). Not reachable with the input blocked. |

### Mutation checks (each reverted)

| Mutation | Tests that failed |
| --- | --- |
| R1 the unlock ignores `taken` | `verifier_startup_after_the_unlock_a_dialog_opened_under_the_lock_has_no_focus`, `verifier_r2_two_dialogs_under_the_lock` |
| R2 no hand-back in `LockScreen::render` | the same two (the closed dialog leaves the focus on the hidden lock view) |
| R3 the old V1 condition (reclaim only when the focus is outside the view) | `verifier_startup_a_background_click_takes_the_keyboard_from_the_input` |

### Tests added (`crates/workspace/src/lock_screen_tests.rs`, all pass)

- `verifier_r2_two_dialogs_under_the_lock`: pins V3; flip when fixed.
- `verifier_r2_a_dialog_closed_under_the_lock_strands_nothing`
- `verifier_r2_a_dialog_open_before_an_idle_lock_keeps_the_keyboard`

### GUI walk

Debug build from this worktree; private profiles `us0159rv-cwd-c` / `-d`; own pids only; the
dev server and the proxy were started and stopped by pid, and the dev server's
`oneterm-sftp-dev-*` temp directory was removed. Screenshots: `us0159rv2-*.png` in the session
scratchpad.

| Step | Result |
| --- | --- |
| Startup lock, `xy`, background click, `abc` | 5 dots, input focused (`01`) |
| Password, mouse held 1.5 s on Unlock, release | unlocked (`02`, `03`) |
| Connect `loop` (127.0.0.1:2299, 4 s proxy delay), `ctrl-shift-l` at once | locked; the prompt arrives under the lock; the lock input stays empty and focused (`05`, `06`) |
| Unlock | "Unknown SSH Host Key" on top with the keyboard (`07`) |
| `echo leak` + Enter | Trust and Connect; the local shell's prompt is unchanged (`08`) |
| Typing afterwards | reaches the new SSH tab (`09`) |
| Relaunch, Quit OneTerm on the startup lock | exits (`10`) |

### Gate (head `86af2afb` plus the three tests)

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| `cargo test -p oneterm-workspace` | 76 passed, 3 ignored |
| `cargo test -p oneterm-state` | 90 passed |
| `cargo test -p oneterm-session-ui` | 101 passed, 1 ignored |
| `cargo test -p oneterm-settings-ui` | 59 passed, 1 ignored |
| `cargo test -p oneterm-app` | 26 passed |
| `--ignored --test-threads=1` workspace / session-ui / settings-ui | 3 / 1 / 1 passed |
| `verify-dependency-graph.py`, `check-doc-paths.py`, `check-english.py` | pass |
| `check-ignored-tests.py` | pass (19 recorded) |
| `Cargo.lock` | unchanged since `b97196e1` |
| `%TEMP%` | 41 directories before, 42 after: the new one is Edge/BITS; no `oneterm-*` left |
