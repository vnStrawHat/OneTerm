//! The idle lock screen (`IN-0050`, `US-0159`; contract:
//! `docs/spec-intakes/IN-0050-master-password/low-level-design/idle-lock-overlay.md`).
//!
//! Once a master password exists, every OneTerm window is covered after
//! `UiConfig::lock_timeout_minutes` without keyboard or mouse input, or at once on
//! [`LockOneTerm`]. The phase lives in `oneterm-state`'s [`VaultState`]
//! (`lock_screen` / `unlock_screen` / `is_screen_locked`); this module owns:
//!
//! - **[`IdleClock`]**, the time of the last user input. Keys are stamped by one
//!   app-wide keystroke interceptor (it runs before any action or raw key handler,
//!   so the keys a terminal consumes count too); mouse moves, clicks and scrolls by
//!   raw window listeners painted from each window's [`LockScreen`] overlay; window
//!   activation by an observer. Terminal output, timers and notifications never
//!   touch it. Idle time is the larger of the monotonic and the wall-clock gap, so
//!   a machine that slept past the timeout locks: input that arrives after more
//!   than the timeout locks *before* it is delivered.
//! - **the deadline timer**, one app task that sleeps until the deadline (at most a
//!   minute at a time, so the wall clock is checked soon after a resume) and is
//!   restarted whenever `VaultState` or `UiConfig` changes. No vault, no timer.
//! - **[`LockScreen`]**, a `gpui_base::RootPlugin` registered after
//!   `gpui_component::init`, so its `decorate` runs last in every window. While
//!   locked it returns the lock view in place of the whole surface: the
//!   application view is never rendered, laid out or painted, so no frame holds a
//!   terminal pixel and a hidden terminal's output asks for no frame.
//! - **input blocking**: while locked the interceptor stops every keystroke except
//!   text and the lock input's own editing keys (see [`allowed_while_locked`]).

use std::cell::Cell;
use std::time::{Duration, Instant, SystemTime};

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, AppContext as _, Context, DispatchPhase, Entity, FocusHandle, Focusable as _,
    Global, InteractiveElement as _, IntoElement, KeyContext, Keymap, Keystroke, MouseDownEvent,
    MouseMoveEvent, ParentElement as _, Render, ScrollWheelEvent, SharedString, Styled as _,
    Subscription, Task, WeakFocusHandle, Window, canvas, div,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, TitleBar, WindowExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    notification::NotificationType,
    v_flex, window_border,
};
use oneterm_actions::LockOneTerm;
use oneterm_core::SecretString;
use oneterm_settings::UiConfig;
use oneterm_state::VaultState;
use oneterm_state::vault_state::{VaultActionError, retry_message};
use oneterm_theme::icon::AppIcon;

/// The lock view's key context: keystrokes are let through only inside it.
const LOCK_CONTEXT: &str = "OneTermLock";

/// The actions a keystroke may resolve to while locked: the password input's own
/// editing and the root's focus navigation, both confined to the lock view.
const LOCK_VIEW_ACTIONS: [&str; 2] = ["input::", "root::"];

/// The longest the deadline timer sleeps, so a deadline that passed while the
/// machine slept is noticed soon after the resume even without input.
const RECHECK: Duration = Duration::from_secs(60);

/// Shown on the lock screen (intake Q7: no reset here; Q1: a restarted OneTerm
/// opens unlocked).
const FORGOT_TEXT: &str = "Forgot it? The lock screen cannot reset the master password. Quit OneTerm and start it again: it opens unlocked, and Settings > Security > Reset deletes the master password and every saved password. Your saved sessions are kept; the sessions open now end when OneTerm quits.";

