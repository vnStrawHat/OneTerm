//! The [`TerminalView`] entity: construction, the session events pump, cursor
//! blink, lifecycle, and the OSC-to-UI event table.
//!
//! Per-frame work lives in [`super::render`], the wrapper's listeners in
//! [`super::input`]; the sibling modules hold the cohesive sub-states the view
//! owns (search, scrollbar, gutter timestamps, completion, agent status).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

use async_channel::Receiver;
use gpui::{
    App, AsyncApp, ClipboardItem, Context, Entity, EntityId, EventEmitter, FocusHandle, KeyBinding,
    NoAction, Subscription, Task, WeakEntity, Window,
};
use gpui_component::ActiveTheme as _;
use oneterm_core::{InputChannel, SessionDuplicateConfig};
use oneterm_settings::{TerminalBlink, TerminalSettings};
use oneterm_state::{
    AgentRegistry, BroadcastInput, CompletionHistory, GlobalCompletionHistory, InputChannelRegistry,
};
use oneterm_terminal::security_policy::MAX_QUEUED_NOTIFICATIONS;
use oneterm_terminal::{
    SessionEvent, SessionKind, TerminalPalette, TerminalProgress, TerminalSession, encode_osc52,
};

use super::completion::CompletionState;
use super::gutter_timestamps::GutterTimestamps;
use super::render::{CachedFont, ThemeKey, initial_inputs};
use super::scrollbar::ScrollbarState;
use super::search::SearchState;
use crate::input::MouseState;
#[cfg(test)]
use crate::render::diagnostics::FrameStats;
use crate::render::state::RenderState;
use crate::space::SplitContext;
use crate::url::UrlHover;

const CURSOR_BLINK_INTERVAL_MS: u64 = 500;

/// The process-level services a terminal view needs, resolved once by the
/// panel that creates the view and handed down (ARCH-20) instead of being
/// reached through globals from every render and input handler.
#[derive(Clone)]
pub(crate) struct TerminalDeps {
    /// Live terminal settings (font, colours, scrollback, security…).
    pub(crate) settings: Entity<TerminalSettings>,
    /// The Agent Panel model this terminal feeds; `None` when the agent
    /// feature is not initialised (tests, tools).
    pub(crate) agent_registry: Option<Entity<AgentRegistry>>,
    /// The cross-tab completion history; `None` when completion is not
    /// initialised.
    pub(crate) completion_history: Option<Entity<CompletionHistory>>,
    /// Broadcast input channel membership; `None` when the channels are not
    /// initialised (tests, tools), which makes every fan-out a no-op.
    pub(crate) input_channels: Option<Entity<InputChannelRegistry>>,
}

impl TerminalDeps {
    /// Resolve the dependencies from the process globals. Called by the panel
    /// (the composition point for terminal views); the view itself never
    /// touches the globals again.
    pub(crate) fn from_globals(cx: &App) -> Self {
        Self {
            settings: TerminalSettings::global(cx),
            agent_registry: AgentRegistry::try_global(cx),
            completion_history: GlobalCompletionHistory::try_global(cx),
            input_channels: InputChannelRegistry::try_global(cx),
        }
    }

    /// Repeat `input` on the channel peers of `origin`. A no-op without a
    /// registry, and (in the registry) for a view that joined no channel.
    pub(crate) fn fan_out(&self, origin: EntityId, input: BroadcastInput<'_>, cx: &mut App) {
        BroadcastOrigin {
            id: origin,
            channels: self.input_channels.clone(),
        }
        .fan_out(input, cx);
    }
}

/// The view an input came from, for the fan-out of the edit commands: they are
/// reached through a fn pointer shared with the menu and the panel actions, so
/// the origin travels as data instead of as a view reference.
#[derive(Clone)]
pub(crate) struct BroadcastOrigin {
    pub(crate) id: EntityId,
    pub(crate) channels: Option<Entity<InputChannelRegistry>>,
}

impl BroadcastOrigin {
    /// The channel this origin is in, if any.
    pub(crate) fn channel(&self, cx: &App) -> Option<InputChannel> {
        self.channels.as_ref()?.read(cx).channel_of(self.id)
    }

    /// Repeat `input` on this origin's channel peers; a no-op without a registry.
    pub(crate) fn fan_out(&self, input: BroadcastInput<'_>, cx: &mut App) {
        let Some(registry) = self.channels.clone() else {
            return;
        };
        let origin = self.id;
        registry.update(cx, |registry, cx| registry.fan_out(origin, input, cx));
    }
}

