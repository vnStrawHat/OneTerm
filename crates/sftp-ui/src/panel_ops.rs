//! File operations for [`SftpPanel`] — directory listing, navigation, refresh,
//! column toggling, and the auto-follow-terminal-cwd logic.
//!
//! Split out from [`super::panel`] to keep each file under the ~400-line guideline.

use std::path::PathBuf;

use gpui::{App, Context};

use oneterm_core::FileEntry;

use super::panel::SftpPanel;
use super::types::SortColumn;

impl SftpPanel {
    /// Read a directory — spawn a background task, does not block the UI.
    pub(crate) fn load_dir(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        log::debug!("SftpPanel::load_dir: path=\"{}\"", path.display());

        let sftp = match &self.sftp {
            Some(s) => s.clone(),
            None => {
                log::warn!("SftpPanel::load_dir: no SFTP connection — ignoring");
                self.table.update(cx, |t, cx| {
                    t.delegate_mut().loading = false;
                    cx.notify();
                });
                return;
            }
        };

        self.table.update(cx, |t, cx| {
            t.delegate_mut().loading = true;
            cx.notify();
        });
        self.error = None;
        self.cwd = path.clone();
        self.selected = None;
        self.mark_state_dirty();
        self.table.update(cx, |t, cx| t.clear_selection(cx));
        cx.notify();

        cx.spawn(async move |this, cx| {
            log::debug!(
                "SftpPanel::load_dir: spawning background read_dir for \"{}\"",
                path.display()
            );

            let result = sftp.read_dir(path).await;

            this.update(cx, |this, cx| {
                this.table.update(cx, |t, cx| {
                    t.delegate_mut().loading = false;
                    cx.notify();
                });
                match result {
                    Ok(entries) => {
                        log::info!(
                            "SftpPanel::load_dir: got {} entries for \"{}\"",
                            entries.len(),
                            this.cwd.display()
                        );

                        // Update cwd with the absolute path from the first entry.
                        let mut cwd = this.cwd.clone();
                        if let Some(first) = entries.first() {
                            if let Some(parent) = first.path.parent() {
                                cwd = parent.to_path_buf();
                            }
                        }
                        this.cwd = cwd;

                        this.table.update(cx, |t, cx| {
                            t.delegate_mut().set_entries(entries);
                            t.refresh(cx);
                        });
                        this.error = None;
                        this.mark_entries_dirty();
                    }
                    Err(e) => {
                        log::error!("SftpPanel::load_dir: read_dir failed: {e}");
                        this.error = Some(e.to_string());
                        this.table.update(cx, |t, cx| {
                            t.delegate_mut().entries.clear();
                            t.refresh(cx);
                        });
                    }
                }
                this.mark_state_dirty();
                cx.notify();
            })
        })
        .detach();
    }

    /// Navigate up to the parent directory.
    pub(crate) fn navigate_parent(&mut self, cx: &mut Context<Self>) {
        let parent = match self.cwd.parent() {
            Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
            _ => {
                log::debug!("SftpPanel::navigate_parent: already at root");
                return;
            }
        };
        log::debug!(
            "SftpPanel::navigate_parent: \"{}\" → \"{}\"",
            self.cwd.display(),
            parent.display()
        );
        self.load_dir(parent, cx);
    }

