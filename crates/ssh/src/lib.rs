//! OneTerm's SSH (specs/oneterm, "SSH (P2)"): `russh` on a `tokio` runtime of its own, behind a
//! blocking API of the shape of `quadra_term::Pty`. What the engine asks (a host key, a
//! password) it asks the app, which answers with [`Ssh::answer`].

mod auth;
mod files;
mod folders;
mod forward;
mod hostkey;
mod prompt;
mod route;
mod session;
mod target;
#[cfg(feature = "testing")]
pub mod testing;
mod traffic;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use tokio::runtime::Runtime;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

pub use files::{Entry, Files, Stat, join, safe_name};
pub use hostkey::{key_changed, key_info, known_key, short_print};
use prompt::Cmd;
pub use prompt::{Ask, Event};
pub use target::{Target, os_user};

/// How to prove who the user is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Auth {
    /// The agent, the usual key files, then the password.
    #[default]
    Auto,
    Agent,
    Key(PathBuf),
    Password,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub target: Target,
    pub auth: Auth,
    /// The host keys known, OpenSSH's file.
    pub known_hosts: PathBuf,
    /// Where the usual key files are (`~/.ssh`).
    pub keys: PathBuf,
    /// The agent's pipe.
    pub agent: String,
    pub rows: u16,
    pub cols: u16,
    /// The jump hosts, in the order they are gone through (4 at most).
    pub jumps: Vec<Hop>,
    pub forwards: Vec<Forward>,
    /// A keepalive asked of the server while idle, every so often (`None`: never), and how
    /// many go unanswered before the connection is given up.
    pub keepalive: Option<Duration>,
    pub keepalive_max: usize,
}

/// A host on the way, and how to sign in to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hop {
    pub target: Target,
    pub auth: Auth,
}

/// Which way a forward carries connections (specs/oneterm, "M10 in detail").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Way {
    /// A port here carried to a host and port as the server sees them.
    #[default]
    Local,
    /// A port of the server carried to a host and port as this machine sees them.
    Remote,
    /// A SOCKS5 proxy here whose connections go out from the server.
    Dynamic,
}

/// A forward: its way, the port listened on (here, or on the server for `Remote`), and where
/// connections go (none for `Dynamic`: the proxy is told).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Forward {
    pub way: Way,
    pub listen: u16,
    pub host: String,
    pub port: u16,
}

impl Forward {
    /// `8080:localhost:80` or `L8080:localhost:80`, `R9000:localhost:3000`, `D1080`.
    pub fn parse(text: &str) -> Option<Forward> {
        let (way, rest) = match text.as_bytes().first()? {
            b'L' => (Way::Local, &text[1..]),
            b'R' => (Way::Remote, &text[1..]),
            b'D' => (Way::Dynamic, &text[1..]),
            _ => (Way::Local, text),
        };
        if way == Way::Dynamic {
            return Some(Forward { way, listen: rest.parse().ok()?, host: String::new(), port: 0 });
        }
        let (listen, rest) = rest.split_once(':')?;
        let (host, port) = rest.rsplit_once(':')?;
        let host = host.trim_start_matches('[').trim_end_matches(']');
        if host.is_empty() {
            return None;
        }
        Some(Forward { way, listen: listen.parse().ok()?, host: host.into(), port: port.parse().ok()? })
    }

    pub fn label(&self) -> String {
        let to = match self.host.contains(':') {
            true => format!("[{}]:{}", self.host, self.port),
            false => format!("{}:{}", self.host, self.port),
        };
        match self.way {
            Way::Local => format!("{}:{to}", self.listen),
            Way::Remote => format!("R{}:{to}", self.listen),
            Way::Dynamic => format!("D{}", self.listen),
        }
    }
}

impl Options {
    /// The user's own files (`%USERPROFILE%\.ssh`) and agent, 24 by 80.
    pub fn new(target: Target) -> Options {
        let home = std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default();
        let ssh = home.join(".ssh");
        Options {
            target,
            auth: Auth::Auto,
            known_hosts: ssh.join("known_hosts"),
            keys: ssh,
            agent: r"\\.\pipe\openssh-ssh-agent".into(),
            rows: 24,
            cols: 80,
            jumps: Vec::new(),
            forwards: Vec::new(),
            keepalive: Some(Duration::from_secs(30)),
            keepalive_max: 3,
        }
    }
}

/// What [`Ssh::files`] says before the shell is open.
pub const NOT_YET: &str = "Not connected yet";

