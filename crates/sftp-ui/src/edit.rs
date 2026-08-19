//! Edit a remote file locally: download it to a managed temp copy, open it in
//! the configured editor, watch the copy for saves, and upload changes back
//! over SFTP.
//!
//! Flow (see `docs/spec-intakes/IN-0011-*`):
//! 1. `do_edit` — size gate, download to `edit-cache/<id>/<name>`, launch editor.
//! 2. A `notify` watcher on the temp copy forwards debounced saves to the UI.
//! 3. On save: prompt to upload (unless the per-session "always upload" flag is
//!    set), warn if the remote mtime changed since the baseline, then upload.
//! 4. The temp copy + watcher are cleaned up when the session ends (backend
//!    disconnect, panel drop) or on the next startup sweep.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use gpui::{AnyWindowHandle, AsyncApp, Context, Entity, ParentElement as _, Window, div};
use gpui_component::{WindowExt as _, checkbox::Checkbox, notification::NotificationType};
use notify::{RecursiveMode, Watcher};
use sysinfo::{Pid, ProcessRefreshKind, RefreshKind, System};

use oneterm_core::{EditorChoice, RemotePath, SftpBackend, config_dir, launch_editor};
use oneterm_settings::{EditorConfig, EditorMode, TerminalSettings};
use oneterm_theme::notif_ext::notify;

use super::browser_state::BackendKey;
use super::panel::SftpPanel;
use super::transfer::{begin_transfer, run_transfer};
use super::types::{TransferDirection, TransferStatus};

/// Debounce window: an editor save often emits several filesystem events (or a
/// truncate + rewrite, or an atomic rename); coalesce them into one save.
const SAVE_DEBOUNCE: Duration = Duration::from_millis(400);

/// Process-local identity for one edit session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct EditSessionId(u64);

impl EditSessionId {
    fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

impl std::fmt::Display for EditSessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// One in-flight "edit this remote file locally" session.
pub(crate) struct EditSession {
    backend_key: BackendKey,
    /// The backend the file belongs to — held so uploads work even after the
    /// user switches to a different SSH tab.
    sftp: std::sync::Arc<dyn SftpBackend>,
    remote_path: RemotePath,
    temp_path: PathBuf,
    /// Remote mtime last known to match `temp_path`'s content on the server.
    baseline_mtime: Option<SystemTime>,
    /// Set by the "always upload this file" checkbox — this session only.
    always_upload: bool,
    /// Fingerprint (mtime, len) of the temp copy the last time we inspected it.
    /// A watcher event whose fresh fingerprint equals this is not a real save
    /// (an editor touching metadata/attributes on open, a read, etc.) and is
    /// ignored, so opening the file never pops the upload dialog on its own.
    last_temp_sig: Option<(SystemTime, u64)>,
    /// An upload is in flight; a save arriving now sets `pending_save`.
    uploading: bool,
    /// A save arrived while an upload or prompt was busy; handle it afterwards.
    pending_save: bool,
    /// An upload prompt is currently open; do not stack another.
    prompt_open: bool,
    /// The window this edit session belongs to — used to open the upload /
    /// conflict dialogs from background tasks that only carry an `App`.
    window_handle: AnyWindowHandle,
    /// Kept alive to keep watching; dropping it stops the watcher.
    _watcher: notify::RecommendedWatcher,
}

impl EditSession {
    fn remote_file_name(&self) -> String {
        self.remote_path
            .file_name()
            .map(str::to_string)
            .unwrap_or_else(|| "file".to_string())
    }

    /// Remove the temp copy and its per-session directory (best-effort), then
    /// prune the now-empty per-process directory so `<pid>` folders do not
    /// linger after their last session ends.
    fn cleanup_temp(&self) {
        if let Some(dir) = self.temp_path.parent() {
            oneterm_core::report_best_effort(
                "SftpPanel: remove edit temp dir",
                std::fs::remove_dir_all(dir),
            );
        }
        prune_empty_process_root();
    }
}

/// Map the persisted editor config to the launcher-agnostic [`EditorChoice`].
fn editor_choice(cfg: &EditorConfig) -> EditorChoice {
    match cfg.mode {
        EditorMode::OsDefault => EditorChoice::OsDefault,
        EditorMode::Custom => EditorChoice::Custom {
            program: cfg.program.clone(),
            args: cfg.args.clone(),
        },
    }
}

/// Replace characters that are not portable in a file name so the temp copy is
/// safe to create on the host, while keeping the extension intact.
fn sanitize_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect();
    if cleaned.trim().is_empty() {
        "file".to_string()
    } else {
        cleaned
    }
}

