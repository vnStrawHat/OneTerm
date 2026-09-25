//! `BUG-0076`: every remote file OneTerm opens must give its handle back to
//! russh-sftp's client-side count.
//!
//! russh-sftp refuses `open` once its own count reaches the server's
//! `limits@openssh.com` `max_open_handles`, and only an awaited `File::close()`
//! decrements that count; dropping a `File` closes the handle on the server but
//! not in the count. The server here advertises `max_open_handles = 2`, so any
//! site that drops instead of closing fails the third open with
//! "handle limit reached".

use std::collections::HashMap;
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};

use russh_sftp::extensions::{LIMITS, LimitsExtension};
use russh_sftp::protocol::{
    Attrs, Data, ExtendedReply, File, FileAttributes, Handle, Name, OpenFlags, Packet, Status,
    StatusCode, Version,
};
use tokio_util::sync::CancellationToken;

use oneterm_core::{AppError, RemotePath};

use super::{load_uid_gid_lookup, sftp_download, sftp_upload};
use crate::session::sftp_config;

const MAX_OPEN_HANDLES: u64 = 2;
/// More files than the advertised limit, so a leak of one unit per file fails.
const FILES: usize = 5;

/// A file-system SFTP server rooted at one directory that advertises
/// `limits@openssh.com` with [`MAX_OPEN_HANDLES`].
struct LimitedServer {
    root: PathBuf,
    next_handle: u64,
    files: HashMap<String, std::fs::File>,
    dirs: HashMap<String, Option<Vec<File>>>,
}

fn ok(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: "Ok".to_owned(),
        language_tag: "en-US".to_owned(),
    }
}

fn io_status(error: std::io::Error) -> StatusCode {
    match error.kind() {
        std::io::ErrorKind::NotFound => StatusCode::NoSuchFile,
        _ => StatusCode::Failure,
    }
}

impl LimitedServer {
    fn resolve(&self, path: &str) -> PathBuf {
        self.root.join(path.trim_start_matches('/'))
    }

    fn handle(&mut self) -> String {
        self.next_handle += 1;
        self.next_handle.to_string()
    }

    fn attrs(&self, id: u32, path: &str) -> Result<Attrs, StatusCode> {
        let meta = std::fs::symlink_metadata(self.resolve(path)).map_err(io_status)?;
        Ok(Attrs {
            id,
            attrs: FileAttributes::from(&meta),
        })
    }
}

impl russh_sftp::server::Handler for LimitedServer {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn init(
        &mut self,
        _version: u32,
        _extensions: HashMap<String, String>,
    ) -> Result<Version, Self::Error> {
        let mut version = Version::new();
        version.extensions.insert(LIMITS.to_owned(), "1".to_owned());
        Ok(version)
    }

