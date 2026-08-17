//! Transfer operations for the SFTP browser — upload, download.
//!
//! Split out from `file_browser.rs` to keep the file shorter.
//! Upload: open the OS-native file/folder picker OR drag & drop external files
//!         → call the SFTP backend → drive the transfer handle.
//! Download: open the OS-native Save dialog (prompt_for_new_path) → call the SFTP
//!           backend → drive the transfer handle. Supports both files and folders
//!           (recursive download).
//!
//! Both directions share [`run_transfer`], which maps `TransferEvent`s and the
//! final result onto the queue item's status. A cancelled or failed item never
//! stays `InProgress`, and one failure never aborts the remaining files of a batch.

use std::path::PathBuf;
use std::sync::Arc;

use gpui::{AsyncApp, Context, Entity, Window};
use gpui_component::{WindowExt as _, notification::NotificationType};
use oneterm_core::{AppError, RemotePath, SftpBackend, TransferEvent, TransferHandle};
use oneterm_state::notif_ext::notify;

use super::browser_state::BackendKey;
use super::panel::SftpPanel;
use super::types::{TransferDirection, TransferItem, TransferStatus};

/// Register a queue item for a transfer that is about to start.
/// Returns the allocated transfer id, or `None` when no backend is active.
fn begin_transfer(
    panel: &Entity<SftpPanel>,
    direction: TransferDirection,
    filename: &str,
    cx: &mut AsyncApp,
) -> Option<usize> {
    cx.update(|cx| {
        panel.update(cx, |this, cx| {
            let id = this.alloc_transfer_id(cx)?;
            this.push_transfer(
                TransferItem {
                    id,
                    direction,
                    filename: filename.to_string(),
                    progress: 0.0,
                    status: TransferStatus::InProgress,
                    error: None,
                },
                cx,
            );
            log::debug!("SftpPanel: added transfer #{id} {direction:?} \"{filename}\"");
            Some(id)
        })
    })
}

/// Drive one transfer to completion: forward progress to the queue item and
/// settle its final status from the result channel.
///
/// The item ends `Completed`, `Cancelled`, or `Error` — never `InProgress`. A
/// `Cancelled` event and an `Err(AppError::Cancelled)` result both mark the
/// item cancelled, so the outcome does not depend on which arrives first.
async fn run_transfer(
    panel: &Entity<SftpPanel>,
    key: BackendKey,
    transfer_id: usize,
    handle: TransferHandle,
    cx: &mut AsyncApp,
) {
    let apply = |cx: &mut AsyncApp, update: &dyn Fn(&mut TransferItem)| {
        cx.update(|cx| {
            panel.update(cx, |this, cx| {
                this.update_transfer_for(key, transfer_id, update, cx);
            })
        });
    };

    while let Ok(event) = handle.events.recv().await {
        match event {
            TransferEvent::Progress(progress) => {
                log::debug!(
                    "SftpPanel: transfer #{transfer_id} progress {:.0}%",
                    progress * 100.0
                );
                apply(cx, &|item| item.progress = progress);
            }
            TransferEvent::Cancelled => {
                log::info!("SftpPanel: transfer #{transfer_id} cancelled");
                apply(cx, &|item| item.status = TransferStatus::Cancelled);
            }
        }
    }

    match handle.result.recv().await {
        Ok(Ok(())) => {
            log::info!("SftpPanel: transfer #{transfer_id} OK");
            apply(cx, &|item| {
                item.status = TransferStatus::Completed;
                item.progress = 1.0;
            });
        }
        Ok(Err(AppError::Cancelled)) => {
            log::info!("SftpPanel: transfer #{transfer_id} cancelled");
            apply(cx, &|item| item.status = TransferStatus::Cancelled);
        }
        Ok(Err(error)) => {
            log::error!("SftpPanel: transfer #{transfer_id} failed: {error}");
            let message = error.to_string();
            apply(cx, &|item| {
                item.status = TransferStatus::Error;
                item.error = Some(message.clone());
            });
        }
        Err(_) => {
            log::error!("SftpPanel: transfer #{transfer_id} result channel closed");
            apply(cx, &|item| {
                item.status = TransferStatus::Error;
                item.error = Some("channel closed".to_string());
            });
        }
    }
}

