//! Transfer-queue bookkeeping for [`SftpPanel`] — the bridge between the
//! background transfer tasks in [`super::transfer`], the per-backend store
//! (source of truth) and the panel's mirrored [`TransferQueueView`].

use gpui::Context;

use super::browser_state::{BackendKey, SftpBrowserStore};
use super::panel::SftpPanel;
use super::types::{TransferItem, TransferStatus};

impl SftpPanel {
    /// Push a transfer item both into the store (source of truth, so it survives
    /// tab switches while the task runs) and the panel's active view. Returns
    /// the item id, or `None` if no SFTP backend is active.
    pub(crate) fn push_transfer(
        &mut self,
        item: TransferItem,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let key = self.active_key()?;
        let id = item.id;
        let store = SftpBrowserStore::global(cx);
        if store
            .with_mut(key, |st| st.transfers.push(item.clone()))
            .is_none()
        {
            log::warn!("SftpPanel: transfer state is unavailable for backend {key:?}");
            return None;
        }
        self.transfers_mut().push(item);
        cx.notify();
        Some(id)
    }

    /// Update a transfer (by id) for the backend identified by `key` in the
    /// store, and mirror the updated item into the active view when `key` is the
    /// active backend. The closure `f` runs once against the store entry; the
    /// active view's matching item is then overwritten with the store's updated
    /// copy (so the UI re-renders without calling `f` twice).
    pub(crate) fn update_transfer_for(
        &mut self,
        key: BackendKey,
        transfer_id: usize,
        f: impl FnOnce(&mut TransferItem),
        cx: &mut Context<Self>,
    ) -> bool {
        let store = SftpBrowserStore::global(cx);
        let updated = store
            .with_mut(key, |st| st.transfers.update(transfer_id, f))
            .flatten();
        let Some(item) = updated else {
            return false;
        };
        if self.active_key() == Some(key) {
            self.transfers_mut().replace(item);
            cx.notify();
        }
        true
    }

    /// Allocate+return the next transfer id for the active backend (counter is
    /// per-backend in the store; this updates the active view's mirror too).
    pub(crate) fn alloc_transfer_id(&mut self, cx: &mut Context<Self>) -> Option<usize> {
        let key = self.active_key()?;
        let id = SftpBrowserStore::global(cx).with_mut(key, |st| st.transfers.allocate_id())?;
        self.transfers_mut().reserve_id(id);
        Some(id)
    }

    /// Clear completed and errored transfers from the active backend's queue.
    ///
    /// Updates both the active view and the per-backend store so the cleanup
    /// survives a tab switch (the queue is per-backend).
    pub(crate) fn clear_completed_transfers(&mut self, cx: &mut Context<Self>) {
        let removed = self.transfers_mut().retain_active();
        if let Some(key) = self.active_key() {
            SftpBrowserStore::global(cx).with_mut(key, |st| {
                st.transfers.retain_active();
            });
        }
        if removed > 0 {
            log::debug!("SftpPanel: cleared {removed} completed/errored transfers");
        }
        cx.notify();
    }

    /// Cancel transfer `id`: signal the backend and mark the item cancelled.
    pub(crate) fn cancel_transfer(&mut self, id: usize, cx: &mut Context<Self>) {
        log::info!("SftpPanel: cancel transfer #{id}");
        if let Some(sftp) = self.sftp() {
            sftp.cancel_transfer(id as u64);
        }
        match self.active_key() {
            Some(key) => {
                self.update_transfer_for(key, id, |t| t.status = TransferStatus::Cancelled, cx);
            }
            None => {
                self.transfers_mut()
                    .update(id, |t| t.status = TransferStatus::Cancelled);
                cx.notify();
            }
        }
    }
}
