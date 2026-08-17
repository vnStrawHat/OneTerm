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
    /// Each call bumps the load generation; the spawned task applies its result
    /// only when the generation and the active backend still match, so a slow
    /// earlier listing (fast navigation, auto-follow racing a click, a tab switch
    /// during load) can neither replace a newer listing nor rewrite `cwd`.
    pub(crate) fn load_dir(&mut self, path: RemotePath, cx: &mut Context<Self>) {
        log::debug!("SftpPanel::load_dir: path=\"{path}\"");

        let sftp = match self.sftp() {
            Some(s) => s.clone(),
            None => {
                log::warn!("SftpPanel::load_dir: no SFTP connection — ignoring");
                self.table().update(cx, |t, cx| {
                    t.delegate_mut().loading = false;
                    cx.notify();
                });
                return;
            }
        };

        let generation = self.next_load_generation();
        let key = self.active_key();

        self.table().update(cx, |t, cx| {
            t.delegate_mut().loading = true;
            t.clear_selection(cx);
            cx.notify();
        });
        self.browser_mut().begin_load(path.clone());
        cx.notify();

        cx.spawn(async move |this, cx| {
            log::debug!("SftpPanel::load_dir: spawning background read_dir for \"{path}\"");

            let result = sftp.read_dir(path.clone()).await;

            // The panel may be gone before the listing arrives; nothing to apply then.
            _ = this.update(cx, |this, cx| {
                if !this.is_current_load(generation, key) {
                    log::debug!(
                        "SftpPanel::load_dir: discarding stale listing for \"{path}\" (generation {generation})"
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
        match result {
            Ok(entries) => {
                log::info!(
                    "SftpPanel::load_dir: got {} entries for \"{}\"",
                    entries.len(),
                    self.browser().cwd()
                );

                // A relative request (e.g. the initial `.`) is resolved by the
                // backend; the entries carry the absolute directory.
                if let Some(parent) = entries.first().and_then(|first| first.path.parent()) {
                    self.browser_mut().set_cwd(parent);
                }

                self.table().update(cx, |t, cx| {
                    t.delegate_mut().loading = false;
                    t.delegate_mut().set_entries(entries);
                    t.refresh(cx);
                });
                self.browser_mut().set_error(None);
                self.mark_entries_dirty();
            }
            Err(e) => {
                log::error!("SftpPanel::load_dir: read_dir failed: {e}");
                self.browser_mut().set_error(Some(e.to_string()));
                self.table().update(cx, |t, cx| {
                    t.delegate_mut().loading = false;
                    t.delegate_mut().set_entries(Vec::new());
                    t.refresh(cx);
                });
            }
        }
        cx.notify();
    }

    /// Navigate up to the parent directory.
    pub(crate) fn navigate_parent(&mut self, cx: &mut Context<Self>) {
        let Some(parent) = self.browser().cwd().parent() else {
            log::debug!("SftpPanel::navigate_parent: already at root");
            return;
        };
        log::debug!(
            "SftpPanel::navigate_parent: \"{}\" → \"{parent}\"",
            self.browser().cwd()
        );
        self.load_dir(parent, cx);
    }

    /// Refresh the current directory.
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        log::debug!(
            "SftpPanel::refresh: refreshing \"{}\"",
            self.browser().cwd()
        );
        self.load_dir(self.browser().cwd().clone(), cx);
    }

    /// Refresh the cached terminal cwd and notify when the sync button state or
    /// tooltip should be re-rendered. The cache is not used for navigation;
    /// sync actions read the cwd source live to avoid stale jumps.
    pub(crate) fn refresh_terminal_cwd_cache(&mut self, cx: &mut Context<Self>) {
        if self.follow_mut().refresh_cache() {
            cx.notify();
        }
    }

    /// Navigate the SFTP browser to the active terminal's current directory.
    /// No-op if there is no SFTP connection or the terminal has not reported a cwd.
    pub(crate) fn sync_to_terminal_cwd(&mut self, cx: &mut Context<Self>) {
        if self.sftp().is_none() {
            return;
        }
        let cwd = match self.follow().terminal_cwd() {
            Some(p) => p,
            None => {
                log::debug!("SftpPanel::sync_to_terminal_cwd: terminal cwd unavailable");
                return;
            }
        };
        log::info!(
            "SftpPanel::sync_to_terminal_cwd: \"{}\" → \"{cwd}\"",
            self.browser().cwd()
        );
        // `goto_path` stats the path (dir check) + handles errors + load_dir.
        self.goto_path(cwd, cx);
    }

    /// Toggle the auto-follow-terminal-cwd flag (from the "..." menu checkbox).
    pub(crate) fn toggle_follow_terminal_cwd(&mut self, cx: &mut Context<Self>) {
        let enabled = self.follow_mut().toggle();
        log::info!(
            "SftpPanel: auto-follow terminal cwd {}",
            if enabled { "enabled" } else { "disabled" }
        );
        // When enabling, immediately attempt a follow so the browser jumps to
        // the terminal's cwd right away (instead of waiting up to 500ms).
        if enabled {
            self.maybe_follow_terminal_cwd(cx);
        }
        cx.notify();
    }

    /// Polling hook — called by the auto-follow timer. If follow is enabled and
    /// the terminal's cwd has changed (and differs from the browser's cwd),
    /// navigate the SFTP browser to the new cwd.
    pub(crate) fn maybe_follow_terminal_cwd(&mut self, cx: &mut Context<Self>) {
        if !self.follow().enabled() || self.sftp().is_none() {
            return;
        }
        let cwd = match self.follow().terminal_cwd() {
            Some(p) => p,
            None => return,
        };
        // Skip if we already followed this exact cwd (no change since last poll).
        if self.follow().last() == Some(&cwd) {
            return;
        }
        // Skip if the browser is already showing this directory.
        if *self.browser().cwd() == cwd {
            self.follow_mut().set_last(Some(cwd));
            return;
        }
        log::debug!(
            "SftpPanel::maybe_follow_terminal_cwd: auto-follow \"{}\" → \"{cwd}\"",
            self.browser().cwd()
        );
        self.follow_mut().set_last(Some(cwd.clone()));
        self.goto_path(cwd, cx);
    }

    /// Navigate into a subdirectory (double-click a folder).
    pub(crate) fn navigate_into(&mut self, idx: usize, cx: &mut Context<Self>) {
        let entry = self.table().read(cx).delegate().entries().get(idx).cloned();
        match entry {
            Some(entry) if entry.is_dir => {
                log::debug!(
                    "SftpPanel::navigate_into: \"{}\" → \"{}\"",
                    self.browser().cwd(),
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
        let changed = self.table().update(cx, |t, cx| {
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
        self.browser()
            .selected()
            .and_then(|ix| self.table().read(cx).delegate().entries().get(ix).cloned())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gpui::{AppContext as _, TestAppContext, VisualTestContext};
    use oneterm_core::RemotePath;

    use super::SftpPanel;
    use crate::test_backend::{FakeSftpBackend, dir_entry};

    fn test_panel(cx: &mut TestAppContext) -> (gpui::Entity<SftpPanel>, &mut VisualTestContext) {
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
        (panel, cx)
    }

    fn attach_backend(
        panel: &gpui::Entity<SftpPanel>,
        cx: &mut VisualTestContext,
    ) -> Arc<FakeSftpBackend> {
        let backend = Arc::new(FakeSftpBackend::new());
        panel.update(cx, |panel, cx| {
            panel.attach_backend_for_test(backend.clone(), RemotePath::new(""), cx);
        });
        backend
    }

    fn listed_names(panel: &gpui::Entity<SftpPanel>, cx: &mut VisualTestContext) -> Vec<String> {
        panel.read_with(cx, |panel, cx| {
            panel
                .table()
                .read(cx)
                .delegate()
                .entries()
                .iter()
                .map(|entry| entry.name.clone())
                .collect()
        })
    }

    fn cwd(panel: &gpui::Entity<SftpPanel>, cx: &mut VisualTestContext) -> RemotePath {
        panel.read_with(cx, |panel, _| panel.browser().cwd().clone())
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
        assert_eq!(cwd(&panel, cx), second_dir);

        // The stale answer must be ignored.
        first_reply
            .try_send(Ok(vec![dir_entry(&first_dir, "a.txt", false)]))
            .unwrap();
        cx.run_until_parked();
        assert_eq!(listed_names(&panel, cx), vec!["b.txt"]);
        assert_eq!(cwd(&panel, cx), second_dir);
        assert!(panel.read_with(cx, |panel, cx| !panel.table().read(cx).delegate().loading));
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
        panel.update(cx, |panel, _| {
            panel.browser_mut().set_cwd(RemotePath::new("/other"))
        });

        reply
            .try_send(Ok(vec![dir_entry(&dir, "stale.txt", false)]))
            .unwrap();
        cx.run_until_parked();

        assert!(listed_names(&panel, cx).is_empty());
        assert_eq!(cwd(&panel, cx), RemotePath::new("/other"));
        assert!(other.read_dir_requests().is_empty());
    }
}
