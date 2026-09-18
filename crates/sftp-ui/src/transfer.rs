//! Transfer operations for the SFTP browser — upload, download.
//!
//! Upload: open the OS-native file/folder picker OR drag & drop external files
//!         → call the SFTP backend → drive the transfer handle.
//! Download: open the OS-native Save dialog (prompt_for_new_path) → call the SFTP
//!           backend → drive the transfer handle. Supports both files and folders
//!           (recursive download).
//!
//! Both directions share [`run_transfer`], which maps `TransferEvent`s and the
//! final result onto the queue item's status. A cancelled or failed item never
//! stays `InProgress`, and one failure never aborts the remaining files of a batch.
//!
//! They also share one overwrite decision: neither direction replaces a file that
//! is already there without asking ([`confirm_replace`], US-0128).

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gpui::{App, AsyncApp, AsyncWindowContext, Context, Entity, ParentElement as _, Window};
use gpui_component::{
    WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::{DialogButtonProps, DialogFooter},
    notification::NotificationType,
};
use oneterm_core::{AppError, FileEntry, RemotePath, SftpBackend, TransferEvent, TransferHandle};
use oneterm_theme::notif_ext::notify;

use super::browser_state::BackendKey;
use super::panel::SftpPanel;
use super::types::{TransferDirection, TransferItem, TransferStatus};

/// Tell the user that an OS file dialog could not be shown (ERR-07). The
/// window may already be closed; then there is nobody to tell.
fn notify_dialog_failure(what: &str, error: &dyn std::fmt::Display, cx: &mut AsyncWindowContext) {
    let message = format!("{what}: {error}");
    _ = cx.update(|window, cx| {
        window.push_notification(notify(NotificationType::Error, message, cx), cx);
    });
}

/// Ask before an existing target is replaced — the one overwrite decision both
/// transfer directions make (US-0128).
///
/// Download calls it before it writes over a local file, upload before it writes
/// over a remote one, so both show the same dialog: the target named in the
/// description, the destructive answer styled `danger`, the keep-what-is-there
/// answer neutral. `answer` runs exactly once — with `true` only when the user
/// chose to replace — so nothing is transferred until the question is answered.
pub(crate) fn confirm_replace(
    title: &'static str,
    description: String,
    replace_label: &'static str,
    keep_label: &'static str,
    answer: impl Fn(bool, &mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    // The dialog builder is a `Fn` and both buttons plus the dismiss path can
    // reach `answer`; the flag keeps the decision to the first one that does.
    let answered = std::cell::Cell::new(false);
    let answer = Rc::new(move |replace: bool, window: &mut Window, cx: &mut App| {
        if !answered.replace(true) {
            answer(replace, window, cx);
        }
    });
    window.open_alert_dialog(cx, move |alert, _window, _cx| {
        let keep = answer.clone();
        let replace = answer.clone();
        alert
            .confirm()
            .title(title)
            .description(description.clone())
            .footer(
                DialogFooter::new()
                    .child(Button::new("keep").label(keep_label).outline().on_click(
                        move |_, window, cx| {
                            window.close_dialog(cx);
                            keep(false, window, cx);
                        },
                    ))
                    .child(
                        Button::new("replace")
                            .label(replace_label)
                            .danger()
                            .on_click(move |_, window, cx| {
                                window.close_dialog(cx);
                                replace(true, window, cx);
                            }),
                    ),
            )
            .button_props(
                DialogButtonProps::default()
                    .on_cancel(|_, _, _| true)
                    .on_ok(|_, _, _| false),
            )
    });
}

/// The name an uploaded path takes in the remote directory.
fn upload_name(local: &Path) -> String {
    local
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "uploaded".to_string())
}

/// The names of `local_paths` that an upload would write over, in batch order.
///
/// `existing` is what the remote directory holds right now. An empty result
/// means the batch can start without asking.
fn colliding_names(local_paths: &[PathBuf], existing: &[String]) -> Vec<String> {
    local_paths
        .iter()
        .map(|path| upload_name(path))
        .filter(|name| existing.contains(name))
        .collect()
}

