//! A client of the engine for tests: what the server says, what the engine tells and asks,
//! answered as the app answers.

use std::cell::Cell;
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::{Ask, Event, Options, Ssh};

pub struct Client {
    pub ssh: Ssh,
    pub out: Arc<Mutex<Vec<u8>>>,
    pub told: Arc<Mutex<Vec<Event>>>,
    /// How many of the questions told were taken by [`Client::ask`].
    taken: Cell<usize>,
    exited: Receiver<i32>,
}

pub fn connect(options: Options) -> Client {
    let (out, told, (said, exited)) =
        (Arc::new(Mutex::new(Vec::new())), Arc::new(Mutex::new(Vec::new())), channel());
    let (into, tells) = (out.clone(), Arc::clone(&told));
    let ssh = Ssh::connect(
        options,
        move |bytes| into.lock().unwrap().extend_from_slice(bytes),
        move |event| tells.lock().unwrap().push(event),
        move |code| {
            let _ = said.send(code);
        },
    );
    Client { ssh, out, told, taken: Cell::new(0), exited }
}

fn soon(what: &str, mut done: impl FnMut() -> bool, said: impl Fn() -> String) {
    let end = Instant::now() + Duration::from_secs(20);
    while !done() {
        assert!(Instant::now() < end, "never {what}; said:\n{}", said());
        std::thread::sleep(Duration::from_millis(5));
    }
}

impl Client {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.out.lock().unwrap()).into_owned()
    }

    pub fn wait_for(&self, words: &str) {
        soon(&format!("said {words:?}"), || self.text().contains(words), || self.text());
    }

    /// The next question the engine asks.
    pub fn ask(&self) -> Ask {
        let end = Instant::now() + Duration::from_secs(20);
        loop {
            let told = self.told.lock().unwrap().clone();
            let mut asks =
                told.into_iter().filter_map(|it| if let Event::Ask(ask) = it { Some(ask) } else { None });
            if let Some(ask) = asks.nth(self.taken.get()) {
                self.taken.set(self.taken.get() + 1);
                return ask;
            }
            assert!(
                Instant::now() < end,
                "never asked; told {:?}\n{}",
                self.told.lock().unwrap(),
                self.text()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    pub fn answer(&self, answer: Option<&str>) {
        self.ssh.answer(answer.map(String::from));
    }

    /// The reason of a connection that failed before its shell; it is given up.
    pub fn failed(&self) -> String {
        match self.ask() {
            Ask::Failed { reason } => {
                self.answer(None);
                reason
            }
            other => panic!("asked {other:?}, not whether to connect again"),
        }
    }

    pub fn code(&self) -> i32 {
        self.exited.recv_timeout(Duration::from_secs(20)).expect("the connection ended")
    }
}
