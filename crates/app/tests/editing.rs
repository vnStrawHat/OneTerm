//! Editing a remote file (specs/oneterm, "Spaces" and "M9 in detail") on a real connection to the
//! engine's test server on 127.0.0.1: Edit opens a space of OneTerm with the file coloured by its
//! type, the header says it has changes and shows their diff, Ctrl+S sends it back, closing
//! with changes asks, a binary file is not edited. The external editor is tests/dock.rs's.

use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use oneterm::{NewSession, OneTerm, Remote, keymap, remote_shell_with};
use oneterm_ssh::Auth;
use oneterm_ssh::testing::{folder, options, serve_files, trust, write_user_key};
use quadra::{Key, Mods, Role, Settings, provide};
use quadra_term::echo;
use quadra_testing::Harness;

struct App {
    h: Harness,
    served: PathBuf,
}

const CONFIG: &str = "name: one\r\nport: 22\r\n";

/// The app with one tab connected to a test server by a key, its dock open, editing with the
/// choice of the settings named.
fn app(name: &str, editor: &str) -> App {
    let dir = folder(name);
    let served = dir.join("served");
    std::fs::create_dir_all(served.join("home")).unwrap();
    std::fs::write(served.join("home/config.yaml"), CONFIG).unwrap();
    std::fs::write(served.join("home/run"), "#!/usr/bin/env python3\nprint(1)\n").unwrap();
    std::fs::write(served.join("home/blob.bin"), [7u8, 0, 1]).unwrap();
    std::fs::create_dir_all(served.join("home/src")).unwrap();
    std::fs::write(served.join("home/src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(served.join("home/src/lib.rs"), "pub fn port() {}\n").unwrap();
    let (port, _) = serve_files(11, served.clone());
    trust(port, 11, &dir.join("known_hosts"));
    write_user_key(&dir.join("id"));
    let settings = Settings::memory();
    settings.value("sftp.editor", String::new()).set(editor.to_string());
    let (base, key_file) = (dir.clone(), dir.join("id"));
    let (home, given) = (dir.join("app"), quadra_shell::FakeShell::new());
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
    let mut a = App { h, served };
    a.wait(|a| a.button("config.yaml"), "the home folder");
    a
}

impl App {
    /// Settles until `done`, 20 s at most: the engine works on threads of its own.
    fn wait(&mut self, done: impl Fn(&App) -> bool, what: &str) {
        let end = Instant::now() + Duration::from_secs(20);
        loop {
            self.h.advance(Duration::from_millis(10));
            self.h.settle();
            if done(self) {
                return;
            }
            assert!(Instant::now() < end, "never: {what}\n{}", self.h.semantics());
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn button(&self, name: &str) -> bool {
        !self.h.query(|n| n.semantics.role == Role::Button && n.semantics.name == name).is_empty()
    }

    /// Edit… in the menu of an entry of the dock.
    fn edit(&mut self, name: &str) {
        let node = self.h.get_by_role(Role::Button, name).node();
        let at = self.h.tree.window_rect(node).center();
        for input in [quadra_tree::PointerInput::Down, quadra_tree::PointerInput::Up] {
            let now = self.h.now();
            self.h.tree.pointer(input, quadra::MouseButton::Right, at, quadra::Mods::NONE, now);
        }
        self.h.settle();
        self.h.get_by_role(Role::MenuItem, "Edit\u{2026}").click();
        self.h.settle();
    }

    /// Whether an editor space's header shows a remote path, after `user@host:port:`.
    fn shows(&self, path: &str) -> bool {
        let at = format!(":{path}");
        !self
            .h
            .query(|n| n.semantics.name.starts_with("tester@127.0.0.1:") && n.semantics.name.ends_with(&at))
            .is_empty()
    }

    /// Presses of the pointer at the middle of a button.
    fn tap(&mut self, name: &str, times: usize) {
        let node = self.h.get_by_role(Role::Button, name).node();
        let at = self.h.tree.window_rect(node).center();
        for _ in 0..times {
            self.h.click(at.x, at.y);
        }
        self.h.settle();
    }

    fn served(&self, name: &str) -> Vec<u8> {
        std::fs::read(self.served.join("home").join(name)).unwrap_or_default()
    }

    /// The texts of the editors of config.yaml, as a reader is told them.
    fn editors(&self) -> Vec<String> {
        let tree = self.h.tree.semantic_tree();
        let texts = tree.nodes.iter().filter(|n| n.semantics.role == Role::TextInput);
        texts.filter_map(|n| n.semantics.value.clone()).filter(|it| it.contains("port")).collect()
    }

    fn editor(&self) -> Option<String> {
        self.editors().into_iter().next()
    }
}

#[test]
fn a_file_opens_in_a_space_is_coloured_by_its_type_and_saved_back() {
    let mut a = app("edit-space", "OneTerm's editor");
    a.edit("config.yaml");
    a.wait(|a| a.shows("/home/config.yaml"), "the editor space");
    assert!(a.h.has_text("yaml"), "its type\n{}", a.h.semantics());
    assert_eq!(a.editor().as_deref(), Some("name: one\nport: 22\n"));
    // It has the focus: what is typed goes to it, and the header says it has changes.
    assert!(!a.h.has_text("\u{25CF} Modified"), "CR LF is not a change");
    a.h.type_text("# ");
    a.h.settle();
    assert!(a.h.has_text("\u{25CF} Modified"), "{}", a.h.semantics());
    assert_eq!(a.editor().as_deref(), Some("# name: one\nport: 22\n"));
    // The diff, in the editor's place, and back: the editor has the focus again.
    a.tap("Diff", 1);
    assert!(a.h.has_text("# name: one") && a.editor().is_none(), "{}", a.h.semantics());
    a.tap("Diff", 1);
    a.h.type_text("x");
    a.h.settle();
    assert_eq!(a.editor().as_deref(), Some("# xname: one\nport: 22\n"));
    // Ctrl+S: on the server, with the file's line ends; no changes left, the base is the text.
    a.h.key(Key::S, Mods::CTRL);
    a.wait(|a| a.served("config.yaml") == b"# xname: one\r\nport: 22\r\n", "saved");
    a.wait(|a| !a.h.has_text("\u{25CF} Modified"), "no changes left");
    assert!(a.h.semantics().contains("config.yaml saved to the server."));
    a.tap("Diff", 1);
    assert!(a.h.has_text("No changes."), "{}", a.h.semantics());
    a.tap("Diff", 1);
    // Opened again, the same space takes the focus.
    a.edit("config.yaml");
    a.wait(|a| a.shows("/home/config.yaml"), "focused");
    assert_eq!(a.editors().len(), 1);
}

#[test]
fn closing_with_changes_asks_and_a_binary_file_is_not_edited() {
    let mut a = app("edit-close", "OneTerm's editor");
    a.edit("blob.bin");
    a.wait(|a| a.h.has_text("Binary file, 3 B: not shown, and not edited here."), "the binary file");
    assert!(!a.button("Save") && !a.button("Diff"));
    a.h.key(Key::W, Mods::CTRL);
    a.h.settle();
    assert!(!a.h.has_text("Binary file, 3 B: not shown, and not edited here."), "closed without asking");

    a.edit("run");
    a.wait(|a| a.shows("/home/run"), "the script");
    assert!(a.h.has_text("python"), "its type by its first line");
    a.h.type_text("x");
    a.h.settle();
    a.h.key(Key::W, Mods::CTRL);
    a.h.settle();
    assert!(a.h.has_text("Save changes?"));
    a.h.get_by_role(Role::Button, "Cancel").click();
    a.h.settle();
    assert!(a.shows("/home/run"), "still open");
    a.h.key(Key::W, Mods::CTRL);
    a.h.settle();
    // The dialog's Save: the last of the two (the header has one).
    let saves = a.h.query(|n| n.semantics.role == Role::Button && n.semantics.name == "Save");
    assert!(a.h.tree.act(*saves.last().unwrap(), &quadra_tree::SemanticAction::Click));
    a.wait(|a| a.served("run").starts_with(b"x#!"), "saved");
    a.wait(|a| !a.shows("/home/run"), "closed once saved");

    a.edit("run");
    a.wait(|a| a.shows("/home/run"), "again");
    a.h.type_text("y");
    a.h.settle();
    a.h.key(Key::W, Mods::CTRL);
    a.h.settle();
    a.h.get_by_role(Role::Button, "Don't save").click();
    a.h.settle();
    assert!(!a.shows("/home/run"), "closed");
    assert!(a.served("run").starts_with(b"x#!"), "not sent");
}

#[test]
fn a_file_edited_from_an_editor_space_keeps_the_dock_where_it_was() {
    let mut a = app("edit-second", "OneTerm's editor");
    a.tap("src", 2);
    a.wait(|a| a.button("main.rs"), "the folder");
    a.edit("main.rs");
    a.wait(|a| a.shows("/home/src/main.rs"), "the first");
    // The dock shows the folder of the space the file came from, and the next file opens too.
    assert!(a.button("lib.rs") && !a.button("config.yaml"), "{}", a.h.semantics());
    a.edit("lib.rs");
    a.wait(|a| a.shows("/home/src/lib.rs"), "the second");
    assert!(a.button("main.rs") && !a.button("config.yaml"), "{}", a.h.semantics());
    assert!(a.shows("/home/src/main.rs"), "both open");
}

#[test]
fn closing_a_tab_or_the_window_with_changes_asks_first() {
    let mut a = app("edit-tab", "OneTerm's editor");
    a.edit("config.yaml");
    a.wait(|a| a.shows("/home/config.yaml"), "the editor");
    assert!(a.h.has_text("1 ssh \u{B7} 1 space"), "an editor is not counted\n{}", a.h.semantics());
    a.h.type_text("x");
    a.h.settle();
    a.h.key(Key::T, Mods::CTRL);
    a.h.settle();
    assert!(!a.shows("/home/config.yaml"), "another tab shows");

    // Its tab's close: the tab shows again with the edit, and the editor asks.
    a.h.get_by_role(Role::Tab, "config.yaml").click();
    a.h.settle();
    assert!(
        a.editor().is_some_and(|it| it.starts_with("xname")),
        "the edit kept
{}",
        a.h.semantics()
    );
    let tabs = a.h.query(|n| n.semantics.role == Role::Tab).len();
    a.h.get_by_role(Role::Button, "Close tab").click();
    a.h.settle();
    assert!(a.h.has_text("Save changes?"), "{}", a.h.semantics());
    a.h.get_by_role(Role::Button, "Cancel").click();
    a.h.settle();
    a.h.key(Key::T, Mods::CTRL);
    a.h.settle();

    // The window's close asks the same; Don't save closes the window.
    assert!(!a.h.tree.may_close());
    a.h.settle();
    assert!(a.h.has_text("Save changes?") && a.shows("/home/config.yaml"), "{}", a.h.semantics());
    assert!(a.h.tree.take_window_requests().is_empty());
    a.h.get_by_role(Role::Button, "Don't save").click();
    a.h.settle();
    assert_eq!(a.h.tree.take_window_requests(), [quadra_tree::WindowRequest::Close]);
    assert!(a.h.tree.may_close(), "nothing left to ask");
    assert_eq!(a.h.query(|n| n.semantics.role == Role::Tab).len(), tabs + 1);
}

#[test]
fn a_closing_given_up_takes_back_its_don_t_saves() {
    let mut a = app("edit-give-up", "OneTerm's editor");
    for name in ["config.yaml", "run"] {
        a.edit(name);
        a.wait(|a| a.shows(&format!("/home/{name}")), name);
        a.h.type_text("x");
        a.h.settle();
    }
    let asks = |a: &App, name: &str| a.h.has_text(&format!("{name} has changes that are not on the server."));
    // Asked from the expanded SFTP view: the workspace shows again, with the question.
    a.h.get_by_role(Role::Button, "Zoom SFTP").click();
    a.h.settle();
    assert!(!a.h.tree.may_close());
    a.h.settle();
    assert!(asks(&a, "config.yaml"), "{}", a.h.semantics());
    a.h.get_by_role(Role::Button, "Don't save").click();
    a.h.settle();
    assert_eq!(a.h.tree.take_window_requests(), [quadra_tree::WindowRequest::Close]);
    assert!(!a.h.tree.may_close());
    a.h.settle();
    assert!(asks(&a, "run"), "the next one");
    a.h.key(Key::Escape, Mods::NONE);
    a.h.settle();
    assert!(!a.h.has_text("Save changes?"), "{}", a.h.semantics());
    assert!(!a.h.tree.may_close());
    a.h.settle();
    assert!(asks(&a, "config.yaml"), "its Don't save taken back");
}
