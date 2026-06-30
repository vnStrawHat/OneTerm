//! SFTP tokio task — xử lý `SftpCmd` từ UI, gọi `russh_sftp` API.
//!
//! Chạy song song với `ssh_main_task` trên cùng tokio runtime.
//! 2 channel (shell + sftp) chia sẻ 1 TCP connection, multiplex bởi russh.
//!
//! Upload/download được spawn thành tokio task riêng — main loop luôn
//! responsive để nhận `SftpCmd::Cancel` và signal `CancellationToken`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use async_channel::{Receiver, Sender};
use russh_sftp::client::SftpSession as SftpChannel;
use russh_sftp::protocol::FileAttributes;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

use myterm2_core::{AppError, FileEntry, FileStat, Result};

use crate::sftp::{SftpCmd, SftpEvent};

// ── UID/GID lookup ────────────────────────────────────────────

/// Lookup uid → username và gid → groupname, parse từ /etc/passwd + /etc/group.
/// Cache 1 lần khi SFTP task start, dùng cho mọi `attrs_to_entry`.
#[derive(Default)]
struct UidGidLookup {
    uid_to_name: HashMap<u32, String>,
    gid_to_name: HashMap<u32, String>,
}

impl UidGidLookup {
    /// Resolve uid → username. None nếu không có trong map.
    fn uid_name(&self, uid: Option<u32>) -> Option<String> {
        uid.and_then(|u| self.uid_to_name.get(&u).cloned())
    }

    /// Resolve gid → groupname. None nếu không có trong map.
    fn gid_name(&self, gid: Option<u32>) -> Option<String> {
        gid.and_then(|g| self.gid_to_name.get(&g).cloned())
    }
}

/// Đọc /etc/passwd + /etc/group qua SFTP, parse uid→name và gid→name maps.
/// Best-effort: nếu không đọc được → map rỗng (hiển thị số thay).
async fn load_uid_gid_lookup(sftp: &SftpChannel) -> UidGidLookup {
    let mut lookup = UidGidLookup::default();

    // /etc/passwd: `root:x:0:0:root:/root:/bin/bash`
    //             field 0 = name, field 2 = uid, field 3 = gid
    match sftp.read("/etc/passwd").await {
        Ok(data) => {
            let text = String::from_utf8_lossy(&data);
            for line in text.lines() {
                let fields: Vec<&str> = line.split(':').collect();
                if fields.len() >= 4 {
                    if let (Ok(uid), Ok(gid)) = (fields[2].parse::<u32>(), fields[3].parse::<u32>())
                    {
                        lookup.uid_to_name.insert(uid, fields[0].to_string());
                        // passwd cũng có gid → có thể dùng cho group lookup.
                        lookup
                            .gid_to_name
                            .entry(gid)
                            .or_insert_with(|| fields[0].to_string());
                    }
                }
            }
            log::debug!(
                "sftp_task: /etc/passwd loaded — {} uids",
                lookup.uid_to_name.len()
            );
        }
        Err(e) => log::debug!("sftp_task: /etc/passwd not readable: {e} — uid/gid sẽ hiển thị số"),
    }

    // /etc/group: `root:x:0:`
    //             field 0 = name, field 2 = gid
    match sftp.read("/etc/group").await {
        Ok(data) => {
            let text = String::from_utf8_lossy(&data);
            for line in text.lines() {
                let fields: Vec<&str> = line.split(':').collect();
                if fields.len() >= 3 {
                    if let Ok(gid) = fields[2].parse::<u32>() {
                        lookup.gid_to_name.insert(gid, fields[0].to_string());
                    }
                }
            }
            log::debug!(
                "sftp_task: /etc/group loaded — {} gids",
                lookup.gid_to_name.len()
            );
        }
        Err(e) => log::debug!("sftp_task: /etc/group not readable: {e}"),
    }

    lookup
}

