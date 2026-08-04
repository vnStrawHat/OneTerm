//! Active SFTP transfer cancellation ownership.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio_util::sync::CancellationToken;

type TransferMap = HashMap<u64, CancellationToken>;

pub(super) type ActiveTransfers = Arc<Mutex<TransferMap>>;

pub(super) struct ActiveTransferGuard {
    transfers: ActiveTransfers,
    transfer_id: u64,
}

impl ActiveTransferGuard {
    pub(super) fn new(transfers: ActiveTransfers, transfer_id: u64) -> Self {
        Self {
            transfers,
            transfer_id,
        }
    }
}

impl Drop for ActiveTransferGuard {
    fn drop(&mut self) {
        self.transfers.lock().unwrap().remove(&self.transfer_id);
    }
}
