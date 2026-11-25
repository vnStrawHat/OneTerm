//! The key files hosts sign in with (specs/oneterm, "Keys"): kept on the SSH & hosts page with
//! their fingerprints, and chosen by name in the Connect dialog, which signs in with the one
//! chosen. A real connection to the engine's test server on 127.0.0.1; every file is under
//! `.tmp`.

use std::rc::Rc;
use std::time::{Duration, Instant};

use oneterm::{Host, Model, NewSession, OneTerm, Remote, keymap, remote_shell_with};
use oneterm_ssh::testing::{folder, options, serve, trust, write_user_key};
use oneterm_ssh::{Auth, Target};
use quadra::{Role, Settings, provide};
use quadra_term::echo;
use quadra_testing::Harness;

struct App {
    h: Harness,
    model: Model,
    port: u16,
    dir: std::path::PathBuf,
}

fn app(name: &str) -> App {
    let dir = folder(name);
    std::fs::create_dir_all(dir.join("keys")).unwrap();
    let (port, _) = serve(61);
    trust(port, 61, &dir.join("known_hosts"));
    let base = dir.clone();
    let sessions: NewSession = Rc::new(move |launch: oneterm::Launch| match launch.ssh {
        Some(_) => remote_shell_with(launch, |target| {
            let mut o = options(target.port, &base, Auth::Auto);
            o.target = target;
            o
        }),
        None => echo(),
    });
    let mut h = Harness::new();
    h.size = quadra::Size::new(1400.0, 800.0);
    h.tree.set_keymap(Rc::new(keymap()));
    let kept = Rc::new(std::cell::RefCell::new(None));
    let made = kept.clone();
    h.mount(move || {
        provide(Settings::memory());
        let app = OneTerm::new(sessions);
        *made.borrow_mut() = Some(app.model.clone());
        app.view()
    });
    h.settle();
    let model = kept.borrow().clone().unwrap();
    model.ui().opening.known_hosts.set(dir.join("known_hosts"));
    App { h, model, port, dir }
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

    fn says(&self, words: &str) -> bool {
        !self.h.query(|n| n.semantics.name.contains(words)).is_empty()
    }

    /// Picks `item` of the select of a test id.
    fn pick(&mut self, id: &str, item: &str) {
        self.h.get_by_test_id(id).click();
        self.h.settle();
        self.h.get_by_role(Role::MenuItem, item).click();
        self.h.settle();
    }
}

#[test]
fn a_key_is_kept_in_the_settings_and_chosen_by_name_in_the_connect_dialog() {
    let mut a = app("ssh-keys");
    let file = a.dir.join("work_key");
    write_user_key(&file);
    a.model.prefs().ssh_keys.set(vec![file.display().to_string(), a.dir.join("gone").display().to_string()]);
    // The page lists each key: its name, its kind and fingerprint, or why it cannot be read.
    a.h.get_by_role(Role::Button, "Settings").click();
    a.h.settle();
    a.h.get_by_role(Role::TreeItem, "SSH & hosts").click();
    a.h.settle();
    assert!(a.says("work_key") && a.says("ED25519 SHA256:"), "{}", a.h.semantics());
    assert!(a.says("Not found"), "a key file gone says so");
    let removes = a.h.query(|n| n.semantics.role == Role::Button && n.semantics.name == "Remove");
    assert_eq!(removes.len(), 2);
    a.h.tree.act(removes[1], &quadra::SemanticAction::Click);
    a.h.settle();
    assert_eq!(a.model.prefs().ssh_keys.get_untracked().len(), 1, "Remove forgets it");
    a.model.ui().settings.set(false);
    a.h.settle();

    // A host saved with a password: its dialog changes to Key file and picks the key by name.
    let remote = Remote::new(Target::parse(&format!("tester@127.0.0.1:{}", a.port)).unwrap(), Auth::Password);
    a.model.save_host(Host { name: "box".into(), group: String::new(), remote, jump: String::new() });
    a.model.ui().opening.host.set(Some(("box".into(), false)));
    a.h.settle();
    a.pick("connect-auth", "Key file");
    assert!(a.says("Choose a key"), "{}", a.h.semantics());
    let name = a.model.prefs().ssh_keys.get_untracked()[0].clone();
    let label = name.replace('\\', "/");
    let label = match std::env::var("USERPROFILE") {
        Ok(home) if !home.is_empty() && name.starts_with(&home) => format!("~{}", &label[home.len()..]),
        _ => label,
    };
    a.pick("connect-key", &label);
    let tabs = a.h.query(|n| n.semantics.role == Role::Tab).len();
    a.h.get_by_role(Role::Button, "Connect").click();
    a.h.settle();
    a.wait(|a| a.h.query(|n| n.semantics.role == Role::Tab).len() == tabs + 1, "signed in with the key");
}