/// Tokio task xử lý SFTP commands.
///
/// Chạy song song với `ssh_main_task` trên cùng tokio runtime.
/// Nhận `SftpCmd` qua `cmd_rx`, gửi `SftpEvent` qua `event_tx`.
///
/// Upload/download được spawn thành tokio task riêng — main loop luôn
/// responsive để nhận `SftpCmd::Cancel` và signal `CancellationToken`.
pub(crate) async fn sftp_task(
    sftp: SftpChannel,
    cmd_rx: Receiver<SftpCmd>,
    event_tx: Sender<SftpEvent>,
    alive: std::sync::Arc<std::sync::Mutex<bool>>,
) {
    log::info!("sftp_task: started");

    // Load uid→name và gid→name maps từ /etc/passwd + /etc/group.
    // Best-effort: nếu không đọc được → map rỗng, hiển thị số.
    let lookup = load_uid_gid_lookup(&sftp).await;
    log::info!(
        "sftp_task: uid/gid lookup loaded ({} uids, {} gids)",
        lookup.uid_to_name.len(),
        lookup.gid_to_name.len()
    );

    // Wrap SftpChannel trong Arc — clone cho mỗi spawned transfer task.
    let sftp = Arc::new(sftp);

    let _ = event_tx.try_send(SftpEvent::Ready);

    // Cancel tokens cho transfer đang chạy — key = transfer_id.
    let mut cancels: HashMap<u64, CancellationToken> = HashMap::new();

    loop {
        match cmd_rx.recv().await {
            Ok(SftpCmd::ReadDir { path, reply }) => {
                log::debug!("sftp_task: ReadDir path=\"{}\"", path.display());
                let result = sftp_read_dir(&sftp, &path, &lookup).await;
                let _ = reply.send(result);
            }
            Ok(SftpCmd::Stat { path, reply }) => {
                log::debug!("sftp_task: Stat path=\"{}\"", path.display());
                let result = sftp_stat(&sftp, &path, &lookup).await;
                let _ = reply.send(result);
            }
            Ok(SftpCmd::Rename { from, to, reply }) => {
                log::debug!(
                    "sftp_task: Rename from=\"{}\" to=\"{}\"",
                    from.display(),
                    to.display()
                );
                let result = sftp
                    .rename(from.to_string_lossy(), to.to_string_lossy())
                    .await
                    .map_err(map_sftp_err);
                let _ = reply.send(result);
            }
            Ok(SftpCmd::Remove { path, reply }) => {
                log::debug!("sftp_task: Remove path=\"{}\"", path.display());
                let result = sftp
                    .remove_file(path.to_string_lossy())
                    .await
                    .map_err(map_sftp_err);
                let _ = reply.send(result);
            }
            Ok(SftpCmd::Rmdir { path, reply }) => {
                log::debug!("sftp_task: Rmdir path=\"{}\"", path.display());
                let sftp = Arc::clone(&sftp);
                tokio::spawn(async move {
                    let result = sftp_remove_recursive(&sftp, &path).await;
                    let _ = reply.send(result);
                });
            }
            Ok(SftpCmd::Mkdir { path, reply }) => {
                log::debug!("sftp_task: Mkdir path=\"{}\"", path.display());
                let result = sftp
                    .create_dir(path.to_string_lossy())
                    .await
                    .map_err(map_sftp_err);
                let _ = reply.send(result);
            }
            Ok(SftpCmd::Upload {
                transfer_id,
                local,
                remote,
                progress,
                reply,
            }) => {
                log::info!(
                    "sftp_task: Upload #{transfer_id} local=\"{}\" remote=\"{}\"",
                    local.display(),
                    remote.display()
                );
                let cancel = CancellationToken::new();
                cancels.insert(transfer_id, cancel.clone());
                let sftp = Arc::clone(&sftp);
                tokio::spawn(async move {
                    let result = sftp_upload(&sftp, &local, &remote, &progress, &cancel).await;
                    log::info!(
                        "sftp_task: Upload #{transfer_id} finished: {}",
                        if result.is_ok() { "OK" } else { "error" }
                    );
                    let _ = reply.try_send(result);
                });
            }
            Ok(SftpCmd::Download {
                transfer_id,
                remote,
                local,
                progress,
                reply,
            }) => {
                log::info!(
                    "sftp_task: Download #{transfer_id} remote=\"{}\" local=\"{}\"",
                    remote.display(),
                    local.display()
                );
                let cancel = CancellationToken::new();
                cancels.insert(transfer_id, cancel.clone());
                let sftp = Arc::clone(&sftp);
                tokio::spawn(async move {
                    let result = sftp_download(&sftp, &remote, &local, &progress, &cancel).await;
                    log::info!(
                        "sftp_task: Download #{transfer_id} finished: {}",
                        if result.is_ok() { "OK" } else { "error" }
                    );
                    let _ = reply.try_send(result);
                });
            }
            Ok(SftpCmd::Cancel { transfer_id }) => {
                log::info!("sftp_task: Cancel transfer #{transfer_id}");
                if let Some(cancel) = cancels.get(&transfer_id) {
                    cancel.cancel();
                    log::info!("sftp_task: Cancel #{transfer_id} — token signalled");
                } else {
                    log::warn!("sftp_task: Cancel #{transfer_id} — not found (already finished?)");
                }
            }
            Ok(SftpCmd::Close) => {
                log::info!("sftp_task: close requested");
                break;
            }
            Err(_) => {
                log::info!("sftp_task: cmd_rx closed — session dropped");
                break;
            }
        }
        // Cleanup: remove cancel tokens cho transfers đã xong.
        // Tokens được insert khi upload/download bắt đầu. Spawned task không
        // thể xoá map → giữ lại. Map nhỏ (chỉ transfer đang chạy), không đáng kể.
    }

    {
        let mut a = alive.lock().unwrap();
        *a = false;
    }
    let _ = event_tx.try_send(SftpEvent::Closed);
    log::info!("sftp_task: exiting");
}

