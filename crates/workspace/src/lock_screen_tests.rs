use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use gpui::{
    AppContext as _, AsKeystroke as _, Entity, FocusHandle, Focusable, InteractiveElement as _,
    IntoElement, KeyBinding, ParentElement as _, Render, Styled as _, TestAppContext,
    VisualTestContext, Window, actions, div,
};
use gpui_component::Root;
use oneterm_settings::ui_config::UiConfigGlobal;
use oneterm_state::vault_state::VaultStateGlobal;

use super::*;

/// A vault header at the floor parameters (`crates/vault`'s fixture).
const V1: &str = include_str!("../../vault/tests/fixtures/persistence/vault-v1.json");
const V1_PASSWORD: &str = "correct horse battery staple";

actions!(lock_screen_test, [TestQuit, TestSplit]);

fn moment(monotonic: Instant, wall: SystemTime) -> Moment {
    Moment { monotonic, wall }
}

#[test]
fn idle_time_is_the_larger_of_the_two_clocks() {
    let start = moment(Instant::now(), SystemTime::now());
    let later = |monotonic: u64, wall: i64| {
        let wall = if wall >= 0 {
            start.wall + Duration::from_secs(wall as u64)
        } else {
            start.wall - Duration::from_secs(wall.unsigned_abs())
        };
        moment(start.monotonic + Duration::from_secs(monotonic), wall)
    };
    assert_eq!(idle_time(start, later(5, 5)), Duration::from_secs(5));
    // The machine slept: the monotonic clock did not count it, the wall clock did.
    assert_eq!(idle_time(start, later(5, 7200)), Duration::from_secs(7200));
    // The wall clock was set back: the monotonic gap stands.
    assert_eq!(idle_time(start, later(30, -600)), Duration::from_secs(30));
}

#[test]
fn the_timer_sleeps_to_the_deadline_a_minute_at_most_and_then_locks() {
    let fifteen = Duration::from_secs(15 * 60);
    assert_eq!(next_check(Duration::ZERO, fifteen), Some(RECHECK));
    assert_eq!(
        next_check(fifteen - Duration::from_secs(10), fifteen),
        Some(Duration::from_secs(10))
    );
    assert_eq!(next_check(fifteen, fifteen), None);
    assert_eq!(next_check(fifteen * 8, fifteen), None);
}

// ── gpui tests ──────────────────────────────────────────────────────

/// Removes the directory on drop, including when an assertion panics.
struct TemporaryDirectory(PathBuf);

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        // Best effort: nothing in a destructor can report a failed cleanup.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The application view a locked window must not render.
struct Content {
    focus: FocusHandle,
    renders: Rc<Cell<usize>>,
}

impl Focusable for Content {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Content {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div()
            .key_context("TestContent")
            .track_focus(&self.focus)
            .size_full()
            .child("terminal text")
    }
}

struct Fixture<'a> {
    cx: &'a mut VisualTestContext,
    content: Entity<Content>,
    renders: Rc<Cell<usize>>,
    vault: Entity<VaultState>,
    quits: Rc<Cell<usize>>,
    _dir: TemporaryDirectory,
}

/// A window whose `Root` carries the lock plugin, over a vault in `Locked` read
/// from the floor-parameter fixture, with a few global bindings shaped like
/// OneTerm's (one shares its keys with the input's word selection).
fn fixture(cx: &mut TestAppContext) -> Fixture<'_> {
    build_fixture(cx, false)
}

/// [`fixture`] as OneTerm starts (`US-0159` rework): the vault is read with
/// `load_at_start`, and the application view focuses itself while it is built,
/// as the workspace focuses its first terminal before `Root` builds the plugins.
fn startup_fixture(cx: &mut TestAppContext) -> Fixture<'_> {
    build_fixture(cx, true)
}

