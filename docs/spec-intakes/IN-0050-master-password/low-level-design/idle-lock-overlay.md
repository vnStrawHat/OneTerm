# Low-Level Design: Idle lock screen

Intake: IN-0050
HLD: `docs/spec-intakes/IN-0050-master-password/high-level-design.md`
Topic: activity detection, idle timer, the lock `RootPlugin`, input blocking, interactions
Date: 2026-10-09

## Concern

Once a vault exists, cover every OneTerm window after a period without user input, keep the
sessions running underneath without drawing them, and let only the master password bring the
UI back. Owned by `oneterm-workspace` (`crates/workspace/src/lock_screen.rs`, new), with the
phase and the unlock call in `oneterm-state`'s `VaultState`.

## Design

### 1. Why a `RootPlugin`, and why `decorate` replaces the surface

GPUI Kit 0.7 splits the window root: `gpui_base::Root` owns the window content and runs
registered plugins (`reference/gpui-kit/crates/base/src/root.rs`):

- `RootPlugin` (root.rs:46) has three hooks: `prepare`, `style`, and `decorate(surface, root,
  window, cx) -> impl IntoElement`, which wraps (or replaces) the completed surface.
- `Root::register_plugin::<V>(cx, build)` (root.rs:101) registers a factory once per
  application; every **future** `Root` builds one instance. So one registration covers the main
  window and the Settings window, and any later window type.
- `impl Render for Root` (root.rs:280) builds `content = div(TextSelectionLayer, app view,
  absolute overlay of every plugin's view)`, applies each plugin's `style`, then folds each
  plugin's `decorate` over it **in registration order**.
- `gpui_component::init` registers the component layer `WindowState` first
  (`reference/gpui-kit/crates/component/src/root.rs:21`); its overlay holds the sheet, dialog and
  notification layers (`WindowStateLayers`, root.rs:484) and its `decorate` wraps the surface in
  `window_border()` (root.rs:450).

OneTerm registers `LockScreen` **after** `gpui_component::init` (in `app::init`, before the main
window opens; `crates/app/src/lib.rs:204` then `crates/app/src/init.rs`). Its `decorate` is
therefore the outermost one:

```text
unlocked:  decorate(surface) = surface                     (zero cost)
locked:    decorate(surface) = window_border().child(lock_view)   // surface dropped unbuilt
```

When locked, the surface element (workspace or Settings panel, dialogs, toasts, tooltips) is
dropped before layout, so the application view is never rendered, laid out or painted, and its
accessibility nodes do not exist. That is stronger than an opaque layer on top: no frame
contains terminal pixels, a screenshot or screen share shows only the lock, and terminal output
costs no frames while locked (a view that was not rendered in the last frame does not dirty the
window when it notifies). The workspace entity, its terminals and their PTY / SSH tasks keep
running.

**Rejected:** a gpui-component `Dialog` or `Sheet`. Both live inside `WindowState`'s layers, so
the application is still painted underneath, `Escape` and click-outside close them, and
another dialog can open above. An opaque `Render` overlay alone has the same painting problem.

`lock_view` is a child `Entity<LockView>` owned by the plugin (so its handlers get a proper
`Context`), wrapped again in `gpui_component::window_border()` because dropping the surface
also dropped the border `WindowState` added. `LockView` renders `gpui_component::TitleBar`
(window controls, drag area; the elevated marker from
`oneterm_core::elevation::window_title_parts` when restricted, M5), the lock text, a masked
`Input` with key context `OneTermLock`, the Unlock button, the error / back-off line, the
"Forgot it?" disclosure and a Quit button. Theme tokens only. No host name, tab title, session
count or notification text appears on it.

### 2. Detecting user activity

The `IdleClock` (a `Global` in `oneterm-workspace`) holds `Cell<Instant>` and
`Cell<SystemTime>` of the last input. Stamping is a `Cell::set` through `cx.global::<IdleClock>()`:
no `update_global`, so no observer runs and no frame is requested.