// ── Helpers ──────────────────────────────────────────────────

/// Chuyển lỗi russh-sftp sang `AppError`.
fn map_sftp_err(e: russh_sftp::client::error::Error) -> AppError {
    AppError::msg(e.to_string())
}

/// Chuyển `FileAttributes` (russh-sftp) sang `FileEntry`.
///
/// QUAN TRỌNG: SFTP path luôn dùng `/` (Unix style), kể cả khi client chạy
/// trên Windows. `PathBuf::join` trên Windows dùng `\` → SFTP server không
/// hiểu. Nên dùng string concatenation với `/` thay vì `Path::join`.
fn attrs_to_entry(
    name: String,
    parent: &str,
    attrs: &FileAttributes,
    lookup: &UidGidLookup,
) -> FileEntry {
    // Nối parent + name bằng `/` — đảm bảo path Unix style cho SFTP.
    let path = if parent.ends_with('/') {
        format!("{parent}{name}")
    } else {
        format!("{parent}/{name}")
    };
    let uid = attrs.uid;
    let gid = attrs.gid;
    FileEntry {
        name,
        path: PathBuf::from(path),
        is_dir: attrs.is_dir(),
        is_symlink: attrs.is_symlink(),
        size: attrs.size.unwrap_or(0),
        modified: attrs
            .mtime
            .map(|t| UNIX_EPOCH + Duration::from_secs(t as u64)),
        accessed: attrs
            .atime
            .map(|t| UNIX_EPOCH + Duration::from_secs(t as u64)),
        permissions: attrs.permissions.unwrap_or(0),
        uid,
        gid,
        owner: lookup.uid_name(uid),
        group: lookup.gid_name(gid),
    }
}

/// Đọc thư mục — trả về danh sách entry đã sort (folder trước, rồi file theo tên).
///
/// Nếu `path` là relative (vd `"."`), dùng `canonicalize` để resolve thành
/// absolute path trước — một số SFTP server không hiểu relative path.
async fn sftp_read_dir(
    sftp: &SftpChannel,
    path: &Path,
    lookup: &UidGidLookup,
) -> Result<Vec<FileEntry>> {
    // to_string_lossy() có thể trả về `\` trên Windows → convert sang `/`.
    let path_str = path.to_string_lossy().replace('\\', "/");
    log::debug!("sftp_read_dir: path=\"{path_str}\"");

    // Resolve relative path → absolute path qua SFTP realpath.
    // Dùng starts_with('/') thay vì Path::is_absolute() vì Windows không
    // xem `/root` là absolute (cần drive letter).
    let abs_path = if path_str.starts_with('/') {
        path_str
    } else {
        match sftp.canonicalize(&path_str).await {
            Ok(resolved) => {
                log::debug!("sftp_read_dir: canonicalize(\"{path_str}\") → \"{resolved}\"");
                resolved
            }
            Err(e) => {
                log::warn!(
                    "sftp_read_dir: canonicalize(\"{path_str}\") failed: {e} — trying original path"
                );
                path_str
            }
        }
    };

    // abs_path đã là string với `/` separator (từ canonicalize hoặc input).
    // KHÔNG dùng Path::new — PathBuf trên Windows sẽ convert `/` → `\`.

    let read_dir = sftp.read_dir(&abs_path).await.map_err(|e| {
        log::error!("sftp_read_dir: read_dir(\"{abs_path}\") failed: {e}");
        map_sftp_err(e)
    })?;

    let mut entries: Vec<FileEntry> = read_dir
        .map(|entry| {
            let name = entry.file_name();
            let attrs = entry.metadata();
            attrs_to_entry(name, &abs_path, &attrs, lookup)
        })
        .collect();

    // Bỏ entry `.` và `..` (một số SFTP server trả về).
    entries.retain(|e| e.name != "." && e.name != "..");

    log::debug!(
        "sftp_read_dir: got {} entries for \"{abs_path}\"",
        entries.len()
    );

    // Sort: folder trước, rồi file theo tên (case-insensitive).
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(entries)
}