/// What is left of the batch once the user has answered: `replace` keeps all of
/// it, otherwise the colliding files are dropped and the rest still goes up.
fn keep_after_answer(
    local_paths: Vec<PathBuf>,
    colliding: &[String],
    replace: bool,
) -> Vec<PathBuf> {
    if replace {
        return local_paths;
    }
    local_paths
        .into_iter()
        .filter(|path| !colliding.contains(&upload_name(path)))
        .collect()
}

/// Word the overwrite question for `colliding` out of a batch of `batch_len`:
/// title, description, the destructive label and the neutral one.
///
/// The neutral answer is "Cancel" when skipping leaves nothing to upload and
/// "Skip" when the rest of the batch still goes up, so the button never claims
/// to stop more than it stops.
fn collision_prompt(
    colliding: &[String],
    batch_len: usize,
) -> (&'static str, String, &'static str, &'static str) {
    let keep_label = if colliding.len() < batch_len {
        "Skip"
    } else {
        "Cancel"
    };
    if let [name] = colliding {
        return (
            "Replace Remote File",
            format!("\"{name}\" already exists in the remote folder. Replace it?"),
            "Replace",
            keep_label,
        );
    }
    // Long batches name the first few files rather than filling the dialog.
    const SHOWN: usize = 5;
    let mut listed = colliding
        .iter()
        .take(SHOWN)
        .map(|name| format!("\"{name}\""))
        .collect::<Vec<_>>()
        .join(", ");
    if colliding.len() > SHOWN {
        listed.push_str(&format!(", and {} more", colliding.len() - SHOWN));
    }
    (
        "Replace Remote Files",
        format!(
            "{} files already exist in the remote folder: {listed}. Replace them?",
            colliding.len()
        ),
        "Replace all",
        keep_label,
    )
}

/// Register a queue item for a transfer that is about to start.
/// Returns the allocated transfer id, or `None` when no backend is active.
pub(crate) fn begin_transfer(
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
pub(crate) async fn run_transfer(
    panel: &Entity<SftpPanel>,
    key: BackendKey,
    transfer_id: usize,
    handle: TransferHandle,
    cx: &mut AsyncApp,
) -> TransferStatus {
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
            TransferStatus::Completed
        }
        Ok(Err(AppError::Cancelled)) => {
            log::info!("SftpPanel: transfer #{transfer_id} cancelled");
            apply(cx, &|item| item.status = TransferStatus::Cancelled);
            TransferStatus::Cancelled
        }
        Ok(Err(error)) => {
            log::error!("SftpPanel: transfer #{transfer_id} failed: {error}");
            let message = error.to_string();
            apply(cx, &|item| {
                item.status = TransferStatus::Error;
                item.error = Some(message.clone());
            });
            TransferStatus::Error
        }
        Err(_) => {
            log::error!("SftpPanel: transfer #{transfer_id} result channel closed");
            apply(cx, &|item| {
                item.status = TransferStatus::Error;
                item.error = Some("channel closed".to_string());
            });
            TransferStatus::Error
        }
    }
}