fn build_fixture(cx: &mut TestAppContext, at_start: bool) -> Fixture<'_> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = TemporaryDirectory(std::env::temp_dir().join(format!(
        "oneterm-lock-screen-{}-{nonce}",
        std::process::id()
    )));
    std::fs::create_dir_all(&dir.0).unwrap();
    std::fs::write(dir.0.join("vault.json"), V1).unwrap();
    let quits = Rc::new(Cell::new(0));
    let vault = cx.update(|cx| {
        gpui_component::init(cx);
        // `persist_blocked` keeps the test from writing a real `ui_config.json`.
        let config = cx.new(|_| UiConfig {
            persist_blocked: true,
            ..UiConfig::default()
        });
        cx.set_global(UiConfigGlobal(config));
        let vault = cx.new(|_| {
            if at_start {
                VaultState::load_at_start(dir.0.clone(), false)
            } else {
                VaultState::load(dir.0.clone(), false)
            }
        });
        cx.set_global(VaultStateGlobal(vault.clone()));
        init(cx);
        cx.bind_keys([
            KeyBinding::new("ctrl-shift-q", TestQuit, None),
            KeyBinding::new("ctrl-shift-left", TestSplit, None),
            KeyBinding::new("ctrl-shift-l", LockOneTerm, None),
            // A global binding on a bare character still loses to the lock.
            KeyBinding::new("z", TestQuit, None),
        ]);
        let counter = quits.clone();
        cx.on_action(move |_: &TestQuit, _| counter.set(counter.get() + 1));
        let counter = quits.clone();
        cx.on_action(move |_: &TestSplit, _| counter.set(counter.get() + 1));
        vault
    });
    let renders = Rc::new(Cell::new(0));
    let counted = renders.clone();
    let mut content = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| Content {
            focus: cx.focus_handle(),
            renders: counted,
        });
        content = Some(view.clone());
        if at_start {
            let focus = view.read(cx).focus.clone();
            window.focus(&focus, cx);
        }
        Root::new(view, window, cx)
    });
    let content = content.unwrap();
    cx.run_until_parked();
    Fixture {
        cx,
        content,
        renders,
        vault,
        quits,
        _dir: dir,
    }
}

fn lock_view(cx: &mut VisualTestContext) -> Entity<LockView> {
    cx.update(|window, cx| {
        let root = Root::read(window, cx);
        root.plugin::<LockScreen>()
            .unwrap()
            .read(cx)
            .lock_view
            .clone()
    })
}

fn typed(cx: &mut VisualTestContext) -> String {
    let view = lock_view(cx);
    cx.update(|_, cx| view.read(cx).input.read(cx).text().to_string())
}

