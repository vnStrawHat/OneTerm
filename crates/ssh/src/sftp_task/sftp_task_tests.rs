use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use oneterm_core::RemotePath;

use super::transfer::staging::{finalize_local_file, temporary_local_sibling};
use super::transfer::upload::{LocalUploadEntry, stream_local_upload_entries};
use super::*;

fn temporary_dir() -> PathBuf {
    // Distinct per call so parallel tests never share a directory, and so two
    // calls within one test (e.g. `root` and `outside`) can't alias when the
    // wall clock is too coarse to separate them (as on macOS). The atomic
    // counter guarantees uniqueness within the process; the pid and timestamp
    // keep names unique across processes and test runs.
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "oneterm-sftp-security-{}-{nonce}-{sequence}",
        std::process::id()
    ))
}

#[test]
fn rejects_remote_names_that_can_escape_or_alias() {
    for name in [
        "",
        ".",
        "..",
        "../outside",
        "dir/file",
        "dir\\file",
        "/absolute",
        "C:outside",
        "name\0tail",
        "CON",
        "nul.txt",
        "COM1.log",
        "trailing.",
        "trailing ",
    ] {
        assert!(
            validate_remote_entry_name(name).is_err(),
            "{name:?} must be rejected"
        );
    }
}

#[test]
fn accepts_one_safe_component_and_keeps_it_below_root() {
    let root = Path::new("download-root");
    for name in ["file.txt", "folder", "héllo 世界.txt", "..safe"] {
        let child = safe_local_child(root, name).unwrap();
        assert_eq!(child, root.join(name));
        assert!(child.starts_with(root));
    }
}

#[tokio::test]
async fn local_finalization_replaces_only_after_complete_write() {
    let root = temporary_dir();
    std::fs::create_dir_all(&root).unwrap();
    let target = root.join("result.txt");
    let temporary = temporary_local_sibling(&target, "part").unwrap();
    std::fs::write(&target, b"old").unwrap();
    std::fs::write(&temporary, b"complete").unwrap();

    finalize_local_file(&temporary, &target).await.unwrap();

    assert_eq!(std::fs::read(&target).unwrap(), b"complete");
    assert!(!temporary.exists());
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    let _ = std::fs::remove_dir_all(root);
}

/// Create a directory symlink `link` → `target`, or `None` when the platform
/// refuses (Windows requires Developer Mode or `SeCreateSymbolicLinkPrivilege`).
fn try_symlink_dir(target: &Path, link: &Path) -> Option<()> {
    #[cfg(unix)]
    let result = std::os::unix::fs::symlink(target, link);
    #[cfg(windows)]
    let result = std::os::windows::fs::symlink_dir(target, link);
    match result {
        Ok(()) => Some(()),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!("skipping symlink test: cannot create symlinks here ({error})");
            None
        }
        // Windows reports the missing privilege as os error 1314, which std
        // maps to `Uncategorized`.
        #[cfg(windows)]
        Err(error) if error.raw_os_error() == Some(1314) => {
            eprintln!("skipping symlink test: cannot create symlinks here ({error})");
            None
        }
        Err(error) => panic!("symlink creation failed: {error}"),
    }
}

/// TEST-11: the download-root symlink guard must run on every client platform.
#[tokio::test]
async fn refuses_preexisting_symlink_below_download_root() {
    let root = temporary_dir();
    let outside = temporary_dir();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    let root = std::fs::canonicalize(&root).unwrap();
    if try_symlink_dir(&outside, &root.join("linked")).is_none() {
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);
        return;
    }
    let candidate = root.join("linked").join("file.txt");

    let error = create_safe_parent_dirs(&root, &candidate)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("symlink"));

    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&outside);
}

#[test]
fn active_transfer_guard_removes_token_on_drop() {
    let transfers: ActiveTransfers = Arc::new(std::sync::Mutex::new(HashMap::new()));
    transfers
        .lock()
        .unwrap()
        .insert(7, CancellationToken::new());
    {
        let _guard = ActiveTransferGuard::new(Arc::clone(&transfers), 7);
    }
    assert!(transfers.lock().unwrap().is_empty());
}

