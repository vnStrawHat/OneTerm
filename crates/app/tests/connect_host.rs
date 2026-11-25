//! A saved host's Connect dialog (specs/oneterm, "The rail") on real connections to the engine's
//! test server on 127.0.0.1 (never another host): the focus in what is missing, Connecting…
//! while it connects, a refused password and a failure as alerts with no tab, the user's
//! `user@host:port` in the saved target's place, a new key trusted where its fingerprint
//! shows. Every file is under `.tmp`.

use std::rc::Rc;
use std::time::{Duration, Instant};

use oneterm::{Host, Model, NewSession, OneTerm, Remote, keymap, remote_shell_with};
use oneterm_ssh::testing::{folder, options, serve, trust};
use oneterm_ssh::{Auth, Target};
use quadra::{Key, Mods, Role, Settings, provide};
use quadra_term::echo;
use quadra_testing::Harness;

struct App {
    h: Harness,
    model: Model,
    port: u16,
}

fn app(name: &str) -> App {
    let dir = folder(name);
    std::fs::create_dir_all(dir.join("keys")).unwrap();
    let (port, _) = serve(51);
    trust(port, 51, &dir.join("known_hosts"));
    let known = dir.join("known_hosts");
    let sessions: NewSession = Rc::new(move |launch: oneterm::Launch| match launch.ssh {
        Some(_) => remote_shell_with(launch, |target| {
            let mut o = options(target.port, &dir, Auth::Auto);
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
    model.ui().opening.known_hosts.set(known);
    App { h, model, port }
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

    fn save(&mut self, name: &str, target: &str) {
        let remote = Remote::new(Target::parse(target).unwrap(), Auth::Password);
        self.model.save_host(Host { name: name.into(), group: String::new(), remote, jump: String::new() });
        self.h.settle();
    }

    fn click(&mut self, name: &str) {
        let row = self.h.get_by_role(Role::Button, name).node();
        let at = self.h.tree.window_rect(row).center();
        self.h.click(at.x, at.y);
        self.h.settle();
        self.h.advance(Duration::from_millis(20));
        self.h.settle();
    }

    fn tabs(&self) -> usize {
        self.h.query(|n| n.semantics.role == Role::Tab).len()
    }

    /// Whether the field of a test id has the focus.
    fn focused(&self, id: &str) -> bool {
        !self.h.query(|n| n.focused && n.semantics.test_id.as_deref() == Some(id)).is_empty()
    }

    fn welcomed(&self) -> bool {
        let says = |n: &quadra_tree::SemanticNode| {
            n.semantics.value.as_deref().is_some_and(|it| it.contains("welcome"))
        };
        !self.h.query(says).is_empty()
    }

    fn press_connect(&mut self) {
        self.h.get_by_role(Role::Button, "Connect").click();
        self.h.settle();
    }
}

#[test]
fn the_focus_is_in_what_is_missing_and_the_tab_opens_once_connected() {
    let mut a = app("connect-dialog");
    a.save("box", &format!("127.0.0.1:{}", a.port));
    let tabs = a.tabs();
    a.click("box");
    assert!(a.h.has_text("Connect to box") && a.h.has_text("127.0.0.1"));
    assert!(a.h.has_text(&format!("port {}", a.port)) && a.h.has_text("Known"), "{}", a.h.semantics());
    assert!(a.focused("connect-user"), "no user saved\n{}", a.h.semantics());
    // A user, a wrong password: Connecting… and off while it connects, then refused, no tab.
    a.h.get_by_test_id("connect-user").set_value("tester");
    a.h.get_by_test_id("connect-password").set_value("wrong");
    a.h.settle();
    a.press_connect();
    let off = a.h.query(|n| n.semantics.name == "Connecting\u{2026}" && n.semantics.disabled);
    assert_eq!(off.len(), 1, "{}", a.h.semantics());
    assert!(a.h.has_text("Reach host") && a.h.has_text("Authenticate"), "the steps\n{}", a.h.semantics());
    a.wait(|a| a.h.has_text("Permission denied: the password was refused."), "refused");
    assert!(a.h.has_text("Could not connect") && a.h.has_text("Connect to box"));
    assert_eq!(a.tabs(), tabs, "no tab for a failure");
    assert!(a.focused("connect-password"), "{}", a.h.semantics());
    // The right one, by Enter: the tab once the shell is open, the dialog gone.
    a.h.get_by_test_id("connect-password").set_value("secret");
    a.h.key(Key::Enter, Mods::NONE);
    a.wait(|a| a.tabs() == tabs + 1 && a.welcomed() && !a.h.has_text("Connect to box"), "connected");
}

#[test]
fn the_username_s_host_and_port_take_the_saved_ones_place_and_a_failure_is_an_alert() {
    let mut a = app("connect-far");
    let nobody = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead = nobody.local_addr().unwrap().port();
    drop(nobody);
    a.save("far", &format!("tester@127.0.0.1:{dead}"));
    let tabs = a.tabs();
    a.click("far");
    assert!(a.focused("connect-password"), "Password with none saved\n{}", a.h.semantics());
    // A key not in the known hosts is said, and nothing is ticked to trust what is not seen.
    assert!(a.h.has_text("New") && a.h.has_text("Not in known hosts"), "{}", a.h.semantics());
    assert!(a.h.query(|n| n.semantics.name == "Connect" && n.semantics.disabled).is_empty());
    a.press_connect();
    a.wait(
        |a| a.h.has_text("Could not connect") && !a.h.query(|n| n.semantics.name == "Connect").is_empty(),
        "the failure",
    );
    assert_eq!(a.tabs(), tabs);
    a.h.get_by_test_id("connect-user").set_value(&format!("tester@127.0.0.1:{}", a.port));
    a.h.get_by_test_id("connect-password").set_value("secret");
    a.h.settle();
    a.press_connect();
    a.wait(|a| a.tabs() == tabs + 1 && a.welcomed(), "connected to the port typed");
    let saved = a.model.hosts().into_iter().find(|it| it.name == "far").unwrap();
    assert_eq!(saved.remote.target.port, dead, "the saved host stays as it was");
}

#[test]
fn edit_host_returns_to_the_dialog_and_save_keeps_what_worked() {
    let mut a = app("connect-save");
    a.save("box", &format!("127.0.0.1:{}", a.port));
    a.click("box");
    assert!(a.h.query(|n| n.semantics.name == "Connect" && n.semantics.disabled).len() == 1, "no user");
    // Edit host… opens the editor; Cancel shows the Connect dialog again.
    a.h.get_by_role(Role::Button, "Edit host\u{2026}").click();
    a.h.settle();
    assert!(a.h.has_text("Edit box") && !a.h.has_text("Connect to box"), "{}", a.h.semantics());
    a.h.get_by_role(Role::Button, "Cancel").click();
    a.h.settle();
    a.h.advance(Duration::from_millis(20));
    a.h.settle();
    assert!(a.h.has_text("Connect to box"), "back\n{}", a.h.semantics());
    // A user and the password, saved to the host once they worked.
    a.h.get_by_test_id("connect-user").set_value("tester");
    a.h.get_by_test_id("connect-password").set_value("secret");
    a.h.get_by_test_id("connect-save").click();
    a.h.settle();
    let tabs = a.tabs();
    a.press_connect();
    a.wait(|a| a.tabs() == tabs + 1 && a.welcomed(), "connected");
    let saved = a.model.hosts().into_iter().find(|it| it.name == "box").unwrap();
    assert_eq!(saved.remote.target.user, "tester");
}

#[test]
fn a_new_host_key_is_trusted_where_its_fingerprint_shows() {
    let mut a = app("connect-new-key");
    // A second server, whose key the known hosts do not have.
    let (other, _) = serve(52);
    let known = folder("connect-new-key").join("known_hosts");
    let has = |port: u16| std::fs::read_to_string(&known).unwrap_or_default().contains(&port.to_string());
    a.save("fresh", &format!("tester@127.0.0.1:{other}"));
    let tabs = a.tabs();
    for trusts in [false, true] {
        a.click("fresh");
        a.h.get_by_test_id("connect-password").set_value("secret");
        a.h.settle();
        a.press_connect();
        // The step says whose key it is and shows its fingerprint; nothing is written yet.
        a.wait(
            |a| a.h.has_text("Continue") && !a.h.query(|n| n.semantics.name.contains("SHA256:")).is_empty(),
            "the key",
        );
        let whose = format!("the key of [127.0.0.1]:{other}");
        assert!(!a.h.query(|n| n.semantics.name.contains(&whose)).is_empty(), "{}", a.h.semantics());
        assert!(!has(other), "not trusted before it is seen");
        // Enter, the key that pressed Connect, does not answer for the user here.
        a.h.key(Key::Enter, Mods::NONE);
        for _ in 0..20 {
            a.h.advance(Duration::from_millis(10));
            a.h.settle();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            !has(other) && a.h.has_text("Continue"),
            "Enter trusted it
{}",
            a.h.semantics()
        );
        let answer = if trusts { "Continue" } else { "Cancel" };
        a.h.get_by_role(Role::Button, answer).click();
        a.h.settle();
        if !trusts {
            // Given up: no tab, and the known hosts as they were.
            a.wait(|a| !a.h.has_text("Continue"), "given up");
            assert!(!has(other) && a.tabs() == tabs, "{}", a.h.semantics());
            if a.h.has_text("Connect to fresh") {
                a.h.get_by_role(Role::Button, "Cancel").click();
                a.h.settle();
            }
        }
    }
    a.wait(|a| a.tabs() == tabs + 1 && a.welcomed(), "connected");
    assert!(has(other), "trusted by Continue");
}
