//! A connection's files over SFTP (specs/oneterm, "M9 in detail"). Each call blocks on the
//! engine's runtime: an app makes them on its pool, never on its UI thread.

use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use russh::client::Handle;
use russh_sftp::client::SftpSession;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::hostkey::Checker;
use crate::runtime;

const MOST: usize = 100_000;
const CHUNK: usize = 64 * 1024;

/// An entry of a remote folder.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub dir: bool,
    pub size: u64,
    /// When it last changed, in seconds since 1970; 0 when the server does not say.
    pub modified: u64,
    /// Its kind and permission bits, as `stat` gives them (`0o100644`); 0 when the server
    /// does not say, and for a local file where the system has none.
    pub mode: u32,
}

/// A remote path's properties.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stat {
    pub dir: bool,
    /// A symbolic link; the rest is what it points to, when that is there.
    pub link: bool,
    pub size: u64,
    /// The kind's bits and the permission bits; 0 when the server does not say.
    pub mode: u32,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    /// The names of the ids, from the server's `/etc/passwd` and `/etc/group` if it lets them be read.
    pub owner: Option<String>,
    pub group: Option<String>,
    /// Seconds since 1970; 0 when the server does not say.
    pub modified: u64,
    pub accessed: u64,
}

/// SFTP on a connection; a copy is the same session.
#[derive(Clone)]
pub struct Files {
    sftp: Arc<SftpSession>,
}

fn said(e: impl std::fmt::Display) -> String {
    e.to_string()
}

impl Files {
    pub(crate) fn open(handle: Arc<Handle<Checker>>) -> Result<Files, String> {
        runtime().block_on(async move {
            let channel = handle.channel_open_session().await.map_err(said)?;
            channel.request_subsystem(true, "sftp").await.map_err(said)?;
            let sftp = SftpSession::new(channel.into_stream()).await.map_err(said)?;
            Ok(Files { sftp: Arc::new(sftp) })
        })
    }

    /// The folder the server starts in.
    pub fn home(&self) -> Result<String, String> {
        runtime().block_on(self.sftp.canonicalize(".")).map_err(said)
    }