#[test]
fn local_traversal_stops_before_filesystem_access_when_cancelled() {
    let cancel = CancellationToken::new();
    cancel.cancel();
    let (entries, _receiver) = async_channel::bounded(1);
    let error = stream_local_upload_entries(
        PathBuf::from("missing"),
        RemotePath::new("/remote"),
        cancel,
        &entries,
    )
    .unwrap_err();
    assert!(matches!(error, AppError::Cancelled));
}

/// CORR-18: the walker parks in `send_blocking` while the channel is full;
/// cancelling and dropping the consumer (what `stop_traversal` does) wakes it
/// up with `Cancelled` instead of leaving it spinning or stuck.
#[test]
fn local_upload_discovery_cancels_while_channel_is_full() {
    let cancel = CancellationToken::new();
    let (entries, receiver) = async_channel::bounded(1);
    entries
        .try_send(LocalUploadEntry::Directory(RemotePath::new("/occupied")))
        .unwrap();
    let traversal_cancel = cancel.clone();
    let traversal = std::thread::spawn(move || {
        stream_local_upload_entries(
            PathBuf::from("missing"),
            RemotePath::new("/remote"),
            traversal_cancel,
            &entries,
        )
    });

    std::thread::sleep(std::time::Duration::from_millis(10));
    cancel.cancel();
    drop(receiver);
    assert!(matches!(
        traversal.join().unwrap().unwrap_err(),
        AppError::Cancelled
    ));
}

#[test]
fn local_upload_discovery_streams_directories_and_files() {
    let root = temporary_dir();
    std::fs::create_dir_all(root.join("nested")).unwrap();
    std::fs::write(root.join("first.txt"), b"first").unwrap();
    std::fs::write(root.join("nested").join("second.txt"), b"second").unwrap();
    let (entries, receiver) = async_channel::bounded(8);

    stream_local_upload_entries(
        root.clone(),
        RemotePath::new("/remote"),
        CancellationToken::new(),
        &entries,
    )
    .unwrap();
    drop(entries);
    let mut discovered = Vec::new();
    while let Ok(entry) = receiver.try_recv() {
        discovered.push(entry);
    }

    // Remote targets are built with `/` on every host OS (ARCH-12).
    for entry in &discovered {
        let remote = match entry {
            LocalUploadEntry::Directory(remote) => remote,
            LocalUploadEntry::File { remote, .. } => remote,
        };
        assert!(remote.as_str().starts_with("/remote"), "{remote}");
        assert!(!remote.as_str().contains('\\'), "{remote}");
    }

    assert_eq!(
        discovered
            .iter()
            .filter(|entry| matches!(entry, LocalUploadEntry::Directory(_)))
            .count(),
        2
    );
    assert_eq!(
        discovered
            .iter()
            .filter(|entry| matches!(entry, LocalUploadEntry::File { .. }))
            .count(),
        2
    );
    let _ = std::fs::remove_dir_all(root);
}

/// SEC-16: the id databases are parsed line by line, so a truncated read
/// (the last line cut by the size cap) still yields every complete entry.
#[test]
fn id_database_parsers_tolerate_truncated_input() {
    let mut lookup = metadata::UidGidLookup::default();
    metadata::parse_passwd(
        "root:x:0:0:root:/root:/bin/bash
alice:x:1000:1000::/home/alice:/bin/sh
bob:x:10",
        &mut lookup,
    );
    metadata::parse_group(
        "wheel:x:10:
users:x:100:
trunc:x:1",
        &mut lookup,
    );
    assert_eq!(lookup.uid_to_name.get(&0).map(String::as_str), Some("root"));
    assert_eq!(
        lookup.uid_to_name.get(&1000).map(String::as_str),
        Some("alice")
    );
    assert_eq!(lookup.uid_to_name.len(), 2);
    assert_eq!(
        lookup.gid_to_name.get(&10).map(String::as_str),
        Some("wheel")
    );
    assert_eq!(
        lookup.gid_to_name.get(&100).map(String::as_str),
        Some("users")
    );
    // The passwd gid fallback is kept for groups /etc/group did not name.
    assert_eq!(
        lookup.gid_to_name.get(&1000).map(String::as_str),
        Some("alice")
    );
    assert!(metadata::MAX_ID_DATABASE_BYTES >= 1024 * 1024);
}
