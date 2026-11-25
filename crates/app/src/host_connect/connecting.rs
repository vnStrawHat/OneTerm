//! A saved host's connection from its Connect dialog (specs/oneterm, "The rail"): made as the form
//! says, followed until its shell opens, then placed; what worked saved when asked.

use oneterm_ssh::Ask;
use quadra::{effect, untrack};

use super::Form;
use super::username::overridden;
use crate::hosts::Host;
use crate::model::{Model, Space};
use crate::remote::{Conn, Phase};
use crate::vault::Secret;

/// Connects as the form says, unless it already does.
pub(super) fn connect(model: &Model, name: &str, form: Form) {
    let opening = model.ui().opening;
    if opening.space.with_untracked(Option::is_some) {
        return;
    }
    let Some(host) = model.hosts().into_iter().find(|it| it.name == name) else { return };
    let here = form.here.get_untracked();
    form.failed.set(String::new());
    let remote = used(&host, form);
    let typed = form.password.get_untracked();
    // The password typed, else the saved one (bound to the saved target), else the empty one.
    let secret = match typed.is_empty() && remote.target == host.remote.target && model.sealed(name).is_some()
    {
        false => Secret::new(typed),
        true => match model.saved_secret(&host.remote) {
            Some(secret) => secret,
            // ponytail: a locked vault asks the master password and connects as before (the tab
            // at once); through this dialog when that matters.
            None if model.lock().can_lock() => {
                opening.host.set(None);
                return model.connect_all(vec![remote], here);
            }
            None => Secret::new(String::new()),
        },
    };
    let launch = crate::shells::Launch {
        ssh: Some(remote),
        secret: Some(secret),
        startup: model.startup(name),
        ..model.launch(None)
    };
    let space = model.space(launch);
    match space.session.kept::<Conn>() {
        Some(_) => opening.space.set(Some(space)),
        // A session that does not connect (a test's): placed at once.
        None => {
            opening.host.set(None);
            crate::vault::wipe(form.password);
            model.place(space, here);
        }
    }
}

/// The host's remote as the form changes it: the user's target, the auth chosen.
fn used(host: &Host, form: Form) -> crate::shells::Remote {
    let mut remote = host.remote.clone();
    remote.target = overridden(&remote.target, &form.user.get_untracked());
    remote.auth = crate::hosts::auth_of(form.auth.get_untracked(), form.key.get_untracked());
    remote
}

/// Follows the space connecting: placed once its shell opens (what worked saved to the host
/// when asked); a failure, or the
/// password refused, ends it and says why.
pub(super) fn watch(model: &Model, form: Form) {
    let (opening, model) = (model.ui().opening, model.clone());
    effect(move || {
        let Some(space) = opening.space.get() else { return };
        let Some(conn) = space.session.kept::<Conn>() else { return };
        let phase = conn.phase.get();
        untrack(|| {
            let why = match phase {
                Phase::Done if space.session.exited().get().is_none() => {
                    let name = opening.host.get().map(|it| it.0).unwrap_or_default();
                    if form.save.get() {
                        keep(&model, &name, form);
                    }
                    crate::vault::wipe(form.password);
                    opening.space.set(None);
                    opening.host.set(None);
                    return model.place(space, form.here.get());
                }
                Phase::Ask(Ask::Failed { reason }) => crate::asking::sentence(&reason),
                Phase::Ask(Ask::Password { again: true, .. }) => {
                    form.refused.set(true);
                    "Permission denied: the password was refused.".into()
                }
                _ => return,
            };
            crate::vault::wipe(form.password);
            form.failed.set(why);
            opening.space.set(None);
            crate::split::end(space);
        });
    });
}

/// Saves what worked to the host: the target, the auth, and the password or passphrase typed
/// (sealed while the vault's key is in memory).
fn keep(model: &Model, name: &str, form: Form) {
    let Some(host) = model.hosts().into_iter().find(|it| it.name == name) else { return };
    let remote = used(&host, form);
    let typed = quadra_shell::Zeroizing::new(form.password.get_untracked());
    let key = model.lock().key.get_untracked();
    if let Some(key) = key.filter(|_| !typed.is_empty() && matches!(form.auth.get_untracked(), 2 | 3)) {
        model.set_sealed(name, crate::vault::seal_secret(&key, &typed, &crate::vault::bound(&remote)));
    }
    model.save_host(Host { remote, ..host });
}

impl Model {
    /// A space connected: in a tab of its own, or beside the focused one; its target recent.
    pub(crate) fn place(&self, space: Space, here: bool) {
        let label = space.remote.as_ref().map(|it| it.target.label());
        match self.active_tab().filter(|_| here) {
            Some(tab) => self.open_space(tab, space),
            None => self.insert_tab(usize::MAX, space),
        }
        if let Some(label) = label {
            self.remember(label);
        }
    }
}
