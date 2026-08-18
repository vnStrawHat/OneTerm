//! The [`LocalTerminalView`] type — the GPUI view that renders one terminal
//! session (local or ssh) — plus its event loop and lifecycle.
//!
//! Per-frame rendering lives in [`super::render`]; input handling in
//! [`crate::handlers`]; the sibling modules under [`super`] hold the cohesive
//! sub-states the view owns (search, scrollbar, gutter timestamps, completion)
//! and the small helpers (grid coordinates, key mapping, IME).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use async_channel::Receiver;
use gpui::{
    ClipboardItem, Context, Entity, EventEmitter, FocusHandle, KeyBinding, NoAction, Subscription,
    Window,
};
use oneterm_settings::TerminalBlink;
use oneterm_terminal::{
    AgentStatusEvent, SessionEvent, TerminalPalette, TerminalProgress, TerminalSecurityPolicy,
    TerminalSession,
};

use super::completion::CompletionState;
use super::deps::TerminalDeps;
use super::gutter_timestamps::GutterTimestamps;
use super::render::CachedFont;
use super::scrollbar::ScrollbarState;
use super::search::SearchState;
use crate::element::RenderCache;
use crate::highlight::SemanticOverlay;
use crate::security::security_policy_from_settings;
use crate::url::UrlHover;

const CURSOR_BLINK_INTERVAL_MS: u64 = 500;

/// GPUI events emitted by [`LocalTerminalView`] for its containing panel to
/// observe. The dock's tab title is rendered by `TerminalPanel::title()`, which
/// only re-runs when the panel (or its `TabPanel`) re-renders — so the view
/// emits these to let the panel `cx.notify()` and refresh the tab strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TerminalViewEvent {
    /// The OSC 0/2 window title changed (or was reset via `ResetTitle`). The
    /// panel should re-read the live title via `TerminalSession::title()`.
    TitleChanged,
}

