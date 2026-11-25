//! A program's notifications as cards (specs/oneterm, "Signals"): OSC 9 and 777 words, and OSC 99
//! in full: buttons, opening, closing and the reports a program asked for, a card that takes
//! the place of one with its id, the program's close and its question of what still shows,
//! when it wants to show, its icon and its sound.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use quadra::{Window, untrack, use_context};
use quadra_design::{Notice, Tone, icons};
use quadra_shell::{Sound, Sounds};
use quadra_term::vt::input::NotificationReport;
use quadra_term::vt::{
    NotificationExpiry, NotificationOccasion, NotificationSource, NotificationSupport, NotificationUrgency,
};
use quadra_term::{Notice as Told, Session};

use crate::model::Model;

/// How long a card lasts unless the setting keeps it or the program says.
const NOTICE: Duration = Duration::from_secs(6);

/// How many icons a space's program has kept by their ids: each may be 1 MiB.
const ICONS: usize = 8;

/// The sounds a program may name: the system's for that kind, `silent` none.
const SOUNDS: [&str; 7] = ["system", "silent", "info", "warning", "warn", "error", "question"];

/// What notifications reach outside the model: the window's state and the system's sounds,
/// taken where the window's view is built.
pub(crate) struct Outside {
    window: Option<Window>,
    sounds: Option<Rc<dyn Sounds>>,
}

impl Outside {
    pub(crate) fn here() -> Outside {
        Outside { window: use_context::<Window>(), sounds: use_context::<Rc<dyn Sounds>>() }
    }
}

/// What OneTerm does with an OSC 99 notification, from which the engine answers `p=?`.
pub fn abilities() -> NotificationSupport {
    NotificationSupport {
        focus: true,
        report: true,
        close_events: true,
        unfocused: true,
        invisible: true,
        icons: true,
        buttons: true,
        close: true,
        alive: true,
        sounds: SOUNDS.into_iter().map(Into::into).collect(),
        urgency: true,
        expiry: true,
    }
}

/// The kit's icon of a name a program gives.
fn named(name: &str) -> Option<&'static str> {
    match name {
        "error" => Some(icons::CIRCLE_X),
        "warn" | "warning" => Some(icons::TRIANGLE_ALERT),
        "info" => Some(icons::INFO),
        "question" | "help" => Some(icons::CIRCLE_HELP),
        _ => None,
    }
}

/// The system's sound of a name a program gives; none for none and `silent`.
fn sound(name: &str) -> Option<Sound> {
    Some(match name {
        "" | "silent" => return None,
        "info" => Sound::Info,
        "warn" | "warning" => Sound::Warning,
        "error" => Sound::Error,
        "question" => Sound::Question,
        _ => Sound::Default,
    })
}

/// Icon data a space's program sent, by the ids it gave, the oldest first.
type Kept = Vec<(String, Arc<[u8]>)>;

/// The icon data a notification brings, kept under its id; or what was kept under the id it
/// names alone.
fn picture(kept: &RefCell<Kept>, n: &Told) -> Option<Arc<[u8]>> {
    let mut kept = kept.borrow_mut();
    if n.icon.is_empty() {
        return kept.iter().find(|it| !n.icon_id.is_empty() && it.0 == n.icon_id).map(|it| it.1.clone());
    }
    if !n.icon_id.is_empty() {
        kept.retain(|it| it.0 != n.icon_id);
        if kept.len() == ICONS {
            kept.remove(0);
        }
        kept.push((n.icon_id.clone(), n.icon.clone()));
    }
    Some(n.icon.clone())
}

impl Model {
    /// Shows a space's notifications as cards and answers what its program asks of them.
    pub(crate) fn follow_notices(&self, id: u64, session: &Session) {
        // The cards that show, by the id the program gave them.
        let cards: Rc<RefCell<Vec<(String, u64)>>> = Rc::default();
        let (me, shown, kept) = (self.clone(), cards.clone(), RefCell::default());
        session.on_notification(move |n| {
            let lasts = match me.prefs().notices.get_untracked() {
                0 => Some(NOTICE),
                1 => None,
                _ => return, // silent
            };
            let picture = picture(&kept, n);
            if untrack(|| me.held_back(id, n.occasion)) {
                return;
            }
            let mut card = me.card_of(id, n, lasts, &shown);
            if let Some(svg) = n.icon_names.iter().find_map(|name| named(name)) {
                card = card.icon(svg);
            } else if let Some(file) = picture {
                card = card.picture(file);
            }
            let held = shown.borrow().iter().find(|it| !n.id.is_empty() && it.0 == n.id).map(|it| it.1);
            let card = match held.filter(|&at| me.notices().shows(at)) {
                Some(at) => Some(me.notices().replace(at, card)),
                None => me.notify_card(card),
            };
            if let (Some(_), Some(sound)) = (card, sound(&n.sound)) {
                me.play(sound);
            }
            let mut shown = shown.borrow_mut();
            shown.retain(|it| it.0 != n.id || n.id.is_empty());
            shown.extend(card.filter(|_| !n.id.is_empty()).map(|card| (n.id.clone(), card)));
        });
        let (me, shown) = (self.clone(), cards.clone());
        session.on_notice_close(move |asked| {
            let card = shown.borrow().iter().find(|it| it.0 == asked).map(|it| it.1);
            card.into_iter().for_each(|card| me.notices().close(card));
        });
        let me = self.clone();
        session.on_notice_alive(move |asked| {
            let notices = me.notices();
            let alive: Vec<String> =
                cards.borrow().iter().filter(|it| notices.shows(it.1)).map(|it| it.0.clone()).collect();
            let alive: Vec<&str> = alive.iter().map(String::as_str).collect();
            me.tell(id, NotificationReport::Alive { id: asked, alive: &alive });
        });
    }

