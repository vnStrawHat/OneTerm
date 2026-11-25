//! The engine against a `russh` server in this process on 127.0.0.1 (specs/oneterm, "M7 in
//! detail"; never another host): host keys unknown, known and changed, the password and a
//! key file and its passphrase, a user not given, typing, a resize, the exit status, a
//! connection dropped, a cancel. The questions are answered as the app answers them. Every
//! file is under `.tmp/ssh-tests`; the user's agent and keys are never tried.

use std::time::{Duration, Instant};

use oneterm_ssh::testing::{connect, folder, key, options, serve};
use oneterm_ssh::{Ask, Auth, Event};
use russh::keys::known_hosts::learn_known_hosts_path;
use russh::keys::ssh_key::LineEnding;

#[test]
fn an_unknown_host_is_asked_then_the_password_opens_a_shell() {
    let ((port, seen), dir) = (serve(1), folder("unknown"));
    let c = connect(options(port, &dir, Auth::Password));
    let Ask::HostKey { host, kind, fingerprint } = c.ask() else { panic!("not the host key") };
    assert_eq!((host, kind.as_str()), (format!("[127.0.0.1]:{port}"), "ssh-ed25519"));
    assert_eq!(fingerprint, key(1).public_key().fingerprint(Default::default()).to_string());
    // Typed in the space before the shell: dropped.
    c.ssh.write(b"early");
    c.answer(Some("yes"));
    let target = format!("tester@127.0.0.1:{port}");
    assert_eq!(c.ask(), Ask::Password { target: target.clone(), again: false });
    c.answer(Some("secret"));
    c.wait_for("welcome");
    // The socket's bytes are counted, both ways.
    let (received, sent) = c.ssh.traffic();
    assert!(received > 1000 && sent > 1000, "{received} {sent}");
    let told = c.told.lock().unwrap().clone();
    assert_eq!(told.first(), Some(&Event::Connecting(target)));
    assert_eq!(told.last(), Some(&Event::Open));
    c.ssh.write(b"hi");
    c.wait_for("echo:hi");
    assert!(!c.text().contains("early"), "{}", c.text());
    let end = Instant::now() + Duration::from_secs(10);
    while c.ssh.latency().is_none() {
        assert!(Instant::now() < end, "no ping came back");
        std::thread::sleep(Duration::from_millis(5));
    }
    c.ssh.resize(30, 100);
    c.ssh.write(b"x");
    c.wait_for("echo:x");
    assert_eq!(*seen.lock().unwrap(), ["pty xterm-256color 80x24", "resize 100x30"]);
    c.ssh.write(b"exit");
    assert_eq!(c.code(), 7);
    let known = std::fs::read_to_string(dir.join("known_hosts")).unwrap();
    assert!(known.trim_start().starts_with(&format!("[127.0.0.1]:{port} ssh-ed25519 ")), "{known}");
}

#[test]
fn a_known_host_goes_on_and_a_key_file_signs_in() {
    let ((port, _), dir) = (serve(2), folder("known"));
    learn_known_hosts_path("127.0.0.1", port, key(2).public_key(), dir.join("known_hosts")).unwrap();
    let file = dir.join("id");
    std::fs::write(&file, key(9).to_openssh(LineEnding::LF).unwrap().as_bytes()).unwrap();
    let c = connect(options(port, &dir, Auth::Key(file)));
    c.wait_for("welcome");
    assert!(!c.told.lock().unwrap().iter().any(|it| matches!(it, Event::Ask(_))));
    c.ssh.write(b"exit");
    assert_eq!(c.code(), 7);
}

#[test]
fn a_changed_host_key_is_refused_without_asking() {
    let ((port, _), dir) = (serve(3), folder("changed"));
    learn_known_hosts_path("127.0.0.1", port, key(4).public_key(), dir.join("known_hosts")).unwrap();
    let c = connect(options(port, &dir, Auth::Password));
    let why = c.failed();
    assert!(why.starts_with(&format!("the host key of '[127.0.0.1]:{port}' has changed")), "{why}");
    assert!(oneterm_ssh::key_changed(&why) && !oneterm_ssh::key_changed("permission denied"));
    assert!(why.contains("is on line 2 of") && why.contains("known_hosts"), "{why}");
    assert_eq!(c.code(), 255);
    assert!(c.text().contains("[connection closed: the host key of"), "{}", c.text());
    assert_eq!(c.told.lock().unwrap().iter().filter(|it| matches!(it, Event::Ask(_))).count(), 1);
}

