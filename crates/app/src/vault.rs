//! Saved passwords in the app (specs/oneterm, "Saved passwords"): the vault's document is the
//! setting `ssh.vault`, each host's sealed secret a row of `ssh.secrets`; the sealing itself
//! is the crate `oneterm-vault`. Here: which secret is a host's, what a connection takes of
//! them, and the dialog that asks for the master password (`unlock.rs`).

mod unlock;

pub(crate) use oneterm_vault::{DataKey, Header, Opened, Secret, make, reseal, unseal};
pub(crate) use unlock::dialog;

use oneterm_ssh::Target;
use quadra::Signal;
use quadra_shell::Zeroize;

use crate::model::Model;
use crate::shells::Remote;

/// Empties a field that held a password, wiping what it held.
pub(crate) fn wipe(field: Signal<String>) {
    if !field.with_untracked(String::is_empty) {
        field.update(|it| it.zeroize());
    }
}

/// What a host's secret is bound to: its target, so it opens for no other.
pub(crate) fn bound(remote: &Remote) -> Target {
    remote.target.clone()
}

/// A secret sealed under the data key for a target, as `ssh.secrets` keeps it.
pub(crate) fn seal_secret(vault: &Opened, plain: &str, to: &Target) -> Option<String> {
    oneterm_vault::seal_secret(vault, plain, &to.host, to.port, &to.user)
}

/// A secret opened, if it was sealed by this vault for `to`.
pub(crate) fn open_secret(vault: &Opened, sealed: &str, to: &Target) -> Option<Secret> {
    oneterm_vault::open_secret(vault, sealed, &to.host, to.port, &to.user)
}

impl Model {
    /// The sealed secret saved for a host.
    pub(crate) fn sealed(&self, name: &str) -> Option<String> {
        let rows = self.prefs().secrets;
        rows.with_untracked(|rows| {
            rows.iter().find(|it| it.first().map(String::as_str) == Some(name))?.get(1).cloned()
        })
    }

    /// Saves a host's sealed secret, or forgets it.
    pub(crate) fn set_sealed(&self, name: &str, sealed: Option<String>) {
        self.prefs().secrets.update(|rows| {
            rows.retain(|it| it.first().map(String::as_str) != Some(name));
            rows.extend(sealed.map(|it| vec![name.to_string(), it]));
        });
    }

    /// The sealed secret of the saved host a connection goes to.
    fn sealed_for(&self, remote: &Remote) -> Option<String> {
        let host = self.hosts().into_iter().find(|it| it.remote.target == remote.target)?;
        self.sealed(&host.name)
    }

    /// Connects to each remote in a tab of its own, or in a space here: with its saved secret,
    /// after the master password is asked for when one is saved and the data key is not in
    /// memory.
    pub(crate) fn connect_all(&self, remotes: Vec<Remote>, here: bool) {
        let key = self.lock().key.get_untracked();
        let needs = remotes.iter().any(|it| self.sealed_for(it).is_some());
        if needs && key.is_none() && self.lock().can_lock() {
            return self.ui().unlocking.set(Some((remotes, here)));
        }
        for remote in remotes {
            let secret = self.saved_secret(&remote);
            self.connect_with(remote, here, secret);
        }
    }

    /// The saved secret of the host a connection goes to, opened, if the data key is in memory.
    pub(crate) fn saved_secret(&self, remote: &Remote) -> Option<Secret> {
        let key = self.lock().key.get_untracked()?;
        open_secret(&key, &self.sealed_for(remote)?, &bound(remote))
    }
}
