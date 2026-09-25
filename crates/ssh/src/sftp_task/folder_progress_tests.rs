//! `BUG-0077`: a folder transfer reports bytes done over the bytes of the whole
//! tree, not over the bytes discovered so far.
//!
//! The fixture nests the directories so the big file is discovered last on any
//! server's `readdir` order: `one.bin` (1 MiB) sits in the root, `two.bin`
//! (1 MiB) one level down, `big.bin` (8 MiB) two levels down. Before the fix the
//! fraction reached 0.99 at the end of `one.bin` and stayed there.

use std::path::Path;

use tokio_util::sync::CancellationToken;

use oneterm_core::{RemotePath, TransferEvent};

use super::handle_limit_tests::{limited_session, scratch};
use super::{sftp_download, sftp_upload};

const MIB: usize = 1024 * 1024;

fn write_tree(root: &Path) {
    std::fs::create_dir_all(root.join("d1/d2")).expect("create tree");
    std::fs::write(root.join("one.bin"), vec![1u8; MIB]).expect("one.bin");
    std::fs::write(root.join("d1/two.bin"), vec![2u8; MIB]).expect("two.bin");
    std::fs::write(root.join("d1/d2/big.bin"), vec![3u8; 8 * MIB]).expect("big.bin");
}

fn events(receiver: &async_channel::Receiver<TransferEvent>) -> Vec<TransferEvent> {
    std::iter::from_fn(|| receiver.try_recv().ok()).collect()
}

fn fractions(events: &[TransferEvent]) -> Vec<f64> {
    events
        .iter()
        .filter_map(|event| match event {
            TransferEvent::Progress(fraction) => Some(*fraction),
            _ => None,
        })
        .collect()
}

/// The fraction tracks bytes over the whole tree: it passes ~0.1 and ~0.2 as
/// the two small files finish, then climbs through the big one to 1.0.
fn assert_whole_tree_fractions(direction: &str, fractions: &[f64]) {
    let rounded: Vec<String> = fractions.iter().map(|f| format!("{f:.3}")).collect();
    eprintln!("{direction} fractions: [{}]", rounded.join(", "));
    let near = |target: f64| fractions.iter().any(|f| (f - target).abs() < 0.005);
    assert!(near(0.1), "{direction}: no sample at 1 MiB / 10 MiB");
    assert!(near(0.2), "{direction}: no sample at 2 MiB / 10 MiB");
    assert!(
        fractions.windows(2).all(|pair| pair[0] <= pair[1]),
        "{direction}: not monotonic"
    );
    assert!(
        fractions.iter().filter(|f| **f > 0.2 && **f < 1.0).count() >= 20,
        "{direction}: the big file should climb in steps from 0.2 to 1.0"
    );
    assert_eq!(fractions.last(), Some(&1.0), "{direction}: must end at 1.0");
}

#[tokio::test]
async fn folder_download_progress_counts_the_whole_tree() {
    let remote = scratch("progress-download-remote");
    let local = scratch("progress-download-local");
    write_tree(&remote.0.join("tree"));
    let sftp = limited_session(&remote.0).await;
    let (progress, receiver) = async_channel::bounded(4096);

    sftp_download(
        &sftp,
        &RemotePath::new("/tree"),
        &local.0.join("tree"),
        &progress,
        &CancellationToken::new(),
    )
    .await
    .expect("folder download");

    let events = events(&receiver);
    assert_whole_tree_fractions("download", &fractions(&events));
    // Discovery reports its running file count before any byte moves.
    let first_progress = events
        .iter()
        .position(|event| matches!(event, TransferEvent::Progress(_)))
        .expect("progress");
    assert_eq!(
        events[..first_progress].last(),
        Some(&TransferEvent::Discovering(3))
    );
    assert_eq!(
        std::fs::read(local.0.join("tree/d1/d2/big.bin"))
            .expect("big.bin")
            .len(),
        8 * MIB
    );
}

#[tokio::test]
async fn folder_upload_progress_counts_the_whole_tree() {
    let remote = scratch("progress-upload-remote");
    let local = scratch("progress-upload-local");
    write_tree(&local.0.join("tree"));
    let sftp = limited_session(&remote.0).await;
    let (progress, receiver) = async_channel::bounded(4096);

    sftp_upload(
        &sftp,
        &local.0.join("tree"),
        &RemotePath::new("/tree"),
        &progress,
        &CancellationToken::new(),
    )
    .await
    .expect("folder upload");

    assert_whole_tree_fractions("upload", &fractions(&events(&receiver)));
    assert_eq!(
        std::fs::read(remote.0.join("tree/d1/d2/big.bin"))
            .expect("big.bin")
            .len(),
        8 * MIB
    );
}

/// An empty folder and a zero-byte file have no bytes to divide by; both
/// still end with `Progress(1.0)` and land on the other side.
#[tokio::test]
async fn empty_folders_and_zero_byte_files_still_complete() {
    let remote = scratch("progress-empty-remote");
    let local = scratch("progress-empty-local");
    std::fs::create_dir_all(remote.0.join("down/empty")).expect("create down");
    std::fs::write(remote.0.join("down/zero.txt"), b"").expect("zero.txt");
    std::fs::create_dir_all(local.0.join("up/empty")).expect("create up");
    std::fs::write(local.0.join("up/zero.txt"), b"").expect("zero.txt");
    let sftp = limited_session(&remote.0).await;
    let cancel = CancellationToken::new();

    let (progress, receiver) = async_channel::bounded(64);
    sftp_download(
        &sftp,
        &RemotePath::new("/down"),
        &local.0.join("down"),
        &progress,
        &cancel,
    )
    .await
    .expect("empty download");
    assert_eq!(fractions(&events(&receiver)), vec![1.0]);
    assert!(local.0.join("down/empty").is_dir());
    assert!(local.0.join("down/zero.txt").is_file());

    sftp_upload(
        &sftp,
        &local.0.join("up"),
        &RemotePath::new("/up"),
        &progress,
        &cancel,
    )
    .await
    .expect("empty upload");
    assert_eq!(fractions(&events(&receiver)), vec![1.0]);
    assert!(remote.0.join("up/empty").is_dir());
    assert!(remote.0.join("up/zero.txt").is_file());
}
