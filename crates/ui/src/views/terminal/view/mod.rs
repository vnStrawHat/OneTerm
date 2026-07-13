//! `LocalTerminalView` — GPUI view that renders one terminal session (local/ssh).
//!
//! The original `view.rs` module was split into `view/`.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    ClipboardItem, Context, Entity, EventEmitter, FocusHandle, KeyBinding, NoAction, Window,
};

use async_channel::Receiver;
use gpui_component::input::InputState;
use oneterm_core::{
    SearchMatch, SearchOptions, SessionEvent, TerminalInfo, TerminalProgress, TerminalSession,
};

use super::element::{GridMetrics, RowLayoutCache};
use super::scrollbar::TerminalScrollHandle;

pub(crate) mod cursor;
pub(crate) mod font;
pub(crate) mod grid;
pub(crate) mod key;
pub(crate) mod scrollbar;
#[cfg(test)]
mod tests;

const CURSOR_BLINK_INTERVAL_MS: u64 = 500;

/// GPUI events emitted by [`LocalTerminalView`] for its containing panel to
/// observe. The dock's tab title is rendered by `TerminalPanel::title()`, which
/// only re-runs when the panel (or its `TabPanel`) re-renders — so the view
/// emits these to let the panel `cx.notify()` and refresh the tab strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalViewEvent {
    /// The OSC 0/2 window title changed (or was reset via `ResetTitle`). The
    /// panel should re-read the live title via `TerminalSession::title()`.
    TitleChanged,
}

/// View that renders one terminal session (local or ssh — via `dyn TerminalSession`).
pub struct LocalTerminalView {
    pub(crate) session: Entity<Box<dyn TerminalSession>>,
    pub(crate) focus: FocusHandle,
    /// Layout metrics sink (Element writes in prepaint, mouse handler reads).
    pub(crate) metrics: Rc<RefCell<GridMetrics>>,
    /// Scrollbar handle — caches scrollback state, applies drag → session.
    pub(crate) scroll_handle: TerminalScrollHandle,
    /// Whether the cursor is currently shown (blink toggle). True = draw, false = hide.
    pub(crate) cursor_blink_visible: bool,
    /// Bell indicator — true when `\x07` is received, cleared when the user presses a key.
    pub(crate) has_bell: bool,
    /// Pending OSC 9 desktop notifications — drained in `render` via
    /// `window.push_notification` (which needs a `Window`, unavailable in the
    /// async subscribe task).
    pub(crate) pending_notifications: Vec<String>,
    /// Current OSC 9;4 taskbar progress (`None` = no progress / removed).
    pub(crate) progress: Option<TerminalProgress>,
    /// Scrollbar drag state: Some(drag_start_y) while dragging the thumb.
    pub(crate) scrollbar_drag_start: Option<f32>,
    /// Last scroll time — used to auto-hide the scrollbar after 2s.
    pub(crate) last_scroll_time: Option<std::time::Instant>,
    /// URL currently hovered (Ctrl held) — for highlight + click to open URL.
    pub(crate) hovered_url: Option<super::url::DetectedUrl>,
    /// Ctrl currently held — tracked to toggle the cursor style.
    pub(crate) ctrl_held: bool,
    /// Last mouse position — used to re-detect the URL when Ctrl is pressed/released
    /// without a mouse move.
    pub(crate) last_mouse_pos: Option<gpui::Point<gpui::Pixels>>,
    /// Per-line timestamps (gutter). `line_times[j]` = render time of the line whose
    /// **absolute index** (0-based) = `line_time_base + j`. Grow-only: each line is
    /// stamped exactly once and never overwritten (see `update_line_times`).
    pub(crate) line_times: Vec<String>,
    /// Absolute index (0-based) of `line_times[0]` — the oldest line still tracked.
    /// Increases as old lines leave the scrollback.
    pub(crate) line_time_base: usize,
    /// `clear_epoch` from the most recent update — when it changes (screen `clear`),
    /// reset `line_times` so new content is stamped with the current time.
    pub(crate) last_clear_epoch: usize,
    /// Per-row layout cache — skip recompute for non-dirty rows.
    pub(crate) row_cache: Rc<RefCell<RowLayoutCache>>,
    /// Cached gutter width + num_digits — only recompute when num_digits changes.
    /// Avoids calling shape_line every frame → prevents gutter_width oscillation that
    /// causes a resize loop.
    pub(crate) cached_gutter: Rc<RefCell<Option<(gpui::Pixels, usize)>>>,
    /// Last terminal size (rows, cols) — persisted across frames to avoid calling
    /// s.resize() every frame (TerminalElement is recreated each frame).
    pub(crate) last_grid_size: Rc<RefCell<Option<(u16, u16)>>>,
    // ── In-buffer search (Ctrl+F) ──────────────────────────────
    /// Whether the search bar is open.
    pub(crate) search_active: bool,
    /// The search query (kept in sync with the `InputState`).
    pub(crate) search_query: String,
    /// Search options (case-sensitivity, whole-word).
    pub(crate) search_options: SearchOptions,
    /// Matches in grid coordinates (top-to-bottom order).
    pub(crate) search_matches: Vec<SearchMatch>,
    /// Index into `search_matches` of the active (current) match.
    pub(crate) search_active_idx: Option<usize>,
    /// The `InputState` for the search bar input.
    pub(crate) search_input: Option<gpui::Entity<InputState>>,
    /// Split context — set by the owning `TerminalPanel` so this terminal's
    /// context menu can dispatch Split / Close-Space to the right Space. `None`
    /// until the panel wires it up (always set for a live terminal leaf).
    pub(crate) split_ctx: Option<super::space::SplitContext>,
    /// Handle to the event-loop task — stored so it can be cancelled on drop/close.
    pub(crate) event_task: Option<gpui::Task<()>>,
    /// Handle to the cursor-blink task — stored so it can be cancelled on drop/close.
    pub(crate) blink_task: Option<gpui::Task<()>>,
    /// Whether the view is alive (not yet closed). Used to gate the blink task.
    pub(crate) alive: bool,
}