/// Install the idle clock, the keystroke interceptor, the deadline timer, the
/// [`LockOneTerm`] handler and the [`LockScreen`] plugin. Call from `app::init`
/// after `gpui_component::init` and every feature `init` (so the plugin decorates
/// last) and before the first window opens. Requires the `VaultState` global.
pub fn init(cx: &mut App) {
    cx.set_global(IdleClock::new(IdleClock::now()));
    gpui_base::Root::register_plugin::<LockScreen>(cx, LockScreen::new);
    cx.intercept_keystrokes(|event, _, cx| {
        if !record_input(cx) {
            return;
        }
        let keymap = cx.key_bindings();
        if !allowed_while_locked(&event.keystroke, &event.context_stack, &keymap.borrow()) {
            cx.stop_propagation();
        }
    })
    .detach();
    cx.on_action(lock_now);
    let vault = VaultState::global(cx);
    let config = UiConfig::global(cx);
    let subscriptions = [
        cx.observe(&vault, |_, cx| restart_timer(cx)),
        cx.observe(&config, |_, cx| restart_timer(cx)),
    ];
    cx.set_global(LockTimer {
        task: Task::ready(()),
        _subscriptions: subscriptions,
    });
    restart_timer(cx);
}

// ── Idle clock ──────────────────────────────────────────────────────

/// When the user last pressed a key, moved, clicked or scrolled the mouse, or
/// switched to a OneTerm window. Stamped with a `Cell::set`, so input requests no
/// frame and runs no observer.
struct IdleClock {
    last: Cell<Moment>,
}

impl Global for IdleClock {}

/// A point in time on both clocks.
#[derive(Clone, Copy, Debug)]
struct Moment {
    monotonic: Instant,
    wall: SystemTime,
}

impl IdleClock {
    fn new(at: Moment) -> Self {
        Self {
            last: Cell::new(at),
        }
    }

    fn now() -> Moment {
        Moment {
            monotonic: Instant::now(),
            wall: SystemTime::now(),
        }
    }

    fn stamp(cx: &App) {
        if let Some(clock) = cx.try_global::<Self>() {
            clock.last.set(Self::now());
        }
    }
}

/// How long `now` is after `last`: the larger of the two clocks' gaps, so a
/// sleep the monotonic clock did not count still counts. A wall clock set back
/// counts as no gap; one set forward can lock early (accepted).
fn idle_time(last: Moment, now: Moment) -> Duration {
    let monotonic = now.monotonic.saturating_duration_since(last.monotonic);
    let wall = now.wall.duration_since(last.wall).unwrap_or_default();
    monotonic.max(wall)
}

/// How long the deadline timer may sleep after `idle` of a `timeout`, or `None`
/// when the deadline has passed and the screen locks now.
fn next_check(idle: Duration, timeout: Duration) -> Option<Duration> {
    timeout
        .checked_sub(idle)
        .filter(|remaining| !remaining.is_zero())
        .map(|remaining| remaining.min(RECHECK))
}

fn lock_timeout(cx: &App) -> Duration {
    let minutes = cx
        .try_global::<oneterm_settings::ui_config::UiConfigGlobal>()
        .map(|config| config.0.read(cx).lock_timeout_minutes())
        .unwrap_or(UiConfig::DEFAULT_LOCK_AFTER_IDLE_MINUTES);
    Duration::from_secs(minutes * 60)
}

/// Record user input. Input that comes after more than the timeout without any
/// (the timer was late, or the machine slept) locks first. Returns whether the
/// screen is locked now, in which case the input must not reach the application.
fn record_input(cx: &mut App) -> bool {
    let Some(vault) = VaultState::try_global(cx) else {
        return false;
    };
    let (locked, armed) = {
        let state = vault.read(cx);
        (state.is_screen_locked(), state.can_lock_screen())
    };
    if locked {
        return true;
    }
    let Some(clock) = cx.try_global::<IdleClock>() else {
        return false;
    };
    let now = IdleClock::now();
    let overdue = armed && next_check(idle_time(clock.last.get(), now), lock_timeout(cx)).is_none();
    clock.last.set(now);
    if !overdue {
        return false;
    }
    vault.update(cx, |state, cx| state.lock_screen(cx));
    true
}

// ── Deadline timer ──────────────────────────────────────────────────

struct LockTimer {
    task: Task<()>,
    _subscriptions: [Subscription; 2],
}

impl Global for LockTimer {}