| Signal | Hook | Notes |
| --- | --- | --- |
| Keys, in every window | `cx.intercept_keystrokes` (`gpui-pre 0.3.7` `src/app.rs:2325`), one app-wide subscription | Runs **before** actions and raw key handlers (`window.rs:5861` `dispatch_keystroke_interceptors`). `observe_keystrokes` is not usable: it skips keystrokes consumed by a raw key handler, which is how the terminal takes typing (`app.rs:2295-2299` doc comment). Standalone modifiers arrive on release. |
| Mouse move, button, scroll | A zero-size `gpui::canvas` in the plugin's `Render` overlay whose paint closure calls `window.on_mouse_event::<MouseMoveEvent / MouseDownEvent / ScrollWheelEvent>` (`window.rs:5246`) | Raw window listeners receive every event of their type in both phases, whatever hitbox is on top; element-level `on_mouse_move` would miss moves over an occluding dialog. Re-registered each frame the root paints; between frames the last frame's listeners stay active. |
| Focus / activation | `cx.observe_window_activation(window, ...)` (`src/app/context.rs:420`) in the plugin's `build` | Switching to a OneTerm window is interaction. |
| **Not** activity | PTY or SSH output, OSC events, timers, notifications, programmatic focus | Nothing on those paths touches the clock, by construction. |

Idle time = `max(Instant::now() - last_instant, SystemTime::now() - last_wall)` (saturating). The
wall-clock term makes a machine that slept past the timeout lock on resume even if the
monotonic clock did not advance during sleep (unverified on Windows; `US-0159` checks it).

### 3. The timer

One application task, started by `lock_screen::init`:

```text
loop {
    if phase is Uninitialised or IdleLocked or Unavailable { wait for a VaultState change; continue }
    let timeout = UiConfig.lock_after_idle_minutes (default 15, 1..=480)
    let remaining = timeout - idle_time()
    if remaining <= 0 { VaultState::lock_screen(cx); continue }
    sleep(remaining)            // cx.background_executor().timer(remaining)
}
```

- At most one wake per timeout period plus one per extension: nothing on the frame grid, so
  `oneterm_state::until_next_tick` is not used.
- A `UiConfig` or `VaultState` change restarts the task (drop and respawn), so a shorter timeout
  takes effect at once.
- Each process (the elevated window is one) has its own timer and clock: input in one process
  does not keep the other awake.

### 4. Locking and unlocking

`VaultState::lock_screen(cx)`: drop the `UnlockedVault` (the key), set `IdleLocked`, `notify`.
Each `LockScreen` plugin observes `VaultState` (`cx.observe_in(&vault_state, window, ...)`),
and on the transition:

1. saves `window.focused(cx)` as a weak handle;
2. clears the window's text selection (`gpui_base::TextSelection::clear`), as dialogs do;
3. defers focusing the lock input (`window.defer`; a `WindowExt` call inside a `Root` update
   panics on GPUI Kit 0.7, see the `US-0149` lesson);
4. notifies itself, which re-renders the root, so the next frame is the lock screen.

Unlock: the lock input's Enter or the button calls `VaultState::unlock_screen(password, cx)`,
which verifies against the **in-memory** header (never a fresh read of `vault.json`, see the
edge cases), runs the KDF on the background executor and, on success, stores the
`UnlockedVault`, sets `Unlocked` and resets the failure count. Each plugin restores its saved
focus (if the handle is still alive) and notifies; the surface comes back on the next frame.
The `IdleClock` is stamped.

**At start** (`US-0159` acceptance rework; intake Q1 re-ruled 2026-10-13). `VaultState::init`
reads `vault.json` with `VaultState::load_at_start`, which sets the screen lock when the file is
Ready (no event: nothing subscribes yet). `app::init` runs it before `lock_screen::init`, and
both before the main window opens, so the window's `LockScreen` is **born locked** (the plugin
is built in `Root::new`, after the workspace) and the first frame `Root` renders is the lock
view: the application view is never rendered before the first unlock. The focus the workspace
gave its first terminal while it was built is the focus the unlock restores. No timer runs
while the startup lock is up (`restart_timer` finds the screen locked); the unlock stamps the
`IdleClock` before the re-armed timer first reads it, however long the lock screen was up.
Uninitialised and Unavailable (damaged, newer, unreadable) start unlocked: there is nothing to
unlock with. The elevated instance follows the same rule read-only (Q5). No setting turns it
off.