/// Lấy metadata chi tiết.
async fn sftp_stat(sftp: &SftpChannel, path: &Path, lookup: &UidGidLookup) -> Result<FileStat> {
    // Sanitize backslashes → forward slashes cho SFTP server.
    let path_str = path.to_string_lossy().replace('\\', "/");
    let attrs = sftp.metadata(&path_str).await.map_err(map_sftp_err)?;

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    Ok(FileStat {
        name,
        path: path.to_path_buf(),
        is_dir: attrs.is_dir(),
        is_symlink: attrs.is_symlink(),
        size: attrs.size.unwrap_or(0),
        modified: attrs
            .mtime
            .map(|t| UNIX_EPOCH + Duration::from_secs(t as u64)),
        accessed: attrs
            .atime
            .map(|t| UNIX_EPOCH + Duration::from_secs(t as u64)),
        permissions: attrs.permissions.unwrap_or(0),
        uid: attrs.uid,
        gid: attrs.gid,
        owner: lookup.uid_name(attrs.uid),
        group: lookup.gid_name(attrs.gid),
    })
}


/// Xoá file/thư mục đệ quy — nếu là thư mục, đọc contents → xoá từng entry → xoá dir.
/// Dùng cho `SftpCmd::Rmdir` — hỗ trợ xoá thư mục không rỗng.
async fn sftp_remove_recursive(sftp: &SftpChannel, path: &Path) -> Result<()> {
    let path_str = path.to_string_lossy().replace('\\', "/");

    // Đọc thư mục — nếu read_dir fail, có thể path là file → thử remove_file.
    let read_dir = match sftp.read_dir(&path_str).await {
        Ok(rd) => rd,
        Err(_) => {
            // Path có thể là file → thử remove_file.
            log::debug!("sftp_remove_recursive: \"{path_str}\" not a dir, trying remove_file");
            return sftp.remove_file(&path_str).await.map_err(map_sftp_err);
        }
    };

    let entries: Vec<(String, bool)> = read_dir
        .filter_map(|e| {
            let name = e.file_name();
            if name == "." || name == ".." {
                return None;
            }
            let is_dir = e.metadata().is_dir();
            Some((name, is_dir))
        })
        .collect();

    for (name, is_dir) in entries {
        // Dùng string concat với '/' thay vì Path::join (tránh '\' trên Windows).
        let child_path = format!("{path_str}/{name}");
        if is_dir {
            Box::pin(sftp_remove_recursive(sftp, &PathBuf::from(&child_path))).await?;
        } else {
            log::debug!("sftp_remove_recursive: remove_file \"{child_path}\"");
            sftp.remove_file(&child_path)
                .await
                .map_err(map_sftp_err)?;
        }
    }

    // Thư mục đã rỗng → remove_dir.
    log::debug!("sftp_remove_recursive: remove_dir \"{path_str}\"");
    sftp.remove_dir(&path_str).await.map_err(map_sftp_err)
}

