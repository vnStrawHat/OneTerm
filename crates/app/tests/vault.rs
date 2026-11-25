//! Saved passwords and Connect all (specs/oneterm, "Saved passwords", "M8 in detail") on real
//! connections to the engine's test server on 127.0.0.1 (never another host): a password
//! saved sealed signs in by itself, the master password opens the vault when its key is not
//! in memory, a group connects all its hosts, a startup command is written once the shell
//! opens. Every file is under `.tmp`.

use std::rc::Rc;
use std::time::{Duration, Instant};

use oneterm::{Host, Model, NewSession, OneTerm, Remote, keymap, remote_shell_with};
use oneterm_ssh::Auth;
use oneterm_ssh::testing::{folder, options, serve, trust, write_locked_user_key, write_user_key};
use quadra::{Role, Settings, provide};
use quadra_term::echo;
use quadra_testing::Harness;

struct App {
    h: Harness,
    model: Model,
    settings: Rc<Settings>,
    port: u16,
    dir: std::path::PathBuf,
}

fn app(name: &str) -> App {
    let dir = folder(name);
    std::fs::create_dir_all(dir.join("keys")).unwrap();
    let (port, _) = serve(41);
    trust(port, 41, &dir.join("known_hosts"));
    let base = dir.clone();
    let sessions: NewSession = Rc::new(move |launch: oneterm::Launch| match launch.ssh {
        Some(_) => remote_shell_with(launch, |target| {
            let mut o = options(target.port, &base, Auth::Auto);
            o.target = target;
            o
        }),
        None => echo(),
    });
    let settings = Settings::memory();
    let mut h = Harness::new();
    h.size = quadra::Size::new(1400.0, 800.0);
    h.tree.set_keymap(Rc::new(keymap()));
    let (given, kept) = (settings.clone(), Rc::new(std::cell::RefCell::new(None)));
    let (made, fake) = (kept.clone(), quadra_shell::FakeShell::new());
    h.mount(move || {
        provide(given.clone());
        fake.provide();
        let app = OneTerm::new(sessions);
        *made.borrow_mut() = Some(app.model.clone());
        app.view()
    });
    h.settle();
    let model = kept.borrow().clone().unwrap();
    model.ui().opening.known_hosts.set(dir.join("known_hosts"));
    App { h, model, settings, port, dir }
}

impl App {
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

    fn host(&mut self, name: &str, group: &str) {
        self.host_with(name, group, Auth::Password);
    }

    fn host_with(&mut self, name: &str, group: &str, auth: Auth) {
        let target = oneterm_ssh::Target::parse(&format!("tester@127.0.0.1:{}", self.port)).unwrap();
        let remote = Remote::new(target, auth);
        self.model.save_host(Host { name: name.into(), group: group.into(), remote, jump: String::new() });
        self.h.settle();
    }

    /// Picks an item of the menu of a row of the rail.
    fn menu(&mut self, row: &str, item: &str) {
        let node = self.h.get_by_role(Role::Button, row).node();
        let at = self.h.tree.window_rect(node).center();
        for input in [quadra_tree::PointerInput::Down, quadra_tree::PointerInput::Up] {
            let now = self.h.now();
            self.h.tree.pointer(input, quadra::MouseButton::Right, at, quadra::Mods::NONE, now);
        }
        self.h.settle();
        self.h.get_by_role(Role::MenuItem, item).click();
        self.h.settle();
    }

    /// Its menu's Connect, then the Connect dialog's.
    fn connect(&mut self, row: &str) {
        self.menu(row, "Connect");
        self.h.get_by_role(Role::Button, "Connect").click();
        self.h.settle();
    }

    fn secrets(&self) -> Vec<Vec<String>> {
        self.settings.value("ssh.secrets", Vec::<Vec<String>>::new()).get_untracked()
    }

    /// How many tabs are signed in: their terminal says the server's welcome.
    fn welcomed(&self) -> usize {
        self.h
            .query(|n| {
                n.semantics.role == Role::TextInput
                    && n.semantics.value.as_deref().is_some_and(|it| it.contains("welcome"))
            })
            .len()
    }

