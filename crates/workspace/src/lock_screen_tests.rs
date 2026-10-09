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
        let vault = cx.new(|_| VaultState::load(dir.0.clone(), false));
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
    cx.update(|window, cx| window.blur(cx));
    cx.simulate_input("q");
    cx.simulate_keystrokes("ctrl-shift-q");
    assert_eq!(f.quits.get(), 0);
    assert_eq!(typed(cx), "ab");
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
