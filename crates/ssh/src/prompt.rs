//! What the engine asks the app (specs/oneterm, "M7 in detail"): an [`Ask`] told as an [`Event`]
//! from the engine's threads, answered by `Ssh::answer`; and what it writes to the space.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex as Slot};

use tokio::sync::Mutex;
use tokio::sync::mpsc::UnboundedReceiver;

/// What the engine tells the app while it connects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A hop is being reached: `user@host`.
    Connecting(String),
    /// A hop's host key is known or trusted: signing in to it, `user@host`.
    Authenticating(String),
    /// A question, answered by `Ssh::answer`.
    Ask(Ask),
    /// The shell is open.
    Open,
}

/// A question of the engine's. `Ssh::answer(None)` gives it up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    /// No user was given for `host`; `user` is the OS's, to fill in.
    User { host: String, user: String },
    /// A host key not in the known hosts: any answer trusts it, and it is written there.
    HostKey { host: String, kind: String, fingerprint: String },
    /// `again`: the one before was refused.
    Password { target: String, again: bool },
    /// The passphrase of a key file; `again`: the one before did not open it.
    Passphrase { file: PathBuf, again: bool },
    /// The connection ended before its shell opened: any answer connects again from the
    /// start, `None` gives up.
    Failed { reason: String },
}

/// What the app sends the engine.
pub(crate) enum Cmd {
    Input(Vec<u8>),
    Answer(Option<String>),
    Resize(u16, u16),
    Close,
}

pub(crate) type Out = Arc<dyn Fn(&[u8]) + Send + Sync>;
pub(crate) type Told = Arc<dyn Fn(Event) + Send + Sync>;

pub(crate) struct Prompt {
    out: Out,
    told: Told,
    pub(crate) input: Mutex<UnboundedReceiver<Cmd>>,
    /// Rows in the high half, columns in the low.
    size: AtomicU32,
    /// The space was closed: every question is given up.
    closed: AtomicBool,
    /// The shell opened.
    opened: AtomicBool,
    /// Why a host key was refused, for the error the connection fails with.
    pub(crate) why: Slot<Option<String>>,
}

impl Prompt {
    pub(crate) fn new(out: Out, told: Told, input: UnboundedReceiver<Cmd>, rows: u16, cols: u16) -> Prompt {
        Prompt {
            out,
            told,
            input: Mutex::new(input),
            size: AtomicU32::new(pack(rows, cols)),
            closed: AtomicBool::new(false),
            opened: AtomicBool::new(false),
            why: Slot::new(None),
        }
    }

    /// Writes to the space; a line feed is a new line.
    pub(crate) fn say(&self, text: &str) {
        (self.out)(text.replace("\r\n", "\n").replace('\n', "\r\n").as_bytes());
    }

    pub(crate) fn write(&self, bytes: &[u8]) {
        (self.out)(bytes);
    }

    pub(crate) fn tell(&self, event: Event) {
        if event == Event::Open {
            self.opened.store(true, Ordering::Relaxed);
        }
        (self.told)(event);
    }

    pub(crate) fn opened(&self) -> bool {
        self.opened.load(Ordering::Relaxed)
    }

    pub(crate) fn closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    /// Rows and columns.
    pub(crate) fn size(&self) -> (u16, u16) {
        let both = self.size.load(Ordering::Relaxed);
        ((both >> 16) as u16, both as u16)
    }

    /// Asks the app; its answer, or None when it gave up or the space was closed. What is
    /// typed in the space meanwhile is dropped.
    pub(crate) async fn ask(&self, ask: Ask) -> Option<String> {
        if self.closed() {
            return None;
        }
        // An answer sent twice to the question before is not this one's.
        let mut input = self.input.lock().await;
        while let Ok(cmd) = input.try_recv() {
            if !self.take(cmd) {
                return None;
            }
        }
        self.tell(Event::Ask(ask));
        loop {
            match input.recv().await? {
                Cmd::Answer(answer) => return answer,
                cmd => {
                    if !self.take(cmd) {
                        return None;
                    }
                }
            }
        }
    }

    /// Takes in what the app sent while no question is asked (a size; an answer late, or
    /// typing, is dropped). False once the space is closed.
    pub(crate) fn take(&self, cmd: Cmd) -> bool {
        match cmd {
            Cmd::Input(_) | Cmd::Answer(_) => {}
            Cmd::Resize(rows, cols) => self.size.store(pack(rows, cols), Ordering::Relaxed),
            Cmd::Close => self.closed.store(true, Ordering::Relaxed),
        }
        !self.closed()
    }
}

fn pack(rows: u16, cols: u16) -> u32 {
    u32::from(rows) << 16 | u32::from(cols)
}