/// Root directory holding every edit session's temp copy, across all
/// processes.
pub(crate) fn edit_cache_root() -> PathBuf {
    config_dir().join("edit-cache")
}

/// Per-process subdirectory under [`edit_cache_root`].
///
/// The process id isolates concurrent OneTerm instances: two instances never
/// share a temp directory (their `EditSessionId` counters both start at 1, so
/// without this they would collide at `edit-cache/1/`), and one instance's
/// startup sweep never deletes another running instance's live temp files.
pub(crate) fn process_cache_root() -> PathBuf {
    edit_cache_root().join(std::process::id().to_string())
}

/// Whether uploading would clobber a remote change made since the file was
/// opened. `baseline` is the mtime recorded at download; `current` is a fresh
/// stat. Anything but two equal, known timestamps is treated as "cannot prove
/// unchanged" and warns, upholding the never-silently-clobber invariant.
fn is_conflict(baseline: Option<SystemTime>, current: Option<SystemTime>) -> bool {
    match (baseline, current) {
        (Some(b), Some(c)) => b != c,
        _ => true,
    }
}

/// Parameters needed to register an edit session once its temp copy has been
/// downloaded. Bundled to keep the register function's signature small.
struct EditSessionInit {
    id: EditSessionId,
    backend_key: BackendKey,
    sftp: std::sync::Arc<dyn SftpBackend>,
    remote_path: RemotePath,
    temp_path: PathBuf,
    baseline_mtime: Option<SystemTime>,
}

/// Whether a filesystem watcher event on the edit-cache directory should be
/// treated as a save of the temp file named `target_name`.
///
/// Matches by file *name* (not full path): `notify` reports absolute,
/// canonicalised paths while the temp path may be relative, and editors often
/// save by writing a sibling and renaming it over the original. Pure metadata /
/// access reads are ignored so opening the file does not trigger an upload.
///
/// This is only a cheap first filter; whether the bytes actually changed is
/// confirmed later against [`temp_signature`], because on Windows
/// `ReadDirectoryChangesW` reports a generic `Modify` for attribute/metadata
/// touches an editor makes when it merely opens the file.
fn event_is_save(event: &notify::Event, target_name: Option<&std::ffi::OsStr>) -> bool {
    if event.kind.is_access() {
        return false;
    }
    event.paths.iter().any(|p| p.file_name() == target_name)
}