/// Upload file hoặc thư mục local → remote với progress reporting.
///
/// - File: đọc nội dung → write chunk 32KB → report progress 0.0–1.0.
/// - Thư mục: walk đệ quy → tạo remote dirs → upload từng file,
///   progress = cumulative bytes / total bytes.
///
/// Kiểm tra `cancel.is_cancelled()` sau mỗi chunk write.
/// Nếu cancelled → trả về `Err("cancelled")`.
async fn sftp_upload(
    sftp: &SftpChannel,
    local: &Path,
    remote: &Path,
    progress: &Sender<f64>,
    cancel: &CancellationToken,
) -> Result<()> {
    let metadata = tokio::fs::metadata(local)
        .await
        .map_err(|e| AppError::msg(format!("stat local: {e}")))?;

    if metadata.is_dir() {
        sftp_upload_dir(sftp, local, remote, progress, cancel).await
    } else {
        sftp_upload_file(sftp, local, remote, progress, cancel).await
    }
}

/// Upload một file đơn lẻ — chunk 32KB, progress 0.0–1.0.
async fn sftp_upload_file(
    sftp: &SftpChannel,
    local: &Path,
    remote: &Path,
    progress: &Sender<f64>,
    cancel: &CancellationToken,
) -> Result<()> {
    let local_data = tokio::fs::read(local)
        .await
        .map_err(|e| AppError::msg(format!("read local: {e}")))?;
    let total = local_data.len() as u64;

    // Dùng `create` — mở file với WRITE|CREATE|TRUNCATE.
    let remote_str = remote.to_string_lossy().replace('\\', "/");
    let mut remote_file = sftp.create(&remote_str).await.map_err(map_sftp_err)?;

    const CHUNK: usize = 32 * 1024;
    let mut written: u64 = 0;
    for chunk in local_data.chunks(CHUNK) {
        // Check cancel trước khi write — nếu cancelled thì dừng ngay.
        if cancel.is_cancelled() {
            log::info!("sftp_upload_file: cancelled at {written}/{total} bytes");
            let _ = progress.try_send(-1.0); // -1 = cancelled signal
            return Err(AppError::msg("cancelled"));
        }
        remote_file
            .write_all(chunk)
            .await
            .map_err(|e| AppError::msg(format!("write remote: {e}")))?;
        written += chunk.len() as u64;
        let pct = if total > 0 {
            written as f64 / total as f64
        } else {
            1.0
        };
        let _ = progress.try_send(pct);
    }

    remote_file
        .flush()
        .await
        .map_err(|e| AppError::msg(format!("flush remote: {e}")))?;
    let _ = progress.try_send(1.0);

    Ok(())
}