### 5. Wrong passwords

- Inline "Wrong master password." under the input; the input is cleared.
- The failure count lives in `VaultState` (process-wide, in memory, shared by every window and
  by the unlock dialog). From the 5th consecutive failure the input and button are disabled for
  `min(2^(n-4), 30)` seconds with a countdown (`2, 4, 8, 16, 30, 30 ...`).
- Never a permanent lockout, never a wipe, never persisted (a restart resets the count; the
  KDF cost, not the UI counter, is what limits guessing).
- Each attempt runs one KDF off the UI thread; the button shows a spinner and further
  submissions are ignored while one is in flight.

### 6. What input reaches what while locked

1. **The application is not in the element tree**, so no workspace, panel, terminal or dialog
   handler is on any dispatch path. Its bindings cannot fire.
2. **The interceptor gates the rest.** While `IdleLocked`, for every keystroke in every window:
   if the context stack does not contain `OneTermLock`, `cx.stop_propagation()`; if it does,
   allow keystrokes that produce text (`key_char` is some, which keeps AltGr characters
   working) and the editing keys the input needs (Backspace, Delete, arrows, Home, End, Enter,
   Tab, Escape, and paste for password managers); stop everything else, which removes the
   context-free global bindings (`ctrl-shift-q` Quit, theme switch, FPS toggle ...).
3. **Mouse:** only the lock view's elements exist to receive clicks.
4. **Window close** (the caption button) is an OS event, not a keystroke. Closing the main
   window quits the app (its existing `on_release` hook), which the design allows (intake Q6);
   closing the Settings window just closes it. **Alt+F4 is not an OS-level bypass** (`US-0159`
   verify D4): on Windows `WM_SYSKEYDOWN` goes through gpui's keystroke dispatch first, so the
   interceptor sees it; it closes the window only because an unbound key passes inside the lock
   view. A physical Alt+F4 was not walked (a posted one closes nothing, locked or not).

### 7. Interactions

| With | Behaviour |
| --- | --- |
| Several windows | One `VaultState` per process; every `Root` has its own `LockScreen`; all lock and unlock together. A Settings window opened while locked is born locked. |
| Elevated window (`IN-0043`) | Own process: reads `vault.json` (Q5), starts locked when it is usable, and locks on its own timer; unlock verifies the password read-only (`persistence-and-migration.md` § 7). Keeps its Administrator marker on the lock screen (M5). No vault in its profile means no lock. |
| "Run as administrator" / UAC | The "+" menu is part of the dropped surface, so no elevation can start from a locked window. The UAC prompt itself is on the secure desktop and is not OneTerm's. |
| Toasts / notifications | The notification layer is inside the surface: nothing shows while locked. Toasts raised meanwhile may expire unseen; failures that matter are also shown in place (connect dialog inline error, tab disconnect state). |
| Open dialogs (connect, host key, session edit) | Hidden, not closed; back unchanged after unlock. A typed but unsent SSH password stays in that dialog's memory (accepted). A dialog opened while locked does not keep the focus (As built, lock at startup). |
| App menus | Windows and Linux draw them in-window (title bar): gone with the surface. macOS native menu items dispatch to the focused window; workspace handlers are unreachable; the few global handlers are harmless. Swapping the macOS menu while locked is a follow-up (untested platform). |
| Native OS dialogs (file picker, Save dialog) | **Not covered by the lock**: a dialog open when the lock comes is a window of its own and stays usable over the lock screen. **Rule:** a continuation that applies a native dialog's result checks `VaultState::screen_locked(cx)` first and drops the result while locked (`US-0159` verify D2: the SSH key picker, the log-folder picker, SFTP upload and download Save). There is no shared picker wrapper; every new `prompt_for_paths` / `prompt_for_new_path` caller must follow the rule. |
| Other keystroke interceptors | gpui runs every interceptor whatever an earlier one did, so the key that locks (late input) still reaches them. **Rule:** an interceptor that acts on a key returns early when `VaultState::screen_locked(cx)` (the Key Bindings recorder does, `US-0159` verify D1). |
| Element-keyed UI state | gpui keeps keyed element state only for elements rendered in the last frame, so a lock drops it: the Settings window comes back on its first page (General), because the kit's `Settings` keeps the selected page in element state with no way to set it from outside. Entity state (dialogs, inputs, terminals, the dock) survives. Known limit (`US-0159` verify D3). |
| Clipboard | A remote OSC 52 write still reaches the system clipboard (terminal policy unchanged); nothing is read from it. |
| Screenshots, screen sharing, Alt+Tab, taskbar hover | Live captures show only the lock screen. **Open risk:** a window minimised before the lock may keep a stale DWM thumbnail; `US-0159` checks it and, if so, forces a repaint or an iconic thumbnail. |
| FPS HUD | Part of the workspace: not drawn while locked. |
| OS session lock or sleep | Not hooked (gpui exposes no session-change event); sleep is covered by the wall-clock term, the late-input rule and the one-minute recheck (As built). Follow-up. |
| No vault | No timer, no plugin effect, Lock action shows "Set up a master password first" with a button to Settings > Security. |