    /// The card of a notification: its words, its buttons, what opening and closing it tell.
    fn card_of(
        &self,
        id: u64,
        n: &Told,
        lasts: Option<Duration>,
        shown: &Rc<RefCell<Vec<(String, u64)>>>,
    ) -> Notice {
        // The title, else the text in its place; then where it came from.
        let (title, words) = match n.title.is_empty() {
            true => (n.body.clone(), String::new()),
            false => (n.title.clone(), n.body.clone()),
        };
        let critical = n.urgency == Some(NotificationUrgency::Critical);
        let lasts = match n.expiry {
            _ if critical => None,
            NotificationExpiry::Never => None,
            NotificationExpiry::After(ms) => Some(Duration::from_millis(ms.into())),
            NotificationExpiry::Platform => lasts,
        };
        let source = match n.source {
            Some(NotificationSource::Osc777) => "OSC 777",
            Some(NotificationSource::Osc99) => "OSC 99",
            _ => "OSC 9",
        };
        let mut card = Notice::new(title).body(words).note(format!("{} \u{B7} {source}", self.tab_name(id)));
        card = card.tone(if critical { Tone::Danger } else { Tone::Info });
        if let Some(time) = lasts {
            card = card.lasts(time);
        }
        for (at, label) in n.buttons.iter().enumerate() {
            let (me, told) = (self.clone(), n.clone());
            card = card.action(label, move || {
                if told.report_activation {
                    me.tell(id, NotificationReport::Button { id: &told.id, button: at as u32 + 1 });
                }
            });
        }
        let (me, told) = (self.clone(), n.clone());
        card = card.on_open(move || {
            if told.focus {
                me.reveal(id);
            }
            if told.report_activation {
                me.tell(id, NotificationReport::Activated { id: &told.id });
            }
        });
        let (me, told, shown) = (self.clone(), n.clone(), shown.clone());
        card.on_close(move || {
            shown.borrow_mut().retain(|it| it.0 != told.id);
            if told.report_close {
                me.tell(id, NotificationReport::Closed { id: &told.id });
            }
        })
    }

    /// Plays one of the system's sounds, where the app has them.
    pub(crate) fn play(&self, sound: Sound) {
        if let Some(sounds) = &self.outside().sounds {
            sounds.play(sound);
        }
    }

    /// Whether a notification that wants to show only while its space lacks the keyboard, or
    /// is not seen, waits for nothing: it is dropped. A covered window counts as seen.
    fn held_back(&self, id: u64, occasion: NotificationOccasion) -> bool {
        let window = self.outside().window.as_ref();
        let (active, minimized) = window.map_or((true, false), |w| (w.is_active(), w.is_minimized()));
        let settings = self.ui().settings.get();
        let shown = || {
            !settings && self.active_tab().is_some_and(|tab| tab.spaces.get().iter().any(|it| it.id == id))
        };
        match occasion {
            NotificationOccasion::Always => false,
            NotificationOccasion::Unfocused => active && !settings && self.looks_at(id),
            NotificationOccasion::Invisible => active || (!minimized && shown()),
        }
    }

    /// Writes a report to the program of a space, while it is there.
    fn tell(&self, id: u64, report: NotificationReport) {
        let tabs = self.tabs().get_untracked();
        let spaces = tabs.iter().flat_map(|tab| tab.spaces.get_untracked());
        if let Some(space) = spaces.into_iter().find(|space| space.id == id) {
            space.session.write(&report.encode());
        }
    }

    /// Shows the tab of a space and gives the space the focus.
    fn reveal(&self, id: u64) {
        let tabs = self.tabs().get_untracked();
        for tab in tabs {
            let at = tab.spaces.with_untracked(|spaces| spaces.iter().position(|space| space.id == id));
            if let Some(at) = at {
                self.show(tab.id);
                tab.focus.set(at);
                return;
            }
        }
    }
}