#[gpui::test]
async fn a_locked_window_does_not_render_the_application(cx: &mut TestAppContext) {
    let f = fixture(cx);
    let cx = f.cx;
    assert!(f.renders.get() > 0);
    f.vault.update(cx, |state, cx| state.lock_screen(cx));
    cx.run_until_parked();
    let before = f.renders.get();
    // The application keeps notifying (terminal output); nothing is rendered.
    for _ in 0..5 {
        f.content.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
    }
    assert_eq!(f.renders.get(), before, "a hidden view must cost no frame");
    // Unlocked again, it renders on the next frame.
    f.vault
        .update(cx, |state, cx| {
            state.unlock_screen(SecretString::new(V1_PASSWORD), cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();
    assert!(f.renders.get() > before);
}

#[gpui::test]
async fn while_locked_no_binding_fires_and_only_the_input_takes_keys(cx: &mut TestAppContext) {
    let f = fixture(cx);
    let cx = f.cx;
    f.vault.update(cx, |state, cx| state.lock_screen(cx));
    cx.run_until_parked();

    // Every binding in the keymap, at the lock input: stopped unless all it
    // resolves to is the input's own editing or the root's focus navigation.
    let view = lock_view(cx);
    let focus = cx.update(|_, cx| view.read(cx).input_focus(cx));
    let stack = cx.update(|window, _| {
        assert!(focus.is_focused(window), "the lock input has focus");
        window.context_stack()
    });
    let keymap = cx.update(|_, cx| cx.key_bindings());
    let keymap = keymap.borrow();
    let mut checked = 0;
    for binding in keymap.bindings() {
        let Some(first) = binding.keystrokes().first() else {
            continue;
        };
        let keystroke = first.as_keystroke().clone();
        checked += 1;
        if allowed_while_locked(&keystroke, &stack, &keymap) {
            let (resolved, _) = keymap.bindings_for_input(std::slice::from_ref(&keystroke), &stack);
            for binding in resolved {
                let name = binding.action().name();
                assert!(
                    name.starts_with("input::") || name.starts_with("root::"),
                    "{keystroke} reaches {name} while locked"
                );
            }
        }
    }
    assert!(
        checked > 50,
        "the kit's and the test's bindings were enumerated"
    );
    drop(keymap);

    // And live: the global bindings do nothing, text reaches the input.
    cx.simulate_keystrokes("ctrl-shift-q ctrl-shift-left z");
    assert_eq!(f.quits.get(), 0);
    cx.simulate_input("abc");
    cx.simulate_keystrokes("backspace");
    assert_eq!(typed(cx), "ab");
    // Outside the lock view (no focus at all) nothing passes, text included.
    let keymap = cx.update(|_, cx| cx.key_bindings());
    for key in ["q", "ctrl-shift-q", "backspace"] {
        let keystroke = Keystroke::parse(key).unwrap();
        assert!(!allowed_while_locked(&keystroke, &[], &keymap.borrow()));
    }
    // And no focus does not last (`US-0159` rework): the next frame gives
    // the lock input its keyboard back, so typing goes on there.
    cx.update(|window, cx| window.blur(cx));
    cx.run_until_parked();
    cx.update(|window, _| assert!(focus.is_focused(window), "the lock took no focus back"));
    cx.simulate_input("q");
    cx.simulate_keystrokes("ctrl-shift-q");
    assert_eq!(f.quits.get(), 0);
    assert_eq!(typed(cx), "abq");
    let _ = &f.content;
}

#[gpui::test]
async fn unlocking_restores_the_focus_and_counts_a_wrong_password(cx: &mut TestAppContext) {
    let f = fixture(cx);
    let cx = f.cx;
    let content_focus = cx.update(|window, cx| {
        let focus = f.content.read(cx).focus.clone();
        window.focus(&focus, cx);
        focus
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("ctrl-shift-l");
    assert!(f.vault.read_with(cx, |state, _| state.is_screen_locked()));
    cx.run_until_parked();
    cx.update(|window, _| assert!(!content_focus.is_focused(window)));

    // An empty Enter is not an attempt (verify I4): no error, nothing running.
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let view = lock_view(cx);
    cx.update(|_, cx| {
        assert_eq!(view.read(cx).error, None);
        assert!(!view.read(cx).busy);
    });

    cx.simulate_input("wrong");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(f.vault.read_with(cx, |state, _| state.is_screen_locked()));
    assert_eq!(typed(cx), "", "a rejected password leaves the input");
    let view = lock_view(cx);
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).error.as_deref(),
            Some("Wrong master password.")
        )
    });

    cx.simulate_input(V1_PASSWORD);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(!f.vault.read_with(cx, |state, _| state.is_screen_locked()));
    cx.update(|window, _| assert!(content_focus.is_focused(window)));
    assert_eq!(typed(cx), "");
}

/// Input that arrives after more than the timeout (the machine slept, or the
/// timer was late) locks before it is delivered; input in time does not.
#[gpui::test]
async fn late_input_locks_first_and_on_time_input_postpones(cx: &mut TestAppContext) {
    let f = fixture(cx);
    let cx = f.cx;
    let set_idle = |idle: Duration, cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            let clock = cx.global::<IdleClock>();
            let now = IdleClock::now();
            clock.last.set(moment(now.monotonic, now.wall - idle));
        })
    };
    let fourteen = Duration::from_secs(14 * 60);
    set_idle(fourteen, cx);
    cx.simulate_keystrokes("x");
    assert!(!f.vault.read_with(cx, |state, _| state.is_screen_locked()));
    // The key stamped the clock: idle is back near zero.
    cx.update(|_, cx| {
        let idle = idle_time(cx.global::<IdleClock>().last.get(), IdleClock::now());
        assert!(idle < Duration::from_secs(60));
    });

    // A session event (any entity notifying) is not input.
    set_idle(fourteen, cx);
    f.content.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    cx.update(|_, cx| {
        let idle = idle_time(cx.global::<IdleClock>().last.get(), IdleClock::now());
        assert!(idle >= fourteen);
    });

    set_idle(Duration::from_secs(16 * 60), cx);
    cx.simulate_keystrokes("ctrl-shift-q");
    assert!(f.vault.read_with(cx, |state, _| state.is_screen_locked()));
    assert_eq!(f.quits.get(), 0, "the key that locked was not delivered");
}

#[gpui::test]
async fn the_timer_locks_an_idle_window_and_never_without_a_vault(cx: &mut TestAppContext) {
    let f = fixture(cx);
    let cx = f.cx;
    cx.update(|_, cx| {
        let clock = cx.global::<IdleClock>();
        let now = IdleClock::now();
        clock.last.set(moment(
            now.monotonic,
            now.wall - Duration::from_secs(20 * 60),
        ));
        restart_timer(cx);
    });
    cx.run_until_parked();
    assert!(f.vault.read_with(cx, |state, _| state.is_screen_locked()));
    let _ = (f.content, f.renders, f.quits);
}

