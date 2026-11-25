//! For tests only (the feature `testing`): a `russh` server in the test process on
//! 127.0.0.1, with SFTP over a folder, and a client of the engine. Every file is under
//! `.tmp/ssh-tests`; the user's agent, keys and known hosts are never used.

mod client;
mod fs;
mod keys;

pub use client::{Client, connect};
pub use keys::{key, trust, write_locked_user_key, write_user_key};

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::{Auth, Options, Target};
use russh::keys::PublicKey;
use russh::server::{self, Auth as Verdict, Msg, Session, VerifiedPublicKey};
use russh::{Channel, ChannelId, MethodKind};

#[derive(Clone)]
struct Shell {
    user_key: PublicKey,
    seen: Arc<Mutex<Vec<String>>>,
    /// The folder SFTP serves.
    root: PathBuf,
    /// The channels opened, until one asks for SFTP.
    channels: Arc<Mutex<HashMap<ChannelId, Channel<Msg>>>>,
    /// The channels SFTP runs on: their data is not the shell's.
    sftp: Arc<Mutex<Vec<ChannelId>>>,
}

impl server::Handler for Shell {
    type Error = russh::Error;

    /// `open` has no password; `pam` takes its own (empty) by `keyboard-interactive` alone.
    async fn auth_none(&mut self, user: &str) -> Result<Verdict, Self::Error> {
        let pam = [MethodKind::KeyboardInteractive, MethodKind::PublicKey];
        Ok(match user {
            "open" => Verdict::Accept,
            "pam" => {
                Verdict::Reject { proceed_with_methods: Some((&pam[..]).into()), partial_success: false }
            }
            _ => Verdict::reject(),
        })
    }

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Verdict, Self::Error> {
        Ok(if (user, password) == ("tester", "secret") { Verdict::Accept } else { Verdict::reject() })
    }

    async fn auth_keyboard_interactive<'a>(
        &'a mut self,
        user: &str,
        _: &str,
        response: Option<server::Response<'a>>,
    ) -> Result<Verdict, Self::Error> {
        Ok(match (user, response) {
            ("pam", None) => Verdict::Partial {
                name: "".into(),
                instructions: "".into(),
                prompts: vec![("Password: ".into(), false)].into(),
            },
            ("pam", Some(mut answers)) => match answers.next() {
                Some(password) if password.is_empty() => Verdict::Accept,
                _ => Verdict::reject(),
            },
            _ => Verdict::reject(),
        })
    }

    async fn auth_publickey(
        &mut self,
        user: &str,
        key: VerifiedPublicKey<'_>,
    ) -> Result<Verdict, Self::Error> {
        Ok(if user == "tester" && *key.public_key == self.user_key {
            Verdict::Accept
        } else {
            Verdict::reject()
        })
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: russh::server::ChannelOpenHandle,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        self.channels.lock().unwrap().insert(channel.id(), channel);
        reply.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        _: ChannelId,
        term: &str,
        cols: u32,
        rows: u32,
        _: u32,
        _: u32,
        _: &[(russh::Pty, u32)],
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        self.seen.lock().unwrap().push(format!("pty {term} {cols}x{rows}"));
        Ok(())
    }

    async fn shell_request(&mut self, channel: ChannelId, session: &mut Session) -> Result<(), Self::Error> {
        session.data(channel, b"welcome\r\n".to_vec())?;
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        _: ChannelId,
        cols: u32,
        rows: u32,
        _: u32,
        _: u32,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        self.seen.lock().unwrap().push(format!("resize {cols}x{rows}"));
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        id: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let channel = self.channels.lock().unwrap().remove(&id);
        match channel.filter(|_| name == "sftp") {
            Some(channel) => {
                self.sftp.lock().unwrap().push(id);
                session.channel_success(id)?;
                russh_sftp::server::run(channel.into_stream(), fs::Fs::new(self.root.clone())).await;
            }
            None => session.channel_failure(id)?,
        }
        Ok(())
    }

    /// A remote forward: listens on this machine's loopback, each connection opened back to
    /// the client on a channel of its own.
    async fn tcpip_forward(
        &mut self,
        _: &str,
        port: &mut u32,
        session: &mut Session,
    ) -> Result<bool, Self::Error> {
        let Ok(listener) = tokio::net::TcpListener::bind(("127.0.0.1", *port as u16)).await else {
            return Ok(false);
        };
        *port = listener.local_addr().map_or(0, |it| it.port().into());
        let (handle, asked, quiet) = (session.handle(), *port, self.sftp.clone());
        tokio::spawn(async move {
            while let Ok((mut socket, from)) = listener.accept().await {
                let from = from.port().into();
                let Ok(channel) =
                    handle.channel_open_forwarded_tcpip("localhost", asked, "127.0.0.1", from).await
                else {
                    break;
                };
                quiet.lock().unwrap().push(channel.id());
                tokio::spawn(async move {
                    let _ = tokio::io::copy_bidirectional(&mut socket, &mut channel.into_stream()).await;
                });
            }
        });
        Ok(true)
    }

    /// Carries a channel to a host and port of this machine (only 127.0.0.1 is asked).
    async fn channel_open_direct_tcpip(
        &mut self,
        channel: Channel<Msg>,
        host: &str,
        port: u32,
        _from: &str,
        _from_port: u32,
        reply: server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        let Ok(mut socket) = tokio::net::TcpStream::connect((host, port as u16)).await else {
            return Ok(());
        };
        self.sftp.lock().unwrap().push(channel.id());
        reply.accept().await;
        tokio::spawn(async move {
            let mut stream = channel.into_stream();
            let _ = tokio::io::copy_bidirectional(&mut socket, &mut stream).await;
        });
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if self.sftp.lock().unwrap().contains(&channel) {
            return Ok(());
        }
        match data {
            b"exit" => {
                session.exit_status_request(channel, 7)?;
                session.close(channel)?;
            }
            b"drop" => return Err(russh::Error::Disconnect),
            _ => session.data(channel, [b"echo:", data].concat())?,
        }
        Ok(())
    }
}

