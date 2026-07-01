//! Transfer operations for the SFTP browser — upload, download.
//!
//! Split out from `file_browser.rs` to keep the file shorter.
//! Upload: open the OS-native file/folder picker OR drag & drop external files
//!         → call the SFTP backend → poll progress.
//! Download: open the OS-native Save dialog (prompt_for_new_path) → call the SFTP
//!           backend → poll progress. Supports both files and folders (recursive download).

use std::path::PathBuf;

use gpui::{Context, Window};
use gpui_component::WindowExt as _;

use super::panel::SftpPanel;
use super::types::{TransferDirection, TransferItem, TransferStatus};

impl SftpPanel {
    /// Upload a list of local paths → remote cwd.
    ///
    /// Core logic — used by both the file picker (`do_upload`) and drag & drop
    /// (`on_drop` in render). Uploads each path sequentially, adds a TransferItem,
    /// polls progress, and refreshes when done.
    pub(crate) fn do_upload_paths(&mut self, local_paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        if local_paths.is_empty() {
            return;
        }

        let sftp = match self.sftp.clone() {
            Some(s) => s,
            None => {
                log::warn!("SftpPanel::do_upload_paths: no SFTP connection");
                return;
            }
        };
        let panel = cx.entity();
        let cwd = self.cwd.clone();

        log::info!(
            "SftpPanel::do_upload_paths: {} path(s) → \"{}\"",
            local_paths.len(),
            cwd.display()
        );

        cx.spawn(async move |_panel, cx| {
            for local_path in local_paths {
                // Remote path: cwd / filename
                let filename = local_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "uploaded".to_string());
                let remote_path = cwd.join(&filename);

                log::info!(
                    "SftpPanel: upload \"{}\" → \"{}\"",
                    local_path.display(),
                    remote_path.display()
                );

                // Add a TransferItem to the panel — get transfer_id before calling upload.
                let transfer_id = cx.update(|cx| {
                    panel.update(cx, |this, cx| {
                        let id = this.next_transfer_id;
                        this.next_transfer_id += 1;
                        this.transfers.push(TransferItem {
                            id,
                            direction: TransferDirection::Upload,
                            filename: filename.clone(),
                            progress: 0.0,
                            status: TransferStatus::InProgress,
                            error: None,
                        });
                        log::debug!("SftpPanel: added transfer #{id} upload \"{filename}\"");
                        cx.notify();
                        id
                    })
                });

                // Call upload with transfer_id.
                let (progress_rx, result_rx) =
                    sftp.upload(transfer_id as u64, local_path, remote_path);

                // Poll progress — sequential, each file finishes before the next one.
                while let Ok(progress) = progress_rx.recv().await {
                    // progress = -1.0 → cancelled signal.
                    if progress < 0.0 {
                        log::info!("SftpPanel: upload #{transfer_id} cancelled");
                        cx.update(|cx| {
                            panel.update(cx, |this, cx| {
                                if let Some(item) =
                                    this.transfers.iter_mut().find(|t| t.id == transfer_id)
                                {
                                    item.status = TransferStatus::Cancelled;
                                }
                                cx.notify();
                            })
                        });
                        return; // ← exit task, do not upload the next file.
                    }
                    log::debug!(
                        "SftpPanel: upload #{transfer_id} progress {:.0}%",
                        progress * 100.0
                    );
                    cx.update(|cx| {
                        panel.update(cx, |this, cx| {
                            if let Some(item) =
                                this.transfers.iter_mut().find(|t| t.id == transfer_id)
                            {
                                item.progress = progress;
                                cx.notify();
                            }
                        })
                    });
                }

                // Wait for the result.
                match result_rx.recv().await {
                    Ok(Ok(())) => {
                        log::info!("SftpPanel: upload #{transfer_id} OK");
                        cx.update(|cx| {
                            panel.update(cx, |this, cx| {
                                if let Some(item) =
                                    this.transfers.iter_mut().find(|t| t.id == transfer_id)
                                {
                                    item.status = TransferStatus::Completed;
                                    item.progress = 1.0;
                                }
                                cx.notify();
                            })
                        });
                    }
                    Ok(Err(e)) => {
                        if e.to_string() == "cancelled" {
                            return;
                        }
                        log::error!("SftpPanel: upload #{transfer_id} failed: {e}");
                        cx.update(|cx| {
                            panel.update(cx, |this, cx| {
                                if let Some(item) =
                                    this.transfers.iter_mut().find(|t| t.id == transfer_id)
                                {
                                    item.status = TransferStatus::Error;
                                    item.error = Some(e.to_string());
                                }
                                cx.notify();
                            })
                        });
                    }
                    Err(_) => {
                        log::error!("SftpPanel: upload #{transfer_id} result channel closed");
                        cx.update(|cx| {
                            panel.update(cx, |this, cx| {
                                if let Some(item) =
                                    this.transfers.iter_mut().find(|t| t.id == transfer_id)
                                {
                                    item.status = TransferStatus::Error;
                                    item.error = Some("channel closed".to_string());
                                }
                                cx.notify();
                            })
                        });
                    }
                }
            }

            // Refresh after all files have been uploaded.
            cx.update(|cx| {
                panel.update(cx, |this, cx| {
                    this.refresh(cx);
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
            self.cwd.display()
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
    /// After the user picks a path → sftp.download() → poll progress.
    ///
    /// The backend branches between file/folder automatically:
    /// - File: download directly.
    /// - Folder: recursively walk the remote tree, create local dirs, download each file.
    pub(crate) fn do_download(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let entry = match self.selected_entry(cx) {
            Some(e) => e.clone(),
            None => {
                log::warn!("SftpPanel::do_download: no selection");
                window.push_notification("Select a file or folder to download.", cx);
                return;
            }
        };

        log::info!(
            "SftpPanel::do_download: \"{}\" (is_dir={})",
            entry.name,
            entry.is_dir
        );

        let sftp = self.sftp.clone().unwrap();
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
                "SftpPanel: download \"{}\" → \"{}\"",
                remote_path.display(),
                local_path.display()
            );

            // Add a TransferItem to the panel — get transfer_id before calling download.
            let transfer_id = cx.update(|cx| {
                panel.update(cx, |this, cx| {
                    let id = this.next_transfer_id;
                    this.next_transfer_id += 1;
                    this.transfers.push(TransferItem {
                        id,
                        direction: TransferDirection::Download,
                        filename: entry_name.clone(),
                        progress: 0.0,
                        status: TransferStatus::InProgress,
                        error: None,
                    });
                    log::debug!("SftpPanel: added transfer #{id} download \"{entry_name}\"");
                    cx.notify();
                    id
                })
            });

            // Call download with transfer_id (so it can be cancelled).
            let (progress_rx, result_rx) =
                sftp.download(transfer_id as u64, remote_path.clone(), local_path);

            // Poll progress.
            while let Ok(progress) = progress_rx.recv().await {
                // progress = -1.0 → cancelled signal.
                if progress < 0.0 {
                    log::info!("SftpPanel: download #{transfer_id} cancelled");
                    cx.update(|cx| {
                        panel.update(cx, |this, cx| {
                            if let Some(item) =
                                this.transfers.iter_mut().find(|t| t.id == transfer_id)
                            {
                                item.status = TransferStatus::Cancelled;
                            }
                            cx.notify();
                        });
                    });
                    return; // ← exit spawn task.
                }
                log::debug!(
                    "SftpPanel: download #{transfer_id} progress {:.0}%",
                    progress * 100.0
                );
                cx.update(|cx| {
                    panel.update(cx, |this, cx| {
                        if let Some(item) = this.transfers.iter_mut().find(|t| t.id == transfer_id)
                        {
                            item.progress = progress;
                            cx.notify();
                        }
                    });
                });
            }

            // Wait for the result.
            match result_rx.recv().await {
                Ok(Ok(())) => {
                    log::info!("SftpPanel: download #{transfer_id} OK");
                    cx.update(|cx| {
                        panel.update(cx, |this, cx| {
                            if let Some(item) =
                                this.transfers.iter_mut().find(|t| t.id == transfer_id)
                            {
                                item.status = TransferStatus::Completed;
                                item.progress = 1.0;
                            }
                            cx.notify();
                        });
                    });
                }
                Ok(Err(e)) => {
                    if e.to_string() == "cancelled" {
                        return;
                    }
                    log::error!("SftpPanel: download #{transfer_id} failed: {e}");
                    cx.update(|cx| {
                        panel.update(cx, |this, cx| {
                            if let Some(item) =
                                this.transfers.iter_mut().find(|t| t.id == transfer_id)
                            {
                                item.status = TransferStatus::Error;
                                item.error = Some(e.to_string());
                            }
                            cx.notify();
                        });
                    });
                }
                Err(_) => {
                    log::error!("SftpPanel: download #{transfer_id} result channel closed");
                    cx.update(|cx| {
                        panel.update(cx, |this, cx| {
                            if let Some(item) =
                                this.transfers.iter_mut().find(|t| t.id == transfer_id)
                            {
                                item.status = TransferStatus::Error;
                                item.error = Some("channel closed".to_string());
                            }
                            cx.notify();
                        });
                    });
                }
            }
        })
        .detach();
    }
}