/// GPUI events emitted by [`TerminalView`] for its containing panel. The
/// dock's tab title is rendered by `TerminalPanel::title()`, which only re-runs
/// when the panel re-renders — so the view emits these to let the panel
/// `cx.notify()` and refresh the tab strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TerminalViewEvent {
    /// The OSC 0/2 window title changed (or was reset). The panel re-reads
    /// the live title via `TerminalSession::title()`.
    TitleChanged,
}

/// View that renders one terminal session (local or ssh — via `dyn TerminalSession`).
pub(crate) struct TerminalView {
    pub(crate) session: Entity<Box<dyn TerminalSession>>,
    /// Non-secret launch metadata used by Duplicate Session.
    pub(crate) duplicate_config: Option<SessionDuplicateConfig>,
    /// Split context — set by the owning `TerminalPanel` so this terminal's
    /// context menu can dispatch Split / Close-Space to the right Space. `None`
    /// until the panel wires it up (always set for a live terminal leaf).
    pub(crate) split_ctx: Option<SplitContext>,
    pub(crate) focus: FocusHandle,
    pub(super) deps: TerminalDeps,
    /// Whether `focus` currently holds keyboard focus — kept current by the
    /// focus/blur subscriptions so the blink task can pause while unfocused
    /// without needing a `Window`.
    pub(super) focused: bool,
    /// Render state shared with the element and read by the input handlers
    /// (the hit-test contract lives in its `geometry`).
    pub(super) render_state: Rc<RefCell<RenderState>>,
    /// The terminal `Font` built from settings, rebuilt only when the family,
    /// weight, or feature list changes.
    pub(super) cached_font: Option<CachedFont>,
    /// The inputs the current `RenderInputs.theme` was built from.
    pub(super) theme_key: Option<ThemeKey>,
    /// Last palette pushed to the backend — skip `set_default_colors` when
    /// the palette has not changed.
    pub(super) last_pushed_palette: Option<TerminalPalette>,
    pub(super) scrollbar: ScrollbarState,
    pub(super) mouse: MouseState,
    /// URL under the mouse + Ctrl state (highlight + Ctrl+click to open).
    pub(super) url_hover: UrlHover,
    /// Grow-only per-line timestamps for the gutter.
    pub(super) gutter_times: GutterTimestamps,
    /// In-buffer search (Ctrl+F).
    pub(super) search: SearchState,
    /// Auto-completion controller + overlay anchor.
    pub(super) completion: CompletionState,
    /// Cursor blink phase. True = draw, false = hide.
    pub(super) blink_visible: bool,
    /// Bell indicator — set on `\x07`, cleared when the user presses a key.
    pub(super) has_bell: bool,
    /// Pending OSC 9 desktop notifications — drained in `render` via
    /// `window.push_notification` (which needs a `Window`, unavailable in the
    /// events task).
    pub(super) pending_notifications: VecDeque<String>,
    /// Number of notifications dropped because the queue was full.
    pub(super) dropped_notifications: usize,
    /// Current OSC 9;4 taskbar progress (`None` = no progress / removed).
    pub(super) progress: Option<TerminalProgress>,
    /// Whether the view is alive (not yet shut down). Gates the blink task.
    pub(crate) alive: bool,
    /// A remote SSH close keeps the terminal content available read-only.
    pub(super) ssh_closed: bool,
    /// Deliver the SSH-close toast once from render, where a `Window` exists.
    pub(super) pending_ssh_closed_notification: bool,
    /// The events pump — dropped (cancelled) by `shutdown`.
    pub(crate) event_task: Option<Task<()>>,
    /// The blink task — dropped (cancelled) by `shutdown`.
    pub(crate) blink_task: Option<Task<()>>,
    /// Focus/blur subscriptions (dropped with the view).
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<TerminalViewEvent> for TerminalView {}

impl TerminalView {
    /// Create the view from a session entity, start the events pump and the
    /// blink task, and take focus.
    pub(crate) fn new(
        session: Entity<Box<dyn TerminalSession>>,
        deps: TerminalDeps,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();

        // Tab / Shift+Tab must reach the PTY, not move focus.
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
                "TerminalView: session events were already taken; this view will not receive live updates"
            );
        }
        let event_task = events.map(|rx| {
            cx.spawn(async move |this, cx| {
                while let Ok(ev) = rx.recv().await {
                    // Coalesce every Output already queued behind this one
                    // into a single render; the reliable events in the same
                    // batch are handled in order. No wall-clock sleep: GPUI
                    // merges the resulting `notify()`s into one paint.
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
                let continue_blinking = this.update(cx, |view, cx| {
                    if !view.alive || view.ssh_closed {
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
        ];

        let inputs = {
            let settings = deps.settings.read(cx);
            initial_inputs(settings, cx.theme())
        };

        focus.focus(window, cx);

        Self {
            session,
            duplicate_config: None,
            split_ctx: None,
            focus,
            deps,
            focused: true,
            render_state: Rc::new(RefCell::new(RenderState::new(inputs))),
            cached_font: None,
            theme_key: None,
            last_pushed_palette: None,
            scrollbar: ScrollbarState::default(),
            mouse: MouseState::default(),
            url_hover: UrlHover::default(),
            gutter_times: GutterTimestamps::default(),
            search: SearchState::default(),
            completion: CompletionState::default(),
            blink_visible: true,
            has_bell: false,
            pending_notifications: VecDeque::new(),
            dropped_notifications: 0,
            progress: None,
            alive: true,
            ssh_closed: false,
            pending_ssh_closed_notification: false,
            event_task,
            blink_task: Some(blink_task),
            _subscriptions: subscriptions,
        }
    }

    /// One 500 ms blink tick: toggle + repaint only while the toggle is
    /// visible — the view is focused and blinking is enabled. An unfocused
    /// view or `cursor_blink = Off` always draws a steady cursor, so a tick
    /// there would only cost a frame.
    pub(super) fn blink_tick(&mut self, cx: &mut Context<Self>) {
        let blink_on = self.deps.settings.read(cx).cursor_blink == TerminalBlink::On;
        if !self.focused || !blink_on {
            // Restart from "visible" so the next focused frame does not start
            // with a hidden cursor.
            self.blink_visible = true;
            return;
        }
        self.blink_visible = !self.blink_visible;
        cx.notify();
    }

    /// Drain every event already queued behind an `Output`: consecutive
    /// `Output`s are coalesced into the one the caller is about to handle,
    /// every other event is handled immediately through [`Self::handle_event`]
    /// (a process that prints and exits delivers `Exited`/`Closed` right
    /// behind `Output` — the common case).
    fn drain_coalesced_events(
        rx: &Receiver<SessionEvent>,
        this: &WeakEntity<Self>,
        cx: &mut AsyncApp,
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
    /// both the events pump and the coalescing drain.
    pub(crate) fn handle_event(&mut self, ev: SessionEvent, cx: &mut Context<Self>) {
        match ev {
            SessionEvent::Clipboard(text) => {
                cx.write_to_clipboard(ClipboardItem::new_string(text.unwrap_or_default()));
            }
            SessionEvent::Output => {
                // The viewport is intentionally left where the user scrolled
                // it; keyboard input re-snaps to the bottom.
                //
                // Stamp at the OUTPUT moment — the single stamper: the events
                // task runs independently of render, so an inactive tab (not
                // rendering) still stamps lines with the time they appeared.
                let info = self.session.read(cx).terminal_info();
                self.gutter_times.update(&info);
                // New output shifts the grid coordinate system, so stored
                // search matches would point at the wrong rows. Mark them
                // stale; `render` refreshes once per frame instead of once
                // per PTY read.
                self.search.mark_dirty();
            }
            SessionEvent::Bell => self.has_bell = true,
            SessionEvent::Notification(msg) => self.queue_notification(msg),
            SessionEvent::ClipboardRead => self.reply_clipboard_read(cx),
            SessionEvent::Title(_) => {
                // The panel's `title()` reads the live session title; it only
                // re-runs when the panel is notified.
                cx.emit(TerminalViewEvent::TitleChanged);
            }
            SessionEvent::Progress(p) => self.set_progress(p),
            SessionEvent::AgentStatus(ev) => self.push_agent_status(&ev, cx),
            SessionEvent::Exited(code) => self.mark_agent_ended(code, cx),
            SessionEvent::Closed => {
                self.mark_agent_ended(None, cx);
                let kind = self.session.read(cx).kind();
                self.handle_session_closed(kind);
            }
            // OSC 7 cwd is read on demand through the capabilities; OSC 133
            // row roles are not consumed by the UI yet.
            SessionEvent::Cwd(_) | SessionEvent::ShellIntegration(_) => {}
        }
        cx.notify();
    }

    pub(super) fn handle_session_closed(&mut self, kind: SessionKind) {
        if kind == SessionKind::Ssh && !self.ssh_closed {
            self.ssh_closed = true;
            self.pending_ssh_closed_notification = true;
        }
    }

    pub(super) fn queue_notification(&mut self, message: String) {
        if self.pending_notifications.len() >= MAX_QUEUED_NOTIFICATIONS {
            self.pending_notifications.pop_front();
            self.dropped_notifications = self.dropped_notifications.saturating_add(1);
        }
        self.pending_notifications.push_back(message);
    }

    /// Shut down this view: cancel tasks, close the session, drop the agent
    /// cards. Idempotent — safe to call multiple times.
    pub(crate) fn shutdown(&mut self, cx: &mut Context<Self>) {
        if !self.alive {
            return;
        }
        self.alive = false;
        // GPUI cancels a task when its handle is dropped.
        drop(self.event_task.take());
        drop(self.blink_task.take());
        self.session.update(cx, |s, _| {
            if let Err(error) = s.close() {
                log::warn!("terminal close failed: {error}");
            }
        });
        // A true close removes this terminal's Agent Panel cards + navigation
        // entry, unlike process exit which only marks them Ended.
        let key = cx.entity_id();
        if let Some(registry) = self.deps.agent_registry.clone() {
            registry.update(cx, |reg, cx| reg.remove_terminal(key, cx));
        }
        // A closed Space is no longer a broadcast target.
        self.leave_channel(cx);
    }

    /// Join `channel`, leaving whichever channel this Space was in.
    pub(crate) fn join_channel(&mut self, channel: InputChannel, cx: &mut Context<Self>) {
        let Some(registry) = self.deps.input_channels.clone() else {
            return;
        };
        let id = cx.entity_id();
        let session = self.session.clone();
        registry.update(cx, |registry, cx| registry.join(id, channel, session, cx));
    }

    /// Leave the channel this Space is in, if any.
    pub(crate) fn leave_channel(&mut self, cx: &mut Context<Self>) {
        let Some(registry) = self.deps.input_channels.clone() else {
            return;
        };
        let id = cx.entity_id();
        registry.update(cx, |registry, cx| {
            registry.leave(id, cx);
        });
    }

    /// The channel this Space is in, if any.
    pub(crate) fn channel(&self, cx: &Context<Self>) -> Option<InputChannel> {
        let registry = self.deps.input_channels.as_ref()?;
        registry.read(cx).channel_of(cx.entity_id())
    }

    /// The origin of any input this view produces, for the fan-out.
    pub(crate) fn broadcast_origin(&self, cx: &Context<Self>) -> BroadcastOrigin {
        BroadcastOrigin {
            id: cx.entity_id(),
            channels: self.deps.input_channels.clone(),
        }
    }

    /// The counters of the most recently painted frame. Test seam only: the
    /// `terminal-diagnostics` feature reports through the element's log line
    /// and has no in-crate consumer for a snapshot.
    #[cfg(test)]
    pub(crate) fn render_diagnostics(&self) -> FrameStats {
        self.render_state.borrow().stats
    }

    /// Update the OSC 9;4 progress state. `Remove` clears it (`None`).
    pub(super) fn set_progress(&mut self, progress: TerminalProgress) {
        self.progress = match progress {
            TerminalProgress::Remove => None,
            other => Some(other),
        };
    }

    /// Reply to an OSC 52 clipboard-read request (`52;c;?`) with the current
    /// system clipboard content, base64-encoded. Gated behind the
    /// `allow_clipboard_read` setting (default off): reading exposes the local
    /// clipboard to the requesting program — including remote ones over SSH.
    fn reply_clipboard_read(&self, cx: &mut Context<Self>) {
        let allowed = self.deps.settings.read(cx).allow_clipboard_read;
        if !allowed {
            log::debug!("OSC 52 clipboard read refused (allow_clipboard_read = false)");
            return;
        }
        let text = cx
            .read_from_clipboard()
            .and_then(|c| c.text())
            .unwrap_or_default();
        let reply = format!("\x1b]52;c;{}\x07", encode_osc52(&text));
        if let Err(error) = self.session.read(cx).write(reply.as_bytes()) {
            log::warn!("OSC 52 clipboard response delivery failed: {error}");
        }
    }
}
