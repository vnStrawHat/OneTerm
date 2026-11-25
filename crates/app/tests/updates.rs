//! Updates (specs/oneterm, "Updates"): the About page's row from the check to the files in place,
//! and the card a start shows, against a server of the test on 127.0.0.1 that answers as
//! GitHub does. OneTerm's folder here is the test's, under `.tmp`; Restart is not pressed.
#![cfg(windows)]

use std::path::PathBuf;
use std::rc::Rc;

use oneterm::{Model, NewSession, OneTerm, Update};
use quadra::{Role, Settings, provide};
use quadra_term::echo;
use quadra_testing::Harness;
use quadra_updater::testing::{package, release, serve};

const TARGET: &str = "x86_64-pc-windows-msvc";

struct App {
    h: Harness,
    model: Model,
    root: PathBuf,
}

/// A window whose updates ask `api`, with OneTerm's folder and the cache under `.tmp/<name>`.
fn app(name: &str) -> App {
    let root = std::path::absolute(".tmp/updates").unwrap().join(name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("home")).unwrap();
    std::fs::write(root.join("home/oneterm.exe"), b"the version before").unwrap();
    let mut h = Harness::new();
    h.size = quadra::Size::new(1200.0, 700.0);
    let (settings, sessions): (_, NewSession) = (Settings::memory(), Rc::new(|_| echo()));
    let slot = Rc::new(std::cell::RefCell::new(None));
    let out = slot.clone();
    h.mount(move || {
        provide(settings.clone());
        let app = OneTerm::new(sessions);
        *out.borrow_mut() = Some(app.model.clone());
        app.view()
    });
    h.settle();
    let model = slot.borrow_mut().take().unwrap();
    App { h, model, root }
}

impl App {
    /// Points the updates at a server, as version 0.7.0 in the test's folders.
    fn asks(&self, api: &str) {
        self.model.ui().updates.source.update(|it| {
            (it.api, it.version) = (api.into(), "0.7.0".into());
            (it.home, it.cache) = (self.root.join("home"), self.root.join("cache"));
        });
    }

    fn wait(&mut self, what: &str) {
        let end = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !self.h.has_text(what) {
            assert!(std::time::Instant::now() < end, "never: {what}\n{}", self.h.semantics());
            self.h.advance(std::time::Duration::from_millis(10));
            self.h.settle();
        }
    }

    fn press(&mut self, button: &str) {
        self.h.get_by_role(Role::Button, button).click();
        self.h.settle();
    }
}

#[test]
fn the_about_page_checks_installs_and_offers_the_restart() {
    let mut a = app("page");
    let files: [(&str, &[u8]); 2] = [("oneterm.exe", b"the new version"), ("conpty.dll", b"host")];
    let (name, zip) = package("oneterm", "0.8.0", TARGET, &files);
    let (at, served) = serve(|at| {
        vec![
            ("500 Oops", b"down".to_vec()),
            ("200 OK", release("v0.7.0", &[(&name, Some(&zip))], at)),
            ("200 OK", release("v0.8.0", &[(&name, Some(&zip))], at)),
            ("200 OK", zip.clone()),
        ]
    });
    a.asks(&at);
    a.press("Settings");
    a.h.get_by_role(Role::TreeItem, "About").click();
    a.h.settle();
    assert!(a.h.has_text("Updates") && a.h.has_text("Check at start"));

    a.press("Check for updates");
    a.wait("GitHub could not be reached. Check the network connection.");
    a.press("Check for updates");
    a.wait("OneTerm is up to date.");
    a.press("Check for updates");
    a.wait("Version 0.8.0 is available.");
    a.press("Install");
    a.wait("Version 0.8.0 is installed.");
    assert_eq!(a.model.ui().updates.state.get_untracked(), Update::Installed("0.8.0".into()));
    // The files are in OneTerm's folder, the ones before moved aside; nothing restarted.
    let home = a.root.join("home");
    assert_eq!(std::fs::read(home.join("oneterm.exe")).unwrap(), b"the new version");
    assert_eq!(std::fs::read(home.join("oneterm.exe.quadra-old")).unwrap(), b"the version before");
    assert_eq!(std::fs::read(home.join("conpty.dll")).unwrap(), b"host");
    let restarts = a.h.query(|n| n.semantics.role == Role::Button && n.semantics.name == "Restart OneTerm");
    assert_eq!(restarts.len(), 1);
    assert!(!a.model.ui().updates.closing.get_untracked());
    let asked = served.join().unwrap();
    assert!(asked[0].starts_with("GET /repos/vnStrawHat/OneTerm/releases/latest "), "{}", asked[0]);
    assert!(asked[3].starts_with(&format!("GET /get/{name} ")), "{}", asked[3]);
}

#[test]
fn a_start_shows_a_card_for_a_newer_version_and_says_nothing_otherwise() {
    let mut a = app("start");
    let name = format!("oneterm-0.8.0-{TARGET}.zip");
    let (at, served) = serve(|at| {
        vec![
            ("200 OK", release("v0.7.0", &[(&name, Some(b"zip"))], at)),
            ("200 OK", release("v0.8.0", &[(&name, Some(b"zip"))], at)),
        ]
    });
    a.asks(&at);
    let state = a.model.ui().updates.state;
    a.model.check_for_updates_at_start();
    let end = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while state.get_untracked() != Update::Latest {
        assert!(std::time::Instant::now() < end, "never checked");
        a.h.advance(std::time::Duration::from_millis(10));
        a.h.settle();
    }
    assert!(!a.h.has_text("is available"));
    a.model.check_for_updates_at_start();
    a.wait("OneTerm 0.8.0 is available");
    assert!(a.h.has_text("Install it from Settings, About."));
    // Turned off, a start asks nothing: the server would have no answer left.
    a.model.prefs().update_check.set(false);
    a.model.check_for_updates_at_start();
    a.h.settle();
    assert!(matches!(state.get_untracked(), Update::Found(_)));
    served.join().unwrap();
}

#[test]
fn the_updater_is_given_the_proxy_and_the_certificates_the_advanced_page_says() {
    let mut a = app("proxy");
    let first = a.model.ui().updates.updater().network;
    assert_eq!((first.proxy, first.verify_tls), (None, true), "the system's proxy, checked");
    a.press("Settings");
    a.h.get_by_role(Role::TreeItem, "Advanced").click();
    a.h.settle();
    assert!(a.h.has_text("Network") && !a.h.has_text("Not checked"), "{}", a.h.semantics());
    a.h.get_by_role(Role::Radio, "Manual").click();
    a.h.settle();
    a.h.get_by_role(Role::TextInput, "Proxy address").type_text("proxy.corp:3128");
    a.h.settle();
    a.h.get_by_role(Role::Switch, "Verify certificates").click();
    a.h.settle();
    assert!(a.h.has_text("Not checked"), "said while certificates are not checked");
    // What the next check or fetch is made with (that a proxy named so is the one asked:
    // quadra-shell's tests/fetch.rs).
    let now = a.model.ui().updates.updater().network;
    assert_eq!((now.proxy.as_deref(), now.verify_tls), (Some("proxy.corp:3128"), false));
    a.h.get_by_role(Role::Radio, "System").click();
    a.h.settle();
    assert_eq!(a.model.ui().updates.updater().network.proxy, None);
}
