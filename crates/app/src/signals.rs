//! What programs signal (specs/oneterm, "Signals"): OSC 7501 status, OSC 9;4 progress, the bell in
//! a space that is not looked at, OSC 9 notifications. A tab sums its spaces; seeing a tab
//! clears its bells and the "done" it showed.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use quadra::{Scope, Signal, effect, signal, timeout, untrack};
use quadra_design::Notice;
use quadra_term::vt::Progress;
use quadra_term::{ProgramState, Session};

use crate::model::{Model, Tab};

/// How long a bell flashes its space.
const FLASH: Duration = Duration::from_millis(120);

/// Whether a space flashes for its bell, kept with its session for its view.
pub(crate) struct Flash(pub(crate) Signal<bool>);

impl crate::model::Space {
    /// Whether its bell flashes it now.
    pub fn flashing(&self) -> bool {
        self.session.kept::<Flash>().is_some_and(|it| it.0.get())
    }
}

impl Tab {
    /// The strongest state of its spaces' programs; idle when the settings ignore states.
    pub fn status(&self) -> ProgramState {
        if self.prefs.status_shown.get() == 2 {
            return ProgramState::Idle;
        }
        self.spaces
            .with(|spaces| spaces.iter().map(|space| space.session.status().get()).max().unwrap_or_default())
    }

    /// The progress of the first space that reports one.
    pub fn progress(&self) -> Option<Progress> {
        self.spaces.with(|spaces| spaces.iter().find_map(|space| space.session.progress().get()))
    }

    /// The bells of its spaces not looked at yet.
    pub fn bells(&self) -> u32 {
        self.spaces.with(|spaces| spaces.iter().map(|space| space.session.bells().get()).sum())
    }
}

/// The word a state is said with, none for idle.
pub fn state_word(state: ProgramState) -> Option<&'static str> {
    match state {
        ProgramState::Idle => None,
        ProgramState::Done => Some("done"),
        ProgramState::Working => Some("working"),
        ProgramState::Error => Some("error"),
        ProgramState::Blocked => Some("blocked"),
    }
}

/// The part done of a progress, in percent, or none when it has no part (indeterminate).
pub fn percent(progress: Progress) -> Option<u8> {
    match progress {
        Progress::Set(n) | Progress::Error(n) | Progress::Paused(n) => Some(n.min(100)),
        _ => None,
    }
}

impl Model {
    /// Shows a tab: what it signalled is seen.
    pub fn show(&self, id: u64) {
        self.active().set(id);
        let Some(tab) = self.active_tab() else { return };
        for space in tab.spaces.get_untracked() {
            seen(&space.session);
        }
    }

    /// Follows what a new space's program signals: a bell in the space that has the focus is
    /// seen at once, a notification and a program that waits or failed raise a card.
    pub(crate) fn watch(&self, id: u64, session: &Session) {
        let (bells, prefs) = (session.bells(), self.prefs());
        let (looked_at, rang, flash, scope) = (self.clone(), Cell::new(0), signal(false), Scope::current());
        session.keep(Rc::new(Flash(flash)));
        effect(move || {
            // A new bell sounds or flashes, as the settings say.
            let n = bells.get();
            if n > rang.replace(n) {
                untrack(|| looked_at.ring(flash, scope));
            }
            // Bells the settings ignore are seen as they ring.
            if n > 0 && (prefs.bell.get() == 3 || looked_at.looks_at(id)) {
                bells.set(0);
            }
        });
        // A local shell's directory is the last one used.
        if session.kept::<crate::shells::ShellName>().is_some() {
            let (cwd, last) = (session.cwd(), prefs.last_dir);
            effect(move || {
                let at = cwd.get();
                // A bash of Windows says a folder its own way: the folder, for what starts there.
                if let Some(folder) = crate::local::integrate::windows_folder(&at) {
                    return cwd.set(folder);
                }
                if !at.is_empty() && untrack(|| last.get()) != at {
                    last.set(at);
                }
            });
        }
        self.follow_notices(id, session);
    }

    /// Whether a space is the focused one of the tab that shows.
    /// A bell's sound, or its space's flash, when the settings ask for one. The flash ends by
    /// a timer of the space's scope, which outlives the effect that rang.
    fn ring(&self, flash: Signal<bool>, scope: Scope) {
        match self.prefs().bell.get_untracked() {
            1 => self.play(quadra_shell::Sound::Default),
            2 => {
                flash.set(true);
                scope.run(|| timeout(FLASH, move || flash.set(false)));
            }
            _ => {}
        }
    }

    pub(crate) fn looks_at(&self, id: u64) -> bool {
        self.active_tab().and_then(|tab| tab.focused()).is_some_and(|space| space.id == id)
    }

    /// The title of the tab that holds a space.
    pub(crate) fn tab_name(&self, id: u64) -> String {
        let tabs = self.tabs().get_untracked();
        let tab =
            tabs.iter().find(|tab| tab.spaces.with_untracked(|spaces| spaces.iter().any(|it| it.id == id)));
        tab.map(|tab| untrack(|| tab.title())).unwrap_or_default()
    }

    /// A card, and one more unseen; locked, only counted (a card would show what it says).
    pub(crate) fn notify(&self, notice: Notice) {
        self.notify_card(notice);
    }

    /// As [`Model::notify`], returning the card's id when it shows.
    pub(crate) fn notify_card(&self, notice: Notice) -> Option<u64> {
        let card = (!self.lock().locked.get_untracked()).then(|| self.notices().notify(notice));
        self.unread().update(|n| *n += 1);
        card
    }
}

/// What a space signalled is seen: its bells, and a "done".
pub(crate) fn seen(session: &Session) {
    session.bells().set(0);
    if session.status().get_untracked() == ProgramState::Done {
        session.status().set(ProgramState::Idle);
    }
}

/// The count of what is not seen yet, for the title bar: bells everywhere.
pub fn all_bells(tabs: Signal<Vec<Tab>>) -> u32 {
    tabs.with(|tabs| tabs.iter().map(Tab::bells).sum())
}