impl SftpPanel {
    /// Upload a list of local paths → remote cwd.
    ///
    /// Core logic — every GUI upload path reaches the backend through here: the
    /// file picker (`do_upload`), the Local pane's Upload button, and both drop
    /// targets (`on_drop` in render). So this is also where the overwrite
    /// question is asked (US-0128): the remote cwd is listed first, and a batch
    /// that would write over something already there starts nothing until the
    /// user answers [`confirm_replace`].
    pub(crate) fn do_upload_paths(
        &mut self,
        local_paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

        cx.spawn_in(window, async move |_panel, cx| {
            // What is in the remote directory right now decides which of the
            // batch would be overwritten. A directory we cannot list is a
            // best-effort miss, not a reason to refuse the upload: it only means
            // the collision cannot be known, so the batch runs as it always did.
            let existing: Vec<String> = match sftp.read_dir(cwd.clone()).await {
                Ok(entries) => entries.into_iter().map(|entry| entry.name).collect(),
                Err(error) => {
                    log::warn!(
                        "SftpPanel::do_upload_paths: could not list \"{cwd}\" to check for \
                         overwrites: {error}"
                    );
                    Vec::new()
                }
            };
            let colliding = colliding_names(&local_paths, &existing);
            if colliding.is_empty() {
                Self::upload_batch(&panel, sftp, cwd, local_paths, cx).await;
                return;
            }

            log::info!(
                "SftpPanel::do_upload_paths: {} of {} path(s) already exist in \"{cwd}\" — asking",
                colliding.len(),
                local_paths.len()
            );
            let (title, description, replace_label, keep_label) =
                collision_prompt(&colliding, local_paths.len());
            _ = cx.update(|window, cx| {
                confirm_replace(
                    title,
                    description,
                    replace_label,
                    keep_label,
                    move |replace, window, cx| {
                        let keep = keep_after_answer(local_paths.clone(), &colliding, replace);
                        if keep.is_empty() {
                            log::info!("SftpPanel::do_upload_paths: nothing left to upload");
                            return;
                        }
                        let panel = panel.clone();
                        let sftp = sftp.clone();
                        let cwd = cwd.clone();
                        window
                            .spawn(cx, async move |cx| {
                                Self::upload_batch(&panel, sftp, cwd, keep, cx).await;
                            })
                            .detach();
                    },
                    window,
                    cx,
                );
            });
        })
        .detach();
    }

