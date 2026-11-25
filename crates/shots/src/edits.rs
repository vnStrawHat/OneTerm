//! The editor space's scenes (specs/oneterm, "Editing in OneTerm"): a tab connected by a key to the
//! engine's file server on 127.0.0.1 (never another host), its files under `.tmp/ssh-tests`;
//! Edit… on a file of the dock opens it beside the terminal, then a line is typed, and in
//! `edit-diff` the diff shows.

use oneterm::{Launch, Model, Remote, remote_shell_with};
use oneterm_ssh::testing::{folder, options, serve_files, trust, write_user_key};
use oneterm_ssh::{Auth, Target};
use quadra_platform::{HeadlessPlatform, MouseButton, PlatformEvent, PointerKind, WindowId};
use quadra_prim::Point;
use quadra_term::Session;

use super::{Shots, click, pass, shows};

pub const SCENES: [&str; 2] = ["edit", "edit-diff"];

const FILE: &str = "# The gateway's deployment\nservice: neo-gateway\nreplicas: 3\nimage:\n  name: neo-gateway\n  tag: 8e77d10\nports:\n  - 8080\n  - 9090 # metrics\nenv:\n  LOG_LEVEL: info\n  REGION: ap-southeast-1\n";

/// The scenes' folder, `folder("shots-edit")`, which [`start`] empties.
fn dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.tmp/ssh-tests/shots-edit")
}

/// The session of a launch to the test server, signed in by the key of the scenes' folder.
pub fn session(launch: Launch) -> Session {
    remote_shell_with(launch, |target| {
        let mut o = options(target.port, &dir(), Auth::Auto);
        o.target = target;
        o
    })
}

/// A press of a button on what is named.
fn press(platform: &HeadlessPlatform, app: &Shots, window: WindowId, name: &str, button: MouseButton) {
    let Some(tree) = app.tree(window) else { return };
    let nodes = tree.semantic_tree().nodes;
    let Some(node) = nodes.iter().find(|n| n.semantics.name == name).and_then(|n| tree.node_of(n.id)) else {
        return;
    };
    let at = tree.window_rect(node).center();
    for (kind, button) in
        [(PointerKind::Move, MouseButton::None), (PointerKind::Down, button), (PointerKind::Up, button)]
    {
        let at = Point::new(at.x, at.y);
        platform.push(window, PlatformEvent::Pointer { kind, button, at, mods: Default::default() });
    }
}

/// Waits, 10 s at most, until a node is named with `words` in it.
fn until(platform: &HeadlessPlatform, app: &mut Shots, window: WindowId, words: &str) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !shows(app, window, words) && std::time::Instant::now() < end {
        pass(platform, app, 75);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Connects a new tab, opens `deploy.yaml` from its dock and changes a line.
pub fn start(model: &Model, scene: &str, platform: &HeadlessPlatform, app: &mut Shots, window: WindowId) {
    let dir = folder("shots-edit");
    let served = dir.join("served");
    let _ = std::fs::create_dir_all(served.join("home/logs"));
    let _ = std::fs::write(served.join("home/deploy.yaml"), FILE);
    let _ = std::fs::write(served.join("home/notes.md"), "# Notes\n");
    let (port, _) = serve_files(51, served);
    trust(port, 51, &dir.join("known_hosts"));
    write_user_key(&dir.join("id"));
    let target = Target { user: "tester".into(), host: "127.0.0.1".into(), port };
    model.connect_to(Remote::new(target, Auth::Key(dir.join("id"))), false);
    model.ui().dock.set(true);
    until(platform, app, window, "deploy.yaml");
    press(platform, app, window, "deploy.yaml", MouseButton::Right);
    pass(platform, app, 150);
    click(platform, app, window, "Edit\u{2026}");
    until(platform, app, window, "/home/deploy.yaml");
    pass(platform, app, 150);
    // The caret at the start: a line more there.
    platform.push(window, PlatformEvent::Text("# Reviewed\n".into()));
    pass(platform, app, 150);
    if scene == "edit-diff" {
        click(platform, app, window, "Diff");
    }
}
