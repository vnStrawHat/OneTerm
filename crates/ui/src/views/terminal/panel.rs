//! [`TerminalPanel`] — leaf panel displaying one Terminal session.
//!
//! MVP: creates its own `LocalSession` (default cmd) + `LocalTerminalView`.
//! TODO: move session construction to the app layer to make SSH pluggable (the
//! View still uses `dyn TerminalSession`, only the factory changes).

use std::sync::Arc;

use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, MouseButton, ParentElement, Render,
    StatefulInteractiveElement, Styled, Subscription, WeakEntity, Window, div,
    prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable,
    dock::{Panel, PanelControl, PanelEvent, PanelView, TabPanel},
    h_flex,
};
use oneterm_core::TerminalSession;
use oneterm_local::{LocalSession, PtySize};

use crate::state::{AppState, TabTitleMode, TerminalSettings};

use super::view::{LocalTerminalView, TerminalViewEvent};

/// Panel displaying one Terminal session.
pub struct TerminalPanel {
    view: Entity<LocalTerminalView>,
    /// Reference to the `TabPanel` containing this panel — used for the close-tab button.
    tab_panel: Option<WeakEntity<TabPanel>>,
    /// Whether this panel is the currently selected tab in the `TabPanel`.
    ///
    /// We can't read the `TabPanel` inside `title()` (it is rendering at that
    /// point), so we mirror this state via the [`Panel::set_active`] hook, which
    /// the `TabPanel` calls whenever the active tab changes.
    is_active: bool,
    /// Tab title — "Terminal" for local, session label for SSH.
    tab_title: String,
    /// Subscription to the view's `TerminalViewEvent::TitleChanged` — re-renders
    /// the panel so the dock's tab strip picks up the live OSC 0/2 title.
    _title_sub: Subscription,
    /// Subscription to global `TerminalSettings` changes — re-renders the panel
    /// so the tab title picks up a `tab_title_mode` switch immediately (and other
    /// settings propagate to the live terminal).
    _settings_sub: Subscription,
}

/// Resolve the tab label from the live OSC 0/2 title and the static fallback.
///
/// Uses the live title (`TerminalSession::title()`, cached by the listener
/// from `Event::Title`) when the shell has set one, otherwise falls back to the
/// static label ("Terminal" for local, the SSH session label for remote). An
/// empty/absent title — e.g. after `ResetTitle` (the listener maps it to
/// `None`) — also falls back, so the tab is never blank.
pub(crate) fn resolve_tab_label(live: Option<&str>, fallback: &str) -> String {
    match live.filter(|s| !s.is_empty()).map(trim_path_title) {
        Some(t) => t.to_string(),
        None => fallback.to_string(),
    }
}

/// Shorten a title that is just an absolute path (a Windows drive path like
/// `C:\Windows\system32\cmd.exe` or a POSIX path like `/usr/bin/bash`) to its
/// last path component, so the tab stays compact. Titles that don't look like
/// an absolute path (e.g. `user@host: ~/repo`, `vim — main.rs`, `cmd.exe`) are
/// returned unchanged — only full paths are shortened.
fn trim_path_title(title: &str) -> &str {
    let t = title.trim();
    let bytes = t.as_bytes();
    let is_abs = t.starts_with('/')
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'\\' || bytes[2] == b'/'));
    if !is_abs {
        return title;
    }
    match t.rsplit(|c| c == '\\' || c == '/').next() {
        Some(last) if !last.is_empty() => last,
        _ => title,
    }
}