### 8. Lock on demand

- Action `oneterm_actions::LockOneTerm`, default binding `ctrl-shift-l` (`cmd-ctrl-l` on macOS
  proposed), listed in the Key Bindings registry
  (`crates/settings-ui/src/key_bindings/key_bindings_actions.rs`) so it is configurable and
  conflict-checked. `DEC-0018` leaves single-Ctrl keys to the terminal; `ctrl-shift-l` is not
  among the current defaults (`n`, `q`, `c`, `v`, `a`, `x`, ...), to be re-checked in the packet.
- Also: a "Lock OneTerm" entry in the app menu and the Settings > Security button.
- The action handler is a global `cx.on_action` in `lock_screen::init`.

## Interfaces

```rust
// oneterm-state, vault.rs (US-0157; the lock screen uses the same API)
pub enum VaultPhase { Uninitialised, Locked, Unlocked, IdleLocked, Unavailable }
pub enum VaultEvent { Initialised, Unlocked, Locked, ScreenLocked, Reset }
impl VaultState {
    pub fn global(cx: &App) -> Entity<Self>;
    pub fn phase(&self) -> VaultPhase;
    pub fn retry_after(&self) -> Option<Duration>;                 // back-off remaining
    pub fn unlock(&mut self, password: SecretString, cx: &mut Context<Self>)
        -> Task<Result<(), UnlockError>>;     // Locked -> Unlocked; in-memory header only (US-0157: no re-read)
    pub fn unlock_screen(&mut self, password: SecretString, cx: &mut Context<Self>)
        -> Task<Result<(), UnlockError>>;     // IdleLocked -> Unlocked; in-memory header only
    pub fn lock_key(&mut self, cx: &mut Context<Self>);             // Unlocked -> Locked
    pub fn lock_screen(&mut self, cx: &mut Context<Self>);          // any initialised phase -> IdleLocked
}

// oneterm-workspace, lock_screen.rs (US-0159)
pub fn init(cx: &mut App);   // IdleClock global, register_plugin::<LockScreen>, interceptor, timer, LockOneTerm handler
struct LockScreen { lock_view: Entity<LockView>, previous_focus: Option<WeakFocusHandle>, _subscriptions: Vec<Subscription> }
impl gpui_base::RootPlugin for LockScreen { fn decorate(..) /* see § 1 */ }
impl Render for LockScreen { /* unlocked: the activity canvas; locked: nothing */ }

// oneterm-settings, ui_config.rs
pub fn set_lock_after_idle_minutes(minutes: u16, cx: &mut App);
```

`app::init` calls `oneterm_workspace::lock_screen::init(cx)` after every feature `init` and
before `open_window`.

## Edge Cases and Failure Modes