/// A cheap fingerprint of the temp copy's on-disk content: `(mtime, len)`.
///
/// Used to tell a genuine save apart from the metadata/attribute/read events an
/// editor emits when it only *opens* the file (those leave the write-time and
/// length unchanged). `None` when the file cannot be stat'd (treated as
/// "changed" by the caller, on the safe side).
fn temp_signature(path: &Path) -> Option<(SystemTime, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

impl SftpPanel {
    /// Edit the selected remote file locally (context-menu "Edit").
    ///
    /// Files only. Applies the configured size gate, downloads the file to a
    /// managed temp copy, launches the editor, and registers a watcher that
    /// uploads saves back.
    pub(crate) fn do_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let entry = match self.selected_entry(cx) {
            Some(entry) if !entry.is_dir => entry,
            Some(_) => {
                log::debug!("SftpPanel::do_edit: selection is a directory — ignored");
                return;
            }
            None => {
                window.push_notification(
                    notify(NotificationType::Warning, "Select a file to edit.", cx),
                    cx,
                );
                return;
            }
        };

        let sftp = match self.sftp() {
            Some(sftp) => sftp.clone(),
            None => {
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
        let Some(backend_key) = self.active_key() else {
            return;
        };

        let cfg = TerminalSettings::global(cx).read(cx).sftp.clone();
        let choice = editor_choice(&cfg.editor);
        let limit = cfg.edit_max_file_size;

        // Size gate: files larger than the limit prompt before downloading.
        if limit != 0 && entry.size > limit {
            let panel = cx.entity();
            let human = format!(
                "\"{}\" is {} — larger than the {} MB edit limit. Open for editing anyway?",
                entry.name,
                super::types::format_size(entry.size),
                limit / (1024 * 1024),
            );
            let entry_for_confirm = entry.clone();
            window.open_alert_dialog(cx, move |alert, _window, _cx| {
                let panel = panel.clone();
                let sftp = sftp.clone();
                let choice = choice.clone();
                let entry = entry_for_confirm.clone();
                alert
                    .confirm()
                    .title("Large file")
                    .description(human.clone())
                    .on_ok(move |_, window, cx| {
                        panel.update(cx, |this, cx| {
                            this.start_edit(
                                entry.clone(),
                                sftp.clone(),
                                backend_key,
                                choice.clone(),
                                window,
                                cx,
                            );
                        });
                        true
                    })
            });
            return;
        }

        self.start_edit(entry, sftp, backend_key, choice, window, cx);
    }

    /// Download `entry` to a temp copy, launch the editor, and register a
    /// watcher. Assumes the size gate has already been cleared.
    pub(crate) fn start_edit(
        &mut self,
        entry: oneterm_core::FileEntry,
        sftp: std::sync::Arc<dyn SftpBackend>,
        backend_key: BackendKey,
        choice: EditorChoice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Reuse an existing session for the same backend + path: re-launch the
        // editor instead of downloading a second copy.
        if let Some((_, session)) = self
            .edit_sessions()
            .iter()
            .find(|(_, s)| s.backend_key == backend_key && s.remote_path == entry.path)
        {
            let temp = session.temp_path.clone();
            if let Err(error) = launch_editor(&choice, &temp) {
                window.push_notification(
                    notify(
                        NotificationType::Error,
                        format!("Could not open the editor: {error}"),
                        cx,
                    ),
                    cx,
                );
            }
            return;
        }

        let id = EditSessionId::next();
        let file_name = sanitize_file_name(&entry.name);
        let temp_dir = process_cache_root().join(id.to_string());
        let temp_path = temp_dir.join(&file_name);
        let remote_path = entry.path.clone();
        let baseline_mtime = entry.modified;
        let entry_name = entry.name.clone();
        let panel = cx.entity();

        log::info!(
            "SftpPanel::do_edit: \"{remote_path}\" → temp \"{}\"",
            temp_path.display()
        );

        cx.spawn_in(window, async move |_panel, cx| {
            // Create the per-session temp directory off the UI thread.
            let make_dir = {
                let temp_dir = temp_dir.clone();
                cx.background_executor()
                    .spawn(async move { std::fs::create_dir_all(&temp_dir) })
                    .await
            };
            if let Err(error) = make_dir {
                log::error!("SftpPanel::do_edit: create temp dir failed: {error}");
                notify_on_window(
                    &panel,
                    "Could not create the edit temp directory",
                    &error,
                    cx,
                );
                return;
            }

            // Download the file through the transfer queue (cancellable, shown).
            let Some(transfer_id) =
                begin_transfer(&panel, TransferDirection::Download, &entry_name, cx)
            else {
                let _ = std::fs::remove_dir_all(&temp_dir);
                return;
            };
            let handle = sftp.download(transfer_id as u64, remote_path.clone(), temp_path.clone());
            let status = run_transfer(&panel, backend_key, transfer_id, handle, cx).await;
            if status != TransferStatus::Completed {
                log::info!("SftpPanel::do_edit: download did not complete ({status:?})");
                let _ = std::fs::remove_dir_all(&temp_dir);
                return;
            }

            // Launch the editor + register the watcher on the UI thread.
            _ = cx.update(|window, cx| {
                panel.update(cx, |this, cx| {
                    this.register_edit_session(
                        EditSessionInit {
                            id,
                            backend_key,
                            sftp: sftp.clone(),
                            remote_path: remote_path.clone(),
                            temp_path: temp_path.clone(),
                            baseline_mtime,
                        },
                        &choice,
                        window,
                        cx,
                    );
                });
            });
        })
        .detach();
    }

    /// Launch the editor and register the watcher for a downloaded temp copy.
    fn register_edit_session(
        &mut self,
        init: EditSessionInit,
        choice: &EditorChoice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let EditSessionInit {
            id,
            backend_key,
            sftp,
            remote_path,
            temp_path,
            baseline_mtime,
        } = init;
        if let Err(error) = launch_editor(choice, &temp_path) {
            log::error!("SftpPanel: launch editor failed: {error}");
            window.push_notification(
                notify(
                    NotificationType::Error,
                    format!("Could not open the editor: {error}"),
                    cx,
                ),
                cx,
            );
            if let Some(dir) = temp_path.parent() {
                let _ = std::fs::remove_dir_all(dir);
            }
            return;
        }

        // Watch the per-session directory (non-recursive) and forward save
        // events for the temp file over a channel to a UI-thread loop.
        //
        // The directory holds exactly this one file, so match by file *name*:
        // `notify` reports absolute, canonicalised paths, while `temp_path` may
        // be relative (`config_dir()` is `target/` in debug builds) — an
        // exact-path comparison would never match. Matching the name also
        // survives editors that save by writing a sibling temp file and
        // renaming it over the original.
        let (tx, rx) = async_channel::unbounded::<()>();
        let target_name = temp_path.file_name().map(|n| n.to_os_string());
        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let Ok(event) = res else {
                return;
            };
            if event_is_save(&event, target_name.as_deref()) {
                log::debug!("SftpPanel: edit watcher event {:?}", event.kind);
                let _ = tx.try_send(());
            }
        });
        let mut watcher = match watcher {
            Ok(w) => w,
            Err(error) => {
                log::error!("SftpPanel: create watcher failed: {error}");
                window.push_notification(
                    notify(
                        NotificationType::Error,
                        format!("Could not watch the file for changes: {error}"),
                        cx,
                    ),
                    cx,
                );
                if let Some(dir) = temp_path.parent() {
                    let _ = std::fs::remove_dir_all(dir);
                }
                return;
            }
        };
        // Watch the parent directory. Prefer the canonical absolute path so
        // the watcher does not depend on the process CWD (the debug
        // `config_dir()` is the relative `target/`).
        let watch_dir = temp_path.parent().map(PathBuf::from).unwrap_or_default();
        let watch_dir = std::fs::canonicalize(&watch_dir).unwrap_or(watch_dir);
        if let Err(error) = watcher.watch(&watch_dir, RecursiveMode::NonRecursive) {
            log::error!("SftpPanel: watch failed: {error}");
            window.push_notification(
                notify(
                    NotificationType::Error,
                    format!("Could not watch the file for changes: {error}"),
                    cx,
                ),
                cx,
            );
            let _ = std::fs::remove_dir_all(&watch_dir);
            return;
        }
        log::info!("SftpPanel: watching \"{}\" for edits", watch_dir.display());

        self.edit_sessions_mut().insert(
            id,
            EditSession {
                backend_key,
                sftp,
                remote_path,
                temp_path: temp_path.clone(),
                baseline_mtime,
                always_upload: false,
                // Fingerprint the freshly downloaded copy so the first watcher
                // event (the editor opening the file) is recognised as "no
                // real change" and does not pop the upload dialog.
                last_temp_sig: temp_signature(&temp_path),
                uploading: false,
                pending_save: false,
                prompt_open: false,
                window_handle: window.window_handle(),
                _watcher: watcher,
            },
        );

        // Drive the debounced save loop.
        let panel = cx.entity();
        cx.spawn_in(window, async move |_panel, cx| {
            loop {
                // Wait for the first save event of a burst.
                if rx.recv().await.is_err() {
                    break;
                }
                // Coalesce the rest of the burst.
                cx.background_executor().timer(SAVE_DEBOUNCE).await;
                while rx.try_recv().is_ok() {}

                let keep = cx
                    .update(|window, cx| {
                        panel.update(cx, |this, cx| this.on_temp_saved(id, window, cx))
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
            log::debug!("SftpPanel: edit watcher loop for session {id} ended");
        })
        .detach();
    }

    /// Handle a debounced save of the temp copy. Returns `true` while the
    /// session is still active (keep watching).
    fn on_temp_saved(
        &mut self,
        id: EditSessionId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session) = self.edit_sessions().get(&id) else {
            return false;
        };

        // Confirm the bytes actually changed. Opening a file in an editor emits
        // watcher events (metadata/attribute touches on Windows, sibling temp
        // writes, etc.) that `event_is_save` cannot rule out; without this the
        // upload dialog would pop the instant the editor opens the file, before
        // any edit. A fresh fingerprint equal to the last one we recorded means
        // no real save.
        let current_sig = temp_signature(&session.temp_path);
        if current_sig.is_some() && current_sig == session.last_temp_sig {
            log::debug!("SftpPanel: temp file unchanged since last seen — ignoring event");
            return true;
        }
        // A real change: record the new fingerprint before deciding what to do.
        if let Some(s) = self.edit_sessions_mut().get_mut(&id) {
            s.last_temp_sig = current_sig;
        }
        let Some(session) = self.edit_sessions().get(&id) else {
            return false;
        };

        // While an upload or a prompt is busy, remember the save and handle it
        // once that finishes, so there is at most one upload per file at a time.
        if session.uploading || session.prompt_open {
            if let Some(s) = self.edit_sessions_mut().get_mut(&id) {
                s.pending_save = true;
            }
            return true;
        }

        if session.always_upload {
            self.begin_conflict_check_and_upload(id, window, cx);
            return true;
        }

        // Ask whether to upload, with a per-session "always" checkbox.
        let file_name = session.remote_file_name();
        if let Some(s) = self.edit_sessions_mut().get_mut(&id) {
            s.prompt_open = true;
        }
        let panel = cx.entity();
        let always_cell = Rc::new(Cell::new(false));

        use oneterm_state::form_dialog::FormDialog;
        let message = format!("\"{file_name}\" was saved. Upload it to the remote host?");
        let cell_for_content = always_cell.clone();
        let panel_for_cancel = panel.clone();
        let panel_for_submit = panel.clone();
        FormDialog::new(
            "Upload change",
            move |content, _window, _cx| {
                let cell = cell_for_content.clone();
                content.child(div().child(message.clone())).child(
                    Checkbox::new("sftp-edit-always-upload")
                        .label("Always upload this file while editing")
                        .checked(cell.get())
                        .on_click(move |checked: &bool, _window, _cx| {
                            cell.set(*checked);
                        }),
                )
            },
            move |_window, cx| {
                let always = always_cell.get();
                panel_for_submit.update(cx, |this, cx| {
                    if let Some(s) = this.edit_sessions_mut().get_mut(&id) {
                        s.prompt_open = false;
                        s.always_upload = always;
                    }
                    // The conflict check + upload run in their own tasks; the
                    // dialog closes immediately.
                    this.begin_conflict_check_and_upload_ctx(id, cx);
                });
                true
            },
        )
        .confirm_label("Upload")
        .on_cancel(move |_window, cx| {
            panel_for_cancel.update(cx, |this, _cx| {
                if let Some(s) = this.edit_sessions_mut().get_mut(&id) {
                    s.prompt_open = false;
                }
            });
        })
        .open(window, cx);

        true
    }

    /// Entry point used from a `Context` (no `&mut Window`): spawns the conflict
    /// check + upload.
    fn begin_conflict_check_and_upload_ctx(&mut self, id: EditSessionId, cx: &mut Context<Self>) {
        // Read what we need, set the in-flight flag, and release the borrow
        // before spawning.
        let (sftp, remote, baseline) = {
            let Some(s) = self.edit_sessions_mut().get_mut(&id) else {
                return;
            };
            if s.uploading {
                s.pending_save = true;
                return;
            }
            s.uploading = true;
            (s.sftp.clone(), s.remote_path.clone(), s.baseline_mtime)
        };
        let panel = cx.entity();

        cx.spawn(async move |_panel, cx| {
            let current = sftp
                .stat(remote.clone())
                .await
                .ok()
                .and_then(|e| e.modified);
            let conflict = is_conflict(baseline, current);
            _ = cx.update(|cx| {
                panel.update(cx, |this, cx| {
                    if conflict {
                        this.warn_conflict_then_upload(id, cx);
                    } else {
                        this.upload_edit_now(id, cx);
                    }
                });
            });
        })
        .detach();
    }

    /// Same as [`Self::begin_conflict_check_and_upload_ctx`] but reachable with a
    /// `&mut Window` in scope (from the "always upload" fast path).
    fn begin_conflict_check_and_upload(
        &mut self,
        id: EditSessionId,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.begin_conflict_check_and_upload_ctx(id, cx);
    }

    /// The remote file changed since it was opened: warn before overwriting.
    fn warn_conflict_then_upload(&mut self, id: EditSessionId, cx: &mut Context<Self>) {
        let Some(session) = self.edit_sessions().get(&id) else {
            return;
        };
        let file_name = session.remote_file_name();
        let window_handle = session.window_handle;
        let panel = cx.entity();
        let description = format!(
            "The remote file \"{file_name}\" changed since you started editing. \
             Uploading will overwrite those changes."
        );
        let opened = window_handle.update(cx, |_root, window, cx| {
            let panel_ok = panel.clone();
            let panel_cancel = panel.clone();
            window.open_alert_dialog(cx, move |alert, _window, _cx| {
                let panel_ok = panel_ok.clone();
                let panel_cancel = panel_cancel.clone();
                alert
                    .confirm()
                    .title("Remote file changed")
                    .description(description.clone())
                    .on_ok(move |_, _window, cx| {
                        panel_ok.update(cx, |this, cx| this.upload_edit_now(id, cx));
                        true
                    })
                    .on_cancel(move |_, _window, cx| {
                        panel_cancel.update(cx, |this, cx| this.finish_upload(id, false, None, cx));
                        true
                    })
            });
        });
        if opened.is_err() {
            // The window is gone; do the safe thing and cancel the upload.
            log::warn!("SftpPanel: conflict dialog window unavailable; cancelling upload");
            self.finish_upload(id, false, None, cx);
        }
    }

    /// Upload the temp copy now (conflict already resolved).
    fn upload_edit_now(&mut self, id: EditSessionId, cx: &mut Context<Self>) {
        let Some(session) = self.edit_sessions().get(&id) else {
            return;
        };
        let sftp = session.sftp.clone();
        let key = session.backend_key;
        let remote = session.remote_path.clone();
        let temp = session.temp_path.clone();
        let file_name = session.remote_file_name();
        let panel = cx.entity();

        cx.spawn(async move |_panel, cx| {
            let Some(transfer_id) =
                begin_transfer(&panel, TransferDirection::Upload, &file_name, cx)
            else {
                _ = cx.update(|cx| {
                    panel.update(cx, |this, cx| this.finish_upload(id, false, None, cx));
                });
                return;
            };
            let handle = sftp.upload(transfer_id as u64, temp, remote.clone());
            let status = run_transfer(&panel, key, transfer_id, handle, cx).await;
            let ok = status == TransferStatus::Completed;
            // Refresh the baseline from a fresh stat so the next save compares
            // against the state we just wrote.
            let new_mtime = if ok {
                sftp.stat(remote).await.ok().and_then(|e| e.modified)
            } else {
                None
            };
            _ = cx.update(|cx| {
                panel.update(cx, |this, cx| this.finish_upload(id, ok, new_mtime, cx));
            });
        })
        .detach();
    }

    /// Settle an upload attempt: clear the in-flight flag, refresh the baseline
    /// on success, refresh the listing if the file is in the current cwd, and
    /// re-run a save that arrived while the upload was busy.
    fn finish_upload(
        &mut self,
        id: EditSessionId,
        ok: bool,
        new_mtime: Option<SystemTime>,
        cx: &mut Context<Self>,
    ) {
        let (pending, remote_parent, uploaded_ok) = match self.edit_sessions_mut().get_mut(&id) {
            Some(s) => {
                s.uploading = false;
                if ok {
                    s.baseline_mtime = new_mtime;
                }
                (
                    std::mem::take(&mut s.pending_save),
                    s.remote_path.parent(),
                    ok,
                )
            }
            None => return,
        };

        // Refresh the listing if the edited file lives in the current directory.
        if uploaded_ok {
            if let Some(parent) = remote_parent {
                if parent == *self.browser().cwd() {
                    self.refresh(cx);
                }
            }
        }

        if pending {
            // A save arrived mid-upload; handle it now (skips the prompt when
            // the session is set to always upload).
            self.begin_conflict_check_and_upload_ctx(id, cx);
        }
    }

    /// End one edit session: stop watching and remove the temp copy.
    pub(crate) fn end_edit_session(&mut self, id: EditSessionId) {
        if let Some(session) = self.edit_sessions_mut().remove(&id) {
            session.cleanup_temp();
        }
    }

    /// End every edit session whose SFTP backend has closed (the SSH/SFTP
    /// session ended). Called from the poll tick and on every active-tab change,
    /// so a closed connection's temp copies are cleaned up while the app keeps
    /// running — without ending a session just because the user switched tabs.
    pub(crate) fn reap_dead_edit_sessions(&mut self) {
        let dead: Vec<EditSessionId> = self
            .edit_sessions()
            .iter()
            .filter(|(_, session)| !session.sftp.alive())
            .map(|(id, _)| *id)
            .collect();
        for id in dead {
            log::info!("SftpPanel: SFTP session closed — ending edit session {id}");
            self.end_edit_session(id);
        }
    }
}

/// Whether the `edit-cache/<pid>/` directory owned by `pid` should be
/// reclaimed during a startup sweep, given the current process id and whether
/// `pid` is a live process.
///
/// Keep only directories owned by *another* still-running OneTerm instance; a
/// same-pid leftover (crashed prior run) or a dead pid's directory is reclaimed.
fn should_reclaim_dir(pid: u32, current: u32, pid_is_live: bool) -> bool {
    !(pid != current && pid_is_live)
}

/// Best-effort removal of stale edit-cache directories left by OneTerm
/// processes that are no longer running. Called from the feature's `init`.
///
/// Each `edit-cache/<pid>/` directory is owned by the OneTerm process with that
/// pid. A directory is removed only when its pid is **not** a live process:
/// this reclaims folders left behind by a previous run (normal exit *or* crash,
/// where `Drop`/session cleanup never ran) while never touching the live temp
/// files of another OneTerm instance still running concurrently.
///
/// The current process's own folder does not exist yet at startup, so removing
/// a same-pid folder is safe: it can only be a leftover from a crashed prior
/// run that happened to reuse this pid, and this process has not created any
/// temp files yet.
pub(crate) fn sweep_edit_cache() {
    let root = edit_cache_root();
    let entries = match std::fs::read_dir(&root) {
        Ok(entries) => entries,
        // No cache dir yet (or unreadable) — nothing to sweep.
        Err(_) => return,
    };

    // One process snapshot for the whole sweep.
    let system = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::nothing()),
    );
    let current = std::process::id();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        // The directory name is the owning process id.
        let Some(pid) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        // Keep folders owned by another live OneTerm instance.
        if !should_reclaim_dir(pid, current, system.process(Pid::from_u32(pid)).is_some()) {
            continue;
        }
        oneterm_core::report_best_effort(
            "SftpPanel: sweep stale edit-cache",
            std::fs::remove_dir_all(&path),
        );
    }
}