impl TerminalPanel {
    /// Create a panel + spawn the default local session (cmd on Windows).
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (shell, scrollback_history) = {
            let settings = TerminalSettings::global(cx).read(cx);
            (settings.shell.clone(), settings.scrollback_history)
        };
        let session: Entity<Box<dyn TerminalSession>> = cx.new(|_cx| {
            Box::new(
                LocalSession::spawn(shell, PtySize { rows: 24, cols: 80 }, scrollback_history)
                    .expect("spawn local session"),
            ) as Box<dyn TerminalSession>
        });
        let view = cx.new(|cx| LocalTerminalView::new(session, window, cx));
        // Refresh the tab title when the session emits an OSC 0/2 title change.
        let _title_sub = cx.subscribe(&view, |_this, _view, _ev: &TerminalViewEvent, cx| {
            cx.notify();
        });
        // Re-render when the global terminal settings change (e.g. the user
        // switches Tab Title mode in Settings) so the tab picks it up at once.
        let _settings_sub = cx.observe(&TerminalSettings::global(cx), |_this, _settings, cx| {
            cx.notify();
        });
        // Focus the terminal view right after creation — app startup + new tab.
        view.read(cx).focus_handle(cx).focus(window, cx);
        Self {
            view,
            tab_panel: None,
            is_active: false,
            tab_title: "Terminal".to_string(),
            _title_sub,
            _settings_sub,
        }
    }

    /// Create a panel from an existing session (SSH or local).
    ///
    /// The session is already spawned/connected, the panel just wraps the view.
    /// Used for SSH terminal tabs — `session` is a `Box<dyn TerminalSession>`
    /// from `SshSession::connect()`.
    pub fn from_session(
        session: Box<dyn TerminalSession>,
        title: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let session_entity = cx.new(|_| session);
        let view = cx.new(|cx| LocalTerminalView::new(session_entity, window, cx));
        // Refresh the tab title when the session emits an OSC 0/2 title change.
        let _title_sub = cx.subscribe(&view, |_this, _view, _ev: &TerminalViewEvent, cx| {
            cx.notify();
        });
        // Re-render when the global terminal settings change (e.g. the user
        // switches Tab Title mode in Settings) so the tab picks it up at once.
        let _settings_sub = cx.observe(&TerminalSettings::global(cx), |_this, _settings, cx| {
            cx.notify();
        });
        view.read(cx).focus_handle(cx).focus(window, cx);
        Self {
            view,
            tab_panel: None,
            is_active: false,
            tab_title: title.to_string(),
            _title_sub,
            _settings_sub,
        }
    }

    /// Helper to create an `Entity<Self>` from an existing session.
    pub fn from_session_entity(
        session: Box<dyn TerminalSession>,
        title: &str,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| Self::from_session(session, title, window, cx))
    }

    /// Access the inner `LocalTerminalView` (used by the Edit ▸ Find menu
    /// action to toggle the in-terminal search bar).
    pub(crate) fn view(&self) -> &Entity<LocalTerminalView> {
        &self.view
    }

    /// Session network stats (SSH only — `None` for local).
    /// Used by the StatusBar to show network speed.
    pub fn network_stats(&self, cx: &App) -> Option<oneterm_core::NetStats> {
        self.view.read(cx).session.read(cx).network_stats()
    }

    /// Breadcrumb label for the active session — `"<process> — <cwd>"` (or just
    /// the cwd when no foreground process is running). `None` when the session
    /// has no cwd yet.
    /// Used by the StatusBar to show the active terminal's breadcrumb.
    pub fn breadcrumb_label(&self, cx: &App) -> Option<String> {
        let s = self.view.read(cx).session.read(cx);
        let breadcrumb = s.breadcrumb_text();
        let fg = s.foreground_process();
        breadcrumb.map(|bc| {
            if let Some(proc) = fg {
                format!("{} — {}", proc, bc)
            } else {
                bc
            }
        })
    }

    /// Helper to create an `Entity<Self>` (default local session).
    pub fn new_entity(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }
}

impl EventEmitter<PanelEvent> for TerminalPanel {}

impl Focusable for TerminalPanel {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        // Delegate to the terminal view — when the dock area focuses the panel,
        // the terminal view inside receives focus.
        self.view.read(cx).focus_handle(cx)
    }
}