impl Drop for LocalTerminalView {
    fn drop(&mut self) {
        // Cancel tasks by dropping them (GPUI cancels on drop).
        // If shutdown() was already called, these are None — no-op.
        drop(self.event_task.take());
        drop(self.blink_task.take());
    }
}

impl EventEmitter<TerminalViewEvent> for LocalTerminalView {}

impl LocalTerminalView {
    /// Create the view from a session entity. Subscribe to events → re-render task.
    pub fn new(
        session: Entity<Box<dyn TerminalSession>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();

        cx.bind_keys([
            KeyBinding::new("tab", NoAction {}, Some("Terminal")),
            KeyBinding::new("shift-tab", NoAction {}, Some("Terminal")),
        ]);

        let rx = session.read(cx).subscribe();
        let session_for_spawn = session.clone();
        let event_task = cx.spawn(async move |this, cx| {
            while let Ok(ev) = rx.recv().await {
                match ev {
                    SessionEvent::Clipboard(Some(t)) => {
                        let _ = this.update(cx, |_, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(t));
                        });
                    }
                    SessionEvent::Clipboard(None) => {
                        let _ = this.update(cx, |_, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(String::new()));
                        });
                    }
                    SessionEvent::Output => {
                        let s = session_for_spawn.clone();
                        // Coalesce every Output event already queued behind this
                        // one into a single render. `drain_coalesced_events`
                        // merges them via `try_recv`, so no wall-clock sleep is
                        // needed — the previous fixed 1ms timer only added
                        // latency (capping the frame period at 1ms + render time)
                        // without adding coalescing. GPUI merges the resulting
                        // `notify()`s into one paint per frame.
                        Self::drain_coalesced_events(&rx, &this, cx);
                        let _ = this.update(cx, |view, cx| {
                            cx.notify();
                            s.read(cx).scroll_to_bottom();
                            // Stamp at the OUTPUT moment (not just at render): the
                            // subscribe task runs independently of render, so an
                            // inactive tab (not rendering) still updates timestamps to
                            // the time the line was created, instead of bunching them at
                            // the time the tab becomes active again.
                            let info = s.read(cx).terminal_info();
                            view.update_line_times(&info);
                            // New output shifts the alacritty grid coordinate
                            // system, so stored search matches would point at the
                            // wrong rows — refresh them (keeps the active index).
                            view.refresh_search(cx);
                        });
                    }
                    SessionEvent::Bell => {
                        let _ = this.update(cx, |view, cx| {
                            view.has_bell = true;
                            cx.notify();
                        });
                    }
                    SessionEvent::Notification(msg) => {
                        let _ = this.update(cx, |view, cx| {
                            view.pending_notifications.push(msg);
                            cx.notify();
                        });
                    }
                    SessionEvent::ClipboardRead => {
                        let _ = this.update(cx, |view, cx| view.reply_clipboard_read(cx));
                    }
                    SessionEvent::Title(_) => {
                        // OSC 0/2 title changed — notify the containing panel
                        // so its `title()` (which reads the live session title)
                        // re-runs and the tab strip refreshes.
                        let _ = this.update(cx, |_, cx| {
                            cx.emit(TerminalViewEvent::TitleChanged);
                            cx.notify();
                        });
                    }
                    SessionEvent::Progress(p) => {
                        let _ = this.update(cx, |view, cx| {
                            view.set_progress(p);
                            cx.notify();
                        });
                    }
                    _ => {
                        let _ = this.update(cx, |_, cx| cx.notify());
                    }
                }
            }
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
                    view.cursor_blink_visible = !view.cursor_blink_visible;
                    cx.notify();
                    true
                });
                if !continue_blinking.unwrap_or(false) {
                    break;
                }
            }
        });

        focus.focus(window, cx);

        Self {
            session,
            focus,
            metrics: Rc::new(RefCell::new(GridMetrics::default())),
            scroll_handle: TerminalScrollHandle::new(),
            cursor_blink_visible: true,
            has_bell: false,
            pending_notifications: Vec::new(),
            progress: None,
            scrollbar_drag_start: None,
            last_scroll_time: None,
            hovered_url: None,
            ctrl_held: false,
            last_mouse_pos: None,
            line_times: Vec::new(),
            line_time_base: 0,
            last_clear_epoch: 0,
            row_cache: Rc::new(RefCell::new(RowLayoutCache::new())),
            cached_gutter: Rc::new(RefCell::new(None)),
            last_grid_size: Rc::new(RefCell::new(None)),
            search_active: false,
            search_query: String::new(),
            search_options: SearchOptions::default(),
            search_matches: Vec::new(),
            search_active_idx: None,
            search_input: None,
            split_ctx: None,
            event_task: Some(event_task),
            blink_task: Some(blink_task),
            alive: true,
        }
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
        self.session.update(cx, |s, _| s.close());
    }

    /// Return a snapshot of the most recently painted renderer counters.
    #[cfg(any(test, feature = "terminal-diagnostics"))]
    pub fn render_diagnostics(&self) -> super::diagnostics::TerminalRenderDiagnostics {
        self.row_cache.borrow().stats.into()
    }

    /// Update the OSC 9;4 progress state. `Remove` clears it (`None`).
    pub(crate) fn set_progress(&mut self, progress: TerminalProgress) {
        self.progress = match progress {
            TerminalProgress::Remove => None,
            other => Some(other),
        };
    }

    /// Reply to an OSC 52 clipboard-read request (`52;c;?`) with the current
    /// system clipboard content, base64-encoded. Gated behind the
    /// `allow_clipboard_read` setting (default off): reading exposes the local
    /// clipboard to the requesting program — including remote ones over SSH.
    pub(crate) fn reply_clipboard_read(&self, cx: &mut Context<Self>) {
        let allowed = crate::state::TerminalSettings::global(cx)
            .read(cx)
            .allow_clipboard_read;
        if !allowed {
            log::debug!("OSC 52 clipboard read refused (allow_clipboard_read = false)");
            return;
        }
        let text = cx
            .read_from_clipboard()
            .and_then(|c| c.text())
            .unwrap_or_default();
        let reply = format!(
            "\x1b]52;c;{}\x07",
            oneterm_core::terminal::encode_osc52(&text)
        );
        self.session.read(cx).write(reply.as_bytes());
    }

    /// Drain all pending events in the channel — coalesce Output events,
    /// handle Clipboard/Bell/Title immediately.
    pub(crate) fn drain_coalesced_events(
        rx: &Receiver<SessionEvent>,
        this: &gpui::WeakEntity<Self>,
        cx: &mut gpui::AsyncApp,
    ) {
        loop {
            match rx.try_recv() {
                Ok(SessionEvent::Output) => {}
                Ok(SessionEvent::Clipboard(Some(t))) => {
                    let _ = this.update(cx, |_, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(t));
                    });
                }
                Ok(SessionEvent::Clipboard(None)) => {
                    let _ = this.update(cx, |_, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(String::new()));
                    });
                }
                Ok(SessionEvent::Bell) => {
                    let _ = this.update(cx, |view, cx| {
                        view.has_bell = true;
                        cx.notify();
                    });
                }
                Ok(SessionEvent::Notification(msg)) => {
                    let _ = this.update(cx, |view, cx| {
                        view.pending_notifications.push(msg);
                        cx.notify();
                    });
                }
                Ok(SessionEvent::ClipboardRead) => {
                    let _ = this.update(cx, |view, cx| view.reply_clipboard_read(cx));
                }
                Ok(SessionEvent::Title(_)) => {
                    // OSC 0/2 title arrived in the same batch as Output —
                    // emit so the containing panel refreshes its tab title.
                    let _ = this.update(cx, |_, cx| {
                        cx.emit(TerminalViewEvent::TitleChanged);
                        cx.notify();
                    });
                }
                Ok(SessionEvent::Progress(p)) => {
                    let _ = this.update(cx, |view, cx| {
                        view.set_progress(p);
                        cx.notify();
                    });
                }
                Ok(_) => {
                    let _ = this.update(cx, |_, cx| cx.notify());
                }
                Err(_) => break,
            }
        }
    }

    /// Update `line_times` at **render time**, using a **grow-only** model keyed
    /// by each line's absolute index.
    ///
    /// Each line is assigned a timestamp exactly **once** — on the first frame it
    /// appears — and is **never overwritten**. This is the key to resisting ConPTY
    /// repaint / reflow: those operations make `total_lines` (and therefore
    /// `absolute_line_count` via `terminal_info`) temporarily dip. The old code
    /// reacted by clearing + refilling with `now` → every line jumped to the same
    /// time. Here a temporary dip simply means "add nothing", so existing
    /// timestamps are kept.
    ///
    /// `line_times[j]` ↔ the line with absolute index `line_time_base + j`.
    pub(crate) fn update_line_times(&mut self, info: &TerminalInfo) {
        let total = info.total_lines;
        let absolute = info.absolute_line_count;
        let now = chrono::Local::now().format("%H:%M:%S").to_string();

        // ── Reset when the screen is cleared (`clear`/`cls`/RIS) ──
        // `clear` resets the absolute line counter in the event loop → new content
        // REUSES old indices. If we keep the old `line_times`, new lines would hit
        // stale timestamps → "time doesn't change". Clear so new lines are stamped
        // again.
        if info.clear_epoch != self.last_clear_epoch {
            self.last_clear_epoch = info.clear_epoch;
            self.line_times.clear();
            self.line_time_base = absolute.saturating_sub(total);
        }

        // Number of lines that ALREADY HAVE CONTENT (high-water mark).
        //
        // `absolute_line_count` is "inflated" to the bottom of the viewport because
        // `total_lines = history + screen_lines` always includes the EMPTY lines
        // below the cursor (the grid is always `num_lines` tall). If we stamped up
        // to `absolute`, those empty lines would get the current time; when later
        // output overwrites them, they keep the old time → exactly the symptom
        // "a block of lines carries the wrong time".
        //
        // The content mark must match the gutter region actually rendered — i.e. up
        // to the last line **with content** (`last_content_line`), NOT just up to
        // the cursor. For TUI / progress bars that use cursor-up, content is BELOW
        // the cursor; if we stopped stamping at the cursor, those lines would render
        // `[--:--:--]`.
        // Absolute index = absolute − num_lines + row.
        let content_row = info.cursor_line.max(info.last_content_line).max(0) as usize;
        let content_high = absolute
            .saturating_sub(info.num_lines)
            .saturating_add(content_row + 1)
            .min(absolute);

        // Hard reset: only when new content starts BEFORE the oldest tracked line
        // (the absolute counter was fully reset). ConPTY repaint/reflow only
        // fluctuates within existing content, so it does NOT trigger this branch.
        if absolute < self.line_time_base {
            self.line_times.clear();
            self.line_time_base = absolute.saturating_sub(total);
        }
        if self.line_times.is_empty() {
            self.line_time_base = absolute.saturating_sub(total);
        }

        // Stamp the new lines WITH CONTENT (index ≥ covered) with the current render
        // time. Grow-only: a temporary dip → push nothing; empty lines below the
        // cursor are not stamped until the cursor (content) actually reaches them.
        let covered = self.line_time_base + self.line_times.len();
        if content_high > covered {
            let new_lines = content_high - covered;
            self.line_times.reserve(new_lines);
            for _ in 0..new_lines {
                self.line_times.push(now.clone());
            }
        }

        // Drop timestamps of lines that have left the scrollback (front) to bound memory.
        let oldest = absolute.saturating_sub(total);
        if oldest > self.line_time_base {
            let drop = (oldest - self.line_time_base).min(self.line_times.len());
            self.line_times.drain(0..drop);
            self.line_time_base += drop;
        }
    }
}
