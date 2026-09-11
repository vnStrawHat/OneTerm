//! Loopback SSH + SFTP server for manual walks of the SFTP browser (developer
//! diagnostic, never shipped).
//!
//! Accepts any user name and password, answers a shell request with a plain
//! echo, and serves one local directory over the SFTP subsystem so OneTerm can
//! be pointed at `127.0.0.1:<port>` without a real host.
//!
//! ```text
//! cargo run -p oneterm-tools --bin sftp-dev-server -- [--port 2222] [--root DIR]
//! ```
//!
//! Without `--root` a fresh temp directory with a few sample files is served.
//! The host key is random per run, so the client sees an unknown host each time.

use std::collections::{HashMap, HashSet};
use std::fs::{Metadata, OpenOptions};
use std::io::{self, Read as _, Seek as _, SeekFrom, Write as _};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use russh::server::{Auth, Msg, Server, Session};
use russh::{Channel, ChannelId};
use russh_sftp::protocol::{
    Attrs, Data, File, FileAttributes, Handle, Name, OpenFlags, Status, StatusCode, Version,
};
use russh_sftp::server::StatusReply;

// ── SSH side ─────────────────────────────────────────────────

#[derive(Clone)]
struct DevServer {
    root: PathBuf,
}

impl Server for DevServer {
    type Handler = ClientHandler;

    fn new_client(&mut self, peer: Option<SocketAddr>) -> Self::Handler {
        println!("client connected: {peer:?}");
        ClientHandler {
            root: self.root.clone(),
            channels: HashMap::new(),
            sftp_channels: HashSet::new(),
        }
    }
}

struct ClientHandler {
    root: PathBuf,
    /// Session channels by id; the SFTP subsystem takes its channel out.
    channels: HashMap<ChannelId, Channel<Msg>>,
    /// Channels running the SFTP subsystem: their bytes belong to the SFTP
    /// stream and must never be echoed back.
    sftp_channels: HashSet<ChannelId>,
}

impl russh::server::Handler for ClientHandler {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, _password: &str) -> Result<Auth, Self::Error> {
        println!("password auth for {user:?}: accepted");
        Ok(Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        self.channels.insert(channel.id(), channel);
        Ok(true)
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }

    async fn env_request(
        &mut self,
        channel: ChannelId,
        _name: &str,
        _value: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        session.data(
            channel,
            "sftp-dev-server: shell is an echo only; use the SFTP browser.\r\n$ ".as_bytes(),
        )?;
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        // Echo typed bytes so the terminal shows something. SFTP channel
        // bytes are protocol packets consumed by the SFTP stream, not input.
        if self.sftp_channels.contains(&channel) {
            return Ok(());
        }
        let echoed: Vec<u8> = data
            .iter()
            .flat_map(|&b| {
                if b == b'\r' {
                    vec![b'\r', b'\n', b'$', b' ']
                } else {
                    vec![b]
                }
            })
            .collect();
        session.data(channel, echoed)?;
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        channel_id: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if name != "sftp" {
            session.channel_failure(channel_id)?;
            return Ok(());
        }
        let Some(channel) = self.channels.remove(&channel_id) else {
            session.channel_failure(channel_id)?;
            return Ok(());
        };
        session.channel_success(channel_id)?;
        self.sftp_channels.insert(channel_id);
        println!("sftp subsystem started");
        let handler = SftpHandler {
            root: self.root.clone(),
            dirs: HashMap::new(),
            files: HashMap::new(),
            next_handle: 1,
        };
        tokio::spawn(russh_sftp::server::run(channel.into_stream(), handler));
        Ok(())
    }
}

// ── SFTP side ────────────────────────────────────────────────

struct SftpHandler {
    root: PathBuf,
    /// Open directory listings: handle → (sent, entries).
    dirs: HashMap<String, (bool, Vec<File>)>,
    files: HashMap<String, std::fs::File>,
    next_handle: u64,
}

fn status_ok(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: "Ok".to_string(),
        language_tag: "en-US".to_string(),
    }
}

fn io_reply(error: io::Error) -> StatusReply {
    let code = match error.kind() {
        io::ErrorKind::NotFound => StatusCode::NoSuchFile,
        io::ErrorKind::PermissionDenied => StatusCode::PermissionDenied,
        _ => StatusCode::Failure,
    };
    StatusReply::new(code).with_message(error.to_string())
}