/// Replace the deadline task: one wake per deadline (and per [`RECHECK`]), none
/// while there is no vault or the screen is already locked.
fn restart_timer(cx: &mut App) {
    if !cx.has_global::<LockTimer>() {
        return;
    }
    let task = cx.spawn(async move |cx| {
        loop {
            let wait = cx.update(|cx| {
                let vault = VaultState::try_global(cx)?;
                let state = vault.read(cx);
                if !state.can_lock_screen() || state.is_screen_locked() {
                    return None;
                }
                let idle = cx
                    .try_global::<IdleClock>()
                    .map(|clock| idle_time(clock.last.get(), IdleClock::now()))?;
                Some(next_check(idle, lock_timeout(cx)))
            });
            match wait {
                None => return,
                Some(Some(wait)) => cx.background_executor().timer(wait).await,
                Some(None) => {
                    // Locking notifies `VaultState`, which replaces this task.
                    cx.update(|cx| {
                        if let Some(vault) = VaultState::try_global(cx) {
                            vault.update(cx, |state, cx| state.lock_screen(cx));
                        }
                    });
                    return;
                }
            }
        }
    });
    cx.global_mut::<LockTimer>().task = task;
}

// ── Lock action ─────────────────────────────────────────────────────

/// [`LockOneTerm`]: lock now, or say why there is nothing to lock with.
fn lock_now(_: &LockOneTerm, cx: &mut App) {
    let Some(vault) = VaultState::try_global(cx) else {
        return;
    };
    let state = vault.read(cx);
    if state.can_lock_screen() {
        vault.update(cx, |state, cx| state.lock_screen(cx));
        return;
    }
    let message = match state.unavailable_reason() {
        Some(reason) => format!("OneTerm cannot lock: {reason}"),
        None if state.is_read_only() => format!(
            "No master password is set up. {}",
            oneterm_state::vault_dialogs::READ_ONLY_NOTE
        ),
        None => {
            "Set up a master password in Settings > Security first: the lock screen asks for it."
                .to_owned()
        }
    };
    // The action runs inside the window's dispatch; the notification host is
    // reached from the next effect cycle.
    cx.defer(move |cx| {
        if let Some(window) = cx.active_window() {
            _ = window.update(cx, |_, window, cx| {
                window.push_notification((NotificationType::Info, message), cx);
            });
        }
    });
}

// ── Input blocking ──────────────────────────────────────────────────

/// Whether `keystroke` may proceed while the screen is locked: only inside the
/// lock view, and only when every binding the keymap resolves for it there is
/// the input's own or the root's focus navigation (no pending chord). Text with
/// no binding passes, which keeps AltGr characters and IME input working; a
/// global binding on the same keys (`ctrl-shift-left` is both the input's word
/// selection and Split Left) stops it.
fn allowed_while_locked(
    keystroke: &Keystroke,
    context_stack: &[KeyContext],
    keymap: &Keymap,
) -> bool {
    if !context_stack
        .iter()
        .any(|context| context.contains(LOCK_CONTEXT))
    {
        return false;
    }
    let (bindings, pending) =
        keymap.bindings_for_input(std::slice::from_ref(keystroke), context_stack);
    !pending
        && bindings.iter().all(|binding| {
            let name = binding.action().name();
            LOCK_VIEW_ACTIONS
                .iter()
                .any(|prefix| name.starts_with(prefix))
        })
}

// ── The plugin ──────────────────────────────────────────────────────

/// One per window (every `Root`): replaces the window's surface while the screen
/// is locked and paints the mouse-activity listeners while it is not.
pub struct LockScreen {
    lock_view: Entity<LockView>,
    locked: bool,
    /// The focus to give back on unlock.
    previous_focus: Option<WeakFocusHandle>,
    _subscriptions: Vec<Subscription>,
}

