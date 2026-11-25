//! What the SFTP dock shows and does (specs/oneterm, "M9 in detail"): the folder of the focused
//! space's connection, its entries, the transfers. Every call of the engine runs on the pool;
//! its answer comes back to the UI thread.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use oneterm_ssh::{Entry, Files, Ssh, join, safe_name};
use quadra::{Scope, Signal, background, interval, signal, spawn, untrack};

use crate::model::Model;

mod edit;
mod kept;
mod transfer;
pub(crate) use transfer::Transfer;
pub(crate) mod properties;
pub(crate) mod upload;
use crate::panes::{Local, free_name};
pub(crate) use edit::{EDITORS, Editing};

/// What the dock says of a space that is not remote.
pub(crate) const LOCAL: &str =
    "The active space is a local shell. Select an SSH space to browse its files here.";

#[derive(Clone, Copy)]
pub(crate) struct Dock {
    pub dir: Signal<String>,
    pub entries: Signal<Vec<Entry>>,
    /// What stands for the list: why it is empty, or what failed.
    pub said: Signal<String>,
    pub transfers: Signal<Vec<Rc<Transfer>>>,
    /// Moves while transfers run, so their percentages show.
    pub tick: Signal<u32>,
    /// The remote entry chosen in the expanded view.
    pub selected: Signal<Option<String>>,
    /// Whether names that start with `.` show.
    pub hidden: Signal<bool>,
    /// The column the remote entries are sorted by, in the dock and in the expanded view.
    pub sort: Signal<crate::panes::Sort>,
    /// The expanded view, whose Local pane downloads go to.
    pub expanded: Signal<bool>,
    pub local: Local,
    pub ask: crate::dock::Ask,
    kept: Signal<kept::Kept>,
    owner: Scope,
}

/// The focused space's connection, if it is remote.
pub(crate) fn connection(model: &Model) -> Option<Arc<Ssh>> {
    let space = untrack(|| model.active_tab().and_then(|tab| tab.focused()))?;
    space.session.kept::<Arc<Ssh>>().map(|it| (*it).clone())
}

impl Dock {
    /// The dock's state; while it is open and the connection is not up yet, it asks again
    /// every second.
    pub fn new(model: &Model, open: Signal<bool>) -> Dock {
        let dock = Dock {
            dir: signal(String::new()),
            entries: signal(Vec::new()),
            said: signal(String::new()),
            transfers: signal(Vec::new()),
            tick: signal(0),
            selected: signal(None),
            hidden: signal(false),
            sort: signal(crate::panes::Sort::default()),
            expanded: model.ui().expanded,
            local: Local::new(downloads(model)),
            ask: crate::dock::Ask::new(),
            kept: signal(kept::Kept::default()),
            owner: Scope::current(),
        };
        let (model, waits) = (model.clone(), std::cell::Cell::new(0u32));
        let _ = interval(Duration::from_millis(250), move || {
            if dock.transfers.with_untracked(|all| all.iter().any(|it| it.end.get_untracked().is_none())) {
                dock.tick.update(|n| *n += 1);
            }
            let early = dock.said.with_untracked(|it| it == oneterm_ssh::NOT_YET);
            if open.get_untracked() && early && waits.replace((waits.get() + 1) % 4) == 3 {
                dock.load(&model, None);
            }
        });
        dock
    }