/// A server on a free port with the host key of `seed`; it takes `tester` with the
/// password `secret` or the key of seed 9.
pub fn serve(seed: u8) -> (u16, Arc<Mutex<Vec<String>>>) {
    serve_files(seed, PathBuf::new())
}

/// As [`serve`], its SFTP serving `root`.
pub fn serve_files(seed: u8, root: PathBuf) -> (u16, Arc<Mutex<Vec<String>>>) {
    let rt = Box::leak(Box::new(tokio::runtime::Runtime::new().unwrap()));
    let listener = rt.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")).unwrap();
    let port = listener.local_addr().unwrap().port();
    let config = Arc::new(server::Config {
        keys: vec![key(seed)],
        auth_rejection_time: Duration::from_millis(1),
        auth_rejection_time_initial: Some(Duration::ZERO),
        ..Default::default()
    });
    let seen = Arc::new(Mutex::new(Vec::new()));
    let shell = Shell {
        user_key: key(9).public_key().clone(),
        seen: seen.clone(),
        root,
        channels: Default::default(),
        sftp: Default::default(),
    };
    rt.spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            let (config, shell) = (
                config.clone(),
                Shell { channels: Default::default(), sftp: Default::default(), ..shell.clone() },
            );
            tokio::spawn(async move {
                if let Ok(running) = server::run_stream(config, socket, shell).await {
                    let _ = running.await;
                }
            });
        }
    });
    (port, seen)
}

/// A folder of its own under `.tmp/ssh-tests`, emptied.
pub fn folder(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.tmp/ssh-tests").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

pub fn options(port: u16, dir: &std::path::Path, auth: Auth) -> Options {
    let mut options = Options::new(Target { user: "tester".into(), host: "127.0.0.1".into(), port });
    (options.auth, options.known_hosts, options.keys) = (auth, dir.join("known_hosts"), dir.join("keys"));
    options.agent = r"\\.\pipe\oneterm-tests-no-agent".into();
    options
}