/// Remove this process's `edit-cache/<pid>/` directory once it is empty, so the
/// per-process folder does not linger after its last edit session ends.
/// Best-effort: a non-empty directory (another live session) simply stays.
fn prune_empty_process_root() {
    let root = process_cache_root();
    // `remove_dir` only succeeds on an empty directory — exactly the guard we
    // want, so a still-active session's directory is never removed.
    let _ = std::fs::remove_dir(&root);
}

impl Drop for SftpPanel {
    /// Clean up every edit session's temp copy when the panel is disposed. The
    /// watchers drop with the sessions; the temp directories are removed here.
    fn drop(&mut self) {
        let sessions = std::mem::take(self.edit_sessions_mut());
        for (_, session) in sessions {
            session.cleanup_temp();
        }
    }
}
/// Notify on the active window from an async task (best-effort).
fn notify_on_window(
    _panel: &Entity<SftpPanel>,
    what: &str,
    error: &dyn std::fmt::Display,
    cx: &mut AsyncApp,
) {
    let message = format!("{what}: {error}");
    _ = cx.update(|cx| {
        if let Some(handle) = cx.windows().first().copied() {
            let _ = handle.update(cx, |_root, window, cx| {
                window.push_notification(notify(NotificationType::Error, message.clone(), cx), cx);
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use oneterm_settings::{EditorConfig, EditorMode};

    use super::{
        editor_choice, event_is_save, is_conflict, process_cache_root, sanitize_file_name,
        should_reclaim_dir, temp_signature,
    };
    use oneterm_core::EditorChoice;

    #[test]
    fn os_default_editor_maps_to_os_default_choice() {
        let cfg = EditorConfig {
            mode: EditorMode::OsDefault,
            program: "ignored".into(),
            args: vec!["ignored".into()],
        };
        assert_eq!(editor_choice(&cfg), EditorChoice::OsDefault);
    }

    #[test]
    fn custom_editor_maps_program_and_args() {
        let cfg = EditorConfig {
            mode: EditorMode::Custom,
            program: "code".into(),
            args: vec!["-n".into(), "--wait".into()],
        };
        assert_eq!(
            editor_choice(&cfg),
            EditorChoice::Custom {
                program: "code".into(),
                args: vec!["-n".into(), "--wait".into()],
            }
        );
    }

    #[test]
    fn equal_known_mtimes_are_not_a_conflict() {
        let t = UNIX_EPOCH + Duration::from_secs(1000);
        assert!(!is_conflict(Some(t), Some(t)));
    }

    #[test]
    fn changed_mtime_is_a_conflict() {
        let a = UNIX_EPOCH + Duration::from_secs(1000);
        let b = UNIX_EPOCH + Duration::from_secs(2000);
        assert!(is_conflict(Some(a), Some(b)));
    }

    #[test]
    fn unknown_mtime_is_treated_as_a_conflict() {
        let t = UNIX_EPOCH + Duration::from_secs(1000);
        // Cannot prove the file is unchanged → warn (safe side).
        assert!(is_conflict(None, Some(t)));
        assert!(is_conflict(Some(t), None));
        assert!(is_conflict(None, None));
    }

    #[test]
    fn sanitize_replaces_path_separators_and_reserved_chars() {
        assert_eq!(
            sanitize_file_name("a/b\\c:d*e?f\"g<h>i|j"),
            "a_b_c_d_e_f_g_h_i_j"
        );
        // A normal name with an extension is preserved.
        assert_eq!(sanitize_file_name("deploy.sh"), "deploy.sh");
        // A blank/whitespace name falls back to a safe default.
        assert_eq!(sanitize_file_name("   "), "file");
    }

    #[test]
    fn temp_signature_changes_on_content_edit_but_not_on_a_read() {
        let dir = std::env::temp_dir().join(format!(
            "oneterm-edit-sig-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("f.txt");
        std::fs::write(&file, b"hello").unwrap();

        let sig = temp_signature(&file);
        assert!(sig.is_some());
        // Merely reading the file leaves the fingerprint unchanged — opening in
        // an editor must not look like a save.
        let _ = std::fs::read(&file).unwrap();
        assert_eq!(temp_signature(&file), sig);
        // Writing different content changes the fingerprint (length differs even
        // if the mtime tick is coarse).
        std::fs::write(&file, b"hello world").unwrap();
        assert_ne!(temp_signature(&file), sig);
        // A missing file has no fingerprint.
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(temp_signature(&file), None);
    }

    #[test]
    fn sweep_reclaims_dead_and_same_pid_dirs_but_keeps_other_live_instances() {
        let me = std::process::id();
        let other = if me == 1 { 2 } else { 1 };
        // A dead pid's directory is reclaimed.
        assert!(should_reclaim_dir(other, me, false));
        // Another live OneTerm instance's directory is kept.
        assert!(!should_reclaim_dir(other, me, true));
        // A leftover directory reusing our own pid is reclaimed even though the
        // pid is "live" (it is us, and we have not created temps yet at startup).
        assert!(should_reclaim_dir(me, me, true));
    }

    #[test]
    fn process_cache_root_is_scoped_to_this_process() {
        let root = process_cache_root();
        // The per-process subdirectory carries this process id, so two OneTerm
        // instances never share a temp directory and one instance's sweep never
        // touches another's live files.
        assert!(root.starts_with(super::edit_cache_root()));
        assert_eq!(
            root.file_name().and_then(|n| n.to_str()),
            Some(std::process::id().to_string().as_str())
        );
    }

    #[test]
    fn watcher_matches_the_temp_file_by_name_regardless_of_event_path_shape() {
        use notify::event::{CreateKind, EventKind, ModifyKind};
        use std::path::PathBuf;

        let target = std::ffi::OsString::from("deploy.sh");

        // A modify event whose absolute path ends in the temp file name matches
        // even though the registered temp path was relative (`target/...`).
        let modify = notify::Event {
            kind: EventKind::Modify(ModifyKind::Any),
            paths: vec![PathBuf::from("C:/abs/edit-cache/7/deploy.sh")],
            attrs: Default::default(),
        };
        assert!(event_is_save(&modify, Some(target.as_os_str())));

        // An atomic-rename create of the same name also matches.
        let create = notify::Event {
            kind: EventKind::Create(CreateKind::Any),
            paths: vec![PathBuf::from("/tmp/edit-cache/7/deploy.sh")],
            attrs: Default::default(),
        };
        assert!(event_is_save(&create, Some(target.as_os_str())));

        // A different file in the directory is ignored.
        let other = notify::Event {
            kind: EventKind::Modify(ModifyKind::Any),
            paths: vec![PathBuf::from("/tmp/edit-cache/7/other.txt")],
            attrs: Default::default(),
        };
        assert!(!event_is_save(&other, Some(target.as_os_str())));

        // Pure access/metadata reads (e.g. the editor opening the file) do not
        // count as a save.
        let access = notify::Event {
            kind: EventKind::Access(notify::event::AccessKind::Read),
            paths: vec![PathBuf::from("/tmp/edit-cache/7/deploy.sh")],
            attrs: Default::default(),
        };
        assert!(!event_is_save(&access, Some(target.as_os_str())));
    }
}