    fn master(&mut self, current: &str, new: &str) {
        let done = Rc::new(std::cell::Cell::new(false));
        let told = done.clone();
        self.model.lock().set_password(current.into(), new.into(), move |_| told.set(true));
        self.wait(|_| done.get(), "the master password");
    }
}

#[test]
fn a_saved_password_signs_in_and_the_master_password_opens_it() {
    let mut a = app("vault-sign-in");
    a.master("", "master");
    a.host("box", "");
    a.menu("box", "Edit\u{2026}");
    a.h.get_by_test_id("host-secret").set_value("secret");
    a.h.settle();
    a.h.get_by_role(Role::Button, "Save").click();
    a.h.settle();
    let sealed = a.secrets();
    assert!(sealed.len() == 1 && sealed[0][1].starts_with("otv1:") && !sealed[0][1].contains("secret"));
    a.connect("box");
    a.wait(|a| a.welcomed() == 1, "signed in by the saved password");
    assert!(!a.h.has_text("Sign in"));

    // As after a start: the data key is not in memory, so the master password is asked.
    a.model.lock().key.set(None);
    a.connect("box");
    assert!(a.h.has_text("Unlock saved passwords"), "{}", a.h.semantics());
    a.h.get_by_test_id("vault-password").set_value("wrong");
    a.h.settle();
    a.h.get_by_role(Role::Button, "Unlock").click();
    a.wait(|a| a.h.has_text("Wrong master password"), "a wrong one said");
    a.h.get_by_test_id("vault-password").set_value("master");
    a.h.settle();
    a.h.get_by_role(Role::Button, "Unlock").click();
    // Only the tab shown is mounted: the new one.
    let tabs = |a: &App| a.h.query(|n| n.semantics.role == Role::Tab).len();
    a.wait(|a| tabs(a) == 3 && a.welcomed() == 1, "the second signed in");
    assert!(!a.h.has_text("Sign in") && !a.h.has_text("Unlock saved passwords"));

    a.menu("box", "Forget password");
    assert!(a.secrets().is_empty());
}

#[test]
fn a_group_connects_all_and_removing_the_master_password_forgets_secrets() {
    let mut a = app("vault-group");
    a.master("", "master");
    a.host("one", "lab");
    a.host("two", "lab");
    let tabs = a.h.query(|n| n.semantics.role == Role::Tab).len();
    a.menu("lab", "Connect all (2)");
    assert_eq!(a.h.query(|n| n.semantics.role == Role::Tab).len(), tabs + 2);
    a.wait(|a| a.h.has_text("Sign in"), "no password saved: the dialog asks");

    a.settings.value("ssh.secrets", Vec::<Vec<String>>::new()).set(vec![vec!["one".into(), "sv1:00".into()]]);
    a.master("master", "");
    assert!(a.secrets().is_empty() && a.model.lock().key.get_untracked().is_none());
}

#[test]
fn a_passphrase_is_saved_by_the_master_password_typed_in_the_dialog() {
    let mut a = app("vault-passphrase");
    a.master("", "master");
    let file = a.dir.join("id");
    write_locked_user_key(&file, "open sesame");
    a.host_with("keyed", "", Auth::Key(file));
    // As after a start: the dialog asks the master password with the passphrase.
    a.model.lock().key.set(None);
    let edit = |a: &mut App, master: &str| {
        a.menu("keyed", "Edit\u{2026}");
        a.h.get_by_test_id("host-secret").set_value("open sesame");
        a.h.settle(); // the master password's field shows once a secret is typed
        a.h.get_by_test_id("host-master").set_value(master);
        a.h.settle();
        a.h.get_by_role(Role::Button, "Save").click();
        a.h.settle();
    };
    edit(&mut a, "wrong");
    a.wait(|a| a.h.has_text("Wrong master password: the password was not saved."), "the card");
    assert!(a.secrets().is_empty());
    edit(&mut a, "master");
    a.wait(|a| a.secrets().len() == 1, "the passphrase saved");
    a.connect("keyed");
    a.wait(|a| a.welcomed() == 1, "signed in by the saved passphrase");
    assert!(!a.h.has_text("Sign in") && !a.h.has_text("Unlock saved passwords"));
}

