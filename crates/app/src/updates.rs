//! OneTerm's updates (specs/oneterm, "Updates"): whether a newer release is published where OneTerm's
//! are, and installing it, by `quadra-updater` (Quadra's specs/22). Checking and fetching run on the
//! pool; nothing is installed unasked, and nothing restarts but by its button.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use quadra::{Align, Scope, Signal, background, inbox, row, signal, spawn, text};
use quadra_design::{Notice, SettingsGroup, Variant, button, switch, token};
use quadra_updater::{Error, Release, Updater};

use crate::model::Model;
use crate::prefs::Prefs;

/// Where OneTerm's releases are published: the repository of the app it replaces, which is
/// where that app's updater looks for them.
const REPOSITORY: &str = "vnStrawHat/OneTerm";

/// Where an update is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Update {
    /// Not asked yet.
    Unknown,
    Checking,
    Latest,
    Found(Release),
    /// The bytes that came, of how many.
    Fetching(Release, u64, u64),
    /// The version whose files are in place: the next start runs it.
    Installed(String),
    Failed(String),
}

/// What stops a fetch, the same one in every copy.
#[derive(Clone)]
struct Stop(Arc<AtomicBool>);

impl PartialEq for Stop {
    fn eq(&self, other: &Stop) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Clone, Copy)]
pub struct Updates {
    pub state: Signal<Update>,
    /// The releases asked and the folders used; a test names its own.
    pub source: Signal<Updater>,
    /// Set by Restart: the window closes for the new version's start.
    pub closing: Signal<bool>,
    stop: Signal<Option<Stop>>,
    /// The proxy and the certificates, as the settings say when something is fetched.
    net: crate::prefs::NetPrefs,
    owner: Scope,
}

impl Updates {
    pub(crate) fn new(net: crate::prefs::NetPrefs) -> Updates {
        let dirs = quadra::use_context::<std::rc::Rc<quadra::AppDirs>>();
        let cache = dirs.as_ref().map_or_else(|| ".tmp/updates".into(), |it| it.data.join("updates"));
        let mut source = Updater::new(REPOSITORY, "oneterm", env!("CARGO_PKG_VERSION"), &cache);
        // Without the app's folders (a test, a picture of the page) no machine is asked and no
        // program's folder is written to, until a test names its own.
        if dirs.is_none() {
            (source.api, source.home) = (String::new(), cache.join("home"));
        }
        Updates {
            state: signal(Update::Unknown),
            source: signal(source),
            closing: signal(false),
            stop: signal(None),
            net,
            owner: Scope::current(),
        }
    }

    /// Where the releases are, reached as the network's settings say now: what a check or a
    /// fetch started now is made with.
    pub fn updater(&self) -> Updater {
        let mut source = self.source.get_untracked();
        source.network = self.net.network();
        source
    }

    /// Asks whether a newer release is published. `then` is told once it is known.
    pub fn check(&self, then: impl FnOnce(&Update) + 'static) {
        if matches!(
            self.state.get_untracked(),
            Update::Checking | Update::Fetching(..) | Update::Installed(_)
        ) {
            return;
        }
        let (state, source) = (self.state, self.updater());
        state.set(Update::Checking);
        self.owner.run(|| {
            spawn(async move {
                let now = match background(move || source.check()).await {
                    Ok(Ok(Some(release))) => Update::Found(release),
                    Ok(Ok(None)) => Update::Latest,
                    Ok(Err(why)) => failed(&why),
                    Err(_) => Update::Failed("The check failed".into()),
                };
                state.set(now);
                then(&state.get_untracked());
            })
            .detach()
        });
    }

