//! SFTP abstraction — trait + types dùng chung cho UI và backend.
//!
//! `core` là leaf crate (không phụ thuộc `ssh`). Trait `SftpBackend` định nghĩa
//! abstract interface; `ssh` crate implement cho `SftpSession`.
//!
//! UI dùng qua `dyn SftpBackend`, không biết `russh-sftp`.

use std::path::PathBuf;
use std::time::SystemTime;

use async_channel::Receiver;

use crate::Result;

// ── File entry cho UI rendering ──────────────────────────────

/// Một entry trong thư mục (file hoặc folder).
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    pub permissions: u32,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    /// Owner name (resolved từ /etc/passwd). None nếu không resolve được.
    pub owner: Option<String>,
    /// Group name (resolved từ /etc/group). None nếu không resolve được.
    pub group: Option<String>,
}

/// File/folder metadata — cho detail dialog.
#[derive(Debug, Clone)]
pub struct FileStat {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    pub permissions: u32,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    pub owner: Option<String>,
    pub group: Option<String>,
}

// ── SftpBackend trait ────────────────────────────────────────

/// Abstract SFTP backend — implement bởi `ssh` crate.
///
/// UI dùng qua `dyn SftpBackend`, không biết `russh-sftp`.
/// Methods sync — bridge sang async bên trong implementation.
pub trait SftpBackend: Send + Sync + 'static {
    /// Đọc thư mục — trả về danh sách entry.
    fn read_dir(&self, path: PathBuf) -> Result<Vec<FileEntry>>;

    /// Lấy metadata chi tiết.
    fn stat(&self, path: PathBuf) -> Result<FileStat>;

    /// Đổi tên file/folder.
    fn rename(&self, from: PathBuf, to: PathBuf) -> Result<()>;

    /// Xoá file.
    fn remove(&self, path: PathBuf) -> Result<()>;

    /// Xoá thư mục rỗng.
    fn rmdir(&self, path: PathBuf) -> Result<()>;

    /// Tạo thư mục.
    fn mkdir(&self, path: PathBuf) -> Result<()>;

    /// Upload file local → remote.
    /// `transfer_id` — ID duy nhất do UI tạo, dùng để cancel.
    /// Trả về progress channel (0.0–1.0) + reply channel (Result<()>).
    /// UI spawn task poll progress — không block UI thread.
    fn upload(
        &self,
        transfer_id: u64,
        local: PathBuf,
        remote: PathBuf,
    ) -> (Receiver<f64>, Receiver<Result<()>>);

    /// Download file remote → local.
    /// Tương tự upload.
    fn download(
        &self,
        transfer_id: u64,
        remote: PathBuf,
        local: PathBuf,
    ) -> (Receiver<f64>, Receiver<Result<()>>);

    /// Hủy transfer đang chạy (upload/download).
    /// `transfer_id` phải khớp với ID đã truyền vào `upload`/`download`.
    fn cancel_transfer(&self, transfer_id: u64);

    /// Đóng SFTP session.
    fn close(&self);

    /// SFTP còn sống?
    fn alive(&self) -> bool;
}
