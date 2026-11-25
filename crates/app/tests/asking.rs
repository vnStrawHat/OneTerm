//! The Connect dialogs (specs/oneterm, "M7 in detail") on real connections to the engine's test
//! servers on 127.0.0.1 (never another host), started from Quick connect: an unknown host
//! trusted or cancelled, a wrong then a right password, a user not given, a changed key, a
//! port that never speaks cancelled, one nobody listens on. Every file is under `.tmp`; the
//! user's agent, keys and known hosts are never used.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use oneterm::{NewSession, OneTerm, keymap, remote_shell_with};
use oneterm_ssh::Auth;
use oneterm_ssh::testing::{folder, options, serve, trust, write_user_key};
use quadra::{Key, Mods, Role};
use quadra_term::{Session, echo};
use quadra_testing::Harness;

struct App {
    h: Harness,
    dir: PathBuf,
    sessions: Rc<RefCell<Vec<Session>>>,
}

/// The app, its remote sessions made with the files of `.tmp/ssh-tests/<name>`.
fn app(name: &str) -> App {
    let dir = folder(name);
    std::fs::create_dir_all(dir.join("keys")).unwrap();
    let (sessions, base) = (Rc::new(RefCell::new(Vec::new())), dir.clone());
    let kept = sessions.clone();
    let made: NewSession = Rc::new(move |launch: oneterm::Launch| match launch.ssh {
        Some(_) => {
            let session = remote_shell_with(launch, |target| {
                let mut o = options(target.port, &base, Auth::Auto);
                o.target = target;
                o
            });
            kept.borrow_mut().push(session.clone());
            session
        }
        None => echo(),
    });
    let mut h = Harness::new();
    h.size = quadra::Size::new(1400.0, 800.0);
    h.tree.set_keymap(Rc::new(keymap()));
    h.mount(move || OneTerm::new(made).view());
    h.settle();
    App { h, dir, sessions }
}

impl App {
    /// Quick connect to `target`.
    fn connect(&mut self, target: &str) {
        self.h.key(Key::K, Mods::CTRL | Mods::SHIFT);
        self.h.settle();
        self.h.get_by_test_id("connect-target").set_value(target);
        self.h.settle();
        self.h.get_by_role(Role::Button, "Connect").click();
        self.h.settle();
    }

