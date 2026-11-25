//! A saved host's Connect dialog (specs/oneterm, "The rail"): `Connect to <name>`, what is saved
//! in fields to change for this connection; the tab, or the space, opens only once the shell
//! is open, a failure is an alert in the dialog.

mod connecting;
mod form;
mod steps;
mod username;

use std::path::PathBuf;
use std::rc::Rc;

use connecting::{connect, watch};
use oneterm_ssh::Ask;
use quadra::{AnyView, NodeRef, Signal, View, div, dynamic, effect, signal, untrack};
use quadra_design::Variant;

use crate::hosts::Host;
use crate::model::{Model, Space};
use crate::remote::{Conn, Phase};

const AUTHS: [&str; 4] = ["Auto", "SSH agent", "Key file", "Password"];
/// The field the focus starts in, the first missing.
const USER: u8 = 1;
const KEY: u8 = 2;
const PASSWORD: u8 = 3;

type Go = Rc<dyn Fn()>;

/// The saved host whose dialog shows, and the space connecting from it.
#[derive(Clone, Copy)]
pub struct Opening {
    /// Its name, and whether it opens a space beside the focused one.
    pub host: Signal<Option<(String, bool)>>,
    /// Placed once its shell opens, ended when it fails or is given up.
    pub space: Signal<Option<Space>>,
    /// The dialog to show again once the host editor opened from it closes.
    pub back: Signal<Option<(String, bool)>>,
    /// The known hosts the dialog says a host's key is in (the user's own; a test's).
    pub known_hosts: Signal<PathBuf>,
}

impl Opening {
    pub(crate) fn new() -> Opening {
        let home = std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default();
        let known_hosts = signal(home.join(".ssh").join("known_hosts"));
        Opening { host: signal(None), space: signal(None), back: signal(None), known_hosts }
    }
}

#[derive(Clone, Copy)]
struct Form {
    user: Signal<String>,
    auth: Signal<usize>,
    /// The auth the host has saved.
    saved_auth: Signal<usize>,
    key: Signal<String>,
    password: Signal<String>,
    /// Why the last try failed, for the alert.
    failed: Signal<String>,
    /// The password was refused: the focus goes to it.
    refused: Signal<bool>,
    /// What worked is saved to the host; a space in this tab.
    save: Signal<bool>,
    here: Signal<bool>,
    /// The answer to what the server asks while connecting.
    answer: Signal<String>,
}

impl Form {
    fn fill(&self, host: &Host, here: bool) {
        let (auth, key) = match &host.remote.auth {
            oneterm_ssh::Auth::Agent => (1, String::new()),
            oneterm_ssh::Auth::Key(path) => (2, path.display().to_string()),
            oneterm_ssh::Auth::Password => (3, String::new()),
            oneterm_ssh::Auth::Auto => (0, String::new()),
        };
        self.auth.set(auth);
        self.saved_auth.set(auth);
        self.key.set(key);
        self.failed.set(String::new());
        self.user.set(host.remote.target.user.clone());
        self.save.set(false);
        self.here.set(here);
        crate::vault::wipe(self.password);
        crate::vault::wipe(self.answer);
    }
}

pub(crate) fn dialog(model: &Model) -> impl View + use<> {
    let (opening, open) = (model.ui().opening, signal(false));
    let (s, b) = (|| signal(String::new()), || signal(false));
    let form = Form {
        user: s(),
        auth: signal(0),
        saved_auth: signal(0),
        key: s(),
        password: s(),
        failed: s(),
        refused: b(),
        save: b(),
        here: b(),
        answer: s(),
    };
    watch(model, form);
    effect(move || {
        let has = opening.host.with(Option::is_some);
        if open.get_untracked() != has {
            open.set(has);
        }
    });
    // Given up (Cancel, the cross, Escape): the connecting too.
    effect(move || {
        if open.get() {
            return;
        }
        untrack(|| {
            if opening.host.with(Option::is_some) {
                opening.host.set(None);
            }
            give_up(opening.space);
        });
    });
    let model = model.clone();
    dynamic(move || {
        let Some((name, here)) = opening.host.get() else { return AnyView::new(div()) };
        let Some(host) = untrack(|| model.hosts()).into_iter().find(|it| it.name == name) else {
            return AnyView::new(div());
        };
        AnyView::new(untrack(|| shown(&model, host, here, open, form)))
    })
}

