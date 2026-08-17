//! File operations for [`SftpPanel`] — directory listing, navigation, refresh,
//! column toggling, and the auto-follow-terminal-cwd logic.
//!
//! Split out from [`super::panel`] to keep each file under the ~400-line guideline.

use gpui::{App, Context};

use oneterm_core::{FileEntry, RemotePath};

use super::panel::SftpPanel;
use super::types::SortColumn;

impl SftpPanel {
    /// Read a directory — spawn a background task, does not block the UI.
    ///
    /// Each call bumps `load_generation`; the spawned task applies its result
    /// only when the generation and the active backend still match, so a slow
    /// earlier listing (fast navigation, auto-follow racing a click, a tab switch
    /// during load) can neither replace a newer listing nor rewrite `cwd`.
    pub(crate) fn load_dir(&mut self, path: RemotePath, cx: &mut Context<Self>) {
        log::debug!("SftpPanel::load_dir: path=\"{path}\"");

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

        self.load_generation = self.load_generation.wrapping_add(1);
        let generation = self.load_generation;
        let key = self.active_key;

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
            log::debug!("SftpPanel::load_dir: spawning background read_dir for \"{path}\"");

            let result = sftp.read_dir(path.clone()).await;

            // The panel may be gone before the listing arrives; nothing to apply then.
            _ = this.update(cx, |this, cx| {
                if this.load_generation != generation || this.active_key != key {
                    log::debug!(
                        "SftpPanel::load_dir: discarding stale listing for \"{path}\" (generation {generation}, current {})",
                        this.load_generation
                    );
                    return;
                }
                this.apply_listing(result, cx);
            })
        })
        .detach();
    }

    /// Apply the outcome of the most recent `read_dir` to the active view.
    fn apply_listing(
        &mut self,
        result: oneterm_core::Result<Vec<FileEntry>>,
        cx: &mut Context<Self>,
    ) {
        self.table.update(cx, |t, cx| {
            t.delegate_mut().loading = false;
            cx.notify();
        });
        match result {
            Ok(entries) => {
                log::info!(
                    "SftpPanel::load_dir: got {} entries for \"{}\"",
                    entries.len(),
                    self.cwd
                );

                // A relative request (e.g. the initial `.`) is resolved by the
                // backend; the entries carry the absolute directory.
                if let Some(parent) = entries.first().and_then(|first| first.path.parent()) {
                    self.cwd = parent;
                }

                self.table.update(cx, |t, cx| {
                    t.delegate_mut().set_entries(entries);
                    t.refresh(cx);
                });
                self.error = None;
                self.mark_entries_dirty();
            }
            Err(e) => {
                log::error!("SftpPanel::load_dir: read_dir failed: {e}");
                self.error = Some(e.to_string());
                self.table.update(cx, |t, cx| {
                    t.delegate_mut().entries.clear();
                    t.refresh(cx);
                });
            }
        }
        self.mark_state_dirty();
        cx.notify();
    }

    /// Navigate up to the parent directory.
    pub(crate) fn navigate_parent(&mut self, cx: &mut Context<Self>) {
        let Some(parent) = self.cwd.parent() else {
            log::debug!("SftpPanel::navigate_parent: already at root");
            return;
        };
        log::debug!(
            "SftpPanel::navigate_parent: \"{}\" → \"{parent}\"",
            self.cwd
        );
        self.load_dir(parent, cx);
    }

    /// Refresh the current directory.
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        log::debug!("SftpPanel::refresh: refreshing \"{}\"", self.cwd);
        self.load_dir(self.cwd.clone(), cx);
    }

    /// The current working directory of the active terminal (OSC 7), read live.
    /// Used to compute the "sync" button's enabled state + tooltip.
    ///
    /// The terminal reports its cwd as a host `PathBuf`; for an SSH tab it names
    /// a remote directory, so it is converted to a [`RemotePath`] here.
    pub(crate) fn terminal_cwd(&self) -> Option<RemotePath> {
        self.cwd_source
            .as_ref()
            .and_then(|s| s.cwd())
            .map(|cwd| RemotePath::new(cwd.to_string_lossy()))
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
            "SftpPanel::sync_to_terminal_cwd: \"{}\" → \"{cwd}\"",
            self.cwd
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
            "SftpPanel::maybe_follow_terminal_cwd: auto-follow \"{}\" → \"{cwd}\"",
            self.cwd
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
                    self.cwd,
                    entry.path
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gpui::{AppContext as _, TestAppContext, VisualTestContext};
    use oneterm_core::{RemotePath, SftpBackend};

    use super::SftpPanel;
    use crate::browser_state::SftpBrowserStore;
    use crate::test_backend::{FakeSftpBackend, dir_entry};

    fn test_panel(cx: &mut TestAppContext) -> (gpui::Entity<SftpPanel>, &mut VisualTestContext) {
        cx.update(gpui_component::init);
        cx.update(oneterm_state::AppState::init);

        let (root, cx) = cx.add_window_view(|window, cx| {
            let panel = cx.new(|cx| SftpPanel::new(window, cx));
            gpui_component::Root::new(panel, window, cx)
        });
        let panel = root.read_with(cx, |root, _| {
            root.view().clone().downcast::<SftpPanel>().unwrap()
        });
        (panel, cx)
    }

    fn attach_backend(
        panel: &gpui::Entity<SftpPanel>,
        cx: &mut VisualTestContext,
    ) -> Arc<FakeSftpBackend> {
        let backend = Arc::new(FakeSftpBackend::new());
        let dynamic: Arc<dyn SftpBackend> = backend.clone();
        panel.update(cx, |panel, cx| {
            let key = SftpBrowserStore::global(cx).track_backend(&dynamic);
            panel.sftp = Some(dynamic);
            panel.active_key = Some(key);
        });
        backend
    }

    fn listed_names(panel: &gpui::Entity<SftpPanel>, cx: &mut VisualTestContext) -> Vec<String> {
        panel.read_with(cx, |panel, cx| {
            panel
                .table
                .read(cx)
                .delegate()
                .entries
                .iter()
                .map(|entry| entry.name.clone())
                .collect()
        })
    }

    /// CORR-09: a listing that arrives after a newer request was issued is
    /// discarded — it must neither replace the newer entries nor rewrite `cwd`.
    #[gpui::test]
    fn stale_listing_is_discarded(cx: &mut TestAppContext) {
        let (panel, cx) = test_panel(cx);
        let backend = attach_backend(&panel, cx);
        let first_reply = backend.arm_read_dir();
        let second_reply = backend.arm_read_dir();

        let first_dir = RemotePath::new("/first");
        let second_dir = RemotePath::new("/second");
        panel.update(cx, |panel, cx| panel.load_dir(first_dir.clone(), cx));
        panel.update(cx, |panel, cx| panel.load_dir(second_dir.clone(), cx));
        cx.run_until_parked();
        assert_eq!(
            backend.read_dir_requests(),
            vec![first_dir.clone(), second_dir.clone()]
        );

        // The newer request answers first.
        second_reply
            .try_send(Ok(vec![dir_entry(&second_dir, "b.txt", false)]))
            .unwrap();
        cx.run_until_parked();
        assert_eq!(listed_names(&panel, cx), vec!["b.txt"]);
        assert_eq!(
            panel.read_with(cx, |panel, _| panel.cwd.clone()),
            second_dir
        );

        // The stale answer must be ignored.
        first_reply
            .try_send(Ok(vec![dir_entry(&first_dir, "a.txt", false)]))
            .unwrap();
        cx.run_until_parked();
        assert_eq!(listed_names(&panel, cx), vec!["b.txt"]);
        assert_eq!(
            panel.read_with(cx, |panel, _| panel.cwd.clone()),
            second_dir
        );
        assert!(panel.read_with(cx, |panel, cx| !panel.table.read(cx).delegate().loading));
    }

    /// A listing that belongs to a backend that is no longer active is discarded.
    #[gpui::test]
    fn listing_from_a_switched_away_backend_is_discarded(cx: &mut TestAppContext) {
        let (panel, cx) = test_panel(cx);
        let backend = attach_backend(&panel, cx);
        let reply = backend.arm_read_dir();

        let dir = RemotePath::new("/old");
        panel.update(cx, |panel, cx| panel.load_dir(dir.clone(), cx));
        cx.run_until_parked();

        // Switch to another backend while the listing is in flight.
        let other = attach_backend(&panel, cx);
        panel.update(cx, |panel, _| panel.cwd = RemotePath::new("/other"));

        reply
            .try_send(Ok(vec![dir_entry(&dir, "stale.txt", false)]))
            .unwrap();
        cx.run_until_parked();

        assert!(listed_names(&panel, cx).is_empty());
        assert_eq!(
            panel.read_with(cx, |panel, _| panel.cwd.clone()),
            RemotePath::new("/other")
        );
        assert!(other.read_dir_requests().is_empty());
    }
}
