//! A transfer of the dock (specs/oneterm, "M9 in detail"): how far it is and how it ended; one
//! waiting its turn keeps its work until the dock starts it.

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use quadra::Signal;

/// What a transfer waiting its turn does once the dock starts it.
pub(crate) type Job = Box<dyn FnOnce(super::Dock)>;

/// A file or a folder going one way or the other.
pub(crate) struct Transfer {
    pub name: String,
    /// To the server.
    pub up: bool,
    /// The folder it goes to.
    pub to: String,
    pub done: Arc<AtomicU64>,
    pub whole: Arc<AtomicU64>,
    pub cancel: Arc<AtomicBool>,
    /// None while it runs; then how it ended.
    pub end: Signal<Option<Result<(), String>>>,
    /// Its work while it waits its turn; taken when it starts.
    pub job: RefCell<Option<Job>>,
}

impl Transfer {
    /// Whether it waits its turn.
    pub fn queued(&self) -> bool {
        self.job.borrow().is_some()
    }

    /// Whether it runs now.
    pub fn running(&self) -> bool {
        !self.queued() && self.end.get_untracked().is_none()
    }

    /// Its cross: a running one is cancelled; true when it goes from the list (it waited, or
    /// it ended).
    pub fn cross(&self) -> bool {
        self.cancel.store(true, Ordering::Relaxed);
        self.job.borrow_mut().take().is_some() || self.end.get_untracked().is_some()
    }

    /// `name → remote 48%`, `name ← local`, or why it failed.
    pub fn said(&self) -> String {
        let arrow = if self.up { "\u{2192} remote" } else { "\u{2190} local" };
        match self.end.get() {
            Some(Ok(())) => format!("{} {arrow} done", self.name),
            Some(Err(why)) => format!("{}: {why}", self.name),
            None if self.queued() => format!("{} {arrow} queued", self.name),
            None => {
                let (done, whole) = (self.done.load(Ordering::Relaxed), self.whole.load(Ordering::Relaxed));
                format!("{} {arrow} {}%", self.name, (done * 100).checked_div(whole).unwrap_or(0))
            }
        }
    }

    /// How much is done, from 0 to 1.
    pub fn part(&self) -> f32 {
        let (done, whole) = (self.done.load(Ordering::Relaxed), self.whole.load(Ordering::Relaxed));
        match self.end.get() {
            Some(Ok(())) => 1.0,
            _ if whole == 0 => 0.0,
            _ => done as f32 / whole as f32,
        }
    }

    /// `48%`, `queued`, `done` or `failed`.
    pub fn state(&self) -> String {
        match self.end.get() {
            Some(Ok(())) => "done".into(),
            Some(Err(_)) => "failed".into(),
            None if self.queued() => "queued".into(),
            None => format!("{:.0}%", self.part() * 100.0),
        }
    }
}