    /// Upload each path of an already-confirmed batch into `cwd`, sequentially,
    /// then refresh the listing. A cancelled or failed file does not stop the
    /// remaining files of the batch.
    async fn upload_batch(
        panel: &Entity<SftpPanel>,
        sftp: Arc<dyn SftpBackend>,
        cwd: RemotePath,
        local_paths: Vec<PathBuf>,
        cx: &mut AsyncApp,
    ) {
        let backend_key = sftp.session_id();
        for local_path in local_paths {
            let filename = upload_name(&local_path);
            let remote_path = cwd.join(&filename);

            log::info!(
                "SftpPanel: upload \"{}\" → \"{remote_path}\"",
                local_path.display()
            );

            let Some(transfer_id) = begin_transfer(panel, TransferDirection::Upload, &filename, cx)
            else {
                return;
            };

            // Sequential: each file finishes before the next one starts.
            let handle = sftp.upload(transfer_id as u64, local_path, remote_path);
            run_transfer(panel, backend_key, transfer_id, handle, cx).await;
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
    }

    /// Upload a local file or folder → remote.
    /// Opens the OS-native open dialog (choose files or a folder) → calls `do_upload_paths`.
    /// `pick_folders` — true: folder picker, false: file picker (multiple).
    /// Windows does not support mixed files+folders in one dialog, so the two modes are separate.
    pub(crate) fn do_upload(
        &mut self,
        pick_folders: bool,
        window: &mut Window,
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
        cx.spawn_in(window, async move |_panel, cx| {
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
                    notify_dialog_failure("Could not open the file picker", &e, cx);
                    return;
                }
                Err(e) => {
                    log::error!("SftpPanel: upload — channel error: {e}");
                    notify_dialog_failure("Could not open the file picker", &e, cx);
                    return;
                }
            };

            log::info!("SftpPanel: upload — {} path(s) selected", paths.len());

            // The panel may be gone before the picker closes; nothing to upload then.
            _ = cx.update(|window, cx| {
                panel.update(cx, |this, cx| {
                    this.do_upload_paths(paths, window, cx);
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
        // With the Local pane shown, download straight into its directory.
        if self.expanded() {
            self.download_entry_to_local(entry, window, cx);
            return;
        }
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
        cx.spawn_in(window, async move |_panel, cx| {
            let local_path = match rx.await {
                Ok(Ok(Some(path))) => path,
                Ok(Ok(None)) => {
                    log::debug!("SftpPanel: download — user cancelled");
                    return;
                }
                Ok(Err(e)) => {
                    log::error!("SftpPanel: download — save dialog error: {e}");
                    notify_dialog_failure("Could not open the save dialog", &e, cx);
                    return;
                }
                Err(e) => {
                    log::error!("SftpPanel: download — channel error: {e}");
                    notify_dialog_failure("Could not open the save dialog", &e, cx);
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

    /// Download `entry` into the Local pane's directory (dual-pane mode): no
    /// save dialog, but an existing target must be confirmed first (IN-0025
    /// LLD). Also the drop target of a remote row dragged onto the local list.
    pub(crate) fn download_entry_to_local(
        &mut self,
        entry: FileEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(sftp) = self.sftp().cloned() else {
            log::warn!("SftpPanel::download_entry_to_local: no active SFTP backend");
            window.push_notification(
                notify(
                    NotificationType::Warning,
                    "No active SFTP connection is available.",
                    cx,
                ),
                cx,
            );
            return;
        };
        let local_dir = self.local().read(cx).cwd().to_path_buf();
        if local_dir.as_os_str().is_empty() {
            window.push_notification(
                notify(
                    NotificationType::Warning,
                    "The local pane has no directory open yet.",
                    cx,
                ),
                cx,
            );
            return;
        }
        let target = local_dir.join(&entry.name);
        let backend_key = sftp.session_id();
        let panel = cx.entity();
        log::info!(
            "SftpPanel: download \"{}\" → \"{}\" (local pane)",
            entry.path,
            target.display()
        );

        cx.spawn_in(window, async move |_panel, cx| {
            let probe = target.clone();
            let exists = cx
                .background_executor()
                .spawn(async move { probe.exists() })
                .await;
            let start = {
                let panel = panel.clone();
                let sftp = sftp.clone();
                let remote_path = entry.path.clone();
                let entry_name = entry.name.clone();
                let target = target.clone();
                move |window: &mut Window, cx: &mut App| {
                    let panel = panel.clone();
                    let sftp = sftp.clone();
                    let remote_path = remote_path.clone();
                    let entry_name = entry_name.clone();
                    let target = target.clone();
                    window
                        .spawn(cx, async move |cx| {
                            Self::download_to(
                                &panel,
                                sftp,
                                backend_key,
                                remote_path,
                                &entry_name,
                                target,
                                cx,
                            )
                            .await;
                        })
                        .detach();
                }
            };
            if !exists {
                _ = cx.update(|window, cx| start(window, cx));
                return;
            }
            let description = format!(
                "\"{}\" already exists in the local folder. Replace it?",
                entry.name
            );
            _ = cx.update(|window, cx| {
                confirm_replace(
                    "Replace Local File",
                    description,
                    "Replace",
                    "Cancel",
                    move |replace, window, cx| {
                        if replace {
                            start(window, cx);
                        }
                    },
                    window,
                    cx,
                );
            });
        })
        .detach();
    }

    /// Register a download in the queue and drive it to completion; the Local
    /// pane (when shown) refreshes afterwards so a new file appears.
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
        cx.update(|cx| {
            panel.update(cx, |this, cx| {
                if this.expanded() {
                    this.local().update(cx, |local, cx| local.refresh(cx));
                }
            })
        });
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use gpui::{AppContext as _, TestAppContext, VisualTestContext};
    use oneterm_core::{AppError, RemotePath, TransferEvent};

    use gpui_component::WindowExt as _;

    use super::SftpPanel;
    use crate::test_backend::{FakeSftpBackend, TempDir, dir_entry};
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

    impl Harness {
        /// Show the Local pane on `dir` (no persistence involved).
        fn expand_into(&self, dir: &std::path::Path, cx: &mut VisualTestContext) {
            self.panel.update(cx, |panel, cx| {
                panel.local().update(cx, |local, cx| {
                    local.set_initial_dir(Some(dir.to_path_buf()), cx)
                });
                panel.set_expanded(true, cx);
            });
            cx.run_until_parked();
        }

        fn has_dialog(&self, cx: &mut VisualTestContext) -> bool {
            self.panel
                .update_in(cx, |_, window, cx| window.has_active_dialog(cx))
        }
    }

    /// IN-0025: with the Local pane shown, a download goes straight into its
    /// directory — no save dialog — and the local list refreshes afterwards.
    #[gpui::test]
    fn expanded_download_targets_the_local_directory_without_a_dialog(cx: &mut TestAppContext) {
        let temp = TempDir::new();
        let (harness, cx) = test_panel(cx);
        let backend = harness.attach_backend(cx);
        let transfer = backend.arm_transfer();
        harness.expand_into(&temp.0, cx);
        harness.list_and_select(
            vec![dir_entry(&RemotePath::new("/home/u"), "example.txt", false)],
            Some(0),
            cx,
        );

        harness.panel.update_in(cx, |panel, window, cx| {
            panel.do_download(window, cx);
        });
        cx.run_until_parked();

        let requests = backend.transfer_requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].remote.as_str(), "/home/u/example.txt");
        assert_eq!(
            requests[0].local,
            std::path::absolute(&temp.0).unwrap().join("example.txt")
        );
        assert!(!harness.has_dialog(cx));
        assert_eq!(harness.statuses(cx), vec![TransferStatus::InProgress]);

        // The backend "writes" the file; settling the transfer refreshes the pane.
        std::fs::write(temp.0.join("example.txt"), b"downloaded").unwrap();
        drop(transfer.events);
        transfer.result.try_send(Ok(())).unwrap();
        cx.run_until_parked();
        assert_eq!(harness.statuses(cx), vec![TransferStatus::Completed]);
        let local_names: Vec<String> = harness.panel.read_with(cx, |panel, cx| {
            let local = panel.local().read(cx);
            local
                .table()
                .read(cx)
                .delegate()
                .entries()
                .iter()
                .map(|e| e.name.clone())
                .collect()
        });
        assert_eq!(local_names, vec!["example.txt"]);
    }

    /// IN-0025 LLD: an existing local file is never replaced without a
    /// confirmation; the dialog opens and nothing is transferred until then.
    #[gpui::test]
    fn expanded_download_onto_an_existing_file_asks_first(cx: &mut TestAppContext) {
        let temp = TempDir::new();
        std::fs::write(temp.0.join("example.txt"), b"keep me").unwrap();
        let (harness, cx) = test_panel(cx);
        let backend = harness.attach_backend(cx);
        harness.expand_into(&temp.0, cx);
        harness.list_and_select(
            vec![dir_entry(&RemotePath::new("/home/u"), "example.txt", false)],
            Some(0),
            cx,
        );

        harness.panel.update_in(cx, |panel, window, cx| {
            panel.do_download(window, cx);
        });
        cx.run_until_parked();

        assert!(harness.has_dialog(cx));
        assert!(backend.transfer_requests().is_empty());
        assert!(harness.statuses(cx).is_empty());
        assert_eq!(
            std::fs::read(temp.0.join("example.txt")).unwrap(),
            b"keep me"
        );
    }

    /// IN-0025: the Local pane's selection uploads into the remote cwd through
    /// the shared upload path.
    #[gpui::test]
    fn local_selection_uploads_into_the_remote_cwd(cx: &mut TestAppContext) {
        let temp = TempDir::new();
        std::fs::write(temp.0.join("notes.md"), b"# hi").unwrap();
        let (harness, cx) = test_panel(cx);
        let backend = harness.attach_backend(cx);
        let _transfer = backend.arm_transfer();
        harness.expand_into(&temp.0, cx);

        let local = harness
            .panel
            .read_with(cx, |panel, _| panel.local().clone());
        local.update_in(cx, |local, window, cx| {
            local.select(Some(0));
            local.upload_selected(window, cx);
        });
        cx.run_until_parked();

        let requests = backend.transfer_requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].remote.as_str(), "/home/u/notes.md");
        assert_eq!(
            requests[0].local,
            std::path::absolute(&temp.0).unwrap().join("notes.md")
        );
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

        harness.panel.update_in(cx, |panel, window, cx| {
            panel.do_upload_paths(
                vec![PathBuf::from("a.txt"), PathBuf::from("b.txt")],
                window,
                cx,
            );
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

    // ── US-0128: the overwrite decision ──────────────────────

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    fn owned(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    /// US-0128: only the batch entries whose remote name is already in the
    /// directory collide, and they are reported in batch order.
    #[test]
    fn collisions_are_the_batch_names_the_remote_directory_already_holds() {
        let batch = paths(&["a/notes.md", "b/new.txt", "c/report.pdf"]);
        let existing = owned(&["report.pdf", "notes.md", "unrelated.bin"]);

        assert_eq!(
            super::colliding_names(&batch, &existing),
            vec!["notes.md".to_string(), "report.pdf".to_string()]
        );
        assert!(super::colliding_names(&batch, &[]).is_empty());
        assert!(super::colliding_names(&[], &existing).is_empty());
    }

    /// A path with no file name still gets the fallback name the upload uses,
    /// so it is matched against the directory like any other entry.
    #[test]
    fn a_nameless_path_collides_under_its_fallback_name() {
        assert_eq!(
            super::colliding_names(&paths(&[".."]), &owned(&["uploaded"])),
            vec!["uploaded".to_string()]
        );
    }

    /// US-0128: Replace uploads the whole batch; Skip drops exactly the
    /// colliding files and still uploads the rest, in the original order.
    #[test]
    fn the_answer_decides_what_is_left_of_the_batch() {
        let batch = paths(&["a/notes.md", "b/new.txt", "c/report.pdf"]);
        let colliding = owned(&["notes.md", "report.pdf"]);

        assert_eq!(
            super::keep_after_answer(batch.clone(), &colliding, true),
            batch
        );
        assert_eq!(
            super::keep_after_answer(batch.clone(), &colliding, false),
            paths(&["b/new.txt"])
        );
        // A batch that collides everywhere uploads nothing when skipped.
        assert!(
            super::keep_after_answer(batch, &owned(&["notes.md", "new.txt", "report.pdf"]), false)
                .is_empty()
        );
    }

    /// US-0128: one collision asks "Replace", several ask "Replace all"; the
    /// neutral button only says "Cancel" when skipping leaves nothing to upload.
    #[test]
    fn the_prompt_names_the_files_and_labels_the_answers() {
        let (title, description, replace, keep) = super::collision_prompt(&owned(&["notes.md"]), 1);
        assert_eq!(title, "Replace Remote File");
        assert_eq!(
            description,
            "\"notes.md\" already exists in the remote folder. Replace it?"
        );
        assert_eq!((replace, keep), ("Replace", "Cancel"));

        // One collision, but the rest of the batch still goes up → "Skip".
        let (_, _, replace, keep) = super::collision_prompt(&owned(&["notes.md"]), 3);
        assert_eq!((replace, keep), ("Replace", "Skip"));

        let (title, description, replace, keep) =
            super::collision_prompt(&owned(&["a.txt", "b.txt"]), 2);
        assert_eq!(title, "Replace Remote Files");
        assert_eq!(
            description,
            "2 files already exist in the remote folder: \"a.txt\", \"b.txt\". Replace them?"
        );
        assert_eq!((replace, keep), ("Replace all", "Cancel"));

        // A long list names the first five and counts the rest.
        let many = owned(&["1", "2", "3", "4", "5", "6", "7"]);
        let (_, description, _, _) = super::collision_prompt(&many, 7);
        assert!(
            description.ends_with("\"1\", \"2\", \"3\", \"4\", \"5\", and 2 more. Replace them?"),
            "{description}"
        );
    }

    /// US-0128: a batch that overwrites nothing is unchanged — no dialog, the
    /// upload starts straight away.
    #[gpui::test]
    fn upload_without_a_collision_asks_nothing(cx: &mut TestAppContext) {
        let (harness, cx) = test_panel(cx);
        let backend = harness.attach_backend(cx);
        let _transfer = backend.arm_transfer();
        backend
            .arm_read_dir()
            .try_send(Ok(vec![dir_entry(
                &RemotePath::new("/home/u"),
                "other.txt",
                false,
            )]))
            .unwrap();

        harness.panel.update_in(cx, |panel, window, cx| {
            panel.do_upload_paths(vec![PathBuf::from("a.txt")], window, cx);
        });
        cx.run_until_parked();

        assert!(!harness.has_dialog(cx));
        let requests = backend.transfer_requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].remote.as_str(), "/home/u/a.txt");
    }

    /// US-0128 (the data-loss path of the UX round's report §4.5): an upload
    /// onto an existing remote file opens the confirmation and asks the backend
    /// for nothing until it is answered.
    #[gpui::test]
    fn upload_onto_an_existing_remote_file_asks_first(cx: &mut TestAppContext) {
        let (harness, cx) = test_panel(cx);
        let backend = harness.attach_backend(cx);
        backend
            .arm_read_dir()
            .try_send(Ok(vec![dir_entry(
                &RemotePath::new("/home/u"),
                "a.txt",
                false,
            )]))
            .unwrap();

        harness.panel.update_in(cx, |panel, window, cx| {
            panel.do_upload_paths(vec![PathBuf::from("a.txt")], window, cx);
        });
        cx.run_until_parked();

        assert!(harness.has_dialog(cx));
        assert!(backend.transfer_requests().is_empty());
        assert!(harness.statuses(cx).is_empty());
    }

    /// US-0128: several colliding files of one batch ask once, not once per file.
    #[gpui::test]
    fn a_batch_with_several_collisions_asks_once(cx: &mut TestAppContext) {
        let (harness, cx) = test_panel(cx);
        let backend = harness.attach_backend(cx);
        let cwd = RemotePath::new("/home/u");
        backend
            .arm_read_dir()
            .try_send(Ok(vec![
                dir_entry(&cwd, "a.txt", false),
                dir_entry(&cwd, "b.txt", false),
            ]))
            .unwrap();

        harness.panel.update_in(cx, |panel, window, cx| {
            panel.do_upload_paths(
                vec![
                    PathBuf::from("a.txt"),
                    PathBuf::from("b.txt"),
                    PathBuf::from("c.txt"),
                ],
                window,
                cx,
            );
        });
        cx.run_until_parked();

        assert!(harness.has_dialog(cx));
        assert!(backend.transfer_requests().is_empty());
        // One listing, one question — not one per file.
        assert_eq!(backend.read_dir_requests(), vec![cwd]);
    }

    /// US-0128: a remote directory that cannot be listed must not block the
    /// upload — the collision is simply unknown.
    #[gpui::test]
    fn an_unlistable_remote_directory_does_not_block_the_upload(cx: &mut TestAppContext) {
        let (harness, cx) = test_panel(cx);
        let backend = harness.attach_backend(cx);
        let _transfer = backend.arm_transfer();
        backend
            .arm_read_dir()
            .try_send(Err(AppError::msg("permission denied")))
            .unwrap();

        harness.panel.update_in(cx, |panel, window, cx| {
            panel.do_upload_paths(vec![PathBuf::from("a.txt")], window, cx);
        });
        cx.run_until_parked();

        assert!(!harness.has_dialog(cx));
        assert_eq!(backend.transfer_requests().len(), 1);
    }

    /// ARCH-05: a `Cancelled` result without a preceding `Cancelled` event still
    /// settles the item as cancelled — never left `InProgress`.
    #[gpui::test]
    fn cancelled_result_alone_marks_the_item_cancelled(cx: &mut TestAppContext) {
        let (harness, cx) = test_panel(cx);
        let backend = harness.attach_backend(cx);
        let transfer = backend.arm_transfer();

        harness.panel.update_in(cx, |panel, window, cx| {
            panel.do_upload_paths(vec![PathBuf::from("a.txt")], window, cx);
        });
        cx.run_until_parked();

        drop(transfer.events);
        transfer.result.try_send(Err(AppError::Cancelled)).unwrap();
        cx.run_until_parked();

        assert_eq!(harness.statuses(cx), vec![TransferStatus::Cancelled]);
    }
}
