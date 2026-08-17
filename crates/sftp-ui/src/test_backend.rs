//! Scriptable in-memory `SftpBackend` for panel tests.
//!
//! Transfers record their arguments and answer from a queue of pre-armed
//! channels so a test controls *when* and *in which order* results arrive.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use async_channel::Sender;

use oneterm_core::{
    AppError, FileEntry, FileStat, RemotePath, Result, SftpBackend, SftpFuture, SftpSessionId,
    TransferEvent, TransferHandle,
};

/// One armed transfer: the test drives `events`/`result`, the panel receives them.
pub(crate) struct ScriptedTransfer {
    pub events: Sender<TransferEvent>,
    pub result: Sender<Result<()>>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TransferRequest {
    pub transfer_id: u64,
    pub remote: RemotePath,
    pub local: PathBuf,
}

#[derive(Default)]
struct Script {
    transfer_handles: VecDeque<TransferHandle>,
    transfer_requests: Vec<TransferRequest>,
}

pub(crate) struct FakeSftpBackend {
    id: SftpSessionId,
    alive: AtomicBool,
    script: Mutex<Script>,
}

impl FakeSftpBackend {
    pub(crate) fn new() -> Self {
        Self {
            id: SftpSessionId::next(),
            alive: AtomicBool::new(true),
            script: Mutex::new(Script::default()),
        }
    }

    /// Arm the handle for the next `upload`/`download` call.
    pub(crate) fn arm_transfer(&self) -> ScriptedTransfer {
        let (events_tx, events) = async_channel::bounded(16);
        let (result_tx, result) = async_channel::bounded(1);
        self.script
            .lock()
            .unwrap()
            .transfer_handles
            .push_back(TransferHandle { events, result });
        ScriptedTransfer {
            events: events_tx,
            result: result_tx,
        }
    }

    /// Transfers requested through `upload`/`download`, in call order.
    pub(crate) fn transfer_requests(&self) -> Vec<TransferRequest> {
        self.script.lock().unwrap().transfer_requests.clone()
    }

    fn unused<T: Send + 'static>() -> SftpFuture<'static, T> {
        Box::pin(async { Err(AppError::msg("unused test operation")) })
    }

    fn next_transfer(
        &self,
        transfer_id: u64,
        remote: RemotePath,
        local: PathBuf,
    ) -> TransferHandle {
        let mut script = self.script.lock().unwrap();
        script.transfer_requests.push(TransferRequest {
            transfer_id,
            remote,
            local,
        });
        script
            .transfer_handles
            .pop_front()
            .unwrap_or_else(|| TransferHandle::failed(AppError::msg("unused test operation")))
    }
}

impl SftpBackend for FakeSftpBackend {
    fn session_id(&self) -> SftpSessionId {
        self.id
    }

    fn read_dir(&self, _path: RemotePath) -> SftpFuture<'_, Vec<FileEntry>> {
        Self::unused()
    }

    fn stat(&self, _path: RemotePath) -> SftpFuture<'_, FileStat> {
        Self::unused()
    }

    fn rename(&self, _from: RemotePath, _to: RemotePath) -> SftpFuture<'_, ()> {
        Self::unused()
    }

    fn remove(&self, _path: RemotePath) -> SftpFuture<'_, ()> {
        Self::unused()
    }

    fn remove_dir_all(&self, _path: RemotePath) -> SftpFuture<'_, ()> {
        Self::unused()
    }

    fn mkdir(&self, _path: RemotePath) -> SftpFuture<'_, ()> {
        Self::unused()
    }

    fn upload(&self, transfer_id: u64, local: PathBuf, remote: RemotePath) -> TransferHandle {
        self.next_transfer(transfer_id, remote, local)
    }

    fn download(&self, transfer_id: u64, remote: RemotePath, local: PathBuf) -> TransferHandle {
        self.next_transfer(transfer_id, remote, local)
    }

    fn cancel_transfer(&self, _transfer_id: u64) {}

    fn close(&self) {
        self.alive.store(false, Ordering::Relaxed);
    }

    fn alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }
}

/// A minimal directory entry for listing tests.
pub(crate) fn dir_entry(parent: &RemotePath, name: &str, is_dir: bool) -> FileEntry {
    FileEntry {
        name: name.to_string(),
        path: parent.join(name),
        is_dir,
        is_symlink: false,
        size: 0,
        modified: None,
        accessed: None,
        permissions: if is_dir { 0o755 } else { 0o644 },
        uid: None,
        gid: None,
        owner: None,
        group: None,
    }
}