/// View that renders one terminal session (local or ssh — via `dyn TerminalSession`).
pub(crate) struct LocalTerminalView {
    pub(crate) session: Entity<Box<dyn TerminalSession>>,
    /// The services this view depends on (settings, registries).
    pub(crate) deps: TerminalDeps,
    /// Non-secret launch metadata used by Duplicate Session.
    pub(crate) duplicate_config: Option<oneterm_core::SessionDuplicateConfig>,
    pub(crate) focus: FocusHandle,
    /// Whether `focus` currently holds keyboard focus — kept current by the
    /// focus/blur subscriptions so the blink task can pause while unfocused
    /// (PERF-07) without needing a `Window`.
    pub(crate) focused: bool,
    /// Render state shared with the per-frame element and the input handlers:
    /// row layout cache, gutter width, grid size, and layout metrics.
    pub(crate) render_cache: Rc<RefCell<RenderCache>>,
    /// The terminal `Font` built from settings, rebuilt only when the family,
    /// weight, or feature list changes (PERF-05).
    pub(crate) font_cache: Option<CachedFont>,
    /// Scrollbar geometry, drag state, pending offset, and auto-hide timer.
    pub(crate) scrollbar: ScrollbarState,
    /// Whether the cursor is currently shown (blink toggle). True = draw, false = hide.
    pub(crate) cursor_blink_visible: bool,
    /// Bell indicator — true when `\x07` is received, cleared when the user presses a key.
    pub(crate) has_bell: bool,
    /// Pending OSC 9 desktop notifications — drained in `render` via
    /// `window.push_notification` (which needs a `Window`, unavailable in the
    /// async subscribe task).
    pub(crate) pending_notifications: VecDeque<String>,
    /// Number of notifications dropped because the display queue was full.
    pub(crate) dropped_notifications: usize,
    /// UI-side notification policy, derived from the user's settings (SEC-08);
    /// listener-side rate limiting happens earlier.
    pub(crate) notification_policy: TerminalSecurityPolicy,
    /// Current OSC 9;4 taskbar progress (`None` = no progress / removed).
    pub(crate) progress: Option<TerminalProgress>,
    /// URL under the mouse + Ctrl state (highlight + Ctrl+click to open).
    pub(crate) url_hover: UrlHover,
    /// Grow-only per-line timestamps for the gutter.
    pub(crate) gutter_times: GutterTimestamps,
    /// Persisted semantic overlay — updated when settings change instead of
    /// recreated every frame.
    pub(crate) semantic_overlay: SemanticOverlay,
    /// Last theme palette pushed to the backend — skip `set_default_colors`
    /// when the palette hasn't changed.
    pub(crate) last_pushed_palette: Option<TerminalPalette>,
    /// In-buffer search (Ctrl+F).
    pub(crate) search: SearchState,
    /// Split context — set by the owning `TerminalPanel` so this terminal's
    /// context menu can dispatch Split / Close-Space to the right Space. `None`
    /// until the panel wires it up (always set for a live terminal leaf).
    pub(crate) split_ctx: Option<crate::space::SplitContext>,
    /// Handle to the event-loop task — stored so it can be cancelled on drop/close.
    pub(crate) event_task: Option<gpui::Task<()>>,
    /// Handle to the cursor-blink task — stored so it can be cancelled on drop/close.
    pub(crate) blink_task: Option<gpui::Task<()>>,
    /// Whether the view is alive (not yet closed). Used to gate the blink task.
    pub(crate) alive: bool,
    /// Auto-completion controller + overlay anchor.
    pub(crate) completion: CompletionState,
    /// Focus/blur + settings subscriptions (dropped with the view).
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<TerminalViewEvent> for LocalTerminalView {}

impl LocalTerminalView {
    /// Create the view from a session entity. Subscribe to events → re-render task.
    pub(crate) fn new(
        session: Entity<Box<dyn TerminalSession>>,
        deps: TerminalDeps,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();

        cx.bind_keys([
            KeyBinding::new("tab", NoAction {}, Some("Terminal")),
            KeyBinding::new("shift-tab", NoAction {}, Some("Terminal")),
        ]);

        // The session hands out its event receiver exactly once. A view is
        // the sole consumer, so `None` means another view already owns the
        // events for this session: log it and render without live updates
        // rather than spinning on a dead channel.
        let events = session.read(cx).take_events();
        if events.is_none() {
            log::error!(
                "LocalTerminalView: session events were already taken; this view will not receive live updates"
            );
        }
        let event_task = events.map(|rx| {
            cx.spawn(async move |this, cx| {
                while let Ok(ev) = rx.recv().await {
                    // Coalesce every Output event already queued behind this
                    // one into a single render. `drain_coalesced_events`
                    // merges them via `try_recv`, so no wall-clock sleep is
                    // needed — a fixed timer would only add latency without
                    // adding coalescing. GPUI merges the resulting
                    // `notify()`s into one paint per frame.
                    if matches!(ev, SessionEvent::Output) {
                        Self::drain_coalesced_events(&rx, &this, cx);
                    }
                    if this
                        .update(cx, |view, cx| view.handle_event(ev, cx))
                        .is_err()
                    {
                        break;
                    }
                }
            })
        });

        let blink_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(CURSOR_BLINK_INTERVAL_MS))
                    .await;
                // Only blink while the view is alive. The task is also
                // cancelled when the view is dropped/closed via the stored
                // Task handle.
                let continue_blinking = this.update(cx, |view, cx| {
                    if !view.alive {
                        return false;
                    }
                    view.blink_tick(cx);
                    true
                });
                if !continue_blinking.unwrap_or(false) {
                    break;
                }
            }
        });

        let subscriptions = vec![
            cx.on_focus(&focus, window, |view, _, _| view.focused = true),
            cx.on_blur(&focus, window, |view, _, _| view.focused = false),
            cx.observe(&deps.settings, |view, settings, cx| {
                view.notification_policy = security_policy_from_settings(settings.read(cx));
            }),
        ];

        focus.focus(window, cx);
        let notification_policy = security_policy_from_settings(deps.settings.read(cx));

        Self {
            session,
            deps,
            duplicate_config: None,
            focus,
            focused: true,
            render_cache: Rc::new(RefCell::new(RenderCache::default())),
            font_cache: None,
            scrollbar: ScrollbarState::default(),
            cursor_blink_visible: true,
            has_bell: false,
            pending_notifications: VecDeque::new(),
            dropped_notifications: 0,
            notification_policy,
            progress: None,
            url_hover: UrlHover::default(),
            gutter_times: GutterTimestamps::default(),
            semantic_overlay: SemanticOverlay::default(),
            last_pushed_palette: None,
            search: SearchState::default(),
            split_ctx: None,
            event_task,
            blink_task: Some(blink_task),
            alive: true,
            completion: CompletionState::default(),
            _subscriptions: subscriptions,
        }
    }

    /// One 500 ms blink tick: toggle + repaint only while the toggle is
    /// visible — the view is focused and blinking is enabled (PERF-07). An
    /// unfocused view or `cursor_blink = Off` always draws a steady cursor
    /// (see `should_show_cursor`), so a tick there would only cost a frame.
    fn blink_tick(&mut self, cx: &mut Context<Self>) {
        let blink_on = self.deps.settings.read(cx).cursor_blink == TerminalBlink::On;
        if !self.focused || !blink_on {
            // Restart from "visible" so the next focused frame does not start
            // with a hidden cursor.
            self.cursor_blink_visible = true;
            return;
        }
        self.cursor_blink_visible = !self.cursor_blink_visible;
        cx.notify();
    }

    /// Drain every event already queued behind an `Output`: consecutive
    /// `Output`s are coalesced into the one the caller is about to handle,
    /// every other event is handled immediately through [`Self::handle_event`]
    /// (a process that prints and exits delivers `Exited`/`Closed` right
    /// behind `Output` — the common case).
    fn drain_coalesced_events(
        rx: &Receiver<SessionEvent>,
        this: &gpui::WeakEntity<Self>,
        cx: &mut gpui::AsyncApp,
    ) {
        while let Ok(ev) = rx.try_recv() {
            if matches!(ev, SessionEvent::Output) {
                continue;
            }
            if this
                .update(cx, |view, cx| view.handle_event(ev, cx))
                .is_err()
            {
                return;
            }
        }
    }

    /// Apply one session event to the view. The single event handler used by
    /// both the main event loop and the coalescing drain.
    pub(crate) fn handle_event(&mut self, ev: SessionEvent, cx: &mut Context<Self>) {
        match ev {
            SessionEvent::Clipboard(text) => {
                cx.write_to_clipboard(ClipboardItem::new_string(text.unwrap_or_default()));
            }
            SessionEvent::Output => {
                // The viewport is intentionally left where the user scrolled
                // it: alacritty keeps `display_offset` anchored to the same
                // content while output streams, and keyboard input re-snaps
                // to the bottom (see `handlers::keyboard`).
                //
                // Stamp at the OUTPUT moment — the single stamper (CORR-66):
                // the subscribe task runs independently of render, so an
                // inactive tab (not rendering) still updates timestamps to
                // the time the line was created, instead of bunching them at
                // the time the tab becomes active again.
                let info = self.session.read(cx).terminal_info();
                self.gutter_times.update(&info);
                // New output shifts the alacritty grid coordinate system, so
                // stored search matches would point at the wrong rows. Mark
                // them stale; `render` refreshes once per frame instead of
                // once per PTY read (PERF-04 / CORR-42).
                self.search.mark_dirty();
            }
            SessionEvent::Bell => self.has_bell = true,
            SessionEvent::Notification(msg) => self.queue_notification(msg),
            SessionEvent::ClipboardRead => self.reply_clipboard_read(cx),
            SessionEvent::Title(_) => {
                // OSC 0/2 title changed — notify the containing panel so its
                // `title()` (which reads the live session title) re-runs and
                // the tab strip refreshes.
                cx.emit(TerminalViewEvent::TitleChanged);
            }
            SessionEvent::Progress(p) => self.set_progress(p),
            SessionEvent::AgentStatus(ev) => self.push_agent_status(&ev, cx),
            SessionEvent::Exited(code) => self.mark_agent_ended(code, cx),
            SessionEvent::Closed => self.mark_agent_ended(None, cx),
            _ => {}
        }
        cx.notify();
    }

    fn queue_notification(&mut self, message: String) {
        let limit = self.notification_policy.max_queued_notifications;
        if limit == 0 {
            self.dropped_notifications = self.dropped_notifications.saturating_add(1);
            return;
        }
        if self.pending_notifications.len() >= limit {
            self.pending_notifications.pop_front();
            self.dropped_notifications = self.dropped_notifications.saturating_add(1);
        }
        self.pending_notifications.push_back(message);
    }

    /// Shut down this view: cancel tasks, close the session, mark as not alive.
    /// Idempotent — safe to call multiple times.
    pub(crate) fn shutdown(&mut self, cx: &mut Context<Self>) {
        if !self.alive {
            return;
        }
        self.alive = false;
        // Cancel the event-loop and blink tasks by dropping them.
        // GPUI Task cancellation happens on drop (not detach).
        drop(self.event_task.take());
        drop(self.blink_task.take());
        // Close the session (PTY/SSH channel).
        self.session.update(cx, |s, _| {
            if let Err(error) = s.close() {
                log::warn!("terminal close failed: {error}");
            }
        });
        // Drop this terminal's Agent Panel cards + navigation entry (spec §9:
        // ended-vs-closed — a true close removes the cards, unlike process exit
        // which only marks them Ended).
        let key = cx.entity_id();
        if let Some(registry) = self.deps.agent_registry.clone() {
            registry.update(cx, |reg, cx| reg.remove_terminal(key, cx));
        }
    }

    /// Return a snapshot of the most recently painted renderer counters.
    #[cfg(any(test, feature = "terminal-diagnostics"))]
    pub(crate) fn render_diagnostics(&self) -> crate::diagnostics::TerminalRenderDiagnostics {
        crate::diagnostics::TerminalRenderDiagnostics::from_cache(&self.render_cache.borrow().rows)
    }

    /// Update the OSC 9;4 progress state. `Remove` clears it (`None`).
    pub(crate) fn set_progress(&mut self, progress: TerminalProgress) {
        self.progress = match progress {
            TerminalProgress::Remove => None,
            other => Some(other),
        };
    }

    /// Fold an OSC 9;7 event into the `AgentRegistry` (Agent Panel model) and
    /// refresh this terminal's navigation entry, tagging the event with its
    /// Tab/Space grouping metadata (`docs/agent-panel-display.md` §3 / §12).
    ///
    /// No-op until the registry is available and the panel has wired up
    /// `split_ctx` (always set for a live terminal leaf).
    pub(crate) fn push_agent_status(&self, ev: &Arc<AgentStatusEvent>, cx: &mut Context<Self>) {
        log::debug!(
            "push_agent_status: recv agent={} type={} seq={}",
            ev.agent(),
            ev.type_name(),
            ev.seq()
        );
        let Some(registry) = self.deps.agent_registry.clone() else {
            log::debug!("push_agent_status: AgentRegistry not available — dropping");
            return;
        };
        let Some(sc) = self.split_ctx.clone() else {
            log::debug!("push_agent_status: no split_ctx — dropping");
            return;
        };
        let Some(panel) = sc.panel.upgrade() else {
            log::debug!("push_agent_status: split_ctx.panel already dropped — dropping");
            return;
        };
        let terminal_key = cx.entity_id();
        let tab_key = panel.entity_id();

        let (grouping, nav) = {
            let p = panel.read(cx);
            // Fetch the live OSC 0/2 title from OUR OWN session (a different
            // entity — safe to read while the view is leased) and pass it to
            // `tab_label_with_title`. We must NOT call `p.tab_label(cx)` here:
            // it reads the active terminal view via `v.read(cx)`, and we ARE
            // that view mid-`update` — re-reading would double-lease the view
            // and panic (`entity_map::read`).
            let live_title = self.session.read(cx).title();
            let grouping = oneterm_state::Grouping {
                tab_key,
                tab_title: p.tab_label_with_title(live_title.as_deref(), cx),
                space_number: sc.space_id.display_number(),
                space_order: p.space_order(sc.space_id),
            };
            let nav = crate::agent::agent_nav(p.tab_panel_weak(), sc.panel.clone(), sc.space_id);
            (grouping, nav)
        };

        registry.update(cx, |reg, cx| {
            reg.set_nav(terminal_key, nav);
            reg.apply(terminal_key, grouping, ev, cx);
        });
    }

    /// Mark this terminal's agent card(s) as `Ended` in the registry (host is
    /// authoritative for process death — spec §5.2.7). No-op if the registry is
    /// not initialized or the terminal never reported an agent.
    pub(crate) fn mark_agent_ended(&self, exit_code: Option<i32>, cx: &mut Context<Self>) {
        if let Some(registry) = self.deps.agent_registry.clone() {
            let key = cx.entity_id();
            registry.update(cx, |reg, cx| {
                reg.set_lifecycle(key, oneterm_state::Lifecycle::Ended { exit_code }, cx);
            });
        }
    }

    /// Reply to an OSC 52 clipboard-read request (`52;c;?`) with the current
    /// system clipboard content, base64-encoded. Gated behind the
    /// `allow_clipboard_read` setting (default off): reading exposes the local
    /// clipboard to the requesting program — including remote ones over SSH.
    pub(crate) fn reply_clipboard_read(&self, cx: &mut Context<Self>) {
        let allowed = self.deps.settings.read(cx).allow_clipboard_read;
        if !allowed {
            log::debug!("OSC 52 clipboard read refused (allow_clipboard_read = false)");
            return;
        }
        let text = cx
            .read_from_clipboard()
            .and_then(|c| c.text())
            .unwrap_or_default();
        let reply = format!("\x1b]52;c;{}\x07", oneterm_terminal::encode_osc52(&text));
        if let Err(error) = self.session.read(cx).write(reply.as_bytes()) {
            log::warn!("OSC 52 clipboard response delivery failed: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use gpui::{AppContext as _, TestAppContext, VisualTestContext};
    use oneterm_terminal::test_support::FakeTerminalSession;

    use super::{LocalTerminalView, TerminalDeps};

    #[gpui::test]
    fn terminal_notification_queue_is_bounded(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        cx.update(crate::init);
        cx.update(oneterm_settings::TerminalSettings::init);

        let (session, _) = FakeTerminalSession::boxed(24, 80, "");
        let (view, cx) = cx.add_window_view(move |window, cx| {
            let session = cx.new(|_| session);
            let deps = TerminalDeps::from_globals(cx);
            LocalTerminalView::new(session, deps, window, cx)
        });
        let cx: &mut VisualTestContext = cx;

        view.update(cx, |view, _| {
            view.notification_policy.max_queued_notifications = 2;
            view.queue_notification("first".to_string());
            view.queue_notification("second".to_string());
            view.queue_notification("third".to_string());
        });

        let (queued, dropped, oldest) = view.read_with(cx, |view, _| {
            (
                view.pending_notifications.len(),
                view.dropped_notifications,
                view.pending_notifications.front().cloned(),
            )
        });
        assert_eq!(queued, 2);
        assert_eq!(dropped, 1);
        assert_eq!(oldest.as_deref(), Some("second"));
    }

    /// PERF-07: a blink tick on an unfocused view (or with blinking off) keeps
    /// the cursor steady instead of toggling + repainting.
    #[gpui::test]
    fn blink_tick_only_toggles_while_focused_and_enabled(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        cx.update(crate::init);
        cx.update(oneterm_settings::TerminalSettings::init);

        let (session, _) = FakeTerminalSession::boxed(24, 80, "");
        let (view, cx) = cx.add_window_view(move |window, cx| {
            let session = cx.new(|_| session);
            let deps = TerminalDeps::from_globals(cx);
            LocalTerminalView::new(session, deps, window, cx)
        });
        let cx: &mut VisualTestContext = cx;

        view.update(cx, |view, cx| {
            view.deps.settings.update(cx, |s, _| {
                s.cursor_blink = oneterm_settings::TerminalBlink::On;
            });
            view.focused = true;
            view.cursor_blink_visible = true;
            view.blink_tick(cx);
            assert!(!view.cursor_blink_visible, "focused + On toggles");

            view.focused = false;
            view.blink_tick(cx);
            assert!(view.cursor_blink_visible, "unfocused resets to visible");
            view.blink_tick(cx);
            assert!(view.cursor_blink_visible, "unfocused never hides");

            view.focused = true;
            view.deps.settings.update(cx, |s, _| {
                s.cursor_blink = oneterm_settings::TerminalBlink::Off;
            });
            view.blink_tick(cx);
            assert!(
                view.cursor_blink_visible,
                "blink Off keeps the cursor steady"
            );
        });
    }
}
