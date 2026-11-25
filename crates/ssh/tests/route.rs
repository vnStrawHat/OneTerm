//! Jump hosts and local forwards (specs/oneterm, "M10 in detail") between servers of this process
//! on 127.0.0.1, never another host: a shell through a jump host, a jump host whose key
//! changed named in the error, a local port carried to a server's port, and a port that
//! cannot listen said in the space; a remote forward carried back here, and the SOCKS5 proxy
//! of a dynamic one.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use oneterm_ssh::testing::{connect, folder, options, serve, trust, write_user_key};
use oneterm_ssh::{Auth, Forward, Hop, Target, Way};

fn hop(port: u16, key: &std::path::Path) -> Hop {
    let target = Target { user: "tester".into(), host: "127.0.0.1".into(), port };
    Hop { target, auth: Auth::Key(key.to_path_buf()) }
}

#[test]
fn a_shell_opens_through_a_jump_host() {
    let ((jump, _), (port, _), dir) = (serve(21), serve(22), folder("jump"));
    let (known, id) = (dir.join("known_hosts"), dir.join("id"));
    trust(jump, 21, &known);
    trust(port, 22, &known);
    write_user_key(&id);
    let mut o = options(port, &dir, Auth::Key(id.clone()));
    o.jumps = vec![hop(jump, &id)];
    let c = connect(o);
    c.wait_for("welcome");
    c.ssh.write(b"hi");
    c.wait_for("echo:hi");
    c.ssh.write(b"exit");
    assert_eq!(c.code(), 7);
}

#[test]
fn a_jump_host_that_fails_is_named() {
    let ((jump, _), (port, _), dir) = (serve(23), serve(24), folder("jump-changed"));
    let (known, id) = (dir.join("known_hosts"), dir.join("id"));
    trust(jump, 25, &known);
    trust(port, 24, &known);
    write_user_key(&id);
    let mut o = options(port, &dir, Auth::Key(id.clone()));
    o.jumps = vec![hop(jump, &id)];
    let c = connect(o);
    let why = c.failed();
    let named =
        format!("jump host tester@127.0.0.1:{jump}: the host key of '[127.0.0.1]:{jump}' has changed");
    assert!(why.starts_with(&named), "{why}");
    assert_eq!(c.code(), 255);
    assert!(c.text().contains(&format!("[connection closed: {named}")), "{}", c.text());
}

#[test]
fn a_local_port_is_carried_and_a_taken_one_is_said() {
    let ((port, _), dir) = (serve(26), folder("forward"));
    let id = dir.join("id");
    trust(port, 26, &dir.join("known_hosts"));
    write_user_key(&id);
    let free = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let taken = TcpListener::bind("127.0.0.1:0").unwrap();
    let taken_port = taken.local_addr().unwrap().port();
    let mut o = options(port, &dir, Auth::Key(id));
    o.forwards = vec![
        Forward { way: Way::Local, listen: free, host: "127.0.0.1".into(), port },
        Forward { way: Way::Local, listen: taken_port, host: "127.0.0.1".into(), port },
    ];
    let c = connect(o);
    c.wait_for("welcome");
    c.wait_for(&format!("[forward {taken_port}:127.0.0.1:{port} could not listen"));
    // The server's own banner, through the forward.
    let mut socket = TcpStream::connect(("127.0.0.1", free)).unwrap();
    socket.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let mut banner = [0; 7];
    socket.read_exact(&mut banner).unwrap();
    assert_eq!(&banner, b"SSH-2.0");
    c.ssh.write(b"exit");
    assert_eq!(c.code(), 7);
}

#[test]
fn forwards_read_and_write_their_words() {
    let f = Forward::parse("8080:localhost:80").unwrap();
    assert_eq!((f.listen, f.host.as_str(), f.port), (8080, "localhost", 80));
    assert_eq!(f.label(), "8080:localhost:80");
    assert_eq!(Forward::parse("5432:[::1]:5432").unwrap().label(), "5432:[::1]:5432");
    for bad in ["8080", "8080:host", "x:host:80", "8080::80", "8080:host:99999"] {
        assert_eq!(Forward::parse(bad), None, "{bad}");
    }
}

/// Ports nothing listens on, for now: held together while they are read, so they differ.
fn free<const N: usize>() -> [u16; N] {
    let held: [TcpListener; N] = std::array::from_fn(|_| TcpListener::bind("127.0.0.1:0").unwrap());
    held.map(|it| it.local_addr().unwrap().port())
}

/// Connects to a port until it answers: a forward starts once the shell is open.
fn reach(port: u16) -> TcpStream {
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(socket) = TcpStream::connect(("127.0.0.1", port)) {
            socket.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
            return socket;
        }
        assert!(Instant::now() < end, "never listened on {port}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_remote_port_is_carried_back_and_a_socks_proxy_goes_out_from_the_server() {
    let ((port, _), dir) = (serve(27), folder("forward-back"));
    let id = dir.join("id");
    trust(port, 27, &dir.join("known_hosts"));
    write_user_key(&id);
    // What answers here: a word, then the connection ends.
    let here = TcpListener::bind("127.0.0.1:0").unwrap();
    let here_port = here.local_addr().unwrap().port();
    std::thread::spawn(move || {
        while let Ok((mut socket, _)) = here.accept() {
            let _ = socket.write_all(b"from here");
        }
    });
    let [remote, socks] = free();
    let mut o = options(port, &dir, Auth::Key(id));
    o.forwards = [format!("R{remote}:127.0.0.1:{here_port}"), format!("D{socks}")]
        .iter()
        .map(|it| Forward::parse(it).unwrap())
        .collect();
    let c = connect(o);
    c.wait_for("welcome");

    // The server's port, carried back to the listener here.
    let mut got = String::new();
    reach(remote).read_to_string(&mut got).unwrap();
    assert_eq!(got, "from here");

    // SOCKS5: no auth, CONNECT to the server's own port by name, its banner through it.
    let mut proxy = reach(socks);
    proxy.write_all(&[5, 1, 0]).unwrap();
    let mut chosen = [0; 2];
    proxy.read_exact(&mut chosen).unwrap();
    assert_eq!(chosen, [5, 0]);
    let mut ask = vec![5, 1, 0, 3, 9];
    ask.extend_from_slice(b"127.0.0.1");
    ask.extend_from_slice(&port.to_be_bytes());
    proxy.write_all(&ask).unwrap();
    let mut answer = [0; 10];
    proxy.read_exact(&mut answer).unwrap();
    assert_eq!(answer[..2], [5, 0], "done");
    let mut banner = [0; 7];
    proxy.read_exact(&mut banner).unwrap();
    assert_eq!(&banner, b"SSH-2.0");
    // BIND is not taken.
    let mut bind = reach(socks);
    bind.write_all(&[5, 1, 0]).unwrap();
    bind.read_exact(&mut chosen).unwrap();
    bind.write_all(&[5, 2, 0, 1, 127, 0, 0, 1, 0, 80]).unwrap();
    bind.read_exact(&mut answer).unwrap();
    assert_eq!(answer[..2], [5, 7]);
    c.ssh.write(b"exit");
    assert_eq!(c.code(), 7);
}
