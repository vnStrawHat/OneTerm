//! The editor space's menu (specs/oneterm, "Editing in OneTerm") on a real connection to the engine's
//! test server on 127.0.0.1: a right click gives Copy, Paste and Find, the editor's own
//! commands, and Close, which asks with changes; a binary file's menu has Close alone. The
//! clipboard is the harness's. And Wrap in its header, which breaks long lines at the space's
//! width.

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
    let mut a = App { h };
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

/// A right click at the middle of the file's editor.
fn right_click(a: &mut App) {
    let node = a.h.query(|n| {
        n.semantics.role == Role::TextInput && n.semantics.value.as_ref().is_some_and(|v| v.contains("port"))
    })[0];
    let at = a.h.tree.window_rect(node).center();
    for input in [quadra_tree::PointerInput::Down, quadra_tree::PointerInput::Up] {
        let now = a.h.now();
        a.h.tree.pointer(input, quadra::MouseButton::Right, at, Mods::NONE, now);
    }
    a.h.settle();
}

fn items(a: &App) -> Vec<String> {
    let tree = a.h.tree.semantic_tree();
    tree.nodes
        .iter()
        .filter(|n| n.semantics.role == Role::MenuItem)
        .map(|n| n.semantics.name.clone())
        .collect()
}

fn choose(a: &mut App, name: &str) {
    a.h.get_by_role(Role::MenuItem, name).click();
    a.h.settle();
    a.h.settle();
}

#[test]
fn the_editor_s_menu_copies_pastes_finds_and_closes() {
    let mut a = app("edit-menu", "OneTerm's editor");
    a.edit("config.yaml");
    a.wait(|a| a.shows("/home/config.yaml"), "the editor");
    a.h.key(Key::A, Mods::CTRL);
    a.h.settle();
    right_click(&mut a);
    assert_eq!(items(&a), ["Copy", "Paste", "Find", "Close"]);
    choose(&mut a, "Copy");
    assert!(a.h.clipboard.read_text().is_some_and(|it| it.contains("port: 22")), "copied");

    a.h.key(Key::Home, Mods::CTRL);
    a.h.settle();
    a.h.clipboard.write_text("zz");
    right_click(&mut a);
    choose(&mut a, "Paste");
    assert!(a.editor().is_some_and(|it| it.starts_with("zzname")), "{:?}", a.editor());

    right_click(&mut a);
    choose(&mut a, "Find");
    assert!(
        !a.h.query(|n| n.semantics.role == Role::TextInput && n.semantics.name == "Find").is_empty(),
        "the find bar
{}",
        a.h.semantics()
    );

    right_click(&mut a);
    choose(&mut a, "Close");
    assert!(a.h.has_text("Save changes?"), "it has changes");
    a.h.get_by_role(Role::Button, "Don't save").click();
    a.h.settle();
    assert!(!a.shows("/home/config.yaml"), "closed");

    a.edit("blob.bin");
    a.wait(|a| a.h.has_text("Binary file, 3 B: not shown, and not edited here."), "the binary file");
    let node = a.h.query(|n| n.semantics.name == "Binary file, 3 B: not shown, and not edited here.")[0];
    let at = a.h.tree.window_rect(node).center();
    for input in [quadra_tree::PointerInput::Down, quadra_tree::PointerInput::Up] {
        let now = a.h.now();
        a.h.tree.pointer(input, quadra::MouseButton::Right, at, Mods::NONE, now);
    }
    a.h.settle();
    assert_eq!(items(&a), ["Close"]);
}

#[test]
fn wrap_breaks_long_lines_at_the_space_s_width_and_back() {
    let mut a = app("edit-wrap", "OneTerm's editor");
    a.edit("config.yaml");
    a.wait(|a| a.shows("/home/config.yaml"), "the editor space");
    a.h.type_text(&"wide ".repeat(60));
    a.h.settle();
    let text = a.editor();
    let inputs = a.h.query(|n| n.semantics.role == Role::TextInput);
    let is = |a: &mut App, id| a.h.tree.widget_mut::<quadra_code::EditorWidget>(id).is_some();
    let id = inputs.into_iter().find(|&id| is(&mut a, id)).unwrap();
    // A press of the pointer on Wrap, which takes the focus as a press does.
    let tap = |a: &mut App| {
        let node = a.h.get_by_role(Role::Button, "Wrap").node();
        let at = a.h.tree.window_rect(node).center();
        a.h.click(at.x, at.y);
        a.h.settle();
    };
    let rows = |a: &mut App| a.h.tree.widget_mut::<quadra_code::EditorWidget>(id).unwrap().row_count();
    assert_eq!(rows(&mut a), 3);
    // Well under its three lines the space is still the editor: Shift and the wheel scroll it.
    let under = quadra::Point::new(a.h.tree.window_rect(id).center().x, 400.0);
    a.h.tree.wheel(quadra::Point::new(0.0, -3.0), false, under, Mods::SHIFT);
    a.h.settle();
    assert!(a.h.tree.widget_mut::<quadra_code::EditorWidget>(id).unwrap().scroll_x > 0.0);
    // Wrap, in the header: the long line takes rows; the text and the focus are as they were.
    tap(&mut a);
    assert!(rows(&mut a) > 3 && a.h.semantics().contains("Button \"Wrap\" [checked]"));
    assert_eq!(a.editor(), text);
    a.h.type_text("x");
    a.h.settle();
    assert_eq!(a.editor(), text.map(|it| it.replacen("wide name", "wide xname", 1)));
    tap(&mut a);
    assert_eq!(rows(&mut a), 3);
    // While the diff shows, Wrap leaves the focus where it is: nothing is typed behind it.
    let text = a.editor();
    a.h.get_by_role(Role::Button, "Diff").click();
    a.h.settle();
    tap(&mut a);
    a.h.type_text("unseen");
    a.h.settle();
    a.h.get_by_role(Role::Button, "Diff").click();
    a.h.settle();
    assert_eq!(a.editor(), text);
}
