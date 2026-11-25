//! OneTerm's screens drawn without a window, to pictures, to compare with its design canvas
//! (specs/oneterm): the software device, the real text, the headless platform, `echo()` sessions
//! fed what the canvas shows. Nothing shows on the desktop.
//!
//!     scripts/cargo.sh run -p oneterm-shots -- [out dir] [scene ..]
//!
//! Scenes: main, collapsed, horizontal, menu, connect, add, settings, lock, sftp, gutter (main
//! with the terminals' gutter on), usage (the pointer resting on RAM: its card), find (the
//! find bar over the focused space, a word typed), page-0 to page-10 (each page of the
//! settings), and the
//! Connect dialogs: ask-connecting, ask-host, ask-sign-in, ask-failed (`asks.rs`); the editor
//! space: edit, edit-diff (`edits.rs`).

mod asks;
mod edits;
mod png;

use std::cell::RefCell;
use std::error::Error;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use oneterm::{Model, NewSession, OneTerm, keymap};
use quadra::{Settings, provide};
use quadra_app::{App, AppError};
use quadra_native_win::{D3d11Renderer, DirectWrite};
use quadra_platform::{
    HeadlessPlatform, MouseButton, Platform, PlatformEvent, PointerKind, WindowId, WindowOptions,
};
use quadra_prim::Point;
use quadra_term::{Session, echo};

type Shots = App<D3d11Renderer>;

const SCENES: [&str; 15] = [
    "main",
    "collapsed",
    "horizontal",
    "menu",
    "connect",
    "add",
    "host-connect",
    "host-edit",
    "keys",
    "settings",
    "lock",
    "sftp",
    "gutter",
    "usage",
    "find",
];

/// Each page of the settings, by its place.
const PAGES: [&str; 11] = [
    "page-0", "page-1", "page-2", "page-3", "page-4", "page-5", "page-6", "page-7", "page-8", "page-9",
    "page-10",
];

fn pass(platform: &HeadlessPlatform, app: &mut Shots, ms: u64) {
    for _ in 0..ms.div_ceil(75) {
        quadra_async::wait_idle(Duration::from_secs(10));
        platform.waker().wake();
        platform.step(app);
        platform.advance(Duration::from_millis(75), app);
    }
}

fn click(platform: &HeadlessPlatform, app: &Shots, window: WindowId, name: &str) {
    point(platform, app, window, name, false);
}

/// Presses what is named; with `only_move`, the pointer only comes to rest on it.
fn point(platform: &HeadlessPlatform, app: &Shots, window: WindowId, name: &str, only_move: bool) {
    let Some(tree) = app.tree(window) else { return };
    let nodes = tree.semantic_tree().nodes;
    let Some(found) = nodes.iter().find(|n| n.semantics.name == name) else { return };
    let Some(node) = tree.node_of(found.id) else { return };
    let at = tree.window_rect(node).center();
    let kinds: &[PointerKind] = if only_move {
        &[PointerKind::Move]
    } else {
        &[PointerKind::Move, PointerKind::Down, PointerKind::Up]
    };
    for &kind in kinds {
        let button = if kind == PointerKind::Move { MouseButton::None } else { MouseButton::Left };
        platform.push(
            window,
            PlatformEvent::Pointer { kind, button, at: Point::new(at.x, at.y), mods: Default::default() },
        );
    }
}

/// Whether a node of the window is named with `words` in it.
fn shows(app: &Shots, window: WindowId, words: &str) -> bool {
    let Some(tree) = app.tree(window) else { return false };
    tree.semantic_tree().nodes.iter().any(|n| n.semantics.name.contains(words))
}

/// The canvas's saved hosts, as rows of the settings, with their colour tags.
fn hosts() -> Vec<Vec<String>> {
    let host = |name: &str, group: &str, target: &str, jump: &str, tag: &str| {
        [name, group, target, "Auto", "", jump, "", tag].map(String::from).to_vec()
    };
    vec![
        host("prod-api-01", "Production", "trung@10.20.1.14", "", "red"),
        host("prod-api-02", "Production", "trung@10.20.1.15", "", "red"),
        host("prod-lb", "Production", "ops@10.20.0.2", "prod-api-01", "purple"),
        host("staging-db-01", "Staging", "trung@10.30.2.8", "", "yellow"),
        host("staging-web", "Staging", "trung@10.30.2.20", "", "yellow"),
        host("home-nas", "Personal", "admin@nas.local", "", "blue"),
    ]
}