/// Upload một thư mục — walk đệ quy, tạo remote dirs, upload từng file.
///
/// Progress = cumulative bytes uploaded / total bytes across all files.
async fn sftp_upload_dir(
    sftp: &SftpChannel,
    local: &Path,
    remote: &Path,
    progress: &Sender<f64>,
    cancel: &CancellationToken,
) -> Result<()> {
    /// Thu thập tất cả file (local_path, remote_path, size) trong thư mục.
    fn collect_files(
        local: &Path,
        remote: &Path,
        files: &mut Vec<(PathBuf, PathBuf, u64)>,
    ) -> std::io::Result<()> {
        for entry in std::fs::read_dir(local)? {
            let entry = entry?;
            let path = entry.path();
            let remote_child = remote.join(entry.file_name());
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                collect_files(&path, &remote_child, files)?;
            } else {
                files.push((path, remote_child, metadata.len()));
            }
        }
        Ok(())
    }

    // 1. Thu thập danh sách file + tính tổng dung lượng.
    let mut files: Vec<(PathBuf, PathBuf, u64)> = Vec::new();
    collect_files(local, remote, &mut files)
        .map_err(|e| AppError::msg(format!("walk local dir: {e}")))?;
    let total_bytes: u64 = files.iter().map(|(_, _, s)| *s).sum();
    log::info!(
        "sftp_upload_dir: \"{}\" → \"{}\" — {} files, {} bytes",
        local.display(),
        remote.display(),
        files.len(),
        total_bytes
    );

    // 2. Thu thập tất cả remote dirs cần tạo (DFS, parents trước).
    fn collect_dirs(local: &Path, remote: &Path, dirs: &mut Vec<PathBuf>) {
        dirs.push(remote.to_path_buf());
        if let Ok(entries) = std::fs::read_dir(local) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let remote_child = remote.join(entry.file_name());
                    collect_dirs(&path, &remote_child, dirs);
                }
            }
        }
    }

    let mut remote_dirs: Vec<PathBuf> = Vec::new();
    collect_dirs(local, remote, &mut remote_dirs);
    for dir in &remote_dirs {
        let dir_str = dir.to_string_lossy().replace('\\', "/");
        // Tạo dir (ignore error nếu đã tồn tại).
        if let Err(e) = sftp.create_dir(&dir_str).await {
            log::debug!("sftp_upload_dir: create_dir \"{dir_str}\" → {e} (may already exist)");
        }
    }

    // 3. Upload từng file, track cumulative progress.
    let mut bytes_done: u64 = 0;
    for (local_path, remote_path, file_size) in &files {
        if cancel.is_cancelled() {
            log::info!("sftp_upload_dir: cancelled at {bytes_done}/{total_bytes} bytes");
            let _ = progress.try_send(-1.0);
            return Err(AppError::msg("cancelled"));
        }

        log::debug!(
            "sftp_upload_dir: uploading \"{}\" → \"{}\" ({file_size} bytes)",
            local_path.display(),
            remote_path.display()
        );

        // Upload file nội bộ — report progress dựa trên cumulative bytes.
        let local_data = tokio::fs::read(local_path)
            .await
            .map_err(|e| AppError::msg(format!("read local: {e}")))?;

        let remote_str = remote_path.to_string_lossy().replace('\\', "/");
        let mut remote_file = sftp.create(&remote_str).await.map_err(map_sftp_err)?;

        const CHUNK: usize = 32 * 1024;
        for chunk in local_data.chunks(CHUNK) {
            if cancel.is_cancelled() {
                log::info!("sftp_upload_dir: cancelled mid-file at {bytes_done}/{total_bytes}");
                let _ = progress.try_send(-1.0);
                return Err(AppError::msg("cancelled"));
            }
            remote_file
                .write_all(chunk)
                .await
                .map_err(|e| AppError::msg(format!("write remote: {e}")))?;
            bytes_done += chunk.len() as u64;
            let pct = if total_bytes > 0 {
                bytes_done as f64 / total_bytes as f64
            } else {
                1.0
            };
            let _ = progress.try_send(pct);
        }

        remote_file
            .flush()
            .await
            .map_err(|e| AppError::msg(format!("flush remote: {e}")))?;
    }

    let _ = progress.try_send(1.0);
    Ok(())
}

/// Download file với progress reporting (chunk 32KB).
///
/// Kiểm tra `cancel.is_cancelled()` sau mỗi chunk read.
/// Nếu cancelled → trả về `Err("cancelled")`.
async fn sftp_download(
    sftp: &SftpChannel,
    remote: &Path,
    local: &Path,
    progress: &Sender<f64>,
    cancel: &CancellationToken,
) -> Result<()> {
    // Lấy size để tính progress.
    let remote_str = remote.to_string_lossy().replace('\\', "/");
    let attrs = sftp.metadata(&remote_str).await.map_err(map_sftp_err)?;
    let total = attrs.size.unwrap_or(0);

    let mut remote_file = sftp.open(&remote_str).await.map_err(map_sftp_err)?;

    let mut local_file = tokio::fs::File::create(local)
        .await
        .map_err(|e| AppError::msg(format!("create local: {e}")))?;

    const CHUNK: usize = 32 * 1024;
    let mut buf = vec![0u8; CHUNK];
    let mut read: u64 = 0;
    loop {
        // Check cancel trước khi read — nếu cancelled thì dừng ngay.
        if cancel.is_cancelled() {
            log::info!("sftp_download: cancelled at {read}/{total} bytes");
            let _ = progress.try_send(-1.0); // -1 = cancelled signal
            return Err(AppError::msg("cancelled"));
        }
        let n = remote_file
            .read(&mut buf)
            .await
            .map_err(|e| AppError::msg(format!("read remote: {e}")))?;
        if n == 0 {
            break;
        }
        local_file
            .write_all(&buf[..n])
            .await
            .map_err(|e| AppError::msg(format!("write local: {e}")))?;
        read += n as u64;
        let pct = if total > 0 {
            read as f64 / total as f64
        } else {
            1.0
        };
        let _ = progress.try_send(pct);
    }

    local_file
        .flush()
        .await
        .map_err(|e| AppError::msg(format!("flush local: {e}")))?;
    let _ = progress.try_send(1.0);

    Ok(())
}
