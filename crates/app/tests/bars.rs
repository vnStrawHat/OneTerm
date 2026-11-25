//! The bars (specs/oneterm, "Window"): the title bar's bells and notices between Search and Lock,
//! with the tabs in the rail and in a strip, gone while locked and in the settings; the status
//! bar's clock by the app's time, and the card of what OneTerm uses from the fake meter.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use oneterm::{Model, NewSession, OneTerm, keymap};
use quadra::time::DateTime;
use quadra::{Role, Settings, provide};
use quadra_shell::{FakeShell, Usage};
use quadra_term::echo;
use quadra_testing::Harness;
use quadra_tree::PointerInput;

struct App {
    h: Harness,
    model: Model,
    shell: Rc<FakeShell>,
    /// Seconds the app's clock reads after 2026-11-16 20:45:07.
    seconds: Rc<Cell<i64>>,
}

const MB: u64 = 1024 * 1024;

fn usage(cpu: u64, uptime: u64) -> Usage {
    Usage {
        cpu: Duration::from_millis(cpu),
        uptime: Duration::from_secs(uptime),
        memory: 182 * MB + MB / 2,
        working_set: 240 * MB,
        peak_working_set: 300 * MB,
        private_bytes: 410 * MB,
        threads: 23,
    }
}

fn app(vertical: bool) -> App {
    let mut h = Harness::new();
    h.size = quadra::Size::new(1200.0, 700.0);
    h.tree.set_keymap(Rc::new(keymap()));
    let (settings, shell, seconds) = (Settings::memory(), FakeShell::new(), Rc::new(Cell::new(0)));
    settings.value("tabs.vertical", true).set(vertical);
    // What a second costs is the clock's alone here: no frames a second.
    settings.value("status_bar.fps", true).set(false);
    shell.set_usage(usage(0, 10));
    let (given, read, made) = (shell.clone(), seconds.clone(), Rc::new(std::cell::RefCell::new(None)));
    let kept = made.clone();
    let sessions: NewSession = Rc::new(|_| echo());
    h.mount(move || {
        provide(settings.clone());
        given.provide();
        let mut app = OneTerm::new(sessions);
        // 2026-11-16 20:45:07 UTC, said at UTC.
        app.now = Rc::new(move || DateTime::from_unix(1_794_861_907 + read.get(), 0));
        *kept.borrow_mut() = Some(app.model.clone());
        app.view()
    });
    h.settle();
    let model = made.borrow_mut().take().expect("the window made its model");
    App { h, model, shell, seconds }
}

impl App {
    fn rect(&mut self, name: &str) -> quadra::Rect {
        let node = self.h.get_by_role(Role::Button, name).node();
        self.h.tree.window_rect(node)
    }

    fn wait(&mut self, ms: u64) {
        self.h.advance(Duration::from_millis(ms));
        self.h.settle();
    }
}

#[test]
fn the_bells_stand_in_the_title_bar_between_search_and_lock() {
    for vertical in [true, false] {
        let mut a = app(vertical);
        a.model.prefs().vault.set("set".into()); // Lock OneTerm now shows
        a.h.settle();
        // No search in the title bar: the `+` menus have the field.
        assert!(a.h.query(|n| n.semantics.name == "Search tabs, hosts and commands").is_empty());
        let bells = a.rect("0 bells and notices");
        let (lock, settings) = (a.rect("Lock"), a.rect("Settings"));
        assert!(bells.y >= 0.0 && bells.bottom() <= 38.0, "in the title bar: {bells:?}");
        assert!(bells.right() <= lock.x && lock.right() <= settings.x);
        // Lock says the minutes before it locks (10 unless told), in the title bar alone.
        let minutes = a.h.get_by_text("10m").node();
        assert!(lock.contains(a.h.tree.window_rect(minutes).center()), "in Lock");
        if !vertical {
            let hosts = a.rect("SSH hosts");
            assert!(hosts.right() <= bells.x, "SSH hosts before the bells");
        }

        a.model.ui().settings.set(true);
        a.h.settle();
        assert!(a.h.query(|n| n.semantics.name == "0 bells and notices").is_empty(), "the settings: Done");
        a.model.ui().settings.set(false);
        a.model.lock().lock_now();
        a.h.settle();
        assert!(a.h.query(|n| n.semantics.name == "0 bells and notices").is_empty(), "locked");
    }
}

#[test]
fn the_clock_says_the_date_and_the_time_and_only_its_text_is_painted_again_each_second() {
    let mut a = app(true);
    assert!(a.h.has_text("2026-11-16 20:45:07"));
    a.seconds.set(1);
    a.h.advance(Duration::from_secs(1));
    let (stats, _) = a.h.frame_if_needed().expect("the clock changed");
    assert!(a.h.has_text("2026-11-16 20:45:08"));
    // The clock's text alone is painted again, and laid out: asked again its four requests, it
    // answers the same size, so nothing above it runs (Quadra's specs/06, "Measuring leaves").
    assert_eq!((stats.bindings_run, stats.nodes_styled, stats.nodes_painted), (1, 0, 1), "{stats:?}");
    assert_eq!((stats.nodes_laid_out, stats.nodes_measured), (4, 4), "{stats:?}");
}