- [ ] A key held down across the deadline: auto-repeat stamps the clock; no lock.
- [ ] Terminal flooding output for an hour with no input: locks at the timeout.
- [ ] Lock while a drag (tab drag, split resize, text selection) is in progress: the surface is
  dropped; gpui's drag state is released on the next mouse-up; verified in the packet.
- [ ] Lock while IME composition is active: the composition is abandoned with the terminal's input
  handler (not painted); acceptable.
- [ ] Unlock after the focused tab was closed meanwhile (remote exit): the weak focus handle is
  dead; focus falls to the workspace default.
- [ ] KDF failure (out of memory): an error line, phase unchanged, no count increment.
- [ ] Timeout changed to 1 minute while idle for 5: locks immediately on the restart of the task.
- [ ] Settings window open and focused at lock: both windows lock; unlocking in either unlocks
  both.
- [ ] Vault reset, replaced or re-created by another process (or by hand) while this one is
  `IdleLocked`. A passer-by can start a second OneTerm, reset from its Settings page (allowed
  while `Locked`, intake Q7) and set up a password of their own. **Rule: the lock screen never
  reads `vault.json`.** It verifies against the header `VaultState` holds in memory (read at
  start, set-up, the last successful unlock or change in this process). Deleting or replacing
  the file therefore cannot unlock a locked window; only the password current for the
  in-memory header does. Cost: after a legitimate password change in *another* process, a
  window that was already locked still wants the previous password (rare; documented).

## As built (`US-0159`)

Where the build differs from, or settles, the design above (`US-0159-idle-lock-screen.md`):

- **Phase.** No fifth `VaultPhase`: `VaultPhase` stays `Locked` while the screen is locked and
  `VaultState::is_screen_locked()` carries the second fact (HLD § Lifecycle, as built).
  `lock_screen` drops the key (emits `Locked` if one was held), sets the lock and emits
  `ScreenLocked`; it records `locked_meanwhile` while an operation runs. `unlock_screen` ends
  Unlocked. Set-up, the dialog `unlock`, change and reset are refused while the screen is locked.
  The plugins follow `is_screen_locked()` on every `VaultState` notification, never the
  `Locked` event (a refused seal emits it too).
