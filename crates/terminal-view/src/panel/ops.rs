//! Space operations for [`TerminalPanel`] — split, close, fill, drag-drop, and
//! the active-session publish logic.

use std::rc::Rc;
use std::sync::Arc;

use gpui::{App, AppContext as _, Entity, Window};

use gpui_component::WindowExt as _;
use gpui_component::dock::PanelView;
use gpui_component::notification::NotificationType;
use gpui_component::resizable::ResizableState;

use oneterm_core::SessionDuplicateConfig;
use oneterm_state::{AppServices, AppState};
use oneterm_terminal::TerminalSession;
use oneterm_theme::notif_ext::notify;

use super::super::security::security_policy_from_settings;
use super::super::space::{
    CloseOutcome, DragTerminalTab, SpaceContent, SpaceId, SpaceLeaf, SplitDir,
};
use super::super::view::LocalTerminalView;
use super::terminal_panel::INITIAL_PTY_SIZE;
use super::{PanelSpec, TerminalPanel};

/// User-facing message when a duplicate's destination disappeared before the
/// session could be installed.
const DUPLICATE_DESTINATION_GONE: &str = "The duplicate destination is no longer available.";

fn apply_duplicate_cwd(
    mut config: oneterm_core::LocalShellConfig,
    live_cwd: Option<std::path::PathBuf>,
) -> oneterm_core::LocalShellConfig {
    config.cwd = live_cwd;
    config
}

struct DuplicatedSession {
    session: Box<dyn TerminalSession>,
    title: String,
    duplicate_config: SessionDuplicateConfig,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DuplicateDestination {
    NewTab,
    ExistingSpace(SpaceId),
    Split(SplitDir),
}

/// A terminal that could not be installed into its destination.
enum Unplaced {
    /// A raw session that never got a view.
    Session(Box<dyn TerminalSession>),
    /// A view whose target Space stopped being empty.
    View(Entity<LocalTerminalView>),
}

/// Release an unplaced terminal (close the session / shut the view down) and
/// tell the user why it did not appear.
fn close_unplaced(
    unplaced: Unplaced,
    kind: NotificationType,
    message: &'static str,
    window: &mut Window,
    cx: &mut App,
) {
    match unplaced {
        Unplaced::Session(session) => {
            if let Err(error) = session.close() {
                log::warn!("close_unplaced: failed to close unplaced session: {error}");
            }
        }
        Unplaced::View(view) => view.update(cx, |view, cx| view.shutdown(cx)),
    }
    window.push_notification(notify(kind, message, cx), cx);
}

impl TerminalPanel {
    /// Create a new tab from the session in `space_id` without changing the source.
    pub(crate) fn duplicate_session(
        &mut self,
        space_id: SpaceId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.duplicate_session_to(space_id, DuplicateDestination::NewTab, window, cx);
    }

