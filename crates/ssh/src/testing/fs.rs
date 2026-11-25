//! An SFTP server over a folder of `.tmp`, enough for the engine's calls: paths are the
//! folder's, `/` its root.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use russh_sftp::protocol::{Attrs, Data, File, FileAttributes, Handle, Name, OpenFlags, Status, StatusCode};

pub struct Fs {
    pub root: PathBuf,
    open: HashMap<String, std::fs::File>,
    listed: HashMap<String, bool>,
    next: u32,
}

impl Fs {
    pub fn new(root: PathBuf) -> Fs {
        Fs { root, open: HashMap::new(), listed: HashMap::new(), next: 0 }
    }

    fn local(&self, path: &str) -> PathBuf {
        self.root.join(path.trim_start_matches('/'))
    }

    fn handle(&mut self) -> String {
        self.next += 1;
        format!("h{}", self.next)
    }
}

fn ok(id: u32) -> Status {
    Status { id, status_code: StatusCode::Ok, error_message: "Ok".into(), language_tag: "en-US".into() }
}

fn failed(_: std::io::Error) -> StatusCode {
    StatusCode::Failure
}

fn attrs(meta: &std::fs::Metadata) -> FileAttributes {
    let mut attrs = FileAttributes::empty();
    attrs.size = Some(meta.len());
    let since = meta.modified().ok().and_then(|it| it.duration_since(std::time::UNIX_EPOCH).ok());
    attrs.mtime = since.map(|it| it.as_secs() as u32);
    let read = meta.accessed().ok().and_then(|it| it.duration_since(std::time::UNIX_EPOCH).ok());
    attrs.atime = read.map(|it| it.as_secs() as u32);
    (attrs.uid, attrs.gid) = (Some(1000), Some(1000));
    attrs.permissions = Some(if meta.is_dir() { 0o040755 } else { 0o100644 });
    attrs
}

impl russh_sftp::server::Handler for Fs {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn realpath(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
        let path = if path == "." || path.is_empty() { "/home".to_string() } else { path };
        Ok(Name { id, files: vec![File::dummy(path)] })
    }

    async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, Self::Error> {
        if !self.local(&path).is_dir() {
            return Err(StatusCode::NoSuchFile);
        }
        self.listed.insert(path.clone(), false);
        Ok(Handle { id, handle: path })
    }

    async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, Self::Error> {
        if self.listed.insert(handle.clone(), true) != Some(false) {
            return Err(StatusCode::Eof);
        }
        let mut files = vec![File::dummy("."), File::dummy("..")];
        for entry in std::fs::read_dir(self.local(&handle)).map_err(failed)? {
            let entry = entry.map_err(failed)?;
            let meta = entry.metadata().map_err(failed)?;
            files.push(File::new(entry.file_name().to_string_lossy(), attrs(&meta)));
        }
        Ok(Name { id, files })
    }

    async fn open(
        &mut self,
        id: u32,
        filename: String,
        flags: OpenFlags,
        _: FileAttributes,
    ) -> Result<Handle, Self::Error> {
        let path = self.local(&filename);
        let file = if flags.contains(OpenFlags::WRITE) {
            std::fs::File::create(path)
        } else {
            std::fs::File::open(path)
        };
        let handle = self.handle();
        self.open.insert(handle.clone(), file.map_err(|_| StatusCode::NoSuchFile)?);
        Ok(Handle { id, handle })
    }

    async fn read(&mut self, id: u32, handle: String, offset: u64, len: u32) -> Result<Data, Self::Error> {
        let file = self.open.get_mut(&handle).ok_or(StatusCode::Failure)?;
        file.seek(SeekFrom::Start(offset)).map_err(failed)?;
        let mut data = vec![0; len as usize];
        let n = file.read(&mut data).map_err(failed)?;
        if n == 0 {
            return Err(StatusCode::Eof);
        }
        data.truncate(n);
        Ok(Data { id, data })
    }

    async fn write(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, Self::Error> {
        let file = self.open.get_mut(&handle).ok_or(StatusCode::Failure)?;
        file.seek(SeekFrom::Start(offset)).map_err(failed)?;
        file.write_all(&data).map_err(failed)?;
        Ok(ok(id))
    }

    async fn fstat(&mut self, id: u32, handle: String) -> Result<Attrs, Self::Error> {
        let file = self.open.get(&handle).ok_or(StatusCode::Failure)?;
        Ok(Attrs { id, attrs: attrs(&file.metadata().map_err(failed)?) })
    }

    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        let meta = std::fs::metadata(self.local(&path)).map_err(|_| StatusCode::NoSuchFile)?;
        Ok(Attrs { id, attrs: attrs(&meta) })
    }

    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        self.stat(id, path).await
    }

    async fn close(&mut self, id: u32, handle: String) -> Result<Status, Self::Error> {
        self.open.remove(&handle);
        self.listed.remove(&handle);
        Ok(ok(id))
    }

    async fn mkdir(&mut self, id: u32, path: String, _: FileAttributes) -> Result<Status, Self::Error> {
        std::fs::create_dir(self.local(&path)).map_err(failed)?;
        Ok(ok(id))
    }

    async fn remove(&mut self, id: u32, filename: String) -> Result<Status, Self::Error> {
        std::fs::remove_file(self.local(&filename)).map_err(|_| StatusCode::NoSuchFile)?;
        Ok(ok(id))
    }

    async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, Self::Error> {
        std::fs::remove_dir(self.local(&path)).map_err(failed)?;
        Ok(ok(id))
    }

    async fn rename(&mut self, id: u32, from: String, to: String) -> Result<Status, Self::Error> {
        if self.local(&to).exists() {
            return Err(StatusCode::Failure);
        }
        std::fs::rename(self.local(&from), self.local(&to)).map_err(failed)?;
        Ok(ok(id))
    }
}
