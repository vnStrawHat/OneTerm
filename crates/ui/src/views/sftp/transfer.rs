//! Transfer operations cho SFTP browser — upload, download.
//!
//! Tách từ `file_browser.rs` để giảm độ dài file.
//! Upload: mở OS native file/folder picker → gọi SFTP backend → poll progress.
//! Download: mở dialog nhập local save path → gọi SFTP backend → poll progress.

use std::path::PathBuf;

use gpui::{App, AppContext, ClickEvent, Context, ParentElement, Styled, Window, div, px};
use gpui_component::{
    ActiveTheme as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::{DialogButtonProps, DialogFooter},
    input::{Input, InputState},
    v_flex,
};

use super::panel::SftpPanel;
use super::types::{TransferDirection, TransferItem, TransferStatus};

impl SftpPanel {
    /// Upload file hoặc thư mục local → remote.
    /// Mở OS native open dialog (chọn file hoặc folder) → sftp.upload() → poll progress.
    pub(crate) fn do_upload(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        log::info!("SftpPanel::do_upload: cwd=\"{}\"", self.cwd.display());

        let sftp = self.sftp.clone().unwrap();
        let panel = cx.entity();
        let cwd = self.cwd.clone();

        // Mở OS native file picker — cho phép chọn files và folders, multiple.
        let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: true,
            multiple: true,
            prompt: Some("Select files or folders to upload".into()),
        });

        // Spawn task đợi user chọn path → upload từng path sequentially.
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

            for local_path in paths {
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

                // Add TransferItem to panel — get transfer_id trước khi gọi upload.
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
                        log::debug!(
                            "SftpPanel: added transfer #{id} upload \"{filename}\""
                        );
                        cx.notify();
                        id
                    })
                });

                // Gọi upload với transfer_id.
                let (progress_rx, result_rx) =
                    sftp.upload(transfer_id as u64, local_path, remote_path);

                // Poll progress — sequential, mỗi file đợi xong rồi tới file tiếp theo.
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
                        return; // ← exit task, không upload file tiếp theo.
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

                // Đợi result.
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
                        log::error!(
                            "SftpPanel: upload #{transfer_id} result channel closed"
                        );
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

            // Refresh sau khi upload tất cả files xong.
            cx.update(|cx| {
                panel.update(cx, |this, cx| {
                    this.refresh(cx);
                })
            });
        })
        .detach();
    }

    /// Download file remote → local.
    /// Mở dialog nhập local save path → sftp.download() → poll progress.
    pub(crate) fn do_download(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let entry = match self.selected_entry(cx) {
            Some(e) => e.clone(),
            None => {
                log::warn!("SftpPanel::do_download: no selection");
                window.push_notification("Select a file to download.", cx);
                return;
            }
        };

        if entry.is_dir {
            log::warn!("SftpPanel::do_download: cannot download directory");
            window.push_notification("Cannot download a folder. Select a file.", cx);
            return;
        }

        log::info!("SftpPanel::do_download: \"{}\"", entry.name);

        let sftp = self.sftp.clone().unwrap();
        let panel = cx.entity();
        let remote_path = entry.path.clone();
        let entry_name = entry.name.clone();

        let path_state = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("C:\\path\\to\\save\\here");
            st.set_value(&entry_name, window, cx);
            st
        });

        let path_ok = path_state.clone();

        let save_logic: std::rc::Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) -> bool> =
            std::rc::Rc::new({
                let path_ok = path_ok.clone();
                let sftp = sftp.clone();
                let panel = panel.clone();
                let remote_path = remote_path.clone();
                let entry_name = entry_name.clone();
                move |_, window, cx| {
                    let local = path_ok.read(cx).value().trim().to_string();
                    if local.is_empty() {
                        window.push_notification("Local save path cannot be empty.", cx);
                        return false;
                    }
                    let local_path = PathBuf::from(&local);

                    log::info!(
                        "SftpPanel: download \"{}\" → \"{}\"",
                        remote_path.display(),
                        local_path.display()
                    );

                    // Add TransferItem to panel — get transfer_id trước khi gọi download.
                    let transfer_id = panel.update(cx, |this, cx| {
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
                        log::debug!(
                            "SftpPanel: added transfer #{id} download \"{entry_name}\""
                        );
                        cx.notify();
                        id
                    });

                    // Gọi download với transfer_id (để có thể cancel).
                    let (progress_rx, result_rx) =
                        sftp.download(transfer_id as u64, remote_path.clone(), local_path);

                    window.push_notification(format!("Downloading \"{entry_name}\"..."), cx);

                    // Clone panel for spawn — save_logic is Fn, can be called multiple times.
                    let panel = panel.clone();
                    cx.spawn(async move |cx| {
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
                                    if let Some(item) =
                                        this.transfers.iter_mut().find(|t| t.id == transfer_id)
                                    {
                                        item.progress = progress;
                                        cx.notify();
                                    }
                                });
                            });
                        }
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
                                log::error!(
                                    "SftpPanel: download #{transfer_id} result channel closed"
                                );
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

                    true
                }
            });

        window.open_dialog(cx, move |dialog, _window, _cx| {
            let save_for_click = save_logic.clone();
            let save_for_kb = save_logic.clone();
            dialog
                .title("Download File")
                .w(px(440.))
                .content({
                    let path_state = path_state.clone();
                    move |content, _window, cx| {
                        content.child(
                            v_flex()
                                .gap_1()
                                .w_full()
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(cx.theme().foreground)
                                        .child("Local save path"),
                                )
                                .child(Input::new(&path_state)),
                        )
                    }
                })
                .footer({
                    DialogFooter::new()
                        .child(Button::new("cancel").label("Cancel").outline().on_click(
                            |_, window, cx| {
                                window.close_dialog(cx);
                            },
                        ))
                        .child(
                            Button::new("download")
                                .label("Download")
                                .primary()
                                .on_click(move |_, window, cx| {
                                    if save_for_click(&ClickEvent::default(), window, cx) {
                                        window.close_dialog(cx);
                                    }
                                }),
                        )
                })
                .button_props(
                    DialogButtonProps::default()
                        .on_cancel(|_, _, _| true)
                        .on_ok(move |_, window, cx| {
                            save_for_kb(&ClickEvent::default(), window, cx)
                        }),
                )
        });
    }
}