    async fn extended(
        &mut self,
        id: u32,
        request: String,
        _data: Vec<u8>,
    ) -> Result<Packet, Self::Error> {
        if request != LIMITS {
            return Err(StatusCode::OpUnsupported);
        }
        let limits = LimitsExtension {
            max_packet_len: 0,
            max_read_len: 0,
            max_write_len: 0,
            max_open_handles: MAX_OPEN_HANDLES,
        };
        let data = russh_sftp::ser::to_bytes(&limits)
            .map_err(|_| StatusCode::Failure)?
            .to_vec();
        Ok(Packet::ExtendedReply(ExtendedReply { id, data }))
    }

    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        self.attrs(id, &path)
    }

    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        self.attrs(id, &path)
    }

    async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, Self::Error> {
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(self.resolve(&path)).map_err(io_status)? {
            let entry = entry.map_err(io_status)?;
            let meta = entry.metadata().map_err(io_status)?;
            entries.push(File::new(
                entry.file_name().to_string_lossy(),
                FileAttributes::from(&meta),
            ));
        }
        let handle = self.handle();
        self.dirs.insert(handle.clone(), Some(entries));
        Ok(Handle { id, handle })
    }

    async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, Self::Error> {
        let pending = self.dirs.get_mut(&handle).ok_or(StatusCode::BadMessage)?;
        match pending.take() {
            Some(files) => Ok(Name { id, files }),
            None => Err(StatusCode::Eof),
        }
    }

    async fn open(
        &mut self,
        id: u32,
        filename: String,
        pflags: OpenFlags,
        _attrs: FileAttributes,
    ) -> Result<Handle, Self::Error> {
        let file = std::fs::OpenOptions::new()
            .read(pflags.contains(OpenFlags::READ))
            .write(pflags.contains(OpenFlags::WRITE))
            .create(pflags.contains(OpenFlags::CREATE))
            .truncate(pflags.contains(OpenFlags::TRUNCATE))
            .open(self.resolve(&filename))
            .map_err(io_status)?;
        let handle = self.handle();
        self.files.insert(handle.clone(), file);
        Ok(Handle { id, handle })
    }

    async fn close(&mut self, id: u32, handle: String) -> Result<Status, Self::Error> {
        self.files.remove(&handle);
        self.dirs.remove(&handle);
        Ok(ok(id))
    }

    async fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> Result<Data, Self::Error> {
        let file = self.files.get_mut(&handle).ok_or(StatusCode::BadMessage)?;
        file.seek(SeekFrom::Start(offset)).map_err(io_status)?;
        let mut data = Vec::new();
        file.take(u64::from(len))
            .read_to_end(&mut data)
            .map_err(io_status)?;
        if data.is_empty() {
            return Err(StatusCode::Eof);
        }
        Ok(Data { id, data })
    }

    async fn write(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, Self::Error> {
        let file = self.files.get_mut(&handle).ok_or(StatusCode::BadMessage)?;
        file.seek(SeekFrom::Start(offset)).map_err(io_status)?;
        file.write_all(&data).map_err(io_status)?;
        Ok(ok(id))
    }

    async fn mkdir(
        &mut self,
        id: u32,
        path: String,
        _attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        std::fs::create_dir(self.resolve(&path)).map_err(io_status)?;
        Ok(ok(id))
    }

    async fn remove(&mut self, id: u32, filename: String) -> Result<Status, Self::Error> {
        std::fs::remove_file(self.resolve(&filename)).map_err(io_status)?;
        Ok(ok(id))
    }

    async fn rename(
        &mut self,
        id: u32,
        oldpath: String,
        newpath: String,
    ) -> Result<Status, Self::Error> {
        std::fs::rename(self.resolve(&oldpath), self.resolve(&newpath)).map_err(io_status)?;
        Ok(ok(id))
    }
}

/// A scratch directory removed when the test ends.
pub(super) struct Scratch(pub(super) PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.0) {
            eprintln!("failed to remove {}: {error}", self.0.display());
        }
    }
}

pub(super) fn scratch(tag: &str) -> Scratch {
    let dir = std::env::temp_dir().join(format!(
        "oneterm-bug0076-{}-{tag}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos())
    ));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    Scratch(dir)
}

/// An SFTP session over `LimitedServer` serving `root`, through OneTerm's own
/// `sftp_config()`.
pub(super) async fn limited_session(root: &Path) -> russh_sftp::client::SftpSession {
    let (client_side, server_side) = tokio::io::duplex(1024 * 1024);
    russh_sftp::server::run(
        server_side,
        LimitedServer {
            root: root.to_path_buf(),
            next_handle: 0,
            files: HashMap::new(),
            dirs: HashMap::new(),
        },
    )
    .await;
    russh_sftp::client::SftpSession::new_with_config(client_side, sftp_config())
        .await
        .expect("sftp handshake")
}

fn write_files(dir: &Path) {
    std::fs::create_dir_all(dir).expect("create source dir");
    for index in 0..FILES {
        std::fs::write(dir.join(format!("f{index}.txt")), format!("file {index}\n"))
            .expect("write source file");
    }
}

fn assert_files(dir: &Path) {
    for index in 0..FILES {
        assert_eq!(
            std::fs::read_to_string(dir.join(format!("f{index}.txt"))).expect("read copy"),
            format!("file {index}\n")
        );
    }
}

async fn download(
    sftp: &russh_sftp::client::SftpSession,
    remote: &str,
    local: &Path,
    cancel: &CancellationToken,
) -> oneterm_core::Result<()> {
    let (progress, _events) = async_channel::bounded(1024);
    sftp_download(sftp, &RemotePath::new(remote), local, &progress, cancel).await
}