/// Ends the space connecting, if one is.
fn give_up(space: Signal<Option<Space>>) {
    if let Some(it) = space.get_untracked() {
        space.set(None);
        crate::split::end(it);
    }
}

/// The connection of the space connecting.
fn conn_of(space: Signal<Option<Space>>) -> Option<Rc<Conn>> {
    space.with(|it| it.as_ref().and_then(|it| it.session.kept::<Conn>()))
}

/// The dialog of `host`: its form, or its steps while it connects.
fn shown(model: &Model, host: Host, here: bool, open: Signal<bool>, form: Form) -> impl View + use<> {
    let opening = model.ui().opening;
    form.fill(&host, here);
    let name = host.name.clone();
    let saved = model.sealed(&name).is_some();
    let target = &host.remote.target;
    let known = oneterm_ssh::known_key(&target.host, target.port, &opening.known_hosts.get_untracked());
    let missing = match form.auth.get_untracked() {
        _ if target.user.is_empty() => USER,
        2 if form.key.with_untracked(String::is_empty) => KEY,
        3 if !saved => PASSWORD,
        _ => 0,
    };
    let (goes, submits, tag) = (model.clone(), model.clone(), model.host_tag(&host).1);
    let named = name.clone();
    let go: Go = Rc::new(move || connect(&submits, &named, form));
    let space = opening.space;
    let busy = move || space.with(Option::is_some);
    // What the server asks: a key to trust takes yes, the rest what was typed.
    let answer: Go = Rc::new(move || {
        let Some(conn) = untrack(|| conn_of(space)) else { return };
        let typed = form.answer.get_untracked();
        crate::vault::wipe(form.answer);
        let key = conn.phase.with_untracked(|it| matches!(it, Phase::Ask(Ask::HostKey { .. })));
        conn.answer(Some(if key { "yes".into() } else { typed }));
    });
    let answers = answer.clone();
    let asking = move || {
        conn_of(space).is_some_and(|conn| {
            conn.phase.try_with(|it| matches!(it, Phase::Ask(ask) if !matches!(ask, Ask::Failed { .. })))
                == Some(true)
        })
    };
    let asking_now = asking;
    let (host_view, known_now, secret_at) = (host.clone(), known.clone(), NodeRef::new());
    let keyed = model.clone();
    let body = move || {
        let model = keyed.clone();
        let (host, known, go, answer) = (host_view.clone(), known_now.clone(), go.clone(), answer.clone());
        let secret_at = secret_at.clone();
        dynamic(move || match space.get() {
            Some(it) => {
                let Some(conn) = it.session.kept::<Conn>() else { return AnyView::new(div()) };
                let last = it.remote.as_ref().map_or_else(String::new, |it| it.target.label());
                let (jump, name) = (host.jump.clone(), host.name.clone());
                AnyView::new(steps::view(conn, last, jump, known.is_some(), form, name, answer.clone()))
            }
            None => {
                let missing = if untrack(|| form.refused.get()) { PASSWORD } else { missing };
                untrack(|| form.refused.set(false));
                AnyView::new(form::body(&model, &host, form, missing, &known, go.clone(), &secret_at))
            }
        })
    };
    let title = format!("Connect to {name}");
    let (said, group) = (title.clone(), host.group.clone());
    let (editing, back, edited) = (model.ui().editing, opening.back, name.clone());
    quadra_design::dialog(open, title, body)
        .head(move || form::head(said.clone(), tag, group.clone()))
        .aside("Edit host\u{2026}", move || {
            back.set(Some((edited.clone(), form.here.get_untracked())));
            editing.set(Some(edited.clone()));
        })
        .shown(move || !busy())
        .action("Cancel", Variant::Secondary, move || match busy() {
            true => give_up(space),
            false => open.set(false),
        })
        .stays_open()
        .action("Continue", Variant::Primary, move || answers())
        .stays_open()
        .shown(asking)
        .action("Connect", Variant::Primary, move || connect(&goes, &name, form))
        .shortcut("Enter")
        .stays_open()
        .disabled(move || form.user.with(|it| it.trim().is_empty()))
        .busy("Connecting\u{2026}", busy)
        .shown(move || !asking_now())
}