#[test]
fn a_startup_command_is_written_once_the_shell_opens() {
    let mut a = app("vault-startup");
    let file = a.dir.join("id");
    write_user_key(&file);
    a.host_with("tmuxed", "", Auth::Key(file.clone()));
    // A command of the settings' list, chosen by its name.
    let commands = a.settings.value("ssh.startup_commands", Vec::<Vec<String>>::new());
    commands.set(vec![vec!["Attach".to_string(), "tmux attach".to_string()]]);
    let choose = |a: &mut App, name: &str| {
        a.menu("tmuxed", "Edit\u{2026}");
        a.h.get_by_test_id("host-startup").click();
        a.h.settle();
        a.h.get_by_role(Role::MenuItem, name).click();
        a.h.settle();
        a.h.get_by_role(Role::Button, "Save").click();
        a.h.settle();
    };
    choose(&mut a, "Attach");
    let kept = a.settings.value("ssh.startup", Vec::<Vec<String>>::new()).get_untracked();
    assert_eq!(kept, [["tmuxed", "Attach"]]);
    a.connect("tmuxed");
    let said = |a: &App, words: &str| {
        let holds = |n: &quadra_tree::SemanticNode| {
            n.semantics.role == Role::TextInput
                && n.semantics.value.as_deref().is_some_and(|it| it.contains(words))
        };
        a.h.query(holds).len()
    };
    let tabs = |a: &App| a.h.query(|n| n.semantics.role == Role::Tab).len();
    a.wait(|a| said(a, "echo:tmux attach") == 1, "the command written");
    // Duplicate writes it too; once None is chosen, nothing is written.
    a.model.duplicate(a.model.active().get_untracked());
    a.wait(|a| tabs(a) == 3 && said(a, "echo:tmux attach") == 1, "the duplicate's");
    // A command added since the dialog was last open is offered the next time.
    commands.update(|rows| rows.push(vec!["Top".to_string(), "htop".to_string()]));
    choose(&mut a, "Top");
    assert_eq!(a.settings.value("ssh.startup", Vec::<Vec<String>>::new()).get_untracked()[0][1], "Top");
    choose(&mut a, "None");
    assert!(a.settings.value("ssh.startup", Vec::<Vec<String>>::new()).get_untracked().is_empty());
    a.connect("tmuxed");
    a.wait(|a| tabs(a) == 4 && a.welcomed() == 1, "connected without it");
    assert_eq!(said(&a, "echo:"), 0);
}

#[test]
fn without_a_master_password_the_dialog_sets_one_and_a_key_is_chosen() {
    let mut a = app("vault-new-master");
    let file = a.dir.join("id");
    write_locked_user_key(&file, "open sesame");
    a.host_with("keyed", "", Auth::Key(std::path::PathBuf::from("typed")));
    // The key chosen by name from the Keys of the settings.
    a.model.prefs().ssh_keys.set(vec![file.display().to_string()]);
    a.menu("keyed", "Edit\u{2026}");
    a.h.get_by_test_id("host-key").click();
    a.h.settle();
    a.h.get_by_role(Role::MenuItem, &file.display().to_string().replace('\\', "/")).click();
    a.h.settle();
    a.h.get_by_test_id("host-secret").set_value("open sesame");
    a.h.settle();
    assert!(a.h.has_text("New master password") && a.h.has_text("Repeat it"), "{}", a.h.semantics());
    a.h.get_by_test_id("host-new-master").set_value("master");
    a.h.get_by_test_id("host-master-again").set_value("master");
    a.h.settle();
    a.h.get_by_role(Role::Button, "Save").click();
    a.h.settle();
    a.wait(|a| a.secrets().len() == 1, "the passphrase saved, the master password set");
    assert!(a.model.lock().can_lock());
    let saved = a.model.hosts().into_iter().find(|it| it.name == "keyed").unwrap();
    assert_eq!(saved.remote.auth, Auth::Key(file.clone()), "the key file chosen");
    a.connect("keyed");
    a.wait(|a| a.welcomed() == 1, "signed in by the saved passphrase");
}