/// What the spaces say: the canvas's lines, its titles and its signals.
fn fill(model: &Model, made: &[Session]) {
    let feed = |n: usize, bytes: &str| {
        if let Some(s) = made.get(n) {
            s.feeder().feed(bytes.replace('\n', "\r\n").as_bytes(), Instant::now());
        }
    };
    let (g, b, y, d, x) = ("\x1b[32m", "\x1b[34m", "\x1b[33m", "\x1b[90m", "\x1b[0m");
    feed(
        1,
        &format!(
            "\x1b]0;neo-gateway deploy\x07\x1b]9;9;~/apps/neo-gateway\x07{g}trung@prod-api-01{x} {b}~/apps/neo-gateway{x} {d}main{x} $ git pull --rebase\nUpdating 3f1c2ab..8e77d10\nFast-forward\n src/auth/session.go   | 42 {g}++++++++{x}\x1b[31m---{x}\n 2 files changed, 31 insertions(+), 11 deletions(-)\n\n{g}trung@prod-api-01{x} $ make deploy ENV=prod\n{b}\u{25B8}{x} building image neo-gateway:8e77d10 ... {g}done{x}\n{b}\u{25B8}{x} running tests ........ {g}142 passed{x}\n{b}\u{25B8}{x} pushing to registry ap-southeast-1 ... {y}64%{x}\n\x1b]7501;state=blocked:id=deploy\x07"
        ),
    );
    feed(
        0,
        &format!(
            "\x1b]9;9;~/Projects/oneterm\x07\x1b[35m\u{276F}{x} pnpm dev\n{d}> oneterm@0.4.2 dev{x}\n{g}\u{2713}{x} vite v6.0.3 ready in {y}412 ms{x}\n  {b}\u{279C}{x}  Local:   http://localhost:5173/\n\x1b[35m\u{276F}{x} "
        ),
    );
    feed(
        2,
        &format!(
            "\x1b]9;9;/var/log/postgresql\x07{g}trung@staging-db-01{x} {b}/var/log/postgresql{x} $ tail -f postgresql-16-main.log\n{d}10:58:41{x} LOG:  checkpoint starting: time\n{d}10:59:20{x} {y}WARNING{x}:  autovacuum: table \"orders\" 12% dead tuples\n{d}11:00:02{x} LOG:  backup \"nightly\" completed (2.1 GB)\n\x1b]9;Backup \"nightly\" completed\x07"
        ),
    );
    feed(3, "\x1b]0;build \u{B7} docker\x07\x1b]9;4;1;64\x07\x1b]7501;state=working:id=build\x07");
    feed(4, "\x1b]0;staging-db-01\x07\x07");
    feed(5, "\x1b]0;zsh \u{B7} cargo test\x07\x1b]7501;state=done:id=test\x07");
    feed(6, "\x1b]0;docs-cdn\x07");
    // The latency an SSH link would note.
    for (n, note) in [(1, "124 ms"), (2, "31 ms")] {
        if let Some(s) = made.get(n) {
            s.link_note().set(note.into());
        }
    }
    let _ = model;
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(args.next().unwrap_or_else(|| ".tmp/shots/oneterm".into()));
    let only: Vec<String> = args.collect();
    std::fs::create_dir_all(&out)?;
    for scene in SCENES
        .iter()
        .chain(&PAGES)
        .chain(&asks::SCENES)
        .chain(&edits::SCENES)
        .filter(|it| only.is_empty() || only.iter().any(|o| o == *it))
    {
        let platform = HeadlessPlatform::new();
        let text = DirectWrite::new().map_err(AppError::Text)?;
        let app = App::with(Rc::new(platform.clone()), D3d11Renderer::new(true)?, Box::new(text));
        let mut app = quadra_design::install(app, &quadra_design::dark())?.keymap(keymap());
        let made: Rc<RefCell<Vec<Session>>> = Rc::default();
        let kept = made.clone();
        let sessions: NewSession = Rc::new(move |launch: oneterm::Launch| {
            let tested = launch.ssh.as_ref().is_some_and(|it| it.target.host == "127.0.0.1");
            let s = match tested {
                true if scene.starts_with("edit") => edits::session(launch),
                true => asks::session(launch),
                false => echo(),
            };
            kept.borrow_mut().push(s.clone());
            s
        });
        let built: Rc<RefCell<Option<Model>>> = Rc::default();
        let tell = built.clone();
        let vertical = *scene != "horizontal";
        let options = WindowOptions::new("OneTerm").size(1440.0, 900.0).custom_frame();
        let window = app.open_window(options, move || {
            let settings = Settings::memory();
            settings.value("ssh.hosts", Vec::<Vec<String>>::new()).set(hosts());
            settings.value("tabs.vertical", true).set(vertical);
            settings.value("terminal.show_gutter", false).set(*scene == "gutter");
            // The local pane lists a folder of the project, not the user's Downloads.
            let local = std::path::absolute(".tmp/shots/oneterm-local").unwrap_or_default();
            let _ = std::fs::create_dir_all(local.join("deploy"));
            let _ = std::fs::write(
                local.join("config.yaml"),
                "env: prod
",
            );
            settings.value("sftp.downloads", String::new()).set(local.display().to_string());
            provide(settings);
            // The app's folders in the project, for a file edited: downloaded there.
            if scene.starts_with("edit") {
                let dirs = quadra::AppDirs::under(
                    &std::path::absolute(".tmp/shots/oneterm-app").unwrap_or_default(),
                );
                provide(Rc::new(dirs));
            }
            let term = OneTerm::new(sessions.clone());
            *tell.borrow_mut() = Some(term.model.clone());
            term.view()
        })?;
        let model = built.borrow().clone().ok_or("no model")?;
        // The first tab holds three spaces, two of them remote (`echo()` sessions too: nothing
        // connects); four tabs more.
        let host = |name: &str| model.hosts().into_iter().find(|it| it.name == name).map(|it| it.remote);
        for name in ["prod-api-01", "staging-db-01"] {
            if let Some(remote) = host(name) {
                model.connect_to(remote, true);
            }
        }
        for _ in 0..4 {
            model.new_tab();
        }
        let first = model.tabs().get_untracked()[0];
        model.active().set(first.id);
        // The canvas's focused space is the deploy on prod-api-01.
        first.focus.set(1);
        pass(&platform, &mut app, 150);
        fill(&model, &made.borrow());
        model.ui().dock.set(*scene != "connect" && !scene.starts_with("ask-") && !scene.starts_with("host-"));
        // A master password, as the canvas has: Lock OneTerm now shows.
        model.prefs().vault.set("set".into());
        match *scene {
            "menu" => click(&platform, &app, window, "New terminal"),
            "connect" => model.ui().connecting.set(true),
            "add" => model.ui().adding.set(true),
            "host-connect" => model.ui().opening.host.set(Some(("prod-lb".into(), false))),
            "host-edit" => model.ui().editing.set(Some("prod-lb".into())),
            "keys" => {
                let key = std::path::absolute(".tmp/ssh-tests/ssh-keys/work_key").unwrap_or_default();
                model.prefs().ssh_keys.set(vec![key.display().to_string()]);
                model.ui().page.set(3);
                model.ui().settings.set(true);
            }
            "settings" => model.ui().settings.set(true),
            page if page.starts_with("page-") => {
                model.ui().page.set(page[5..].parse().unwrap_or(0));
                model.ui().settings.set(true);
            }
            "lock" => model.lock().lock_now(),
            "sftp" => model.ui().expanded.set(true),
            "collapsed" => model.prefs().rail_folded.set(true),
            "find" => {
                for down in [true, false] {
                    let (key, mods) = (quadra_platform::Key::F, quadra_platform::Mods::CTRL);
                    platform.push(window, PlatformEvent::Key { key, code: 0, mods, down, repeat: false });
                }
                pass(&platform, &mut app, 150);
                platform.push(window, PlatformEvent::Text("deploy".into()));
            }
            // Once what was fed has moved the bar's groups to their places.
            "usage" => {
                pass(&platform, &mut app, 150);
                point(&platform, &app, window, "RAM", true);
            }
            ask if ask.starts_with("ask-") => {
                // The engine's threads go at their own pace: 10 s at most.
                let (words, end) = (asks::start(&model, ask), Instant::now() + Duration::from_secs(10));
                while !shows(&app, window, words) && Instant::now() < end {
                    pass(&platform, &mut app, 75);
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
            edit if edit.starts_with("edit") => edits::start(&model, edit, &platform, &mut app, window),
            _ => {}
        }
        pass(&platform, &mut app, 900);
        let pixels = app.read_pixels(window)?;
        let file = out.join(format!("{scene}.png"));
        std::fs::write(&file, png::png(pixels.size.width, pixels.size.height, &pixels.rgba))?;
        println!("{}", file.display());
    }
    Ok(())
}
