//! The dialogs that save a host (specs/oneterm, "M8 in detail"): Add SSH host and Edit SSH host,
//! with the password or passphrase saved sealed (`vault.rs`).

use oneterm_ssh::Target;
use quadra::{Signal, View, div, effect, signal, untrack};
use quadra_design::{Notice, Tone, Variant};

use crate::hosts::tag::{picked, word_of};
use crate::hosts::{Host, auth_of};
use crate::model::Model;
use crate::shells::Remote;

#[derive(Clone, Copy)]
pub(super) struct Form {
    pub(super) name: Signal<String>,
    pub(super) group: Signal<String>,
    /// The address (a target typed whole is taken too), the port, the user.
    pub(super) target: Signal<String>,
    pub(super) port: Signal<String>,
    pub(super) user: Signal<String>,
    pub(super) auth: Signal<usize>,
    pub(super) key: Signal<String>,
    pub(super) jump: Signal<usize>,
    pub(super) forwards: Signal<String>,
    pub(super) startup: Signal<String>,
    /// What the colour picker holds: `#rrggbb`, or empty for the group's.
    pub(super) tag: Signal<String>,
    /// The password or passphrase to save, and the master password that seals it while the
    /// data key is not in memory (or the new one, and again, while there is none).
    pub(super) secret: Signal<String>,
    pub(super) master: Signal<String>,
    pub(super) again: Signal<String>,
}

/// The dialogs that save a host: a new one, or one saved before, filled in.
pub(crate) fn add_dialog(model: &Model) -> impl View + use<> {
    let (open, editing, changing) = (model.ui().adding, model.ui().editing, signal(false));
    let s = || signal(String::new());
    let form = Form {
        name: s(),
        group: s(),
        target: s(),
        port: s(),
        user: s(),
        auth: signal(0),
        key: s(),
        jump: signal(0),
        forwards: s(),
        tag: s(),
        startup: s(),
        secret: s(),
        master: s(),
        again: s(),
    };
    let (saves, changes, fills, gone) = (model.clone(), model.clone(), model.clone(), model.clone());
    let confirming = signal(false);
    // Asked to edit a host, the form says what it is; closed, it is no longer being edited.
    effect(move || match editing.get() {
        Some(name) => {
            if let Some(host) = untrack(|| fills.hosts()).into_iter().find(|it| it.name == name) {
                untrack(|| fill(&fills, form, &host));
                changing.set(true);
            }
        }
        None => changing.set(false),
    });
    effect(move || {
        if changing.get() {
            untrack(|| confirming.set(false));
        }
    });
    // Closed, the Connect dialog it was opened from shows again.
    let back = model.ui().opening;
    effect(move || {
        if !changing.get() {
            editing.set(None);
            if let Some(it) = untrack(|| back.back.get()) {
                back.back.set(None);
                back.host.set(Some(it));
            }
        }
    });
    let (adds_body, edits_body) = (model.clone(), model.clone());
    let adds = quadra_design::dialog(open, "Add SSH host", move || {
        super::layout::body(&adds_body, form, None, confirming)
    })
    .head(move || super::layout::head(form, "Add SSH host".into()))
    .action("Cancel", Variant::Secondary, || {})
    .action("Add host", Variant::Primary, move || save(&saves, form, None))
    .shortcut("Enter");
    let back = model.ui().opening.back;
    let not = move || !confirming.get();
    let edits = quadra_design::dialog(changing, "Edit SSH host", move || {
        super::layout::body(&edits_body, form, editing.get_untracked(), confirming)
    })
    .head(move || super::layout::head(form, format!("Edit {}", editing.get_untracked().unwrap_or_default())))
    .aside("Delete host\u{2026}", move || confirming.set(true))
    .stays_open()
    .shown(not)
    .action("Cancel", Variant::Secondary, || {})
    .shown(not)
    .action("Save", Variant::Primary, move || save(&changes, form, editing.get_untracked()))
    .shortcut("Enter")
    .shown(not)
    .action("Keep", Variant::Secondary, move || confirming.set(false))
    .stays_open()
    .shown(move || confirming.get())
    .action("Delete", Variant::Primary, move || {
        back.set(None);
        if let Some(name) = editing.get_untracked() {
            gone.remove_host(&name);
        }
    })
    .danger()
    .shown(move || confirming.get());
    div().child(adds).child(edits)
}