    /// Runs `work` with the connection's files on the pool, then `then` with its answer.
    fn with_files<T: Send + 'static>(
        &self,
        model: &Model,
        work: impl FnOnce(Files) -> Result<T, String> + Send + 'static,
        then: impl FnOnce(Dock, Result<T, String>) + 'static,
    ) {
        let Some(ssh) = connection(model) else {
            // A remote space whose shell is not open yet is asked again; a local one says so.
            let remote = untrack(|| model.active_tab().and_then(|tab| tab.focused()))
                .is_some_and(|it| it.remote.is_some());
            let why: String = if remote { oneterm_ssh::NOT_YET.into() } else { LOCAL.into() };
            self.said.set(why.clone());
            self.entries.set(Vec::new());
            then(*self, Err(why));
            return;
        };
        let dock = *self;
        self.owner.run(|| {
            spawn(async move {
                let got = background(move || work(ssh.files()?)).await;
                then(dock, got.unwrap_or_else(|_| Err("the work failed".into())));
            })
            .detach()
        });
    }

    /// Shows a folder, the home folder when none is given; an answer that comes once another
    /// space shows is dropped.
    pub fn load(&self, model: &Model, dir: Option<String>) {
        let asked = self.at();
        self.with_files(
            model,
            move |files| {
                let dir = match dir {
                    Some(dir) => dir,
                    None => files.home()?,
                };
                let entries = files.list(&dir)?;
                Ok((dir, entries))
            },
            move |dock, got| match got {
                _ if dock.at() != asked => {}
                Ok((dir, entries)) => {
                    dock.selected.set(None);
                    dock.dir.set(dir);
                    dock.entries.set(entries);
                    dock.said.set(String::new());
                }
                Err(why) => dock.said.set(why),
            },
        );
    }

    /// The folder shown, again.
    pub fn refresh(&self, model: &Model) {
        let dir = self.dir.get_untracked();
        self.load(model, (!dir.is_empty()).then_some(dir));
    }

    pub fn mkdir(&self, model: &Model, name: &str) {
        let (path, again) = (join(&self.dir.get_untracked(), name), model.clone());
        self.with_files(model, move |files| files.mkdir(&path), move |dock, got| dock.after(&again, got));
    }

    /// Renames an entry of the folder shown.
    pub fn rename(&self, model: &Model, from: &str, to: &str) {
        let dir = self.dir.get_untracked();
        let (from, to, again) = (join(&dir, from), join(&dir, to), model.clone());
        self.with_files(
            model,
            move |files| files.rename(&from, &to),
            move |dock, got| dock.after(&again, got),
        );
    }

    pub fn delete(&self, model: &Model, entry: &Entry) {
        let (path, dir, again) = (join(&self.dir.get_untracked(), &entry.name), entry.dir, model.clone());
        self.with_files(
            model,
            move |files| files.remove(&path, dir),
            move |dock, got| dock.after(&again, got),
        );
    }

    /// The list again after a change; what failed is a card, the list kept.
    fn after(&self, model: &Model, got: Result<(), String>) {
        match got {
            Ok(()) => self.refresh(model),
            Err(why) => {
                model.notify(quadra_design::Notice::new("SFTP").body(why).tone(quadra_design::Tone::Warning))
            }
        }
    }

    /// Downloads a file or a folder's tree under a name not taken: to the Local pane's folder
    /// in the expanded view, else to the downloads folder.
    pub fn download(&self, model: &Model, entry: &Entry) {
        if !safe_name(&entry.name) {
            self.said.set(format!("Not a name for a local file: {}", entry.name));
            return;
        }
        let remote = join(&self.dir.get_untracked(), &entry.name);
        let folder =
            if self.expanded.get_untracked() { self.local.dir.get_untracked() } else { downloads(model) };
        let (local, dir) = (free_name(&folder, &entry.name), entry.dir);
        let transfer = self.start(&entry.name, false, folder.display().to_string());
        let (done, whole, cancel) = (transfer.done.clone(), transfer.whole.clone(), transfer.cancel.clone());
        let (model, ran) = (model.clone(), transfer.clone());
        self.queue(&transfer, move |dock| {
            dock.with_files(
                &model,
                move |files| {
                    let tell = |d, w| store(&done, &whole, d, w);
                    match dir {
                        true => files.download_folder(&remote, &local, tell, &cancel),
                        false => files.download(&remote, &local, tell, &cancel),
                    }
                },
                move |dock, got| {
                    if got.is_ok() && dock.local.dir.get_untracked() == folder {
                        dock.local.refresh();
                    }
                    dock.finish(&ran, got);
                },
            )
        });
    }

    /// A transfer waits its turn: one runs at a time, in the order asked.
    fn queue(&self, transfer: &Transfer, job: impl FnOnce(Dock) + 'static) {
        *transfer.job.borrow_mut() = Some(Box::new(job));
        self.next();
    }

    /// Starts the first transfer waiting, when none runs.
    fn next(&self) {
        let all = self.transfers.get_untracked();
        if all.iter().any(|it| it.running()) {
            return;
        }
        if let Some(job) = all.iter().find_map(|it| it.job.borrow_mut().take()) {
            job(*self);
        }
    }

    fn finish(&self, transfer: &Transfer, got: Result<(), String>) {
        transfer.end.set(Some(got));
        self.next();
    }

    fn start(&self, name: &str, up: bool, to: String) -> Rc<Transfer> {
        let transfer = Rc::new(Transfer {
            name: name.to_string(),
            up,
            to,
            done: Arc::default(),
            whole: Arc::default(),
            cancel: Arc::default(),
            end: signal(None),
            job: Default::default(),
        });
        self.transfers.update(|all| all.push(transfer.clone()));
        transfer
    }
}

fn store(done: &AtomicU64, whole: &AtomicU64, d: u64, w: u64) {
    done.store(d, Ordering::Relaxed);
    whole.store(w, Ordering::Relaxed);
}

/// Where downloads go: the setting, else the user's Downloads folder.
fn downloads(model: &Model) -> PathBuf {
    let set = model.prefs().downloads.get_untracked();
    if !set.is_empty() {
        return PathBuf::from(set);
    }
    let home = std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default();
    home.join("Downloads")
}