#[gpui::test]
fn without_a_vault_nothing_locks(cx: &mut TestAppContext) {
    let f = fixture(cx);
    let cx = f.cx;
    let empty = TemporaryDirectory(f._dir.0.join("empty"));
    std::fs::create_dir_all(&empty.0).unwrap();
    let vault = cx.update(|_, cx| {
        let vault = cx.new(|_| VaultState::load(empty.0.clone(), false));
        cx.set_global(VaultStateGlobal(vault.clone()));
        vault
    });
    cx.simulate_keystrokes("ctrl-shift-l");
    assert!(!vault.read_with(cx, |state, _| state.is_screen_locked()));
    cx.update(|_, cx| {
        let clock = cx.global::<IdleClock>();
        let now = IdleClock::now();
        clock.last.set(moment(
            now.monotonic,
            now.wall - Duration::from_secs(600 * 60),
        ));
    });
    cx.simulate_keystrokes("x");
    assert!(!vault.read_with(cx, |state, _| state.is_screen_locked()));
}

// ── Independent verification (`evidence/US-0159-verify.md`) ─────────

/// A terminal-shaped view: raw key and mouse listeners, the way
/// `TerminalView` takes typing and clicks (no actions).
struct Probe {
    focus: FocusHandle,
    captured_keys: Rc<Cell<usize>>,
    keys: Rc<Cell<usize>>,
    mouse_downs: Rc<Cell<usize>>,
}

impl Focusable for Probe {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        let (captured, keys, downs) = (
            self.captured_keys.clone(),
            self.keys.clone(),
            self.mouse_downs.clone(),
        );
        div()
            .key_context("TestTerminal")
            .track_focus(&self.focus)
            .size_full()
            // First listener on the node: gpui runs it even after an
            // interceptor stopped the keystroke (`finish_dispatch_key_event`).
            .capture_key_down(move |_, _, _| captured.set(captured.get() + 1))
            .on_key_down(move |_, _, _| keys.set(keys.get() + 1))
            .on_mouse_down(gpui::MouseButton::Left, move |_, _, _| {
                downs.set(downs.get() + 1)
            })
            .child("terminal text")
    }
}

struct ProbeCounters {
    captured_keys: Rc<Cell<usize>>,
    keys: Rc<Cell<usize>>,
    mouse_downs: Rc<Cell<usize>>,
}