/// A connection running a shell. Dropping it closes it.
pub struct Ssh {
    to: UnboundedSender<Cmd>,
    shared: Arc<Shared>,
}

/// What the connection's task and the handle share.
#[derive(Default)]
pub(crate) struct Shared {
    /// The last ping's time in microseconds; 0 before the first.
    latency: AtomicU32,
    /// The bytes over the network.
    pub(crate) traffic: Arc<traffic::Traffic>,
    /// The connection, once the shell is open.
    handle: Mutex<Option<Arc<russh::client::Handle<hostkey::Checker>>>>,
    files: Mutex<Option<Files>>,
    /// Gives up the connecting under way.
    cancel: tokio::sync::Notify,
}

impl Ssh {
    /// Starts connecting at once and returns: what the server says comes to `output`, how
    /// the connecting goes and what it asks to `told` (answered by [`Ssh::answer`]), both
    /// from the engine's threads; `exited` is told the shell's exit status, 255 when the
    /// connection was given up or dropped.
    pub fn connect(
        options: Options,
        output: impl Fn(&[u8]) + Send + Sync + 'static,
        told: impl Fn(Event) + Send + Sync + 'static,
        exited: impl FnOnce(i32) + Send + 'static,
    ) -> Ssh {
        let (to, from) = unbounded_channel();
        let shared = Arc::new(Shared::default());
        let (output, told) = (Arc::new(output), Arc::new(told));
        runtime().spawn(session::run(options, from, output, told, Box::new(exited), shared.clone()));
        Ssh { to, shared }
    }

    /// The answer to the question asked last ([`Event::Ask`]); `None` gives it up.
    pub fn answer(&self, answer: Option<String>) {
        let _ = self.to.send(Cmd::Answer(answer));
    }

    /// Gives up the connecting under way, at once: it fails `cancelled` (and is asked
    /// about as [`Ask::Failed`]). Nothing once the shell is open.
    pub fn cancel(&self) {
        self.shared.cancel.notify_one();
    }

    /// How long the last ping took, once the shell is open (every 5 s).
    pub fn latency(&self) -> Option<Duration> {
        let micros = self.shared.latency.load(Ordering::Relaxed);
        (micros > 0).then(|| Duration::from_micros(micros.into()))
    }

    /// The bytes received and sent over the network so far, the connecting's too.
    pub fn traffic(&self) -> (u64, u64) {
        let traffic = &self.shared.traffic;
        (traffic.received.load(Ordering::Relaxed), traffic.sent.load(Ordering::Relaxed))
    }

    /// Bytes typed: the shell's input; dropped before the shell is open.
    pub fn write(&self, bytes: &[u8]) {
        let _ = self.to.send(Cmd::Input(bytes.to_vec()));
    }

    /// The connection's files over SFTP, opened the first time; an error before the shell
    /// is open. It blocks: an app calls it on its pool.
    pub fn files(&self) -> Result<Files, String> {
        let mut files = self.shared.files.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(files) = &*files {
            return Ok(files.clone());
        }
        let handle = self.shared.handle.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let opened = Files::open(handle.ok_or(NOT_YET)?)?;
        *files = Some(opened.clone());
        Ok(opened)
    }

    pub fn resize(&self, rows: u16, cols: u16) {
        let _ = self.to.send(Cmd::Resize(rows, cols));
    }

    pub fn close(&self) {
        let _ = self.to.send(Cmd::Close);
        self.cancel();
    }
}

impl Drop for Ssh {
    fn drop(&mut self) {
        self.close();
    }
}

/// The engine's runtime: two threads, made with the first connection.
pub(crate) fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("ssh")
            .enable_all()
            .build()
            .expect("the SSH runtime starts")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwards_read_as_openssh_writes_them_and_back() {
        for text in ["8080:localhost:80", "R9000:localhost:3000", "D1080", "22:[::1]:22"] {
            assert_eq!(Forward::parse(text).map(|it| it.label()).as_deref(), Some(text));
        }
        assert_eq!(Forward::parse("L8080:db:5432").map(|it| it.label()).as_deref(), Some("8080:db:5432"));
        assert_eq!(Forward::parse("R1:h:2").unwrap().way, Way::Remote);
        for bad in ["", "D", "Dx", "8080", "8080::80", "R9000:h", "x:h:1"] {
            assert_eq!(Forward::parse(bad), None, "{bad}");
        }
    }
}