impl LockScreen {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let lock_view = cx.new(|cx| LockView::new(window, cx));
        let mut subscriptions = vec![cx.observe_window_activation(window, |_, _, cx| {
            record_input(cx);
        })];
        if let Some(vault) = VaultState::try_global(cx) {
            subscriptions.push(cx.observe_in(&vault, window, |this, _, window, cx| {
                this.follow_vault(window, cx)
            }));
        }
        let mut this = Self {
            lock_view,
            locked: false,
            previous_focus: None,
            _subscriptions: subscriptions,
        };
        // A window opened while locked is born locked.
        this.follow_vault(window, cx);
        this
    }

    /// Cover or uncover this window when the screen lock changes. Only the
    /// screen-lock fact counts: `VaultEvent::Locked` also comes from a refused seal.
    fn follow_vault(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let locked =
            VaultState::try_global(cx).is_some_and(|vault| vault.read(cx).is_screen_locked());
        if locked == self.locked {
            return;
        }
        self.locked = locked;
        if locked {
            self.previous_focus = window.focused(cx).map(|focus| focus.downgrade());
            gpui_base::TextSelection::clear(window, cx);
            self.lock_view.update(cx, |view, cx| view.reset(window, cx));
            let focus = self.lock_view.read(cx).input_focus(cx);
            // A `WindowExt` call inside a `Root` update panics (`US-0149`): defer.
            window.defer(cx, move |window, cx| focus.focus(window, cx));
        } else {
            IdleClock::stamp(cx);
            // A hidden terminal's output must cost no frame: what the lock view
            // drew is its input's cursor blink and its own changes, nothing more.
            let view = self.lock_view.read(cx);
            log::debug!(
                "lock screen: {} frames in {:.1} s",
                view.frames,
                view.since.elapsed().as_secs_f32()
            );
            if let Some(focus) = self.previous_focus.take().and_then(|focus| focus.upgrade()) {
                window.defer(cx, move |window, cx| focus.focus(window, cx));
            }
        }
        cx.notify();
    }
}

impl Render for LockScreen {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        // Raw window listeners see every mouse event in both phases, whatever is
        // on top (an open dialog included); they are re-registered each frame
        // the root paints. Capture phase, so input that locks (it came after the
        // deadline) is stopped before an element under the pointer sees it.
        canvas(
            |_, _, _| {},
            |_, _, window, _| {
                window.on_mouse_event(|_: &MouseMoveEvent, phase, _, cx| on_mouse(phase, cx));
                window.on_mouse_event(|_: &MouseDownEvent, phase, _, cx| on_mouse(phase, cx));
                window.on_mouse_event(|_: &ScrollWheelEvent, phase, _, cx| on_mouse(phase, cx));
            },
        )
        .absolute()
        .size_full()
    }
}

fn on_mouse(phase: DispatchPhase, cx: &mut App) {
    if phase == DispatchPhase::Capture && record_input(cx) {
        cx.stop_propagation();
    }
}

impl gpui_base::RootPlugin for LockScreen {
    fn decorate(
        &self,
        surface: AnyElement,
        _root: &gpui_base::Root,
        _window: &mut Window,
        _cx: &mut App,
    ) -> impl IntoElement {
        if !self.locked {
            return surface;
        }
        // The surface (the application view, its dialogs, toasts and the
        // component layers) is dropped unbuilt. That also drops the border
        // `WindowState` drew, so the lock view gets its own.
        window_border()
            .child(self.lock_view.clone())
            .into_any_element()
    }
}

// ── The lock view ───────────────────────────────────────────────────

/// What a locked window shows: its own title bar, the password input, the
/// error or back-off line, Unlock, how to reset, and Quit. Nothing about the
/// sessions underneath.
struct LockView {
    input: Entity<InputState>,
    error: Option<SharedString>,
    busy: bool,
    /// A countdown task is repainting the back-off line.
    ticking: bool,
    /// Frames drawn since the lock (logged at unlock).
    frames: u64,
    since: Instant,
    _subscriptions: Vec<Subscription>,
}

