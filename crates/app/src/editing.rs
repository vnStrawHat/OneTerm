//! A remote file edited in OneTerm (specs/oneterm, "Spaces" and "M9 in detail"): a space beside the
//! terminals that holds the file in the kit's code editor, coloured by its type, with a diff
//! of the changes; Ctrl+S sends it back. A binary file says so and is not edited.

mod closing;
mod kind;
mod view;

pub(crate) use closing::Then;

pub(crate) use kind::{Content, kind, label};
pub(crate) use view::view;

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use oneterm_ssh::Ssh;
use quadra::{Action, NodeId, Scope, Signal, background, signal, spawn, untrack};
use quadra_code::EditorWidget;
use quadra_design::{Notice, Tone};
use quadra_tree::Handle;

use crate::model::Model;

/// Saves the file of the editor space the focus is in (Ctrl+S).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Save;

impl Action for Save {
    const NAME: &'static str = "oneterm::SaveFile";
}

/// The key context of an editor space.
pub(crate) const CONTEXT: &str = "file_edit";

/// Where an Edit was chosen: the tab, the space whose dock it was, its connection, and who
/// that space is (`user@host`).
pub(crate) struct Source {
    tab: u64,
    space: u64,
    ssh: Arc<Ssh>,
    remote: Option<crate::shells::Remote>,
    who: String,
}

/// The focused space of the active tab as the source of an Edit, if it is connected: an
/// editor's is the space its file came from.
pub(crate) fn source(model: &Model) -> Option<Source> {
    let (tab, space) = untrack(|| model.active_tab().and_then(|tab| Some((tab.id, tab.focused()?))))?;
    let ssh = (*space.session.kept::<Arc<Ssh>>()?).clone();
    let who = match &space.edit {
        Some(edit) => edit.who.clone(),
        None => {
            let conn = space.session.kept::<crate::remote::Conn>();
            let who = space.remote.as_ref().map(|it| crate::spaces::who(model, it, conn.as_deref()));
            who.unwrap_or_default()
        }
    };
    Some(Source { tab, space: space.files_of(), ssh, remote: space.remote, who })
}

/// The file of an editor space.
pub(crate) struct FileEdit {
    /// `user@host:/full/path`, and who that is.
    pub place: String,
    who: String,
    pub name: String,
    remote: String,
    /// The copy in the app's data folder, which is what is uploaded.
    local: PathBuf,
    ssh: Arc<Ssh>,
    pub content: Content,
    /// What colours it: the file's name, or the program its first line names.
    pub kind: String,
    /// The text as it was opened or last saved: what the diff and the gutter compare with.
    pub base: Signal<String>,
    pub modified: Signal<bool>,
    pub saving: Signal<bool>,
    /// The diff shown in the editor's place, and the text it compares with the base.
    pub diff: Signal<bool>,
    pub now: Signal<String>,
    /// Closing with changes asks first; Don't save lets it go.
    pub asking: Signal<bool>,
    let_go: Cell<bool>,
    /// The closing that asked, to go on once it is answered.
    then: RefCell<Option<Then>>,
    /// The space the file came from: the dock it shows is that space's.
    pub from: u64,
    /// The editor's widget, while it is mounted, and its document while it is not (its tab
    /// does not show).
    widget: RefCell<Option<(Handle, NodeId)>>,
    pub doc: quadra_code::KeptDoc,
    owner: Scope,
}

/// Opens a downloaded file in a space after the focused one of the tab it came from, with
/// the focus; one open already of the same file takes the focus instead.
pub(crate) fn open(model: &Model, from: Source, local: PathBuf, remote: String, content: Content) {
    let Some(tab) = model.tabs().with_untracked(|tabs| tabs.iter().find(|it| it.id == from.tab).copied())
    else {
        return;
    };
    let same = |it: &crate::model::Space| {
        it.edit.as_ref().is_some_and(|e| e.remote == remote && Arc::ptr_eq(&e.ssh, &from.ssh))
    };
    if let Some(at) = tab.spaces.with_untracked(|all| all.iter().position(same)) {
        return tab.focus.set(at);
    }
    let name = remote.rsplit('/').next().unwrap_or(&remote).to_string();
    let kind = match &content {
        Content::Text(text) => kind(&name, text),
        Content::Binary(_) => name.clone(),
    };
    let base = match &content {
        Content::Text(text) => text.replace("\r\n", "\n"),
        Content::Binary(_) => String::new(),
    };
    let place = format!("{}:{remote}", from.who);
    let Source { space: source, ssh, remote: host, who, .. } = from;
    let space = model.editor_space(host, |session| {
        // The dock follows this space's connection: the file's.
        session.keep(Rc::new(ssh.clone()));
        Rc::new(FileEdit {
            place,
            who,
            name,
            remote,
            local,
            ssh,
            content,
            kind,
            base: signal(base),
            modified: signal(false),
            saving: signal(false),
            diff: signal(false),
            now: signal(String::new()),
            asking: signal(false),
            let_go: Cell::new(false),
            then: RefCell::new(None),
            from: source,
            widget: RefCell::new(None),
            doc: Rc::default(),
            owner: Scope::current(),
        })
    });
    model.open_space(tab, space);
}