impl Panel for TerminalPanel {
    fn panel_name(&self) -> &'static str {
        "terminal"
    }

    fn title(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tab_panel = self.tab_panel.clone();
        let panel_entity = cx.entity().clone();
        let theme = cx.theme().muted_foreground;
        // Active tab highlight color — taken from theme (`table.active.border`).
        let highlight = cx.theme().table_active_border;
        let is_active = self.is_active;
        // Live OSC 0/2 title from the session: alacritty caches it in
        // `SessionState.title` via the listener. Only used when the user chose
        // the "OSC 0/2" Tab Title mode; "Default" always shows the static
        // label. Long executable paths are shortened to the basename inside
        // `resolve_tab_label` (e.g. `C:\Windows\system32\cmd.exe` → `cmd.exe`).
        let mode = TerminalSettings::global(cx).read(cx).tab_title_mode;
        let session_title = self.view.read(cx).session.read(cx).title();
        let live = match mode {
            TabTitleMode::Osc => session_title.as_deref(),
            TabTitleMode::Default => None,
        };
        let tab_label = resolve_tab_label(live, &self.tab_title);

        h_flex()
            .id("tab-title")
            .relative()
            .h_full()
            .w_full()
            .min_w(px(100.))
            .items_center()
            .gap_1()
            // Active tab highlight — a 2px top border colored from the theme.
            // `Tab` wraps the title in an inner h_flex (30px tall, centered in the
            // 32px tab) + `overflow_hidden`, so this is the highest point reachable
            // from `title()` (the top edge of the inner box, ~1px below the tab edge).
            // Overflow left/right negatively to cover the full width; the excess is clipped.
            .when(is_active, |this| {
                this.child(
                    div()
                        .absolute()
                        .top_0()
                        .left(-px(20.))
                        .right(-px(20.))
                        .h(px(2.))
                        .bg(highlight),
                )
            })
            // Compensate for the Tab inner_h_flex's 12px right padding so the × sits against the right edge.
            .mr(-px(5.))
            // Middle-click on a tab → close that tab (even an inactive tab).
            .on_mouse_down(MouseButton::Middle, {
                let tp = tab_panel.clone();
                let pe = panel_entity.clone();
                move |_, window, cx| {
                    cx.stop_propagation();
                    if let Some(tp) = tp.as_ref().and_then(|tp| tp.upgrade()) {
                        let panel: Arc<dyn PanelView> = Arc::new(pe.clone());
                        tp.update(cx, |tp, cx| {
                            tp.remove_panel(panel, window, cx);
                        });
                    }
                }
            })
            // Tab title — flexible, truncated with ellipsis when narrow.
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(tab_label),
            )
            // Close button (×) — against the right edge of the tab.
            .when_some(tab_panel, |this, tp| {
                this.child(
                    div()
                        .id("tab-close")
                        .flex_shrink_0()
                        .cursor_pointer()
                        .size_4()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(3.))
                        .hover(move |this| this.bg(theme.opacity(0.15)))
                        // Prevent the click from propagating to the Tab (avoid activating the tab).
                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            if let Some(tp) = tp.upgrade() {
                                let panel: Arc<dyn PanelView> = Arc::new(panel_entity.clone());
                                tp.update(cx, |tp, cx| {
                                    tp.remove_panel(panel, window, cx);
                                });
                            }
                        })
                        .child(Icon::new(IconName::Close).xsmall().text_color(theme)),
                )
            })
    }

    fn closable(&self, _: &App) -> bool {
        true
    }

    fn zoomable(&self, _: &App) -> Option<PanelControl> {
        Some(PanelControl::Both)
    }

    fn on_added_to(
        &mut self,
        tab_panel: WeakEntity<TabPanel>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.tab_panel = Some(tab_panel);
    }

    fn set_active(&mut self, active: bool, window: &mut Window, cx: &mut Context<Self>) {
        // `TabPanel` calls this hook when the active tab changes → mirror it for `title()` to use.
        if self.is_active != active {
            self.is_active = active;
            cx.notify();
        }

        // When this tab becomes active → extract SFTP from the session (if any)
        // and set it into AppState.active_sftp for SftpPanel to observe.
        // The next active tab will overwrite it — no need to set None on deactivate.
        if active {
            let (sftp, cwd_source, is_local) = {
                let session = self.view.read(cx).session.read(cx);
                (session.sftp(), session.cwd_source(), session.is_local())
            };
            AppState::global(cx).update(cx, |state, cx| {
                state.active_sftp = sftp;
                state.active_cwd_source = cwd_source;
                state.active_is_local = is_local;
                cx.notify();
            });

            // Auto-hide the Right Dock when this tab is a local shell.
            // The right dock hosts the Session/SFTP browser, which is only useful
            // for SSH sessions — hide it on local tabs to reclaim space.
            let auto_hide = TerminalSettings::global(cx)
                .read(cx)
                .auto_hide_right_dock_on_local;
            if auto_hide {
                let dock_area = AppState::global(cx)
                    .read(cx)
                    .dock_area
                    .as_ref()
                    .and_then(|w| w.upgrade());
                if let Some(dock_area) = dock_area {
                    crate::layout::workspace::set_right_dock_open(
                        &dock_area, !is_local, window, cx,
                    );
                }
            }
        }
    }
}