impl LockView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).masked(true));
        let mut subscriptions = vec![cx.subscribe_in(
            &input,
            window,
            |view: &mut Self, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => view.submit(window, cx),
                InputEvent::Change => {
                    if view.error.take().is_some() {
                        cx.notify();
                    }
                }
                _ => {}
            },
        )];
        if let Some(vault) = VaultState::try_global(cx) {
            subscriptions.push(cx.observe(&vault, |_, _, cx| cx.notify()));
        }
        Self {
            input,
            error: None,
            busy: false,
            ticking: false,
            frames: 0,
            since: Instant::now(),
            _subscriptions: subscriptions,
        }
    }

    fn input_focus(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus_handle(cx)
    }

    /// Start a lock with an empty input and no message.
    fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.error = None;
        self.frames = 0;
        self.since = Instant::now();
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
    }

    fn retry_after(&self, cx: &App) -> Option<Duration> {
        VaultState::try_global(cx).and_then(|state| state.read(cx).retry_after())
    }

    fn can_submit(&self, cx: &App) -> bool {
        !self.busy && self.retry_after(cx).is_none()
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // An empty Enter is not an attempt: it would cost an Argon2id run and
        // count toward the back-off for nothing (`US-0159` verify I4).
        if !self.can_submit(cx) || self.input.read(cx).text().len_chars() == 0 {
            return;
        }
        let Some(state) = VaultState::try_global(cx) else {
            return;
        };
        // One copy, straight from the rope into a zeroizing string; the input is
        // cleared whatever the answer (`IN-0050` Security Analysis 4).
        let password = SecretString::new(self.input.read(cx).text().to_string());
        let task = state.update(cx, |state, cx| state.unlock_screen(password, cx));
        self.busy = true;
        self.error = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            _ = this.update_in(cx, |view, window, cx| view.finish(result, window, cx));
        })
        .detach();
    }

    fn finish(
        &mut self,
        result: Result<(), VaultActionError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.busy = false;
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.error = match result {
            Ok(()) => None,
            // The back-off line already says when to try again.
            Err(VaultActionError::RetryIn(_)) => None,
            Err(error) => Some(error.message().into()),
        };
        cx.notify();
    }

    /// Repaint the back-off countdown until it runs out.
    fn start_countdown(&mut self, cx: &mut Context<Self>) {
        if self.ticking {
            return;
        }
        self.ticking = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                let waiting = this.update(cx, |view, cx| {
                    cx.notify();
                    let waiting = view.retry_after(cx).is_some();
                    view.ticking = waiting;
                    waiting
                });
                if !matches!(waiting, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }
}

impl Render for LockView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.frames += 1;
        let wait = self.retry_after(cx);
        if wait.is_some() {
            self.start_countdown(cx);
        }
        let can_submit = self.can_submit(cx);
        let theme = cx.theme();
        let (muted, danger) = (theme.muted_foreground, theme.danger);
        let (name, suffix) =
            oneterm_core::elevation::window_title_parts(oneterm_core::elevation::elevation());
        let view = cx.entity();
        v_flex()
            .id("lock-screen")
            .key_context(LOCK_CONTEXT)
            .size_full()
            .font_family(theme.font_family.clone())
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(
                TitleBar::new().child(
                    h_flex()
                        .gap_1()
                        .text_sm()
                        .child(name)
                        .when_some(suffix, |this, suffix| this.child(suffix)),
                ),
            )
            .child(
                v_flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .px_4()
                    .child(
                        v_flex()
                            .w_full()
                            .max_w(gpui::px(420.))
                            .gap_3()
                            .child(
                                v_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(Icon::new(AppIcon::Lock).size_8().text_color(muted))
                                    .child(div().text_xl().child("OneTerm is locked")),
                            )
                            .child(div().text_sm().text_color(muted).child("Master password"))
                            .child(Input::new(&self.input).disabled(self.busy))
                            .when_some(self.error.clone(), |this, error| {
                                this.child(div().text_sm().text_color(danger).child(error))
                            })
                            .when_some(wait, |this, wait| {
                                this.child(
                                    div()
                                        .text_sm()
                                        .text_color(danger)
                                        .child(retry_message(wait)),
                                )
                            })
                            .child(
                                h_flex().justify_end().child(
                                    Button::new("lock-screen-unlock")
                                        .primary()
                                        .label("Unlock")
                                        // The kit draws its spinner in the icon slot only.
                                        .when(self.busy, |button| {
                                            button.icon(IconName::LoaderCircle)
                                        })
                                        .loading(self.busy)
                                        .disabled(!can_submit)
                                        .on_click(move |_, window, cx| {
                                            view.update(cx, |view, cx| view.submit(window, cx));
                                        }),
                                ),
                            )
                            .child(div().pt_4().text_xs().text_color(muted).child(FORGOT_TEXT))
                            .child(
                                h_flex().justify_end().child(
                                    Button::new("lock-screen-quit")
                                        .outline()
                                        .label("Quit OneTerm")
                                        .on_click(|_, _, cx| cx.quit()),
                                ),
                            ),
                    ),
            )
    }
}

#[cfg(test)]
#[path = "lock_screen_tests.rs"]
mod lock_screen_tests;