    /// Refresh the current directory.
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        log::debug!("SftpPanel::refresh: refreshing \"{}\"", self.cwd.display());
        self.load_dir(self.cwd.clone(), cx);
    }

    /// The current working directory of the active terminal (OSC 7), read live.
    /// Used to compute the "sync" button's enabled state + tooltip.
    pub(crate) fn terminal_cwd(&self) -> Option<PathBuf> {
        self.cwd_source.as_ref().and_then(|s| s.cwd())
    }

    /// Refresh the cached terminal cwd and notify when the sync button state or
    /// tooltip should be re-rendered. The cache is not used for navigation;
    /// sync actions read `cwd_source` live to avoid stale jumps.
    pub(crate) fn refresh_terminal_cwd_cache(&mut self, cx: &mut Context<Self>) {
        let current = self.terminal_cwd();
        if self.terminal_cwd_cache != current {
            self.terminal_cwd_cache = current;
            cx.notify();
        }
    }

    /// Navigate the SFTP browser to the active terminal's current directory.
    /// No-op if there is no SFTP connection or the terminal has not reported a cwd.
    pub(crate) fn sync_to_terminal_cwd(&mut self, cx: &mut Context<Self>) {
        if self.sftp.is_none() {
            return;
        }
        let cwd = match self.terminal_cwd() {
            Some(p) => p,
            None => {
                log::debug!("SftpPanel::sync_to_terminal_cwd: terminal cwd unavailable");
                return;
            }
        };
        log::info!(
            "SftpPanel::sync_to_terminal_cwd: \"{}\" → \"{}\"",
            self.cwd.display(),
            cwd.display()
        );
        // `goto_path` stats the path (dir check) + handles errors + load_dir.
        self.goto_path(cwd, cx);
    }

    /// Toggle the auto-follow-terminal-cwd flag (from the "..." menu checkbox).
    pub(crate) fn toggle_follow_terminal_cwd(&mut self, cx: &mut Context<Self>) {
        self.follow_terminal_cwd = !self.follow_terminal_cwd;
        self.mark_state_dirty();
        log::info!(
            "SftpPanel: auto-follow terminal cwd {}",
            if self.follow_terminal_cwd {
                "enabled"
            } else {
                "disabled"
            }
        );
        // When enabling, immediately attempt a follow so the browser jumps to
        // the terminal's cwd right away (instead of waiting up to 500ms).
        if self.follow_terminal_cwd {
            self.maybe_follow_terminal_cwd(cx);
        }
        cx.notify();
    }

    /// Polling hook — called by the auto-follow timer. If follow is enabled and
    /// the terminal's cwd has changed (and differs from the browser's cwd),
    /// navigate the SFTP browser to the new cwd.
    pub(crate) fn maybe_follow_terminal_cwd(&mut self, cx: &mut Context<Self>) {
        if !self.follow_terminal_cwd || self.sftp.is_none() {
            return;
        }
        let cwd = match self.terminal_cwd() {
            Some(p) => p,
            None => return,
        };
        // Skip if we already followed this exact cwd (no change since last poll).
        if self.last_followed_cwd.as_ref() == Some(&cwd) {
            return;
        }
        // Skip if the browser is already showing this directory.
        if self.cwd == cwd {
            self.last_followed_cwd = Some(cwd);
            self.mark_state_dirty();
            return;
        }
        log::debug!(
            "SftpPanel::maybe_follow_terminal_cwd: auto-follow \"{}\" → \"{}\"",
            self.cwd.display(),
            cwd.display()
        );
        self.last_followed_cwd = Some(cwd.clone());
        self.mark_state_dirty();
        self.goto_path(cwd, cx);
    }

    /// Navigate into a subdirectory (double-click a folder).
    pub(crate) fn navigate_into(&mut self, idx: usize, cx: &mut Context<Self>) {
        let entry = self.table.read(cx).delegate().entries.get(idx).cloned();
        match entry {
            Some(entry) if entry.is_dir => {
                log::debug!(
                    "SftpPanel::navigate_into: \"{}\" → \"{}\"",
                    self.cwd.display(),
                    entry.path.display()
                );
                self.load_dir(entry.path.clone(), cx);
            }
            Some(_) => {
                log::debug!("SftpPanel::navigate_into: entry {idx} is not a directory");
            }
            None => {
                log::warn!("SftpPanel::navigate_into: index {idx} out of range");
            }
        }
    }

    /// Toggle the visibility of a column (from the Columns dropdown). Name cannot be hidden.
    pub(crate) fn toggle_column(&mut self, col: SortColumn, cx: &mut Context<Self>) {
        let changed = self.table.update(cx, |t, cx| {
            let changed = t.delegate_mut().toggle_visibility(col);
            if changed {
                t.refresh(cx);
            }
            changed
        });
        if changed {
            self.mark_state_dirty();
            self.schedule_save_table_state(cx);
            cx.notify();
        }
    }

    /// Get selected entry (if any) — cloned for use in a dialog.
    pub(crate) fn selected_entry(&self, cx: &App) -> Option<FileEntry> {
        self.selected
            .and_then(|ix| self.table.read(cx).delegate().entries.get(ix).cloned())
    }
}
