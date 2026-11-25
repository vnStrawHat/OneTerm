//! The app the dock's tests drive: one tab connected by a key to the engine's test server
//! on 127.0.0.1, whose files are a folder of `.tmp`.
#![allow(dead_code)] // each test file uses some of it

use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use oneterm::{NewSession, OneTerm, Remote, keymap, remote_shell_with};
use oneterm_ssh::Auth;
use oneterm_ssh::testing::{folder, options, serve_files, trust, write_user_key};
use quadra::{Role, Settings, provide};
use quadra_term::echo;
use quadra_testing::Harness;

pub struct App {
    pub h: Harness,
    pub served: PathBuf,
    pub downloads: PathBuf,
    /// The app's folders, and what stands for the system's opener.
    pub data: PathBuf,
    pub shell: Rc<quadra_shell::FakeShell>,
}

/// The app with one tab connected to a test server by a key, its dock open as an SSH tab's is.
pub fn app(name: &str) -> App {
    let dir = folder(name);
    let served = dir.join("served");
    std::fs::create_dir_all(served.join("home/src")).unwrap();
    std::fs::write(served.join("home/config.yaml"), vec![b'x'; 2458]).unwrap();
    let (port, _) = serve_files(11, served.clone());
    trust(port, 11, &dir.join("known_hosts"));
    write_user_key(&dir.join("id"));
    let downloads = dir.join("downloads");
    std::fs::create_dir_all(&downloads).unwrap();

    let settings = Settings::memory();
    settings.value("sftp.downloads", String::new()).set(downloads.display().to_string());
    // Edit opens the system's app (tests/editing.rs edits in OneTerm).
    settings.value("sftp.editor", String::new()).set("System default app".into());
    let (base, key_file) = (dir.clone(), dir.join("id"));
    let (home, shell) = (dir.join("app"), quadra_shell::FakeShell::new());
    let (data, given) = (home.join("data"), shell.clone());
    let sessions: NewSession = Rc::new(move |launch: oneterm::Launch| match launch.ssh {
        Some(_) => remote_shell_with(launch, |_| options(port, &base, Auth::Auto)),
        None => echo(),
    });
    let mut h = Harness::new();
    h.size = quadra::Size::new(1400.0, 800.0);
    h.tree.set_keymap(Rc::new(keymap()));
    h.mount(move || {
        provide(settings.clone());
        provide(Rc::new(quadra::AppDirs::under(&home)));
        given.provide();
        let app = OneTerm::new(sessions);
        let target = oneterm_ssh::Target { user: "tester".into(), host: "127.0.0.1".into(), port };
        app.model.connect_to(Remote::new(target, Auth::Key(key_file.clone())), false);
        app.view()
    });
    h.settle();
    // An SSH tab shows its dock by itself.
    App { h, served, downloads, data, shell }
}

impl App {
    /// Settles until `done`, 20 s at most: the engine works on threads of its own.
    pub fn wait(&mut self, done: impl Fn(&App) -> bool, what: &str) {
        let end = Instant::now() + Duration::from_secs(20);
        loop {
            // The app's timers move with the harness's clock.
            self.h.advance(Duration::from_millis(10));
            self.h.settle();
            if done(self) {
                return;
            }
            assert!(Instant::now() < end, "never: {what}\n{}", self.h.semantics());
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn pane(&self, name: &str) -> bool {
        !self.h.query(|n| n.semantics.name == name && n.semantics.role != Role::Button).is_empty()
    }

    /// A click of the other button on an entry, then a row of its menu.
    pub fn menu(&mut self, name: &str, row: &str) {
        let node = self.h.get_by_role(Role::Button, name).node();
        let at = self.h.tree.window_rect(node).center();
        for input in [quadra_tree::PointerInput::Down, quadra_tree::PointerInput::Up] {
            let now = self.h.now();
            self.h.tree.pointer(input, quadra::MouseButton::Right, at, quadra::Mods::NONE, now);
        }
        self.h.settle();
        self.h.get_by_role(Role::MenuItem, row).click();
        self.h.settle();
    }

    /// Clicks of the pointer at the middle of an entry.
    pub fn clicks(&mut self, name: &str, times: usize) {
        let row = self.h.get_by_role(Role::Button, name).node();
        let at = self.h.tree.window_rect(row).center();
        for _ in 0..times {
            self.h.click(at.x, at.y);
        }
        self.h.settle();
    }

    pub fn entry(&self, name: &str) -> bool {
        !self.h.query(|n| n.semantics.role == Role::Button && n.semantics.name == name).is_empty()
    }
}