    /// Settles until `done`, 20 s at most: the engine works on threads of its own.
    fn wait(&mut self, done: impl Fn(&App) -> bool, what: &str) {
        let end = Instant::now() + Duration::from_secs(20);
        loop {
            self.h.advance(Duration::from_millis(10));
            self.h.settle();
            if done(self) {
                return;
            }
            assert!(Instant::now() < end, "never: {what}\n{}\n{}", self.h.semantics(), self.screen());
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Whether a node's name holds `words`.
    fn shows(&self, words: &str) -> bool {
        !self.h.query(|n| n.semantics.name.contains(words)).is_empty()
    }

    fn wait_for(&mut self, words: &str) {
        self.wait(|a| a.shows(words), words);
    }

    fn press(&mut self, button: &str) {
        self.h.get_by_role(Role::Button, button).click();
        self.h.settle();
    }

    fn screen(&self) -> String {
        self.sessions.borrow().last().map_or_else(String::new, |s| s.screen_text().join("\n"))
    }

    /// Answers the Sign in dialog.
    fn sign_in(&mut self, typed: &str) {
        self.h.get_by_test_id("sign-in").set_value(typed);
        self.h.settle();
        self.press("Connect");
    }

    fn no_dialog(&self) -> bool {
        ["Connecting to", "Password for", "can't be established", "Could not connect"]
            .iter()
            .all(|it| !self.shows(it))
    }

    /// Closes a failure, and the space says why and ends with 255.
    fn close_failure(&mut self, why: &str) {
        // The dialog's own Close, after the cross of its title (also "Close").
        let closes = self.h.query(|n| n.semantics.role == Role::Button && n.semantics.name == "Close");
        assert!(self.h.tree.act(*closes.last().unwrap(), &quadra_tree::SemanticAction::Click));
        self.h.settle();
        let session = self.sessions.borrow().last().unwrap().clone();
        self.wait(|_| session.exited().get_untracked() == Some(255), "the end");
        let said = format!("[connection closed: {why}");
        self.wait(|a| a.screen().contains(&said) && a.no_dialog(), "the space says why");
    }
}

#[test]
fn an_unknown_host_is_trusted_and_a_wrong_password_asked_again() {
    let mut a = app("ask-unknown");
    let (port, _) = serve(31);
    a.connect(&format!("tester@127.0.0.1:{port}"));
    a.wait_for(&format!("The authenticity of host '[127.0.0.1]:{port}' can't be established."));
    let print = oneterm_ssh::testing::key(31).public_key().fingerprint(Default::default()).to_string();
    assert!(a.h.has_text("ssh-ed25519 key fingerprint:") && a.h.has_text(&print));
    a.press("Trust and connect");
    a.wait_for(&format!("Password for tester@127.0.0.1:{port}"));
    let known = std::fs::read_to_string(a.dir.join("known_hosts")).unwrap();
    assert!(known.contains(&format!("[127.0.0.1]:{port} ssh-ed25519 ")), "{known}");
    a.sign_in("wrong");
    a.wait_for("Permission denied, please try again.");
    // Enter in the field: answered once (a second answer would go to the next question).
    a.h.get_by_test_id("sign-in").set_value("secret").focus();
    a.h.key(Key::Enter, Mods::NONE);
    a.h.settle();
    // The dialog closes by itself once the shell is open, the focus in the space.
    a.wait(|a| a.screen().contains("welcome") && a.no_dialog(), "the shell");
    a.h.type_text("hi");
    a.wait(|a| a.screen().contains("echo:hi"), "typing reaches the shell");
}

#[test]
fn an_unknown_host_cancelled_is_not_trusted() {
    let mut a = app("ask-cancel");
    let (port, _) = serve(32);
    a.connect(&format!("tester@127.0.0.1:{port}"));
    a.wait_for("can't be established");
    // The focus is on Cancel: Enter never trusts a key.
    a.h.key(Key::Enter, Mods::NONE);
    a.h.settle();
    a.wait_for("Could not connect to tester@127.0.0.1:");
    assert!(a.h.has_text("Cancelled"));
    assert!(!a.dir.join("known_hosts").exists());
    a.close_failure("cancelled");
}

#[test]
fn a_user_not_given_is_asked() {
    let mut a = app("ask-user");
    let (port, _) = serve(33);
    trust(port, 33, &a.dir.join("known_hosts"));
    write_user_key(&a.dir.join("keys/id_ed25519"));
    a.connect(&format!("127.0.0.1:{port}"));
    a.wait_for(&format!("No user was given for 127.0.0.1:{port}."));
    let user = oneterm_ssh::os_user();
    let filled = |n: &quadra_tree::SemanticNode| {
        n.semantics.test_id.as_deref() == Some("sign-in")
            && n.semantics.value.as_deref() == Some(user.as_str())
    };
    assert_eq!(a.h.query(filled).len(), 1, "the OS's user filled in");
    a.sign_in("tester");
    a.wait(|a| a.screen().contains("welcome") && a.no_dialog(), "the shell");
    // The status bar: what crossed the network each second; the server's OSC 7 (echoed back
    // by the test server) is its directory.
    a.wait(|a| a.shows("\u{2193} ") && a.shows(" B/s \u{2191} "), "the network");
    let session = a.sessions.borrow().last().unwrap().clone();
    session.write(b"\x1b]7;file://srv/home/tester/src\x07");
    a.wait(|a| a.h.has_text("/home/tester/src"), "the remote directory");
    // The space's header says the user answered (a tab of two spaces has headers).
    a.h.key(Key::D, Mods::CTRL);
    let header = format!("tester@127.0.0.1:{port}:/home/tester/src");
    a.wait(|a| a.h.query(|n| n.semantics.name.starts_with(&header)).len() == 1, "the header");
    // Its words are a line of 16 from the header's top: one pixel is under it, over the line.
    let words = a.h.query(|n| n.semantics.name.starts_with(&header))[0];
    let bar = a.h.tree.parent(a.h.tree.parent(words).unwrap()).unwrap();
    let (words, bar) = (a.h.tree.window_rect(words), a.h.tree.window_rect(bar));
    assert_eq!((words.height, words.y - bar.y, bar.height), (16.0, 0.0, 18.0));
}

#[test]
fn a_changed_key_is_an_error_that_names_where_the_key_is() {
    let mut a = app("ask-changed");
    let (port, _) = serve(34);
    trust(port, 35, &a.dir.join("known_hosts"));
    a.connect(&format!("tester@127.0.0.1:{port}"));
    a.wait_for(&format!("The host key of '[127.0.0.1]:{port}' has changed"));
    assert!(a.shows("the key known is on line") && a.shows("known_hosts"));
    // Trying again ends the same way: Close is the answer offered first, at the right.
    let at = |a: &App, name: &str| {
        // The lowest of its name: the dialog's foot, not the window's Close or the cross.
        let found = a.h.query(|n| n.semantics.role == Role::Button && n.semantics.name == name);
        let rects = found.into_iter().map(|id| a.h.tree.window_rect(id));
        rects.max_by(|a, b| a.y.total_cmp(&b.y)).expect(name).x
    };
    assert!(at(&a, "Retry") < at(&a, "Close"));
    a.press("Retry");
    a.wait(|a| a.shows("Connecting to") || a.shows("has changed"), "connecting again");
    a.wait_for("has changed");
    a.close_failure("the host key of");
}

#[test]
fn connecting_is_shown_and_cancelled() {
    let mut a = app("ask-silent");
    // A port whose connections are queued and never answered.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    a.connect(&format!("tester@127.0.0.1:{port}"));
    a.wait_for(&format!("Connecting to tester@127.0.0.1:{port}\u{2026}"));
    a.press("Cancel");
    a.wait_for("Could not connect to");
    assert!(a.h.has_text("Cancelled"));
    a.close_failure("cancelled");
    drop(listener);
}

#[test]
fn nobody_listening_is_said() {
    let mut a = app("ask-refused");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    a.connect(&format!("nobody@127.0.0.1:{port}"));
    a.wait_for(&format!("Could not connect to nobody@127.0.0.1:{port}."));
    a.close_failure("");
}