    /// A folder's entries: folders first, then by name whatever the case.
    pub fn list(&self, dir: &str) -> Result<Vec<Entry>, String> {
        let read = runtime().block_on(self.sftp.read_dir(dir)).map_err(said)?;
        let mut entries: Vec<Entry> = read
            .filter(|it| it.file_name() != "." && it.file_name() != "..")
            .take(MOST)
            .map(|it| {
                let meta = it.metadata();
                let modified = meta.mtime.unwrap_or(0).into();
                let mode = meta.permissions.unwrap_or(0);
                Entry { name: it.file_name(), dir: meta.is_dir(), size: meta.len(), modified, mode }
            })
            .collect();
        entries.sort_by(|a, b| {
            b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(entries)
    }

    /// A path's properties, through a link to what it points to.
    pub fn stat(&self, path: &str) -> Result<Stat, String> {
        runtime().block_on(async {
            let own = self.sftp.symlink_metadata(path).await.map_err(said)?;
            let link = own.is_symlink();
            let meta = if link { self.sftp.metadata(path).await.unwrap_or(own) } else { own };
            let (uid, gid) = (meta.uid, meta.gid);
            let owner = match uid {
                Some(id) => name_in(&self.table("/etc/passwd").await, id),
                None => None,
            };
            let group = match gid {
                Some(id) => name_in(&self.table("/etc/group").await, id),
                None => None,
            };
            Ok(Stat {
                dir: meta.is_dir(),
                link,
                size: meta.len(),
                mode: meta.permissions.unwrap_or(0),
                uid,
                gid,
                owner,
                group,
                modified: meta.mtime.unwrap_or(0).into(),
                accessed: meta.atime.unwrap_or(0).into(),
            })
        })
    }

    /// A table of names on the server, 1 MB at most; empty when it cannot be read.
    async fn table(&self, path: &str) -> String {
        let Ok(mut file) = self.sftp.open(path).await else { return String::new() };
        let mut data = Vec::new();
        let _ = (&mut file).take(1 << 20).read_to_end(&mut data).await;
        let _ = file.shutdown().await;
        // A table cut at the limit loses its last line, which may be cut short.
        if data.len() == 1 << 20 {
            data.truncate(data.iter().rposition(|&b| b == b'\n').unwrap_or(0));
        }
        String::from_utf8_lossy(&data).into_owned()
    }

    pub fn mkdir(&self, path: &str) -> Result<(), String> {
        runtime().block_on(self.sftp.create_dir(path)).map_err(said)
    }

    pub fn rename(&self, from: &str, to: &str) -> Result<(), String> {
        runtime().block_on(self.sftp.rename(from, to)).map_err(said)
    }

    /// Removes a file, or an empty folder.
    pub fn remove(&self, path: &str, dir: bool) -> Result<(), String> {
        match dir {
            true => runtime().block_on(self.sftp.remove_dir(path)).map_err(said),
            false => runtime().block_on(self.sftp.remove_file(path)).map_err(said),
        }
    }

    /// Copies a remote file to `local` through `local.part`; `progress` is told the bytes done
    /// and the whole.
    pub fn download(
        &self,
        remote: &str,
        local: &Path,
        progress: impl Fn(u64, u64),
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        let part = local.with_file_name(format!("{}.part", file_name(local)?));
        let done = runtime().block_on(async {
            let mut file = self.sftp.open(remote).await.map_err(said)?;
            let whole = file.metadata().await.map(|it| it.len()).unwrap_or(0);
            let mut out = std::fs::File::create(&part).map_err(said)?;
            let (mut buffer, mut got) = (vec![0u8; CHUNK], 0u64);
            loop {
                if cancel.load(Ordering::Relaxed) {
                    return Err("cancelled".to_string());
                }
                let n = file.read(&mut buffer).await.map_err(said)?;
                if n == 0 {
                    break;
                }
                out.write_all(&buffer[..n]).map_err(said)?;
                got += n as u64;
                progress(got, whole.max(got));
            }
            out.sync_all().map_err(said)
        });
        match done.and_then(|()| std::fs::rename(&part, local).map_err(said)) {
            Ok(()) => Ok(()),
            Err(why) => {
                let _ = std::fs::remove_file(&part);
                Err(why)
            }
        }
    }

    /// Copies a local file to `remote` through `remote.part`.
    pub fn upload(
        &self,
        local: &Path,
        remote: &str,
        progress: impl Fn(u64, u64),
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        let part = format!("{remote}.part");
        let done = runtime().block_on(async {
            let mut input = std::fs::File::open(local).map_err(said)?;
            let whole = input.metadata().map_or(0, |it| it.len());
            let mut file = self.sftp.create(part.as_str()).await.map_err(said)?;
            let (mut buffer, mut sent) = (vec![0u8; CHUNK], 0u64);
            loop {
                if cancel.load(Ordering::Relaxed) {
                    return Err("cancelled".to_string());
                }
                let n = std::io::Read::read(&mut input, &mut buffer).map_err(said)?;
                if n == 0 {
                    break;
                }
                file.write_all(&buffer[..n]).await.map_err(said)?;
                sent += n as u64;
                progress(sent, whole.max(sent));
            }
            file.shutdown().await.map_err(said)?;
            // SFTP 3 renames onto nothing: what was there goes first.
            let _ = self.sftp.remove_file(remote).await;
            self.sftp.rename(part.as_str(), remote).await.map_err(said)
        });
        if done.is_err() {
            let _ = runtime().block_on(self.sftp.remove_file(part.as_str()));
        }
        done
    }
}

fn file_name(path: &Path) -> Result<String, String> {
    path.file_name().map(|it| it.to_string_lossy().into_owned()).ok_or_else(|| "no file name".into())
}

/// The name of `id` in a table such as `/etc/passwd` or `/etc/group`: `name:x:id:..`.
fn name_in(table: &str, id: u32) -> Option<String> {
    table.lines().find_map(|line| {
        let mut fields = line.split(':');
        let name = fields.next()?;
        (fields.nth(1)?.parse::<u32>().ok()? == id && !name.is_empty()).then(|| name.to_string())
    })
}

/// Whether a remote name can be a local file's name: no separator, drive, `..` or control.
pub fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.chars().any(|c| matches!(c, '/' | '\\' | ':') || c.is_control())
}

/// A remote folder and a name in it.
pub fn join(dir: &str, name: &str) -> String {
    match dir {
        "/" => format!("/{name}"),
        _ => format!("{}/{name}", dir.trim_end_matches('/')),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_paths() {
        assert!(safe_name("config.yaml") && safe_name("tệp"));
        for bad in ["", ".", "..", "a/b", "a\\b", "C:x", "a\nb"] {
            assert!(!safe_name(bad), "{bad:?}");
        }
        assert_eq!(join("/", "etc"), "/etc");
        assert_eq!(join("/home/t/", "a"), "/home/t/a");
        let table = "root:x:0:0:root:/root:/bin/sh\n# x\nwww:x:33:33::/var/www:\n";
        assert_eq!(name_in(table, 33).as_deref(), Some("www"));
        assert_eq!(name_in(table, 0).as_deref(), Some("root"));
        assert_eq!(name_in(table, 7), None);
    }
}
