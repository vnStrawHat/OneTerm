//! SFTP UID/GID lookup and remote metadata conversion.

use std::collections::HashMap;
use std::time::{Duration, UNIX_EPOCH};

use russh_sftp::client::SftpSession as SftpChannel;
use russh_sftp::protocol::FileAttributes;

use oneterm_core::{FileEntry, FileStat, RemotePath, Result};

use super::map_sftp_err;

// ── UID/GID lookup ────────────────────────────────────────────

/// Looks up uid → username and gid → groupname, parsed from /etc/passwd +
/// /etc/group. Cached once when the SFTP task starts, used by every
/// `attrs_to_entry`.
#[derive(Default)]
pub(super) struct UidGidLookup {
    pub(super) uid_to_name: HashMap<u32, String>,
    pub(super) gid_to_name: HashMap<u32, String>,
}

impl UidGidLookup {
    /// Resolve uid → username. None if not in the map.
    fn uid_name(&self, uid: Option<u32>) -> Option<String> {
        uid.and_then(|u| self.uid_to_name.get(&u).cloned())
    }

    /// Resolve gid → groupname. None if not in the map.
    fn gid_name(&self, gid: Option<u32>) -> Option<String> {
        gid.and_then(|g| self.gid_to_name.get(&g).cloned())
    }
}

/// Read /etc/passwd + /etc/group over SFTP, parse uid→name and gid→name maps.
/// Best-effort: if unreadable → empty map (numbers shown instead).
pub(super) async fn load_uid_gid_lookup(sftp: &SftpChannel) -> UidGidLookup {
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
                        // passwd also has gid → can be used for group lookup.
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
        Err(e) => {
            log::debug!("sftp_task: /etc/passwd not readable: {e} — uid/gid shown as numbers")
        }
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

/// Convert `FileAttributes` (russh-sftp) to a `FileEntry`.
fn attrs_to_entry(
    name: String,
    parent: &RemotePath,
    attrs: &FileAttributes,
    lookup: &UidGidLookup,
) -> FileEntry {
    let path = parent.join(&name);
    let uid = attrs.uid;
    let gid = attrs.gid;
    FileEntry {
        name,
        path,
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

/// Read a directory — returns the sorted list of entries (folders first, then
/// files by name).
///
/// If `path` is relative (e.g. `"."`), use `canonicalize` to resolve it to an
/// absolute path first — some SFTP servers don't understand relative paths.
pub(super) async fn sftp_read_dir(
    sftp: &SftpChannel,
    path: &RemotePath,
    lookup: &UidGidLookup,
) -> Result<Vec<FileEntry>> {
    log::debug!("sftp_read_dir: path=\"{path}\"");

    // Resolve relative path → absolute path via SFTP realpath.
    let abs_path = if path.is_absolute() {
        path.clone()
    } else {
        match sftp.canonicalize(path.as_str()).await {
            Ok(resolved) => {
                log::debug!("sftp_read_dir: canonicalize(\"{path}\") → \"{resolved}\"");
                RemotePath::new(resolved)
            }
            Err(e) => {
                log::warn!(
                    "sftp_read_dir: canonicalize(\"{path}\") failed: {e} — trying original path"
                );
                path.clone()
            }
        }
    };

    let read_dir = sftp.read_dir(abs_path.as_str()).await.map_err(|e| {
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

    // Drop `.` and `..` entries (returned by some SFTP servers).
    entries.retain(|e| e.name != "." && e.name != "..");

    log::debug!(
        "sftp_read_dir: got {} entries for \"{abs_path}\"",
        entries.len()
    );

    // Sort: folders first, then files by name (case-insensitive).
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(entries)
}

/// Get detailed metadata.
pub(super) async fn sftp_stat(
    sftp: &SftpChannel,
    path: &RemotePath,
    lookup: &UidGidLookup,
) -> Result<FileStat> {
    let attrs = sftp.metadata(path.as_str()).await.map_err(map_sftp_err)?;

    let name = path.file_name().unwrap_or_default().to_string();

    Ok(FileStat {
        name,
        path: path.clone(),
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