    /// Duplicate the session in `space_id` into the requested destination.
    pub(crate) fn duplicate_session_to(
        &mut self,
        space_id: SpaceId,
        destination: DuplicateDestination,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(view) = self.tree.leaf_terminal(space_id) else {
            return;
        };
        let (duplicate_config, live_cwd) = {
            let view = view.read(cx);
            (view.duplicate_config.clone(), view.session.read(cx).cwd())
        };
        let Some(duplicate_config) = duplicate_config else {
            window.push_notification(
                notify(
                    NotificationType::Warning,
                    "This terminal does not provide duplication metadata.",
                    cx,
                ),
                cx,
            );
            return;
        };
        match duplicate_config {
            SessionDuplicateConfig::Local(config) => {
                let config = apply_duplicate_cwd(config, live_cwd);
                let factory = AppServices::session_factory(cx);
                let (scrollback, security) = {
                    let settings = self.deps.settings.read(cx);
                    (
                        settings.scrollback_history,
                        security_policy_from_settings(settings),
                    )
                };
                let duplicate_config = SessionDuplicateConfig::Local(config.clone());
                let session =
                    match factory.spawn_local(config, INITIAL_PTY_SIZE, scrollback, security) {
                        Ok(session) => session,
                        Err(error) => {
                            window.push_notification(
                                notify(
                                    NotificationType::Error,
                                    format!("Failed to duplicate local session: {error}"),
                                    cx,
                                ),
                                cx,
                            );
                            return;
                        }
                    };
                self.place_duplicate_session(
                    DuplicatedSession {
                        session,
                        title: "Terminal".to_string(),
                        duplicate_config,
                    },
                    space_id,
                    destination,
                    window,
                    cx,
                );
            }
            SessionDuplicateConfig::Ssh(config) => {
                let commands = oneterm_state::commands::commands(cx);
                let panel = cx.entity().downgrade();
                let completion: oneterm_state::commands::SshDuplicateCompletion =
                    Rc::new(move |session, label, duplicate_config, window, cx| {
                        if let Some(panel) = panel.upgrade() {
                            panel.update(cx, |panel, cx| {
                                panel.place_duplicate_session(
                                    DuplicatedSession {
                                        session,
                                        title: label,
                                        duplicate_config,
                                    },
                                    space_id,
                                    destination,
                                    window,
                                    cx,
                                );
                            });
                        } else {
                            close_unplaced(
                                Unplaced::Session(session),
                                NotificationType::Warning,
                                DUPLICATE_DESTINATION_GONE,
                                window,
                                cx,
                            );
                        }
                    });
                (commands.open_duplicate_ssh_dialog)(config, live_cwd, completion, window, cx);
            }
        }
    }

    fn place_duplicate_session(
        &mut self,
        duplicate: DuplicatedSession,
        source_space: SpaceId,
        destination: DuplicateDestination,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let DuplicatedSession {
            session,
            title,
            duplicate_config,
        } = duplicate;
        let target = match destination {
            DuplicateDestination::NewTab => {
                let Some(tab_panel) = self.tab_panel.as_ref().and_then(|panel| panel.upgrade())
                else {
                    close_unplaced(
                        Unplaced::Session(session),
                        NotificationType::Error,
                        "The terminal tab container is unavailable.",
                        window,
                        cx,
                    );
                    return;
                };
                let panel = TerminalPanel::open(
                    PanelSpec::Session {
                        session,
                        title,
                        duplicate_config: Some(duplicate_config),
                    },
                    window,
                    cx,
                );
                tab_panel.update(cx, |tabs, cx| {
                    tabs.add_panel(Arc::new(panel), window, cx);
                });
                return;
            }
            DuplicateDestination::ExistingSpace(target) => target,
            DuplicateDestination::Split(dir) => {
                if self.tree.leaf_terminal(source_space).is_none() {
                    close_unplaced(
                        Unplaced::Session(session),
                        NotificationType::Warning,
                        DUPLICATE_DESTINATION_GONE,
                        window,
                        cx,
                    );
                    return;
                }
                let target = self.tree.alloc_id();
                let empty = SpaceLeaf {
                    id: target,
                    content: SpaceContent::Empty,
                    focus: cx.focus_handle(),
                };
                let state = cx.new(|_| ResizableState::default());
                self.tree.split(source_space, dir, empty, state);
                target
            }
        };

        let session = cx.new(|_| session);
        let deps = self.deps.clone();
        let view = cx.new(|cx| {
            let mut view = LocalTerminalView::new(session, deps, window, cx);
            view.duplicate_config = Some(duplicate_config);
            view
        });
        if let Err(view) = self.place_view(target, view, window, cx) {
            close_unplaced(
                Unplaced::View(view),
                NotificationType::Warning,
                DUPLICATE_DESTINATION_GONE,
                window,
                cx,
            );
        }
    }

