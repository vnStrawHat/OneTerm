//! A host's password or key passphrase in its dialog (specs/oneterm, "Saved passwords"): saved
//! sealed with the master password, which is asked for while its key is not in memory, or set
//! there when there is none.

use oneterm_ssh::Target;
use quadra::{View, column, div};
use quadra_design::{Notice, Tone, field, input};

use super::form::Form;
use crate::locked::shown;
use crate::model::Model;
use quadra_design::token;

/// Under the password or passphrase typed, the master password while its key is not in memory,
/// or a new one and again while there is none.
pub(super) fn fields(model: &Model, form: Form) -> impl View + use<> {
    let lock = model.lock();
    let typed = move || !form.secret.with(String::is_empty);
    let opens = move || typed() && lock.can_lock() && lock.key.get().is_none();
    let sets = move || typed() && !lock.can_lock();
    let master = |id: &'static str, value| input(value).password().test_id(id);
    column()
        .gap(token::SPACE_LG)
        .display(shown(move || matches!(form.auth.get(), 2 | 3)))
        .child(
            div().display(shown(opens)).child(field("Master password", master("host-master", form.master))),
        )
        .child(
            column()
                .gap(token::SPACE_LG)
                .display(shown(sets))
                .child(
                    field("New master password", master("host-new-master", form.master))
                        .help("OneTerm has none yet: it also locks the window (Security & lock)."),
                )
                .child(field("Repeat it", master("host-master-again", form.again))),
        )
}

/// Saves the secret typed for a host, sealed for its target `bound`; with none typed the one
/// `kept` stays, sealed again when the target changed (forgotten, with a card, while the data
/// key is not in memory). Another auth than Password and Key file saves none. Without a master
/// password the one typed is set first.
pub(super) fn save(model: &Model, form: Form, name: &str, bound: &Target, kept: Option<(String, Target)>) {
    let typed = quadra_shell::Zeroizing::new(form.secret.get_untracked());
    if !matches!(form.auth.get_untracked(), 2 | 3) {
        return model.set_sealed(name, None);
    }
    if typed.is_empty() {
        let Some((sealed, was)) = kept else { return model.set_sealed(name, None) };
        if was == *bound {
            return model.set_sealed(name, Some(sealed));
        }
        let key = model.lock().key.get_untracked();
        let moved = key.and_then(|key| {
            let secret = crate::vault::open_secret(&key, &sealed, &was)?;
            crate::vault::seal_secret(&key, secret.as_str(), bound)
        });
        if moved.is_none() {
            model.notify(
                Notice::new("Edit SSH host")
                    .body("The target changed: its saved password was forgotten.")
                    .tone(Tone::Warning),
            );
        }
        return model.set_sealed(name, moved);
    }
    let (master, again) =
        (form.master.get_untracked(), quadra_shell::Zeroizing::new(form.again.get_untracked()));
    let (lock, model, name, bound) = (model.lock(), model.clone(), name.to_string(), bound.clone());
    let sealing = move |model: &Model, why: &str| match model.lock().key.get_untracked() {
        Some(key) => model.set_sealed(&name, crate::vault::seal_secret(&key, &typed, &bound)),
        None => model.notify(
            Notice::new("Add SSH host")
                .body(format!("{why}: the password was not saved."))
                .tone(Tone::Warning),
        ),
    };
    if lock.key.get_untracked().is_some() {
        return sealing(&model, "");
    }
    if lock.can_lock() {
        let opens = model.clone();
        return lock.open(master, move |_| sealing(&opens, "Wrong master password"));
    }
    if master.is_empty() || master != *again {
        let why = if master.is_empty() { "No master password" } else { "The master passwords differ" };
        return sealing(&model, why);
    }
    let sets = model.clone();
    lock.set_password(String::new(), master, move |said| sealing(&sets, said));
}