impl Render for TerminalPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("terminal-panel")
            .size_full()
            .bg(cx.theme().background)
            .child(self.view.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_title_is_used() {
        // A descriptive title (not an absolute path) is kept verbatim.
        assert_eq!(
            resolve_tab_label(Some("vim — main.rs"), "Terminal"),
            "vim — main.rs"
        );
        assert_eq!(
            resolve_tab_label(Some("user@host: ~/repo"), "user@host:24"),
            "user@host: ~/repo"
        );
        // A bare shell name (no path separators / drive) is unchanged.
        assert_eq!(resolve_tab_label(Some("cmd.exe"), "Terminal"), "cmd.exe");
    }

    #[test]
    fn none_falls_back_to_static_label() {
        // No title set yet (e.g. right after spawn, before any prompt).
        assert_eq!(resolve_tab_label(None, "Terminal"), "Terminal");
        assert_eq!(resolve_tab_label(None, "prod-server"), "prod-server");
    }

    #[test]
    fn empty_title_falls_back_to_static_label() {
        // `ResetTitle` → the listener maps an empty title to `None`, but even if
        // a backend leaked an empty string we still fall back so the tab is
        // never blank.
        assert_eq!(resolve_tab_label(Some(""), "Terminal"), "Terminal");
    }

    #[test]
    fn fallback_is_returned_by_value() {
        // The fallback is cloned into the result (not borrowed) — verifies the
        // returned `String` is independent of the input lifetime.
        let label = resolve_tab_label(None, "Terminal");
        assert_eq!(label, "Terminal");
    }

    // ── trim_path_title (via resolve_tab_label) ───────────────────────

    #[test]
    fn windows_drive_path_shortened_to_basename() {
        // The classic Windows case: cmd sets the title to its full path.
        assert_eq!(
            resolve_tab_label(Some("C:\\Windows\\system32\\cmd.exe"), "Terminal"),
            "cmd.exe"
        );
        // Forward-slash drive paths are treated the same.
        assert_eq!(
            resolve_tab_label(
                Some("C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe"),
                "Terminal"
            ),
            "powershell.exe"
        );
    }

    #[test]
    fn posix_path_shortened_to_basename() {
        assert_eq!(resolve_tab_label(Some("/usr/bin/bash"), "Terminal"), "bash");
        assert_eq!(resolve_tab_label(Some("/bin/sh"), "Terminal"), "sh");
    }

    #[test]
    fn relative_or_descriptive_titles_not_trimmed() {
        // Titles that don't start with `/` or a `X:\` drive are left alone.
        assert_eq!(resolve_tab_label(Some("~/repo"), "Terminal"), "~/repo");
        assert_eq!(
            resolve_tab_label(Some("user@host: ~/repo"), "Terminal"),
            "user@host: ~/repo"
        );
        assert_eq!(
            resolve_tab_label(Some("vim — main.rs"), "Terminal"),
            "vim — main.rs"
        );
    }

    #[test]
    fn trim_path_title_helper_directly() {
        // Direct unit tests for the helper (returns a borrowed slice of input).
        assert_eq!(trim_path_title("C:\\Windows\\system32\\cmd.exe"), "cmd.exe");
        assert_eq!(trim_path_title("/usr/bin/bash"), "bash");
        assert_eq!(trim_path_title("cmd.exe"), "cmd.exe");
        assert_eq!(trim_path_title("user@host: ~/repo"), "user@host: ~/repo");
        // Surrounding whitespace is ignored when detecting/trimming.
        assert_eq!(trim_path_title("  /usr/bin/zsh  "), "zsh");
    }
}