/// The harness itself: russh-sftp enforces the advertised limit, and only an
/// awaited `close()` gives a unit back. Without this the other tests prove
/// nothing.
#[tokio::test]
async fn the_harness_reaches_russh_sftps_handle_limit() {
    let remote = scratch("harness-remote");
    write_files(&remote.0.join("src"));
    let sftp = limited_session(&remote.0).await;

    let first = sftp.open("/src/f0.txt").await.expect("first open");
    let second = sftp.open("/src/f1.txt").await.expect("second open");
    let third = sftp.open("/src/f2.txt").await;
    assert!(
        third
            .as_ref()
            .is_err_and(|error| error.to_string().contains("handle limit reached")),
        "a third open must hit the limit, got {:?}",
        third.map(|_| ())
    );
    first.close().await.expect("close first");
    second.close().await.expect("close second");
    let again = sftp.open("/src/f2.txt").await.expect("open after close");
    again.close().await.expect("close again");
}

#[tokio::test]
async fn a_folder_with_more_files_than_the_handle_limit_downloads() {
    let remote = scratch("download-remote");
    let local = scratch("download-local");
    write_files(&remote.0.join("src"));
    let sftp = limited_session(&remote.0).await;
    let cancel = CancellationToken::new();

    download(&sftp, "/src", &local.0.join("dst"), &cancel)
        .await
        .expect("folder download");
    assert_files(&local.0.join("dst"));

    // The session must stay usable: the count went back to zero.
    download(&sftp, "/src/f0.txt", &local.0.join("again.txt"), &cancel)
        .await
        .expect("single-file download after the folder");
}

#[tokio::test]
async fn a_folder_with_more_files_than_the_handle_limit_uploads() {
    let remote = scratch("upload-remote");
    let local = scratch("upload-local");
    write_files(&local.0.join("src"));
    let sftp = limited_session(&remote.0).await;
    let (progress, _events) = async_channel::bounded(1024);

    sftp_upload(
        &sftp,
        &local.0.join("src"),
        &RemotePath::new("/dst"),
        &progress,
        &CancellationToken::new(),
    )
    .await
    .expect("folder upload");
    assert_files(&remote.0.join("dst"));
}

#[tokio::test]
async fn the_uid_gid_lookup_gives_its_handles_back() {
    let remote = scratch("lookup-remote");
    let local = scratch("lookup-local");
    std::fs::create_dir_all(remote.0.join("etc")).expect("create etc");
    std::fs::write(remote.0.join("etc/passwd"), "root:x:0:0::/root:/bin/sh\n").expect("passwd");
    std::fs::write(remote.0.join("etc/group"), "root:x:0:\n").expect("group");
    write_files(&remote.0.join("src"));
    let sftp = limited_session(&remote.0).await;

    let lookup = load_uid_gid_lookup(&sftp).await;
    assert_eq!(lookup.uid_to_name.get(&0).map(String::as_str), Some("root"));

    download(
        &sftp,
        "/src/f0.txt",
        &local.0.join("f0.txt"),
        &CancellationToken::new(),
    )
    .await
    .expect("download after the uid/gid lookup");
}

#[tokio::test]
async fn a_cancelled_download_gives_its_handle_back() {
    let remote = scratch("cancel-remote");
    let local = scratch("cancel-local");
    write_files(&remote.0.join("src"));
    let sftp = limited_session(&remote.0).await;

    let cancelled = CancellationToken::new();
    cancelled.cancel();
    for _ in 0..FILES {
        let result = download(&sftp, "/src/f0.txt", &local.0.join("f0.txt"), &cancelled).await;
        assert!(
            matches!(result, Err(AppError::Cancelled)),
            "expected a cancellation, got {result:?}"
        );
    }

    // A cancelled download closes its handle in the background, so the count
    // comes back shortly after, not necessarily before the next call.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let result = download(
            &sftp,
            "/src/f0.txt",
            &local.0.join("f0.txt"),
            &CancellationToken::new(),
        )
        .await;
        match result {
            Ok(()) => break,
            Err(error) if tokio::time::Instant::now() < deadline => {
                assert!(
                    error.to_string().contains("handle limit reached"),
                    "unexpected error: {error}"
                );
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            Err(error) => panic!("the handles never came back: {error}"),
        }
    }
}