- **Interceptor rule (§ 6.2).** Computed from the keymap rather than a key list: a keystroke
  passes only when the context stack holds `OneTermLock` and every binding
  `Keymap::bindings_for_input` resolves for it there is `input::*` or `root::*`, with no pending
  chord. Text with no binding passes (input handler, so AltGr and IME work). `ctrl-shift-left`
  (the input's word selection and Split Left) is therefore stopped, which a password field does
  not miss.
- **Late input locks first (§ 2, new).** Every stamp first checks the idle time: input that
  arrives after more than the timeout (the timer was late, the machine slept) locks and is
  stopped (keys in the interceptor, mouse in the capture phase), so the first key or click after
  a resume never reaches the terminal and never re-arms the clock.
- **Timer (§ 3).** It sleeps `min(remaining, 60 s)`: one wake a minute at most, no frame, so a
  deadline passed during a sleep is noticed within a minute of the resume even without input
  (Windows relative timers may not count the time asleep). The task is restarted on every
  `VaultState` and `UiConfig` notification.
- **Timeout.** `lock_after_idle_minutes` is clamped to 1..=480 on read and parsed leniently (a
  non-integer loads as the default 15); the file is not rewritten for it.
- **No-vault notice.** A plain Info notification naming Settings > Security; no button.
- **Lock view text.** The reset explanation is always shown (no disclosure). Since the Q1
  re-ruling it reads: the lock screen cannot reset the master password; quit OneTerm, then
  delete `vault.json` in `<config folder>` (the absolute `config_dir()`, printed in full: a path
  is not a secret, and the debug build's `config_dir()` is relative); OneTerm then starts
  without a lock and every saved password is lost (`DEC-0021` rule 2); saved sessions are kept,
  their passwords typed again. (Before the re-ruling it said that a restart opens OneTerm
  unlocked, where Settings > Security > Reset is; with the startup lock that is no longer
  true.) No eye toggle on the password input.
- **Lock at startup** (acceptance rework 2026-10-13, § 4 "At start"):
  `VaultState::load_at_start`. Two rules came with it, both general:
  - **The lock screen is the only unlock surface while it is up.** `ensure_unlocked` (the
    "unlock first" entry point of `US-0158`) opens nothing while the screen is locked; an Unlock
    dialog under the lock would be refused (`WrongPhase`) and would pop up after the screen
    unlock for a vault already open. No caller fires at startup today (the center always resets
    to a local shell; connect dialogs are user-initiated); the guard makes it structural.
  - **The lock keeps the keyboard.** A dialog opened under the lock (the crash-report dialog the
    app opens at start, a host-key prompt from a connection started before an idle lock) moves
    the focus to a view that is not drawn, where the interceptor stops every key, so typing on
    the lock screen did nothing until the user clicked the input. `LockView` tracks a focus
    handle on its whole view and, on each render, gives the input the focus back when the focus
    is outside it. The dialog itself is hidden, not closed, and is there after the unlock.
  - **Stale envelopes.** Deleting `vault.json` by hand leaves `sealed_password` envelopes in
    `ssh_session.json`. `US-0158` already treats them cleanly with no vault: the connect dialog
    shows the input with "Saved password unavailable: the master password was reset. Type the
    password." and keeps the envelope until a new one replaces it; under a new vault the
    envelope fails to open as `OtherVault` with the same kind of note. No code change; a test
    pins it (`connect_dialog.rs`).
- **Empty Enter** on the lock input is ignored: no Argon2id run, no failure counted (verify I4).
- **Rework after verification** (`evidence/US-0159-verify.md`): the native-dialog and
  interceptor rules in § 7 (D1, D2); the Settings page reset recorded as a known limit (D3);
  § 6.4 corrected (D4).
- **Diagnostics.** At each unlock every window logs `lock screen: N frames in S s` at debug
  level (the lock view renders on every frame its window draws).

Spike (`US-0159` Evidence): the `decorate` replacement keeps focus (restored to the terminal on
unlock) and keyboard input in the lock input (posted `WM_CHAR`); the own `TitleBar` draws its
caption buttons, and closing the window (`WM_CLOSE`) quits. Dragging and the caption buttons
are hit-tested by Windows (`WM_NCHITTEST`) and were not exercised: the walk drives the window
with posted messages only, never the owner's real mouse. Live captures by another process show only the lock.
A hidden terminal's output costs no frame (gpui test; GUI frame counts with and without a
flooding tab). **Not settled:** the DWM thumbnail of a window minimised before the lock (no
capture of it was possible here; a minimised window's DWM thumbnail is the frame from before the
minimise, so the risk stands); the follow-up is `DWMWA_FORCE_ICONIC_REPRESENTATION` while locked,
which needs the raw `HWND`. `Instant` across a real sleep was not walked: the wall-clock term and
the late-input rule cover it by construction and are unit-tested.

## Verification

- [ ] gpui test: with a fake vault in `IdleLocked`, the root's element tree contains the lock
  view and not the application view (a render counter on a test view stays at its pre-lock
  value while the test view notifies).
- [ ] gpui test: every binding in `cx.key_bindings()` dispatched to a locked window triggers no
  action outside `OneTermLock`; typing text and Backspace still edit the lock input.
- [ ] gpui test with a fake clock: input stamps (key via interceptor, mouse move over an open
  dialog, activation) postpone the lock; a session event does not.
- [ ] gpui test: focus restored after unlock; dead handle tolerated.
- [ ] Back-off sequence after 5 failures; reset on success.
- [ ] E2E Windows: main + Settings windows lock after a 1-minute timeout; a screenshot taken by
  another process shows no terminal text; the FPS counter (or a frame counter log) shows no
  frames while a `cat` loop runs in a locked tab; `ctrl-shift-q` does nothing while locked;
  minimised-window taskbar preview checked; sleep longer than the timeout locks on resume;
  elevated window locks with the same password.
