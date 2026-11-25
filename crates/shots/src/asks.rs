//! The Connect dialogs' scenes (specs/oneterm, "M7 in detail"): a tab connected to the engine's
//! test servers on 127.0.0.1 (never another host), its files under `.tmp/ssh-tests`, left
//! at the question the scene shows.

use oneterm::{Launch, Model, Remote, remote_shell_with};
use oneterm_ssh::testing::{folder, options, serve, trust};
use oneterm_ssh::{Auth, Target};
use quadra_term::Session;

pub const SCENES: [&str; 4] = ["ask-connecting", "ask-host", "ask-sign-in", "ask-failed"];

/// The session of a launch to a test server.
pub fn session(launch: Launch) -> Session {
    remote_shell_with(launch, |target| {
        let mut o = options(target.port, &dir(), Auth::Password);
        o.target = target;
        o
    })
}

/// The scenes' folder, `folder("shots-asks")`, which [`start`] empties.
fn dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.tmp/ssh-tests/shots-asks")
}

/// Connects a new tab as the scene says; the words its dialog shows, to wait for.
pub fn start(model: &Model, scene: &str) -> &'static str {
    let dir = folder("shots-asks");
    let (port, words) = match scene {
        "ask-connecting" => {
            // Its connections queue and are never answered.
            let listener = Box::leak(Box::new(std::net::TcpListener::bind("127.0.0.1:0").unwrap()));
            (listener.local_addr().unwrap().port(), "Connecting to")
        }
        "ask-host" => (serve(41).0, "can't be established"),
        "ask-sign-in" => {
            let (port, _) = serve(42);
            trust(port, 42, &dir.join("known_hosts"));
            (port, "Password for")
        }
        _ => {
            let (port, _) = serve(43);
            trust(port, 44, &dir.join("known_hosts"));
            (port, "has changed")
        }
    };
    let target = Target { user: "tester".into(), host: "127.0.0.1".into(), port };
    model.connect_to(Remote::new(target, Auth::Password), false);
    words
}