#[test]
fn wrong_passwords_and_a_dropped_connection_end_with_255() {
    let ((port, _), dir) = (serve(5), folder("denied"));
    learn_known_hosts_path("127.0.0.1", port, key(5).public_key(), dir.join("known_hosts")).unwrap();
    let c = connect(options(port, &dir, Auth::Auto));
    for again in [false, true, true] {
        assert!(matches!(c.ask(), Ask::Password { again: a, .. } if a == again));
        c.answer(Some("wrong"));
    }
    // Again from the start, then given up.
    assert_eq!(c.ask(), Ask::Failed { reason: "permission denied".into() });
    c.answer(Some(""));
    assert!(matches!(c.ask(), Ask::Password { again: false, .. }));
    c.answer(None);
    assert_eq!(c.failed(), "cancelled");
    assert_eq!(c.code(), 255);
    c.wait_for("[connection closed: cancelled]");

    let c = connect(options(port, &dir, Auth::Password));
    c.ask();
    c.answer(Some("secret"));
    c.wait_for("welcome");
    c.ssh.write(b"drop");
    assert_eq!(c.code(), 255);
    c.wait_for("[connection closed:");
}

#[test]
fn nobody_listening_is_said() {
    let dir = folder("refused");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let c = connect(options(port, &dir, Auth::Password));
    c.failed();
    assert_eq!(c.code(), 255);
    c.wait_for("[connection closed:");
}

#[test]
fn a_user_not_given_is_asked() {
    let ((port, _), dir) = (serve(6), folder("user"));
    learn_known_hosts_path("127.0.0.1", port, key(6).public_key(), dir.join("known_hosts")).unwrap();
    let mut o = options(port, &dir, Auth::Password);
    o.target.user = String::new();
    let c = connect(o);
    assert_eq!(c.ask(), Ask::User { host: format!("127.0.0.1:{port}"), user: oneterm_ssh::os_user() });
    c.answer(Some("tester"));
    assert_eq!(c.ask(), Ask::Password { target: format!("tester@127.0.0.1:{port}"), again: false });
    c.answer(Some("secret"));
    c.wait_for("welcome");
}

#[test]
fn an_account_with_no_password_is_let_in_and_an_empty_one_goes_by_its_prompts() {
    let ((port, _), dir) = (serve(8), folder("empty"));
    learn_known_hosts_path("127.0.0.1", port, key(8).public_key(), dir.join("known_hosts")).unwrap();
    // `none` lets it in: nothing is asked.
    let mut o = options(port, &dir, Auth::Password);
    o.target.user = "open".into();
    let c = connect(o);
    c.wait_for("welcome");
    assert!(!c.told.lock().unwrap().iter().any(|it| matches!(it, Event::Ask(_))));
    // A server of PAM: the password, empty, as the answer to its prompt; a wrong one is asked again.
    let mut o = options(port, &dir, Auth::Auto);
    o.target.user = "pam".into();
    let c = connect(o);
    assert!(matches!(c.ask(), Ask::Password { again: false, .. }));
    c.answer(Some("wrong"));
    assert!(matches!(c.ask(), Ask::Password { again: true, .. }));
    c.answer(Some(""));
    c.wait_for("welcome");
}

#[test]
fn a_cancel_gives_up_a_server_that_never_speaks() {
    let dir = folder("silent");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let c = connect(options(port, &dir, Auth::Password));
    let (_socket, _) = listener.accept().unwrap();
    let start = Instant::now();
    c.ssh.cancel();
    assert_eq!(c.failed(), "cancelled");
    assert!(start.elapsed() < Duration::from_secs(5), "{:?}", start.elapsed());
    assert_eq!(c.code(), 255);
}

#[test]
fn a_key_file_s_passphrase_is_asked_again_when_wrong() {
    let ((port, _), dir) = (serve(7), folder("passphrase"));
    learn_known_hosts_path("127.0.0.1", port, key(7).public_key(), dir.join("known_hosts")).unwrap();
    let file = dir.join("id");
    let locked = key(9).encrypt(&mut Fixed, "open sesame").unwrap();
    std::fs::write(&file, locked.to_openssh(LineEnding::LF).unwrap().as_bytes()).unwrap();
    let c = connect(options(port, &dir, Auth::Key(file.clone())));
    assert_eq!(c.ask(), Ask::Passphrase { file: file.clone(), again: false });
    c.answer(Some("wrong"));
    assert_eq!(c.ask(), Ask::Passphrase { file, again: true });
    c.answer(Some("open sesame"));
    c.wait_for("welcome");
}

/// Salt and nonce for a key encrypted by a test: no randomness wanted.
struct Fixed;

impl russh::keys::ssh_key::rand_core::TryRng for Fixed {
    type Error = std::convert::Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        Ok(7)
    }

    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        Ok(7)
    }

    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), Self::Error> {
        dst.fill(7);
        Ok(())
    }
}

impl russh::keys::ssh_key::rand_core::TryCryptoRng for Fixed {}