impl SftpPanel {
    /// Upload a list of local paths → remote cwd.
    ///
    /// Core logic — used by both the file picker (`do_upload`) and drag & drop
    /// (`on_drop` in render). Uploads each path sequentially, adds a TransferItem,
    /// drives the transfer, and refreshes when done. A cancelled or failed file
    /// does not stop the remaining files of the batch.
    pub(crate) fn do_upload_paths(&mut self, local_paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        if local_paths.is_empty() {
            return;
        }

        let sftp = match self.sftp() {
            Some(s) => s.clone(),
            None => {
                log::warn!("SftpPanel::do_upload_paths: no SFTP connection");
                return;
            }
        };
        let panel = cx.entity();
        let cwd = self.browser().cwd().clone();

        log::info!(
            "SftpPanel::do_upload_paths: {} path(s) → \"{cwd}\"",
            local_paths.len()
        );

        let backend_key = sftp.session_id();

        cx.spawn(async move |_panel, cx| {
            for local_path in local_paths {
                let filename = local_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "uploaded".to_string());
                let remote_path = cwd.join(&filename);

                log::info!(
                    "SftpPanel: upload \"{}\" → \"{remote_path}\"",
                    local_path.display()
                );

                let Some(transfer_id) =
                    begin_transfer(&panel, TransferDirection::Upload, &filename, cx)
                else {
                    return;
                };

                // Sequential: each file finishes before the next one starts.
                let handle = sftp.upload(transfer_id as u64, local_path, remote_path);
                run_transfer(&panel, backend_key, transfer_id, handle, cx).await;
            }

            // Refresh after all files have been uploaded (only if this backend
            // is still the active one — otherwise the user will refresh on switch).
            cx.update(|cx| {
                panel.update(cx, |this, cx| {
                    if this.active_key() == Some(backend_key) {
                        this.refresh(cx);
                    }
                })
            });
        })
        .detach();
    }

    /// Upload a local file or folder → remote.
    /// Opens the OS-native open dialog (choose files or a folder) → calls `do_upload_paths`.
    /// `pick_folders` — true: folder picker, false: file picker (multiple).
    /// Windows does not support mixed files+folders in one dialog, so the two modes are separate.
    pub(crate) fn do_upload(
        &mut self,
        pick_folders: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mode_str = if pick_folders { "folder" } else { "files" };
        log::info!(
            "SftpPanel::do_upload ({mode_str}): cwd=\"{}\"",
            self.browser().cwd()
        );

        // Open the OS-native file picker.
        // Windows does not support mixed files+folders (FOS_PICKFOLDERS toggles mode),
        // so the two modes are separate: files-only (multiple) or folder-only (single).
        let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: !pick_folders,
            directories: pick_folders,
            multiple: !pick_folders,
            prompt: Some(
                if pick_folders {
                    "Select a folder to upload"
                } else {
                    "Select files to upload"
                }
                .into(),
            ),
        });

        // Spawn a task to wait for the user to pick a path → delegate to do_upload_paths.
        let panel = cx.entity();
        cx.spawn(async move |_panel, cx| {
            let paths = match rx.await {
                Ok(Ok(Some(paths))) if !paths.is_empty() => paths,
                Ok(Ok(Some(_))) => {
                    log::debug!("SftpPanel: upload — no paths selected (empty)");
                    return;
                }
                Ok(Ok(None)) => {
                    log::debug!("SftpPanel: upload — user cancelled");
                    return;
                }
                Ok(Err(e)) => {
                    log::error!("SftpPanel: upload — file picker error: {e}");
                    return;
                }
                Err(e) => {
                    log::error!("SftpPanel: upload — channel error: {e}");
                    return;
                }
            };

            log::info!("SftpPanel: upload — {} path(s) selected", paths.len());

            cx.update(|cx| {
                panel.update(cx, |this, cx| {
                    this.do_upload_paths(paths, cx);
                });
            });
        })
        .detach();
    }

    /// Download a remote file or folder → local.
    ///
    /// Opens the OS-native Save dialog (`prompt_for_new_path`) — the user chooses where to save
    /// (file: choose a file name; folder: choose a destination folder name).
    /// After the user picks a path → sftp.download() → drive the transfer.
    ///
    /// The backend branches between file/folder automatically:
    /// - File: download directly.
    /// - Folder: recursively walk the remote tree, create local dirs, download each file.
    pub(crate) fn do_download(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let entry = match self.selected_entry(cx) {
            Some(entry) => entry.clone(),
            None => {
                log::warn!("SftpPanel::do_download: no selection");
                window.push_notification(
                    notify(
                        NotificationType::Warning,
                        "Select a file or folder to download.",
                        cx,
                    ),
                    cx,
                );
                return;
            }
        };
        let sftp = match self.sftp() {
            Some(sftp) => sftp.clone(),
            None => {
                log::warn!("SftpPanel::do_download: no active SFTP backend");
                window.push_notification(
                    notify(
                        NotificationType::Warning,
                        "No active SFTP connection is available.",
                        cx,
                    ),
                    cx,
                );
                return;
            }
        };

        log::info!(
            "SftpPanel::do_download: \"{}\" (is_dir={})",
            entry.name,
            entry.is_dir
        );
        let backend_key = sftp.session_id();
        let panel = cx.entity();
        let remote_path = entry.path.clone();
        let entry_name = entry.name.clone();

        // Starting directory for the Save dialog — use the user's home directory.
        let starting_dir = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .map(PathBuf::from)
            .unwrap_or_default();

        // Open the OS-native Save dialog — the user chooses where to save the file/folder.
        let rx = cx.prompt_for_new_path(&starting_dir, Some(&entry_name));

        // Spawn a task to wait for the user to pick a path → download.
        cx.spawn(async move |_panel, cx| {
            let local_path = match rx.await {
                Ok(Ok(Some(path))) => path,
                Ok(Ok(None)) => {
                    log::debug!("SftpPanel: download — user cancelled");
                    return;
                }
                Ok(Err(e)) => {
                    log::error!("SftpPanel: download — save dialog error: {e}");
                    return;
                }
                Err(e) => {
                    log::error!("SftpPanel: download — channel error: {e}");
                    return;
                }
            };

            log::info!(
                "SftpPanel: download \"{remote_path}\" → \"{}\"",
                local_path.display()
            );

            Self::download_to(
                &panel,
                sftp,
                backend_key,
                remote_path,
                &entry_name,
                local_path,
                cx,
            )
            .await;
        })
        .detach();
    }

    /// Register a download in the queue and drive it to completion.
    async fn download_to(
        panel: &Entity<SftpPanel>,
        sftp: Arc<dyn SftpBackend>,
        backend_key: BackendKey,
        remote_path: RemotePath,
        entry_name: &str,
        local_path: PathBuf,
        cx: &mut AsyncApp,
    ) {
        let Some(transfer_id) = begin_transfer(panel, TransferDirection::Download, entry_name, cx)
        else {
            return;
        };
        let handle = sftp.download(transfer_id as u64, remote_path, local_path);
        run_transfer(panel, backend_key, transfer_id, handle, cx).await;
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use gpui::{AppContext as _, TestAppContext, VisualTestContext};
    use oneterm_core::{AppError, RemotePath, TransferEvent};

    use super::SftpPanel;
    use crate::test_backend::{FakeSftpBackend, dir_entry};
    use crate::types::TransferStatus;

    struct Harness {
        root: gpui::Entity<gpui_component::Root>,
        panel: gpui::Entity<SftpPanel>,
    }

    fn test_panel(cx: &mut TestAppContext) -> (Harness, &mut VisualTestContext) {
        cx.update(gpui_component::init);
        cx.update(oneterm_state::AppState::init);
        cx.update(crate::browser_state::SftpBrowserStore::init);

        let (root, cx) = cx.add_window_view(|window, cx| {
            let panel = cx.new(|cx| SftpPanel::new(window, cx));
            gpui_component::Root::new(panel, window, cx)
        });
        let panel = root.read_with(cx, |root, _| {
            root.view().clone().downcast::<SftpPanel>().unwrap()
        });
        (Harness { root, panel }, cx)
    }

    impl Harness {
        /// Attach a scripted backend as the panel's active session.
        fn attach_backend(&self, cx: &mut VisualTestContext) -> Arc<FakeSftpBackend> {
            let backend = Arc::new(FakeSftpBackend::new());
            self.panel.update(cx, |panel, cx| {
                panel.attach_backend_for_test(backend.clone(), RemotePath::new("/home/u"), cx);
            });
            backend
        }

        /// Show `entries` in the table and select row `selected`.
        fn list_and_select(
            &self,
            entries: Vec<oneterm_core::FileEntry>,
            selected: Option<usize>,
            cx: &mut VisualTestContext,
        ) {
            self.panel.update(cx, |panel, cx| {
                panel.table().update(cx, |table, _| {
                    table.delegate_mut().set_entries(entries);
                });
                panel.browser_mut().select(selected);
            });
        }

        fn statuses(&self, cx: &mut VisualTestContext) -> Vec<TransferStatus> {
            self.panel.read_with(cx, |panel, _| {
                panel
                    .transfers()
                    .items()
                    .iter()
                    .map(|item| item.status)
                    .collect()
            })
        }

        fn notification_count(&self, cx: &mut VisualTestContext) -> usize {
            self.root.read_with(cx, |root, cx| {
                root.notification.read(cx).notifications().len()
            })
        }
    }

    /// TEST-25: with a stale selection and no backend, `do_download` warns the
    /// user and starts nothing — no queue item, no save dialog.
    #[gpui::test]
    fn download_without_backend_warns_and_starts_nothing(cx: &mut TestAppContext) {
        let (harness, cx) = test_panel(cx);
        harness.list_and_select(
            vec![dir_entry(&RemotePath::root(), "example.txt", false)],
            Some(0),
            cx,
        );

        harness.panel.update_in(cx, |panel, window, cx| {
            assert!(panel.sftp().is_none());
            panel.do_download(window, cx);
        });
        cx.run_until_parked();

        assert_eq!(harness.notification_count(cx), 1);
        assert!(harness.statuses(cx).is_empty());
    }

    /// A selection index that no longer points at an entry (the listing changed
    /// underneath it) is reported, not downloaded.
    #[gpui::test]
    fn download_with_out_of_range_selection_warns(cx: &mut TestAppContext) {
        let (harness, cx) = test_panel(cx);
        let backend = harness.attach_backend(cx);
        harness.list_and_select(Vec::new(), Some(3), cx);

        harness.panel.update_in(cx, |panel, window, cx| {
            panel.do_download(window, cx);
        });
        cx.run_until_parked();

        assert_eq!(harness.notification_count(cx), 1);
        assert!(backend.transfer_requests().is_empty());
        assert!(harness.statuses(cx).is_empty());
    }

    /// The happy path: the selected remote file is downloaded to the path the
    /// user picks, the queue item tracks progress and ends `Completed`.
    #[gpui::test]
    fn download_streams_the_selected_file_to_the_chosen_path(cx: &mut TestAppContext) {
        let (harness, cx) = test_panel(cx);
        let backend = harness.attach_backend(cx);
        let transfer = backend.arm_transfer();
        harness.list_and_select(
            vec![dir_entry(&RemotePath::new("/home/u"), "example.txt", false)],
            Some(0),
            cx,
        );

        harness.panel.update_in(cx, |panel, window, cx| {
            panel.do_download(window, cx);
        });
        cx.simulate_new_path_selection(|_| Some(PathBuf::from("picked/example.txt")));
        cx.run_until_parked();

        let requests = backend.transfer_requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].remote.as_str(), "/home/u/example.txt");
        assert_eq!(requests[0].local, PathBuf::from("picked/example.txt"));
        assert_eq!(harness.statuses(cx), vec![TransferStatus::InProgress]);
        assert_eq!(harness.notification_count(cx), 0);

        transfer
            .events
            .try_send(TransferEvent::Progress(0.25))
            .unwrap();
        cx.run_until_parked();
        let progress = harness
            .panel
            .read_with(cx, |panel, _| panel.transfers().items()[0].progress);
        assert!((progress - 0.25).abs() < f64::EPSILON);

        drop(transfer.events);
        transfer.result.try_send(Ok(())).unwrap();
        cx.run_until_parked();
        assert_eq!(harness.statuses(cx), vec![TransferStatus::Completed]);
    }

    /// Cancelling the save dialog leaves the queue untouched.
    #[gpui::test]
    fn dismissed_save_dialog_starts_no_download(cx: &mut TestAppContext) {
        let (harness, cx) = test_panel(cx);
        let backend = harness.attach_backend(cx);
        harness.list_and_select(
            vec![dir_entry(&RemotePath::new("/home/u"), "example.txt", false)],
            Some(0),
            cx,
        );

        harness.panel.update_in(cx, |panel, window, cx| {
            panel.do_download(window, cx);
        });
        cx.simulate_new_path_selection(|_| None);
        cx.run_until_parked();

        assert!(backend.transfer_requests().is_empty());
        assert!(harness.statuses(cx).is_empty());
    }

    /// CORR-31: a cancelled first file must be marked `Cancelled` and the batch
    /// must continue with the next file, whose failure is likewise recorded.
    #[gpui::test]
    fn cancelled_or_failed_upload_does_not_abort_the_batch(cx: &mut TestAppContext) {
        let (harness, cx) = test_panel(cx);
        let backend = harness.attach_backend(cx);
        let first = backend.arm_transfer();
        let second = backend.arm_transfer();

        harness.panel.update(cx, |panel, cx| {
            panel.do_upload_paths(vec![PathBuf::from("a.txt"), PathBuf::from("b.txt")], cx);
        });
        cx.run_until_parked();

        // Only the first transfer has been requested so far.
        let requests = backend.transfer_requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].remote.as_str(), "/home/u/a.txt");
        assert_eq!(harness.statuses(cx), vec![TransferStatus::InProgress]);

        first.events.try_send(TransferEvent::Progress(0.5)).unwrap();
        first.events.try_send(TransferEvent::Cancelled).unwrap();
        drop(first.events);
        first.result.try_send(Err(AppError::Cancelled)).unwrap();
        cx.run_until_parked();

        // The batch moved on to the second file.
        let requests = backend.transfer_requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1].remote.as_str(), "/home/u/b.txt");
        assert_eq!(
            harness.statuses(cx),
            vec![TransferStatus::Cancelled, TransferStatus::InProgress]
        );

        drop(second.events);
        second
            .result
            .try_send(Err(AppError::msg("disk full")))
            .unwrap();
        cx.run_until_parked();

        assert_eq!(
            harness.statuses(cx),
            vec![TransferStatus::Cancelled, TransferStatus::Error]
        );
        let error = harness
            .panel
            .read_with(cx, |panel, _| panel.transfers().items()[1].error.clone());
        assert_eq!(error.as_deref(), Some("disk full"));
    }

    /// ARCH-05: a `Cancelled` result without a preceding `Cancelled` event still
    /// settles the item as cancelled — never left `InProgress`.
    #[gpui::test]
    fn cancelled_result_alone_marks_the_item_cancelled(cx: &mut TestAppContext) {
        let (harness, cx) = test_panel(cx);
        let backend = harness.attach_backend(cx);
        let transfer = backend.arm_transfer();

        harness.panel.update(cx, |panel, cx| {
            panel.do_upload_paths(vec![PathBuf::from("a.txt")], cx);
        });
        cx.run_until_parked();

        drop(transfer.events);
        transfer.result.try_send(Err(AppError::Cancelled)).unwrap();
        cx.run_until_parked();

        assert_eq!(harness.statuses(cx), vec![TransferStatus::Cancelled]);
    }
}