impl crate::model::Space {
    /// The space whose dock this one shows: an editor's is the one its file came from.
    pub(crate) fn files_of(&self) -> u64 {
        self.edit.as_ref().map_or(self.id, |it| it.from)
    }
}

fn card(model: &Model, body: String, tone: Tone) {
    model.notify(Notice::new("SFTP").body(body).tone(tone));
}

impl FileEdit {
    /// Whether closing waits for an answer: there are changes, and Don't save was not chosen.
    /// It asks, and `then` goes on with the closing once it is answered.
    pub fn holds(&self, then: Then) -> bool {
        let holds = self.unsaved();
        if holds {
            *self.then.borrow_mut() = Some(then);
            self.asking.set(true);
        }
        holds
    }

    /// Whether it has changes, and Don't save was not chosen.
    fn unsaved(&self) -> bool {
        self.modified.get_untracked() && !self.let_go.get()
    }

    /// The space closes without its changes: the closing goes on.
    pub fn let_go(&self) {
        self.let_go.set(true);
        if let Some(then) = self.then.take() {
            then();
        }
    }

    /// Its changes hold the closing again (Cancel).
    fn keep(&self) {
        self.let_go.set(false);
        self.then.take();
    }

    /// Calls `f` with the editor's text (as edited, and as its file has it) in the tree's
    /// next pass.
    fn text(&self, f: impl FnOnce(String, String) + 'static) {
        let Some((handle, id)) = self.widget.borrow().clone() else { return };
        handle.with(move |tree| {
            if let Some(w) = tree.widget_mut::<EditorWidget>(id) {
                f(w.doc.text(), w.doc.buffer.file_text());
            }
        });
    }

    /// Gives the editor the focus: after a button of its header.
    pub fn focus(&self) {
        if let Some((handle, id)) = self.widget.borrow().clone() {
            handle.with(move |tree| tree.focus(id));
        }
    }

    /// Shows the diff of the text as it is now against the base, or the editor again.
    pub fn toggle_diff(self: &Rc<Self>) {
        if self.diff.get_untracked() {
            self.diff.set(false);
            // The editor again, with the focus.
            return self.focus();
        }
        let me = self.clone();
        self.text(move |text, _| {
            me.now.set(text);
            me.diff.set(true);
        });
    }

    /// Sends the text to the server through the copy in the data folder; a card says how it
    /// went. Once it is there it is the base, and `then` runs.
    pub fn save(self: &Rc<Self>, model: &Model, then: impl FnOnce() + 'static) {
        if self.saving.get_untracked() || matches!(self.content, Content::Binary(_)) {
            return;
        }
        let (me, model) = (self.clone(), model.clone());
        self.text(move |text, file| {
            me.saving.set(true);
            let (local, remote, ssh) = (me.local.clone(), me.remote.clone(), me.ssh.clone());
            let owner = me.owner;
            owner.run(|| {
                spawn(async move {
                    let sent = background(move || {
                        std::fs::write(&local, file).map_err(|e| e.to_string())?;
                        ssh.files()?.upload(&local, &remote, |_, _| {}, &AtomicBool::new(false))
                    })
                    .await
                    .unwrap_or_else(|_| Err("the work failed".into()));
                    me.saving.set(false);
                    match sent {
                        Ok(()) => {
                            card(&model, format!("{} saved to the server.", me.name), Tone::Success);
                            me.rebase(text);
                            then();
                        }
                        Err(why) => {
                            card(&model, format!("{} was not saved: {why}", me.name), Tone::Warning);
                            model.keep_edits();
                        }
                    }
                })
                .detach()
            });
        });
    }

    /// The text saved is the base of the diff and of the gutter's marks.
    fn rebase(&self, text: String) {
        self.base.set(text.clone());
        let Some((handle, id)) = self.widget.borrow().clone() else { return };
        handle.with(move |tree| {
            if let Some(w) = tree.widget_mut::<EditorWidget>(id) {
                w.set_base(&text);
                tree.mark_paint(id);
            }
        });
    }
}
