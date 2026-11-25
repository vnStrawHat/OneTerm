//! A tab and its spaces (specs/oneterm, "Spaces"), and what the app's keys ask for.

use quadra::{Action, Scope, Signal};
use quadra_term::Session;

use crate::prefs::Prefs;

/// A space: a session, and the scope its signals and tasks live in.
#[derive(Clone)]
pub struct Space {
    pub id: u64,
    pub session: Session,
    pub(crate) scope: Scope,
    /// The host it connected to, if it is remote.
    pub remote: Option<crate::shells::Remote>,
    /// The input channel it is in, 0 to 4 (A to E): what is typed in it goes to the others.
    pub channel: Signal<Option<u8>>,
    /// A remote file edited in OneTerm: the space runs no program, its session is never
    /// connected (it keeps the file's connection, for the dock).
    pub(crate) edit: Option<std::rc::Rc<crate::editing::FileEdit>>,
}

impl PartialEq for Space {
    fn eq(&self, other: &Space) -> bool {
        self.id == other.id
    }
}

/// A tab: its spaces, the one with the focus, and the name the user gave it.
#[derive(Clone, Copy)]
pub struct Tab {
    pub id: u64,
    /// Where the spaces stand: a leaf a space, its group the space's id.
    pub layout: Signal<quadra_design::work::Pane>,
    /// Whether the tab arranges its spaces by itself: false once the layout is the user's.
    pub auto: Signal<bool>,
    /// In the layout's reading order.
    pub spaces: Signal<Vec<Space>>,
    pub focus: Signal<usize>,
    pub name: Signal<Option<String>>,
    /// The settings, for how its signals show.
    pub(crate) prefs: Prefs,
}

impl PartialEq for Tab {
    fn eq(&self, other: &Tab) -> bool {
        self.id == other.id
    }
}

impl Tab {
    /// The space with the focus.
    pub fn focused(&self) -> Option<Space> {
        let at = self.focus.get();
        self.spaces.with(|spaces| spaces.get(at.min(spaces.len().saturating_sub(1))).cloned())
    }

    /// What the tab is called: the user's name, else its focused program's title (a path of
    /// a program shortened to its name), or as the settings say, "Terminal" or the host.
    pub fn title(&self) -> String {
        if let Some(name) = self.name.get() {
            return name;
        }
        let space = self.focused();
        if let Some(edit) = space.as_ref().and_then(|it| it.edit.as_ref()) {
            return edit.name.clone();
        }
        let fixed = || match space.as_ref().and_then(|it| it.remote.as_ref()) {
            Some(remote) => remote.target.label(),
            None => "Terminal".into(),
        };
        if self.prefs.tab_title.get() == 1 {
            return fixed();
        }
        let title = space.as_ref().map(|space| space.session.title().get()).unwrap_or_default();
        if title.is_empty() { fixed() } else { short(&title) }
    }
}

/// A path of a program, `C:\Windows\system32\cmd.exe - ping`, as `cmd.exe - ping`.
pub(crate) fn short(title: &str) -> String {
    let (program, rest) = title.split_once(" - ").map_or((title, None), |(a, b)| (a, Some(b)));
    let program = match program.rsplit_once('\\') {
        Some((_, name)) if !name.is_empty() => name,
        _ => program,
    };
    rest.map_or_else(|| program.to_string(), |rest| format!("{program} - {rest}"))
}

#[cfg(test)]
#[test]
fn paths_of_programs_are_shortened() {
    assert_eq!(short("C:\\Windows\\system32\\cmd.exe"), "cmd.exe");
    assert_eq!(short("C:\\Windows\\system32\\cmd.exe - ping x"), "cmd.exe - ping x");
    assert_eq!(short("vim notes.md"), "vim notes.md");
}

/// What the keys of the app ask for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    NewTab,
    /// Split space: after the focused one, the tab arranged again (to the right of it in a
    /// layout of the user's).
    Split,
    SplitTo(Side),
    CloseSpace,
    /// The space to the right, or to the left.
    FocusNext(bool),
    /// Lock at once (with a master password).
    Lock,
    /// The quick connect dialog.
    QuickConnect,
    /// The Settings window (the page, where no window opens).
    Settings,
    /// The SFTP dock shown or hidden.
    Dock,
    /// The focused space's text all selected; its scrollback cleared.
    SelectAll,
    Clear,
    /// The active tab again, after it.
    Duplicate,
    /// The terminals' text a point larger (1), smaller (-1), or back to its default (0).
    Zoom(i8),
    /// The window closed.
    Quit,
}

/// Where a split puts the new space, beside the focused one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Right,
    Left,
    Up,
    Down,
}

impl Action for Command {
    const NAME: &'static str = "oneterm::Command";
}