impl SftpHandler {
    /// Normalised virtual components of a client path (`/a/../b` → `["b"]`).
    fn components(path: &str) -> Vec<String> {
        let mut parts: Vec<String> = Vec::new();
        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                other => parts.push(other.to_string()),
            }
        }
        parts
    }

    fn virtual_path(parts: &[String]) -> String {
        format!("/{}", parts.join("/"))
    }

    /// Map a client path onto the served directory (never escapes `root`).
    fn resolve(&self, path: &str) -> PathBuf {
        let mut real = self.root.clone();
        for part in Self::components(path) {
            real.push(part);
        }
        real
    }

    fn new_handle(&mut self) -> String {
        let handle = self.next_handle.to_string();
        self.next_handle += 1;
        handle
    }

    fn attrs_of(meta: &Metadata) -> FileAttributes {
        FileAttributes::from(meta)
    }

    fn stat_path(&self, id: u32, path: &str) -> Result<Attrs, StatusReply> {
        let meta = std::fs::metadata(self.resolve(path)).map_err(io_reply)?;
        Ok(Attrs {
            id,
            attrs: Self::attrs_of(&meta),
        })
    }
}

impl russh_sftp::server::Handler for SftpHandler {
    type Error = StatusReply;

    fn unimplemented(&self) -> Self::Error {
        StatusReply::new(StatusCode::OpUnsupported)
    }

    async fn init(
        &mut self,
        _version: u32,
        _extensions: HashMap<String, String>,
    ) -> Result<Version, Self::Error> {
        Ok(Version::new())
    }

    async fn realpath(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
        eprintln!("sftp realpath {path:?}");
        let parts = Self::components(&path);
        Ok(Name {
            id,
            files: vec![File::dummy(Self::virtual_path(&parts))],
        })
    }

    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        eprintln!("sftp stat {path:?}");
        self.stat_path(id, &path)
    }

    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        eprintln!("sftp lstat {path:?}");
        self.stat_path(id, &path)
    }

    async fn fstat(&mut self, id: u32, handle: String) -> Result<Attrs, Self::Error> {
        eprintln!("sftp fstat #{handle}");
        let file = self
            .files
            .get(&handle)
            .ok_or_else(|| StatusReply::new(StatusCode::BadMessage))?;
        let meta = file.metadata().map_err(io_reply)?;
        Ok(Attrs {
            id,
            attrs: Self::attrs_of(&meta),
        })
    }

    async fn setstat(
        &mut self,
        id: u32,
        _path: String,
        _attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        eprintln!("sftp setstat {_path:?}");
        Ok(status_ok(id))
    }

    async fn fsetstat(
        &mut self,
        id: u32,
        _handle: String,
        _attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        eprintln!("sftp fsetstat #{_handle}");
        Ok(status_ok(id))
    }

    async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, Self::Error> {
        eprintln!("sftp opendir {path:?}");
        let dir = self.resolve(&path);
        let mut files = Vec::new();
        for entry in std::fs::read_dir(&dir).map_err(io_reply)? {
            let entry = entry.map_err(io_reply)?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let attrs = match std::fs::metadata(entry.path()) {
                Ok(meta) => Self::attrs_of(&meta),
                Err(_) => FileAttributes::default(),
            };
            files.push(File::new(name, attrs));
        }
        let handle = self.new_handle();
        self.dirs.insert(handle.clone(), (false, files));
        Ok(Handle { id, handle })
    }

    async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, Self::Error> {
        eprintln!("sftp readdir #{handle}");
        let (sent, files) = self
            .dirs
            .get_mut(&handle)
            .ok_or_else(|| StatusReply::new(StatusCode::BadMessage))?;
        if *sent {
            return Err(StatusReply::new(StatusCode::Eof));
        }
        *sent = true;
        Ok(Name {
            id,
            files: files.clone(),
        })
    }

    async fn close(&mut self, id: u32, handle: String) -> Result<Status, Self::Error> {
        eprintln!("sftp close #{handle}");
        self.dirs.remove(&handle);
        self.files.remove(&handle);
        Ok(status_ok(id))
    }

    async fn open(
        &mut self,
        id: u32,
        filename: String,
        pflags: OpenFlags,
        _attrs: FileAttributes,
    ) -> Result<Handle, Self::Error> {
        eprintln!("sftp open {filename:?} {pflags:?}");
        let path = self.resolve(&filename);
        let file = OpenOptions::new()
            .read(pflags.contains(OpenFlags::READ))
            .write(pflags.contains(OpenFlags::WRITE))
            .append(pflags.contains(OpenFlags::APPEND))
            .create(pflags.contains(OpenFlags::CREATE))
            .truncate(pflags.contains(OpenFlags::TRUNCATE))
            .open(&path)
            .map_err(io_reply)?;
        let handle = self.new_handle();
        self.files.insert(handle.clone(), file);
        Ok(Handle { id, handle })
    }

    async fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> Result<Data, Self::Error> {
        eprintln!("sftp read #{handle} @{offset} len {len}");
        let file = self
            .files
            .get_mut(&handle)
            .ok_or_else(|| StatusReply::new(StatusCode::BadMessage))?;
        file.seek(SeekFrom::Start(offset)).map_err(io_reply)?;
        let mut data = vec![0u8; len as usize];
        let mut filled = 0;
        while filled < data.len() {
            let n = file.read(&mut data[filled..]).map_err(io_reply)?;
            if n == 0 {
                break;
            }
            filled += n;
        }
        if filled == 0 {
            return Err(StatusReply::new(StatusCode::Eof));
        }
        data.truncate(filled);
        Ok(Data { id, data })
    }

    async fn write(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, Self::Error> {
        eprintln!("sftp write #{handle} @{offset} {} bytes", data.len());
        let file = self
            .files
            .get_mut(&handle)
            .ok_or_else(|| StatusReply::new(StatusCode::BadMessage))?;
        file.seek(SeekFrom::Start(offset)).map_err(io_reply)?;
        file.write_all(&data).map_err(io_reply)?;
        Ok(status_ok(id))
    }

    async fn mkdir(
        &mut self,
        id: u32,
        path: String,
        _attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        eprintln!("sftp mkdir {path:?}");
        std::fs::create_dir(self.resolve(&path)).map_err(io_reply)?;
        Ok(status_ok(id))
    }

    async fn remove(&mut self, id: u32, filename: String) -> Result<Status, Self::Error> {
        eprintln!("sftp remove {filename:?}");
        std::fs::remove_file(self.resolve(&filename)).map_err(io_reply)?;
        Ok(status_ok(id))
    }

    async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, Self::Error> {
        eprintln!("sftp rmdir {path:?}");
        std::fs::remove_dir(self.resolve(&path)).map_err(io_reply)?;
        Ok(status_ok(id))
    }

    async fn rename(
        &mut self,
        id: u32,
        oldpath: String,
        newpath: String,
    ) -> Result<Status, Self::Error> {
        eprintln!("sftp rename {oldpath:?} -> {newpath:?}");
        std::fs::rename(self.resolve(&oldpath), self.resolve(&newpath)).map_err(io_reply)?;
        Ok(status_ok(id))
    }
}