#[test]
fn ram_is_the_private_working_set_and_its_tooltip_has_the_details() {
    let mut a = app(true);
    assert!(a.h.has_text("182 MB") && a.h.has_text("0%"));
    // 1 s of processor in 2 s of life, over the cores.
    a.shell.set_usage(usage(1000, 12));
    a.wait(2000);
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get()) as f64;
    let cpu = format!("{}%", (50.0 / cores).round());
    assert!(a.h.has_text(&cpu), "{}", a.h.semantics());

    let ram = a.h.get_by_text("RAM").node();
    let at = a.h.tree.window_rect(ram);
    a.h.pointer(PointerInput::Move, at.x + 2.0, at.y + 2.0);
    a.wait(500);
    for words in ["Private working set", "182.5 MB", "Working set", "240.0 MB", "Commit (private bytes)"] {
        assert!(a.h.has_text(words), "{words}: {}", a.h.semantics());
    }
    for words in ["Peak working set", "300.0 MB", "410.0 MB", "CPU time", "0:00:01", "Threads", "23"] {
        assert!(a.h.has_text(words), "{words}");
    }
    assert!(a.h.has_text("Uptime") && a.h.has_text("0:00:12"));
    // In a tooltip; a reader is told what OneTerm uses.
    assert!(!a.h.query(|n| n.semantics.role == Role::Tooltip).is_empty());
    let told = format!("OneTerm: CPU {cpu}, RAM 182 MB");
    assert!(
        !a.h.query(|n| n.semantics.description.as_deref() == Some(told.as_str())).is_empty(),
        "{}",
        a.h.semantics()
    );
    let card = a.h.tree.window_rect(a.h.tree.overlays().next().expect("the tooltip"));
    // The status bar is the window's last 32 px, what it holds in the 22 of its middle: the
    // card stands over that and covers none of it.
    assert!(card.bottom() <= 700.0 - 27.0, "over the status bar's words: {card:?}");
}

#[test]
fn a_reading_as_wide_as_the_last_lays_out_only_its_text() {
    let mut a = app(true);
    let mut reading = usage(0, 10);
    reading.memory = 183 * MB;
    a.shell.set_usage(reading);
    a.h.advance(Duration::from_secs(2));
    let (stats, _) = a.h.frame_if_needed().expect("the reading changed");
    assert!(a.h.has_text("183 MB"));
    assert_eq!((stats.nodes_laid_out, stats.nodes_measured), (4, 4), "{stats:?}");
    // A wider one moves what follows it.
    reading.memory = 1830 * MB;
    a.shell.set_usage(reading);
    a.wait(2000);
    assert!(a.h.has_text("1830 MB"));
}

#[test]
fn a_group_with_nothing_to_say_is_not_in_the_status_bar() {
    let mut a = app(true);
    let named = |a: &App, name: &str| a.h.query(|n| n.semantics.name == name).len();
    // A shell that has said no directory: no folder alone; a space with no connection: no
    // network between two lines.
    assert_eq!((named(&a, "Working directory"), named(&a, "Network")), (0, 0), "{}", a.h.semantics());
    let shell = a.model.active_tab().unwrap().focused().unwrap().session;
    shell.feeder().feed(b"\x1b]9;9;/work\x07", std::time::Instant::now());
    a.wait(100);
    assert_eq!(named(&a, "Working directory"), 1, "{}", a.h.semantics());
}

#[test]
fn git_says_the_changed_files_and_the_lines_added_and_removed() {
    // A repository of the test's own: a commit, then a line changed, a line added, a new file.
    let root = std::path::absolute(".tmp/bars-git").unwrap();
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let git = |args: &[&str]| {
        let quiet = ["-c", "user.name=t", "-c", "user.email=t@t.invalid", "-c", "commit.gpgsign=false"];
        let ran = std::process::Command::new("git").args(quiet).args(args).current_dir(&root).output();
        assert!(ran.is_ok_and(|it| it.status.success()), "git {args:?}");
    };
    git(&["init", "-q", "-b", "trunk"]);
    std::fs::write(root.join("a.txt"), "one\ntwo\nthree\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "first"]);
    std::fs::write(root.join("a.txt"), "one\n2\nthree\nfour\n").unwrap();
    std::fs::write(root.join("new.txt"), "x\n").unwrap();

    let mut a = app(true);
    let shell = a.model.active_tab().unwrap().focused().unwrap().session;
    let said = format!("\x1b]9;9;{}\x07", root.display());
    shell.feeder().feed(said.as_bytes(), std::time::Instant::now());
    let end = std::time::Instant::now() + Duration::from_secs(30);
    while !a.h.has_text("trunk") {
        assert!(std::time::Instant::now() < end, "never: the branch\n{}", a.h.semantics());
        a.wait(20);
    }
    // Two files changed (one of them new: a file, and no lines), two lines added, one removed.
    for part in ["M2", "+2", "-1"] {
        assert!(a.h.has_text(part), "{part}\n{}", a.h.semantics());
    }
}