    /// Install `view` into the empty Space `target`: wire its split context,
    /// fill the Space, resubscribe to title changes, and make it the active
    /// Space. On `Err` the Space was no longer empty and the view is handed
    /// back untouched (still alive) for the caller to place elsewhere or close.
    fn place_view(
        &mut self,
        target: SpaceId,
        view: Entity<LocalTerminalView>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Result<(), Entity<LocalTerminalView>> {
        self.attach_split_ctx(&view, target, cx);
        self.tree.fill_empty(target, view)?;
        self.rebuild_title_subs(cx);
        self.set_active_space(target, window, cx);
        cx.notify();
        Ok(())
    }

    /// Split Space `space_id` in `dir`; the new empty Space becomes active.
    pub(crate) fn split_active_at(
        &mut self,
        space_id: SpaceId,
        dir: SplitDir,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let new_id = self.tree.alloc_id();
        let empty = SpaceLeaf {
            id: new_id,
            content: SpaceContent::Empty,
            focus: cx.focus_handle(),
        };
        let state = cx.new(|_| ResizableState::default());
        self.tree.split(space_id, dir, empty, state);
        self.set_active_space(new_id, window, cx);
        cx.notify();
    }

    /// Close Space `space_id`. Closes the whole tab if it was the last Space.
    pub(crate) fn close_space(
        &mut self,
        space_id: SpaceId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let (outcome, removed) = self.tree.close(space_id);
        if let Some(view) = removed {
            view.update(cx, |v, cx| v.shutdown(cx));
        }
        if outcome == CloseOutcome::LastSpaceClosed {
            if let Some(tp) = self.tab_panel.as_ref().and_then(|w| w.upgrade()) {
                let panel: Arc<dyn PanelView> = Arc::new(cx.entity());
                tp.update(cx, |tp, cx| {
                    tp.remove_panel(panel, window, cx);
                });
            }
            return;
        }
        self.rebuild_title_subs(cx);
        let active = self.tree.active();
        self.set_active_space(active, window, cx);
        cx.notify();
    }

    /// Spawn a local shell directly into empty Space `space_id`.
    pub(crate) fn new_terminal_here(
        &mut self,
        space_id: SpaceId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let view = match TerminalPanel::spawn_local_view(&self.deps, None, window, cx) {
            Some(v) => v,
            None => {
                log::warn!("new_terminal_here: spawn failed");
                return;
            }
        };
        if let Err(view) = self.place_view(space_id, view, window, cx) {
            close_unplaced(
                Unplaced::View(view),
                NotificationType::Warning,
                "The destination Space is no longer available.",
                window,
                cx,
            );
        }
    }

    /// Make Space `space_id` the active Space (focus it + refresh status bar).
    pub(crate) fn set_active_space(
        &mut self,
        space_id: SpaceId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.tree.has_leaf(space_id) {
            return;
        }
        let changed = self.tree.active() != space_id;
        self.tree.set_active(space_id);
        if let Some(fh) = self.tree.active_focus_handle(cx) {
            fh.focus(window, cx);
        }
        if changed {
            if self.is_active {
                self.publish_active_session(cx);
            }
            cx.notify();
        }
    }

    /// Take the active Space's terminal view out of this tree, leaving it empty
    /// (and collapsing that Space if other Spaces remain). Used by drag-drop.
    pub(crate) fn take_active_terminal_view(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<Entity<LocalTerminalView>> {
        let id = self.tree.active();
        let view = self.tree.take_leaf_terminal(id)?;
        if self.tree.leaf_count() > 1 {
            let _ = self.tree.close(id);
            self.rebuild_title_subs(cx);
            let active = self.tree.active();
            self.set_active_space(active, window, cx);
            cx.notify();
        }
        Some(view)
    }

    /// Handle a Terminal Tab dropped onto empty Space `target`: move the source
    /// tab's active terminal into this Space (see `docs/terminal-split/03`).
    pub(crate) fn handle_tab_drop(
        &mut self,
        target: SpaceId,
        drag: &DragTerminalTab,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(src) = drag.panel.upgrade() else {
            return;
        };
        // Verify the destination BEFORE taking the terminal out of the source:
        // once taken, a failed `fill_empty` would leave the live session
        // orphaned. Everything below runs synchronously on the UI thread, so a
        // Space that is empty here is still empty at `fill_empty`.
        if !self.tree.has_leaf(target) || self.tree.leaf_terminal(target).is_some() {
            return;
        }
        let is_self = src == cx.entity();

        let view = if is_self {
            // Dropping within the same tab: no-op for a single Space.
            if self.tree.leaf_count() == 1 {
                return;
            }
            self.take_active_terminal_view(window, cx)
        } else {
            src.update(cx, |sp, cx| sp.take_active_terminal_view(window, cx))
        };
        let Some(view) = view else {
            return;
        };

        if let Err(view) = self.place_view(target, view, window, cx) {
            // Cannot happen after the guard above; if it ever does, hand the
            // terminal back to the source's active (just emptied) Space rather
            // than destroying the user's session.
            log::error!("handle_tab_drop: target Space is no longer empty; restoring source");
            if is_self {
                self.restore_dropped_view(view, window, cx);
            } else {
                src.update(cx, |sp, cx| sp.restore_dropped_view(view, window, cx));
            }
            return;
        }

        // Remove the emptied source tab (only when the source is a different,
        // now-terminal-less panel).
        if !is_self && src.read(cx).has_no_terminals() {
            if let Some(tp) = drag.tab_panel.upgrade() {
                let panel: Arc<dyn PanelView> = Arc::new(src.clone());
                tp.update(cx, |tp, cx| {
                    tp.remove_panel(panel, window, cx);
                });
            }
        }
        cx.notify();
    }

    /// Put a terminal view taken by `handle_tab_drop` back into this panel's
    /// active Space (the one `take_active_terminal_view` just emptied). Last
    /// resort only — the drop path guards the destination before taking.
    fn restore_dropped_view(
        &mut self,
        view: Entity<LocalTerminalView>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let home = self.tree.active();
        if let Err(view) = self.place_view(home, view, window, cx) {
            // Nowhere left to place it: release the session explicitly.
            log::error!("handle_tab_drop: source Space is no longer empty; closing terminal");
            view.update(cx, |view, cx| view.shutdown(cx));
            cx.notify();
        }
    }

    /// Publish the active Space's session into `AppState` (SFTP / cwd / locality).
    pub(super) fn publish_active_session(&mut self, cx: &mut gpui::Context<Self>) {
        let (sftp, cwd_source, is_local) = match self.tree.active_terminal() {
            Some(view) => {
                let session = view.read(cx).session.read(cx);
                let capabilities = session.capabilities();
                (
                    capabilities.sftp,
                    capabilities.cwd_source,
                    session.is_local(),
                )
            }
            None => (None, None, true),
        };
        let workspace_id = self.workspace_id;
        AppState::global(cx).update(cx, |state, cx| {
            state.set_active_workspace(workspace_id, sftp, cwd_source, is_local);
            cx.notify();
        });
    }
}

#[cfg(test)]
mod duplicate_tests {
    use std::path::PathBuf;

    use oneterm_core::{LocalShellConfig, ShellKind};

    use super::apply_duplicate_cwd;

    #[test]
    fn missing_live_cwd_uses_shell_default() {
        let mut config = LocalShellConfig::default();
        config.cwd = Some(PathBuf::from("configured"));
        let duplicate = apply_duplicate_cwd(config, None);
        assert_eq!(duplicate.cwd, None);
    }

    #[test]
    fn local_duplicate_preserves_shell_config_and_replaces_cwd() {
        let mut config = LocalShellConfig::default();
        config.kind = ShellKind::Custom;
        config.program = Some(PathBuf::from("custom-shell"));
        config.args = vec!["--login".to_string()];
        let duplicate = apply_duplicate_cwd(config, Some(PathBuf::from("live")));

        assert_eq!(duplicate.kind, ShellKind::Custom);
        assert_eq!(duplicate.program, Some(PathBuf::from("custom-shell")));
        assert_eq!(duplicate.args, vec!["--login"]);
        assert_eq!(duplicate.cwd, Some(PathBuf::from("live")));
    }
}
