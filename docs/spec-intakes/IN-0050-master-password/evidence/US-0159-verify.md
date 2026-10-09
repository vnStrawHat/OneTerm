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