/// `fixture`'s window with its `Root` rebuilt around a focused [`Probe`].
fn probe_fixture(f: &mut Fixture<'_>) -> ProbeCounters {
    let counters = ProbeCounters {
        captured_keys: Rc::new(Cell::new(0)),
        keys: Rc::new(Cell::new(0)),
        mouse_downs: Rc::new(Cell::new(0)),
    };
    f.cx.update(|window, cx| {
        let probe = cx.new(|cx| Probe {
            focus: cx.focus_handle(),
            captured_keys: counters.captured_keys.clone(),
            keys: counters.keys.clone(),
            mouse_downs: counters.mouse_downs.clone(),
        });
        let root = window.root::<Root>().flatten().unwrap();
        let view = probe.clone();
        root.update(cx, |root, cx| *root = Root::new(view, window, cx));
        let focus = probe.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    f.cx.run_until_parked();
    f.cx.update(|window, _| window.refresh());
    f.cx.run_until_parked();
    counters
}

fn idle_for(idle: Duration, cx: &mut VisualTestContext) {
    cx.update(|_, cx| {
        let clock = cx.global::<IdleClock>();
        let now = IdleClock::now();
        clock.last.set(moment(now.monotonic, now.wall - idle));
    })
}

fn is_locked(vault: &Entity<VaultState>, cx: &mut VisualTestContext) -> bool {
    vault.read_with(cx, |state, _| state.is_screen_locked())
}

/// Attack 2(a)/(e): the key that arrives after the timeout locks and must not
/// reach a terminal's raw key listener; once locked nothing reaches it.
#[gpui::test]
async fn verifier_a_late_key_reaches_no_terminal_key_listener(cx: &mut TestAppContext) {
    let mut f = fixture(cx);
    let probe = probe_fixture(&mut f);
    let cx = f.cx;
    cx.simulate_keystrokes("q");
    assert_eq!(probe.keys.get(), 1, "the probe takes keys while unlocked");
    let captured_before = probe.captured_keys.get();

    idle_for(Duration::from_secs(16 * 60), cx);
    cx.simulate_keystrokes("x");
    assert!(is_locked(&f.vault, cx));
    assert_eq!(
        probe.keys.get(),
        1,
        "the key that locked reached the terminal"
    );
    // gpui quirk: the stopped keystroke is still offered to the first
    // capture-phase key listener on the old focus path. `TerminalView`
    // registers none (`on_key_down` only), so a terminal never sees it.
    let quirk = probe.captured_keys.get() - captured_before;

    cx.simulate_keystrokes("y ctrl-v enter");
    cx.simulate_input("typed");
    assert_eq!(
        probe.keys.get(),
        1,
        "a key reached the terminal while locked"
    );
    assert_eq!(
        probe.captured_keys.get() - captured_before,
        quirk,
        "a capture listener of the hidden view ran while locked"
    );
    assert_eq!(quirk, 1, "the gpui quirk this test documents changed");
    let _ = (f.content, f.renders, f.quits);
}

/// Attack 2(b)/(e): the click that arrives after the timeout locks in the
/// capture phase and reaches no element; once locked the application view is
/// not hit-tested at all.
#[gpui::test]
async fn verifier_a_late_click_reaches_no_terminal_mouse_listener(cx: &mut TestAppContext) {
    let mut f = fixture(cx);
    let probe = probe_fixture(&mut f);
    let cx = f.cx;
    let at = gpui::point(gpui::px(200.), gpui::px(200.));
    let none = gpui::Modifiers::none();
    cx.simulate_mouse_down(at, gpui::MouseButton::Left, none);
    cx.simulate_mouse_up(at, gpui::MouseButton::Left, none);
    assert_eq!(
        probe.mouse_downs.get(),
        1,
        "the probe takes clicks unlocked"
    );

    idle_for(Duration::from_secs(16 * 60), cx);
    cx.simulate_mouse_down(at, gpui::MouseButton::Left, none);
    assert!(is_locked(&f.vault, cx));
    assert_eq!(
        probe.mouse_downs.get(),
        1,
        "the click that locked was delivered"
    );
    cx.simulate_mouse_up(at, gpui::MouseButton::Left, none);
    cx.simulate_click(at, none);
    assert_eq!(
        probe.mouse_downs.get(),
        1,
        "a click was delivered while locked"
    );
    let _ = (f.content, f.renders, f.quits);
}

/// Attack: the lock view's input lives across locks. Its undo history must not
/// hand the last right password to the next person at the keyboard.
#[gpui::test]
async fn verifier_undo_in_the_lock_input_never_returns_a_password(cx: &mut TestAppContext) {
    let f = fixture(cx);
    let cx = f.cx;
    cx.simulate_keystrokes("ctrl-shift-l");
    cx.run_until_parked();
    cx.simulate_input(V1_PASSWORD);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(!is_locked(&f.vault, cx));

    cx.update(|window, cx| {
        let focus = f.content.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("ctrl-shift-l");
    cx.run_until_parked();
    assert!(is_locked(&f.vault, cx));
    cx.simulate_keystrokes("ctrl-z ctrl-z ctrl-shift-z ctrl-y");
    assert_eq!(typed(cx), "", "undo brought text back into the lock input");
    cx.simulate_input("wrong");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.simulate_keystrokes("ctrl-z ctrl-z");
    assert_eq!(typed(cx), "");
    let _ = (f.renders, f.quits);
}

/// Attack 2(c): the timer replaces its own task from inside `cx.update`
/// (lock -> notify -> `restart_timer`). After an unlock it must be armed
/// again, not lost.
#[gpui::test]
async fn verifier_after_an_unlock_the_timer_locks_again(cx: &mut TestAppContext) {
    let f = fixture(cx);
    let cx = f.cx;
    idle_for(Duration::from_secs(20 * 60), cx);
    cx.update(|_, cx| restart_timer(cx));
    cx.run_until_parked();
    assert!(is_locked(&f.vault, cx));
    for _ in 0..2 {
        f.vault
            .update(cx, |state, cx| {
                state.unlock_screen(SecretString::new(V1_PASSWORD), cx)
            })
            .await
            .unwrap();
        cx.run_until_parked();
        assert!(!is_locked(&f.vault, cx));
        // No restart by hand: only the task the unlock re-armed can lock.
        idle_for(Duration::from_secs(20 * 60), cx);
        cx.executor()
            .advance_clock(RECHECK + Duration::from_secs(1));
        cx.run_until_parked();
        assert!(
            is_locked(&f.vault, cx),
            "the timer was lost after the unlock"
        );
    }
    let _ = (f.content, f.renders, f.quits);
}

/// Defect D1 (Low), fixed: gpui runs every keystroke interceptor whatever an
/// earlier one did, so a later one (the Key Bindings recorder) still sees the
/// key that locked. The lock's interceptor is installed first and has locked by
/// then, so an interceptor that asks `VaultState::screen_locked` first (the
/// rule; the recorder's own test is in `oneterm-settings-ui`) takes nothing.
#[gpui::test]
async fn verifier_d1_a_later_interceptor_still_sees_the_key_that_locked(cx: &mut TestAppContext) {
    let f = fixture(cx);
    let cx = f.cx;
    let seen = Rc::new(Cell::new(0));
    let counter = seen.clone();
    let _recorder = cx.update(|_, cx| {
        cx.intercept_keystrokes(move |_, _, cx| {
            if VaultState::screen_locked(cx) {
                return;
            }
            counter.set(counter.get() + 1);
            cx.stop_propagation();
        })
    });
    idle_for(Duration::from_secs(16 * 60), cx);
    cx.simulate_keystrokes("k");
    assert!(is_locked(&f.vault, cx));
    assert_eq!(seen.get(), 0, "D1: the later interceptor took the late key");
    let _ = (f.content, f.renders, f.quits);
}

/// Attack 2(a): a global chord (`ctrl-k ctrl-q`) never completes while locked,
/// neither from the lock input nor from a prefix typed before the lock; paste
/// into the password field works (password managers).
#[gpui::test]
async fn verifier_chords_never_complete_and_paste_reaches_only_the_input(cx: &mut TestAppContext) {
    let f = fixture(cx);
    let cx = f.cx;
    cx.update(|_, cx| cx.bind_keys([KeyBinding::new("ctrl-k ctrl-q", TestQuit, None)]));
    cx.update(|window, cx| {
        let focus = f.content.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    cx.run_until_parked();
    // A prefix pending in the terminal when the lock comes.
    cx.simulate_keystrokes("ctrl-k");
    f.vault.update(cx, |state, cx| state.lock_screen(cx));
    cx.run_until_parked();
    cx.simulate_keystrokes("ctrl-q");
    cx.simulate_keystrokes("ctrl-k ctrl-q");
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert_eq!(f.quits.get(), 0, "a chord completed while locked");

    cx.update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("pasted".into())));
    cx.simulate_keystrokes("ctrl-v");
    assert_eq!(typed(cx), "pasted");
    let _ = f.renders;
}

// ── Lock at startup (`US-0159` acceptance rework, Q1 re-ruled 2026-10-13) ──

/// A usable vault at start: the window is born locked, the application view is
/// never rendered before the first unlock, the lock input has the keyboard,
/// and the right password brings the application with the focus it gave
/// itself while it was built.
#[gpui::test]
async fn a_vault_at_start_locks_the_window_before_its_first_frame(cx: &mut TestAppContext) {
    let f = startup_fixture(cx);
    let cx = f.cx;
    assert!(is_locked(&f.vault, cx));
    assert_eq!(f.renders.get(), 0, "the application was rendered at start");
    let view = lock_view(cx);
    let focus = cx.update(|_, cx| view.read(cx).input_focus(cx));
    cx.update(|window, _| assert!(focus.is_focused(window), "the lock input has focus"));
    cx.simulate_keystrokes("ctrl-shift-q z");
    assert_eq!(f.quits.get(), 0);
    assert_eq!(typed(cx), "", "a bound key reached the input");

    cx.simulate_input("wrong");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(is_locked(&f.vault, cx));
    assert_eq!(f.renders.get(), 0);

    cx.simulate_input(V1_PASSWORD);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(!is_locked(&f.vault, cx));
    assert!(f.renders.get() > 0);
    let content_focus = cx.update(|_, cx| f.content.read(cx).focus.clone());
    cx.update(|window, _| assert!(content_focus.is_focused(window)));
}

/// No timer while the startup lock is up; the unlock starts the idle clock, so
/// a lock screen left up longer than the timeout does not lock again at once,
/// and the timer it arms locks after the timeout.
#[gpui::test]
async fn the_idle_clock_starts_at_the_first_unlock(cx: &mut TestAppContext) {
    let f = startup_fixture(cx);
    let cx = f.cx;
    idle_for(Duration::from_secs(20 * 60), cx);
    f.vault
        .update(cx, |state, cx| {
            state.unlock_screen(SecretString::new(V1_PASSWORD), cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();
    assert!(!is_locked(&f.vault, cx), "locked again at once");
    cx.update(|_, cx| {
        let idle = idle_time(cx.global::<IdleClock>().last.get(), IdleClock::now());
        assert!(idle < Duration::from_secs(60), "the unlock did not stamp");
    });
    idle_for(Duration::from_secs(20 * 60), cx);
    cx.executor()
        .advance_clock(RECHECK + Duration::from_secs(1));
    cx.run_until_parked();
    assert!(is_locked(&f.vault, cx), "no timer after the startup unlock");
    let _ = (f.content, f.quits);
}

/// The lock screen is the only unlock surface while it is up, and a dialog
/// opened under it (a crash report at start) does not take the keyboard away
/// from the lock input.
#[gpui::test]
async fn nothing_under_the_startup_lock_takes_the_keyboard(cx: &mut TestAppContext) {
    let f = startup_fixture(cx);
    let cx = f.cx;
    let ready = Rc::new(Cell::new(0));
    let counter = ready.clone();
    cx.update(|window, cx| {
        oneterm_state::vault_dialogs::ensure_unlocked(window, cx, move |_, _| {
            counter.set(counter.get() + 1)
        });
    });
    cx.run_until_parked();
    assert_eq!(ready.get(), 0);
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "an Unlock dialog was stacked under the lock screen"
    );

    cx.update(|window, cx| window.open_dialog(cx, |dialog, _, _| dialog));
    cx.run_until_parked();
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    let view = lock_view(cx);
    let focus = cx.update(|_, cx| view.read(cx).input_focus(cx));
    cx.update(|window, _| assert!(focus.is_focused(window), "the dialog kept the focus"));
    cx.simulate_input("abc");
    assert_eq!(typed(cx), "abc");

    // Unlocked, `ensure_unlocked` runs its continuation at once.
    f.vault
        .update(cx, |state, cx| {
            state.unlock_screen(SecretString::new(V1_PASSWORD), cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();
    let counter = ready.clone();
    cx.update(|window, cx| {
        oneterm_state::vault_dialogs::ensure_unlocked(window, cx, move |_, _| {
            counter.set(counter.get() + 1)
        });
    });
    assert_eq!(ready.get(), 1);
    let _ = (f.content, f.renders, f.quits);
}

// ── Verification of 6be2402a (`evidence/US-0159-verify.md`, lock at startup) ──

/// A point on the lock view's own background: left of the centred card, below
/// the title bar.
fn lock_background(cx: &mut VisualTestContext) -> gpui::Point<gpui::Pixels> {
    let size = cx.update(|window, _| window.viewport_size());
    gpui::point(gpui::px(8.), size.height / 2.)
}

/// Attack (b): a click on the lock screen's background. gpui moves the focus to
/// a `track_focus` element on mouse down, and the rework tracks a handle on the
/// whole lock view; the render-time reclaim only fires when the focus is
/// outside that view, so after the click the input has lost the keyboard.
/// Pins defect V1 (what happens today); flip the marked assertions once fixed.
#[gpui::test]
async fn verifier_startup_a_background_click_takes_the_keyboard_from_the_input(
    cx: &mut TestAppContext,
) {
    let f = startup_fixture(cx);
    let cx = f.cx;
    let view = lock_view(cx);
    let (input, whole) = cx.update(|_, cx| {
        let view = view.read(cx);
        (view.input_focus(cx), view.focus.clone())
    });
    cx.simulate_input("ab");
    assert_eq!(typed(cx), "ab");
    let at = lock_background(cx);
    cx.simulate_click(at, gpui::Modifiers::none());
    cx.run_until_parked();
    let (on_input, on_whole) =
        cx.update(|window, _| (input.is_focused(window), whole.is_focused(window)));
    cx.simulate_input("c");
    cx.run_until_parked();
    let after = typed(cx);
    // V1: today the click leaves the focus on the view, not the input.
    assert!(on_whole, "the click did not focus the whole lock view");
    assert!(
        !on_input,
        "V1 fixed? the input kept the keyboard: flip this test"
    );
    assert_eq!(
        after, "ab",
        "V1 fixed? typing reached the input: flip this test"
    );
    // Nothing else leaks: still locked, the application never drawn.
    assert!(is_locked(&f.vault, cx));
    assert_eq!(f.renders.get(), 0);
    let _ = (f.content, f.quits);
}

/// Attack (b): Tab on the lock screen. `Root`'s Tab handler (focus next) is not
/// on the dispatch path while the surface is replaced, so Tab never leaves the
/// password input: Unlock and Quit are reached with Enter, the mouse, or a
/// window close (Q6). The gpui test platform then delivers the unhandled tab as
/// text; Windows filters control characters (walked: Tab, then the right
/// password, unlocks), so no text is asserted here.
#[gpui::test]
async fn verifier_startup_tab_does_not_leave_the_lock_input(cx: &mut TestAppContext) {
    let f = startup_fixture(cx);
    let cx = f.cx;
    let view = lock_view(cx);
    let input = cx.update(|_, cx| view.read(cx).input_focus(cx));
    for keys in ["tab", "shift-tab", "tab tab"] {
        cx.simulate_keystrokes(keys);
        cx.run_until_parked();
        cx.update(|window, _| assert!(input.is_focused(window), "{keys} moved the focus"));
    }
    assert!(is_locked(&f.vault, cx));
    assert_eq!(f.quits.get(), 0);
    let _ = (f.content, f.renders);
}

/// Attack (b): the render-time reclaim must not turn into a frame loop, with
/// or without a dialog fighting for the focus under the lock. 10 s of fake
/// time: a blinking cursor costs about 20 frames; a loop would cost thousands
/// (or never park).
#[gpui::test]
async fn verifier_startup_an_idle_lock_screen_draws_no_frame_loop(cx: &mut TestAppContext) {
    let f = startup_fixture(cx);
    let cx = f.cx;
    let view = lock_view(cx);
    let frames = |cx: &mut VisualTestContext| cx.update(|_, cx| view.read(cx).frames);
    let ten_seconds = |cx: &mut VisualTestContext| {
        let start = frames(cx);
        for _ in 0..20 {
            cx.executor().advance_clock(Duration::from_millis(500));
            cx.run_until_parked();
        }
        frames(cx) - start
    };
    let idle = ten_seconds(cx);
    cx.update(|window, cx| window.open_dialog(cx, |dialog, _, _| dialog));
    cx.run_until_parked();
    let with_dialog = ten_seconds(cx);
    eprintln!("startup lock frames in 10 s: idle {idle}, with a dialog under it {with_dialog}");
    assert!(idle <= 30, "{idle} frames in 10 s idle");
    assert!(
        with_dialog <= 30,
        "{with_dialog} frames in 10 s with a dialog"
    );
    let _ = (f.content, f.renders, f.quits);
}

/// Attack (a): a window opened while the startup lock is up (a second window,
/// Settings) is born locked and never renders its application view.
#[gpui::test]
async fn verifier_startup_a_window_opened_under_the_startup_lock_never_renders(
    cx: &mut TestAppContext,
) {
    let f = startup_fixture(cx);
    let cx = f.cx;
    let renders = Rc::new(Cell::new(0));
    let counted = renders.clone();
    let second = cx.update(|_, cx| {
        cx.open_window(gpui::WindowOptions::default(), |window, cx| {
            let view = cx.new(|cx| Content {
                focus: cx.focus_handle(),
                renders: counted,
            });
            cx.new(|cx| Root::new(view, window, cx))
        })
        .unwrap()
    });
    cx.run_until_parked();
    assert_eq!(renders.get(), 0, "the second window drew its application");
    let born_locked = cx.update(|_, cx| {
        second
            .update(cx, |root, _, cx| {
                root.plugin::<LockScreen>().unwrap().read(cx).locked
            })
            .unwrap()
    });
    assert!(born_locked);
    f.vault
        .update(cx, |state, cx| {
            state.unlock_screen(SecretString::new(V1_PASSWORD), cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();
    assert!(renders.get() > 0, "the second window stayed covered");
    assert!(f.renders.get() > 0);
    let _ = (f.content, f.quits);
}

/// Attack (b): a dialog opened under the startup lock (the crash report). After
/// the unlock the plugin gives the focus back to what the application focused
/// while it was built, not to the dialog on top. Pins what happens today (V2):
/// the dialog is open, the focus is behind it.
#[gpui::test]
async fn verifier_startup_after_the_unlock_a_dialog_opened_under_the_lock_has_no_focus(
    cx: &mut TestAppContext,
) {
    let f = startup_fixture(cx);
    let cx = f.cx;
    cx.update(|window, cx| window.open_dialog(cx, |dialog, _, _| dialog));
    cx.run_until_parked();
    f.vault
        .update(cx, |state, cx| {
            state.unlock_screen(SecretString::new(V1_PASSWORD), cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();
    let content_focus = cx.update(|_, cx| f.content.read(cx).focus.clone());
    let (dialog_open, behind) = cx.update(|window, cx| {
        (
            window.has_active_dialog(cx),
            content_focus.is_focused(window),
        )
    });
    assert!(dialog_open);
    assert!(
        behind,
        "V2 fixed? the focus left the view behind the dialog: flip this test"
    );
    let _ = (f.renders, f.quits);
}

/// Attack (e): the first key after a startup lock left up for hours is not
/// "late input" (the unlock stamped the clock), and it reaches the application.
#[gpui::test]
async fn verifier_startup_the_first_key_after_a_long_startup_lock_does_not_relock(
    cx: &mut TestAppContext,
) {
    let f = startup_fixture(cx);
    let cx = f.cx;
    idle_for(Duration::from_secs(9 * 60 * 60), cx);
    cx.simulate_input(V1_PASSWORD);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(!is_locked(&f.vault, cx));
    cx.simulate_keystrokes("x");
    cx.run_until_parked();
    assert!(
        !is_locked(&f.vault, cx),
        "the first key after the unlock locked"
    );
    let _ = (f.content, f.renders, f.quits);
}