    /// Fetches the release found, checks it and puts its files in OneTerm's folder.
    pub fn install(&self) {
        let Update::Found(release) = self.state.get_untracked() else { return };
        let (state, source, stop) = (self.state, self.updater(), Stop(Arc::default()));
        self.stop.set(Some(stop.clone()));
        state.set(Update::Fetching(release.clone(), 0, release.size));
        self.owner.run(|| {
            let came = inbox::<(u64, u64)>();
            let tells = came.sender();
            let shown = release.clone();
            came.on_data(move |burst| {
                // What comes after the fetch ended says nothing any more.
                if let (Some(&(got, of)), Update::Fetching(..)) = (burst.last(), state.get_untracked()) {
                    state.set(Update::Fetching(shown.clone(), got, of));
                }
            });
            spawn(async move {
                let (found, version) = (release.clone(), release.version.clone());
                let done = background(move || {
                    let mut progress = |got, of| tells.push((got, of)) && !stop.0.load(Ordering::Relaxed);
                    let package = source.fetch(&release, &mut progress)?;
                    source.install(&package)
                })
                .await;
                state.set(match done {
                    Ok(Ok(())) => Update::Installed(version),
                    Ok(Err(Error::Cancelled)) => Update::Found(found),
                    Ok(Err(why)) => failed(&why),
                    Err(_) => Update::Failed("The update failed".into()),
                });
            })
            .detach()
        });
    }

    /// Stops a fetch: the release is still there to install.
    pub fn cancel(&self) {
        if let Some(stop) = self.stop.get_untracked() {
            stop.0.store(true, Ordering::Relaxed);
        }
    }

    /// Starts the new version and closes this one.
    pub fn restart(&self) {
        match quadra_updater::restart() {
            Ok(()) => self.closing.set(true),
            Err(why) => self.state.set(Update::Failed(format!("OneTerm could not be started again: {why}"))),
        }
    }
}

/// What failed, in words, with the detail in the log.
fn failed(why: &Error) -> Update {
    quadra::log::warn("updates", format!("{why:?}"));
    Update::Failed(why.to_string())
}

impl Model {
    /// The check a start makes (the setting, on by default): a card when a release is newer;
    /// nothing is said of a check that found none or failed.
    pub fn check_for_updates_at_start(&self) {
        if !self.prefs().update_check.get_untracked() {
            return;
        }
        let model = self.clone();
        self.ui().updates.check(move |now| {
            if let Update::Found(release) = now {
                let title = format!("OneTerm {} is available", release.version);
                model.notify(Notice::new(title).body("Install it from Settings, About."));
            }
        });
    }
}

/// What the row says, and what its button does.
fn said(now: &Update) -> (String, &'static str) {
    match now {
        Update::Unknown => (String::new(), "Check for updates"),
        Update::Checking => ("Checking\u{2026}".into(), "Check for updates"),
        Update::Latest => ("OneTerm is up to date.".into(), "Check for updates"),
        Update::Found(release) => (format!("Version {} is available.", release.version), "Install"),
        Update::Fetching(release, got, of) => {
            let part = if *of > 0 { format!(" {}%", got * 100 / of) } else { String::new() };
            (format!("Downloading {}{part}", release.version), "Cancel")
        }
        Update::Installed(version) => (format!("Version {version} is installed."), "Restart OneTerm"),
        Update::Failed(why) => (why.clone(), "Check for updates"),
    }
}

/// The settings' group: the state and its button, and the check at start.
pub(crate) fn group(p: Prefs, d: Prefs, model: &Model) -> SettingsGroup {
    let updates = model.ui().updates;
    let state = updates.state;
    let act = button(move || said(&state.get()).1.to_string())
        .variant(Variant::Secondary)
        .disabled(move || state.get() == Update::Checking)
        .on_click(move || match state.get_untracked() {
            Update::Found(_) => updates.install(),
            Update::Fetching(..) => updates.cancel(),
            Update::Installed(_) => updates.restart(),
            _ => updates.check(|_| {}),
        });
    let now = row()
        .align_items(Align::Center)
        .gap(token::SPACE_LG)
        .child(text(move || said(&state.get()).0))
        .child(act);
    let about = "A newer version is fetched from GitHub, checked, and put in place while OneTerm runs. \
                 Restarting closes every terminal and connection.";
    crate::page::Back::back(
        SettingsGroup::new("Updates").row("Update", about, now).row(
            "Check at start",
            "Ask GitHub for a newer version each time OneTerm starts.",
            switch(p.update_check),
        ),
        p.update_check,
        d.update_check,
    )
}

/// Closes `window` once Restart started the new version.
pub(crate) fn closes(updates: Updates, window: Option<quadra::Window>) {
    quadra::effect(move || {
        if let (true, Some(window)) = (updates.closing.get(), &window) {
            window.close_now();
        }
    });
}