/// The form as a saved host says.
fn fill(model: &Model, form: Form, host: &Host) {
    let (auth, key) = match &host.remote.auth {
        oneterm_ssh::Auth::Agent => (1, String::new()),
        oneterm_ssh::Auth::Key(path) => (2, path.display().to_string()),
        oneterm_ssh::Auth::Password => (3, String::new()),
        oneterm_ssh::Auth::Auto => (0, String::new()),
    };
    let forwards: Vec<String> = host.remote.forwards.iter().map(oneterm_ssh::Forward::label).collect();
    let jump = model.jump_choices().iter().position(|it| *it == host.jump && !host.jump.is_empty());
    form.name.set(host.name.clone());
    form.group.set(host.group.clone());
    let target = &host.remote.target;
    form.target.set(target.host.clone());
    form.port.set(target.port.to_string());
    form.user.set(target.user.clone());
    form.auth.set(auth);
    form.key.set(key);
    form.forwards.set(forwards.join(" "));
    form.jump.set(jump.unwrap_or(0));
    form.tag.set(model.saved_tag(&host.name).map_or_else(String::new, |(word, _)| picked(&word)));
    form.startup.set(model.startup_name(&host.name).unwrap_or_default());
}

/// Saves the form as a host, in place of `was` (the one edited) when it had another name.
fn save(model: &Model, form: Form, was: Option<String>) {
    let typed = form.target.get_untracked();
    let Some(mut target) = Target::parse(typed.trim()) else {
        model.notify(Notice::new("Add SSH host").body(format!("Not a target: {typed}")).tone(Tone::Warning));
        return;
    };
    // The username and port fields, when filled, take the place of what the address says.
    let user = form.user.get_untracked();
    if !user.trim().is_empty() {
        target.user = user.trim().to_string();
    }
    if let Ok(port) = form.port.get_untracked().trim().parse() {
        target.port = port;
    }
    let name = form.name.get_untracked().trim().to_string();
    let name = if name.is_empty() { target.host.clone() } else { name };
    let mut remote = Remote::new(target, auth_of(form.auth.get_untracked(), form.key.get_untracked()));
    for word in form.forwards.get_untracked().split_whitespace() {
        let Some(forward) = oneterm_ssh::Forward::parse(word) else {
            model.notify(
                Notice::new("Add SSH host").body(format!("Not a forward: {word}")).tone(Tone::Warning),
            );
            return;
        };
        remote.forwards.push(forward);
    }
    let jump = model.jump_host(form.jump.get_untracked()).map_or(String::new(), |it| it.name);
    // The secret saved for the host edited stays unless another is typed, with the target
    // it was sealed for.
    let kept = was.as_ref().and_then(|was| {
        let host = model.hosts().into_iter().find(|it| it.name == *was)?;
        Some((model.sealed(was)?, crate::vault::bound(&host.remote)))
    });
    let bound = crate::vault::bound(&remote);
    let back = model.ui().opening.back;
    if back.with_untracked(|it| it.as_ref().is_some_and(|it| was.as_ref() == Some(&it.0))) {
        back.update(|it| it.iter_mut().for_each(|it| it.0.clone_from(&name)));
    }
    if let Some(was) = was.filter(|it| *it != name) {
        model.remove_host(&was);
    }
    let tag = form.tag.with_untracked(|it| word_of(it));
    model.save_host(Host {
        name: name.clone(),
        group: form.group.get_untracked().trim().into(),
        remote,
        jump,
    });
    model.set_host_tag(&name, tag.as_deref());
    super::secret::save(model, form, &name, &bound, kept);
    model.set_startup(&name, Some(form.startup.get_untracked()));
    for field in [
        form.name,
        form.group,
        form.target,
        form.port,
        form.user,
        form.key,
        form.forwards,
        form.tag,
        form.secret,
        form.master,
        form.again,
        form.startup,
    ] {
        crate::vault::wipe(field);
    }
    form.jump.set(0);
}