// ── main ─────────────────────────────────────────────────────

fn seed_sample_root() -> io::Result<PathBuf> {
    let root = std::env::temp_dir().join(format!("oneterm-sftp-dev-{}", std::process::id()));
    std::fs::create_dir_all(root.join("docs"))?;
    std::fs::create_dir_all(root.join("logs"))?;
    std::fs::create_dir_all(root.join("bin"))?;
    std::fs::write(
        root.join("README.txt"),
        "Served by sftp-dev-server. Edit, upload, download at will.\n",
    )?;
    std::fs::write(root.join("docs/notes.md"), "# Notes\n\n- sample\n")?;
    std::fs::write(
        root.join("logs/app.log"),
        "2026-09-11 12:00:00 INFO started\n".repeat(200),
    )?;
    std::fs::write(root.join("config.toml"), "[server]\nport = 2222\n")?;
    Ok(root)
}

fn usage() -> ! {
    eprintln!("usage: sftp-dev-server [--port PORT] [--root DIR]");
    std::process::exit(2)
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut port: u16 = 2222;
    let mut root: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => {
                port = args
                    .next()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or_else(|| usage())
            }
            "--root" => root = Some(PathBuf::from(args.next().unwrap_or_else(|| usage()))),
            _ => usage(),
        }
    }
    let root = match root {
        Some(root) if Path::new(&root).is_dir() => root,
        Some(root) => {
            eprintln!("--root {} is not a directory", root.display());
            std::process::exit(2)
        }
        None => seed_sample_root()?,
    };

    let host_key =
        russh::keys::PrivateKey::random(&mut rand::rng(), russh::keys::Algorithm::Ed25519)
            .expect("host key generation");
    let config = Arc::new(russh::server::Config {
        keys: vec![host_key],
        auth_rejection_time: Duration::ZERO,
        auth_rejection_time_initial: Some(Duration::ZERO),
        ..Default::default()
    });
    println!(
        "serving {} on 127.0.0.1:{port} (any user / any password)",
        root.display()
    );
    let mut server = DevServer { root };
    server.run_on_address(config, ("127.0.0.1", port)).await
}
