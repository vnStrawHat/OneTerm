//! `TerminalPump` — the parse-batch driver shared by the local event loop and
//! the SSH task.
//!
//! A backend read loop owns one pump and, per chunk of transport bytes:
//!
//! 1. locks the engine and calls [`TerminalPump::advance`], which feeds the
//!    chunk through `Terminal::feed` and drains the returned [`EventBatch`]
//!    through the router (never blocking — see [`super::SessionEventSink`]);
//! 2. answers OSC colour queries collected during the drain — either through
//!    [`TerminalPump::process_chunk`] (lock managed here) or the split
//!    `take_color_queries` / `color_replies` / `write_color_replies` steps when
//!    the loop manages the guard itself;
//! 3. releases the lock and calls [`TerminalPump::finish_batch_blocking`] /
//!    [`TerminalPump::finish_batch`], which publishes the line count, flushes
//!    deferred reliable events (waiting for the UI if needed) and posts the
//!    repaint hint — so reliable events emitted during the batch are seen
//!    before that batch's `Output`.
//!
//! Lifecycle: `publish_exit*` / `publish_closed*` record the state and forward
//! `Exited` / `Closed` after flushing everything queued before them.
//!
//! Since `US-0081` the engine returns events as **values** instead of calling
//! back, so `advance` decides when each one is routed. The order is the design's
//! (`damage-and-render-state.md` § "Fairness and reply latency", R-37):
//! `VtEvent::Reply` bytes reach the transport **first**, before anything else in
//! the batch, because conhost blocks for up to a second waiting for the DA1
//! answer at session start — which is exactly when a burst is arriving.

use std::time::Instant;

use oneterm_vt::{ColorKey, EventBatch};

use crate::engine::{Engine, SharedTerminal};
use crate::osc_color::{PendingColorQuery, default_color_for_index};
use crate::session::SessionEvent;

use super::{LineAccounting, OscRouter, PtyTransport, SharedState};

/// Grid dimensions for [`crate::engine::new_shared_terminal`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridSize {
    /// Columns.
    pub cols: usize,
    /// Visible lines.
    pub lines: usize,
}

/// Parse-batch driver for one session (see module docs).
pub struct TerminalPump<T: PtyTransport> {
    router: OscRouter<T>,
    /// Reused across chunks: the arena and the event vector grow to a
    /// high-water mark and stay, which is what makes the steady state
    /// allocation-free.
    batch: EventBatch,
    lines: LineAccounting,
}

impl<T: PtyTransport> TerminalPump<T> {
    /// Create a pump around the router the session also holds.
    pub fn new(router: OscRouter<T>) -> Self {
        Self {
            router,
            batch: EventBatch::new(),
            lines: LineAccounting::new(),
        }
    }

    /// The router (transport, sink, state).
    pub fn router(&self) -> &OscRouter<T> {
        &self.router
    }

    /// The shared state cache.
    pub fn state(&self) -> &SharedState {
        self.router.state()
    }

    /// Absolute lines output so far (see [`LineAccounting`]).
    pub fn absolute_line_count(&self) -> usize {
        self.lines.absolute()
    }

    /// Feed one chunk into the engine. The caller holds the lock.
    pub fn advance(&mut self, engine: &mut Engine, bytes: &[u8]) {
        self.router.logging().process(bytes);
        engine.feed(bytes, &mut self.batch, Instant::now());
        self.router.drain(&self.batch);
        let screen = engine.screen();
        self.lines.observe(
            screen.history_len() as usize + usize::from(screen.rows()),
            usize::from(screen.rows()),
            bytes,
        );
    }

    /// Whether colour queries are waiting for an answer.
    pub fn has_color_queries(&self) -> bool {
        self.router.has_color_queries()
    }

    /// Drain the colour queries collected during `advance`.
    pub fn take_color_queries(&self) -> Vec<PendingColorQuery> {
        self.router.take_color_queries()
    }

    /// Format replies for `queries` against the live engine colours (caller
    /// holds the lock) with the theme defaults as fallback.
    pub fn color_replies(&self, engine: &Engine, queries: Vec<PendingColorQuery>) -> Vec<String> {
        let defaults = self.state().default_colors();
        // The live override when the program set one via OSC, otherwise the
        // theme default; queries with no answer are skipped.
        queries
            .into_iter()
            .filter_map(|query| {
                let color = ColorKey::from_index(query.index)
                    .and_then(|key| engine.color(key))
                    .map(crate::engine_shim::legacy_rgb)
                    .or_else(|| {
                        default_color_for_index(
                            query.index,
                            defaults.foreground,
                            defaults.background,
                            defaults.cursor,
                            defaults.ansi.as_ref(),
                        )
                    });
                color.map(|color| (query.format)(color))
            })
            .collect()
    }

    /// Send colour replies back through the transport (no lock needed).
    pub fn write_color_replies(&self, replies: Vec<String>) {
        for reply in replies {
            if let Err(error) = self.router.transport().pty_write(reply.as_bytes()) {
                log::warn!("TerminalPump: OSC colour reply delivery failed: {error}");
            }
        }
    }

    /// Lock the engine, feed `bytes`, answer colour queries, unlock, and write
    /// the replies. Call `finish_batch*` afterwards.
    pub fn process_chunk(&mut self, term: &SharedTerminal, bytes: &[u8]) {
        let replies = {
            let mut guard = term.lock();
            self.advance(&mut guard, bytes);
            let queries = self.take_color_queries();
            if queries.is_empty() {
                Vec::new()
            } else {
                self.color_replies(&guard, queries)
            }
        };
        self.write_color_replies(replies);
    }

    /// Publish the absolute line count to the shared state.
    pub fn publish_line_count(&self) {
        self.state().set_absolute_line_count(self.lines.absolute());
    }

    /// End a parse batch (lock released): publish the line count, flush
    /// deferred reliable events (blocking on UI backpressure), then post the
    /// repaint hint when `repaint` is set.
    pub fn finish_batch_blocking(&self, repaint: bool) {
        self.publish_line_count();
        self.router.events().flush_reliable_blocking();
        if repaint {
            self.router.forward(SessionEvent::Output);
        }
    }

    /// Async variant of [`Self::finish_batch_blocking`] for tokio pumps.
    pub async fn finish_batch(&self, repaint: bool) {
        self.publish_line_count();
        self.router.events().flush_reliable().await;
        if repaint {
            self.router.forward(SessionEvent::Output);
        }
    }

    /// Record process exit and forward `Exited(code)` in order.
    pub fn publish_exit_blocking(&self, code: Option<i32>) {
        self.state().record_exit(code);
        self.router
            .events()
            .forward_lifecycle_blocking(SessionEvent::Exited(code));
    }

    /// Async variant of [`Self::publish_exit_blocking`].
    pub async fn publish_exit(&self, code: Option<i32>) {
        self.state().record_exit(code);
        self.router
            .events()
            .forward_lifecycle(SessionEvent::Exited(code))
            .await;
    }

    /// Mark the session dead and forward `Closed` in order.
    pub fn publish_closed_blocking(&self) {
        self.state().set_alive(false);
        self.router
            .events()
            .forward_lifecycle_blocking(SessionEvent::Closed);
    }

    /// Async variant of [`Self::publish_closed_blocking`].
    pub async fn publish_closed(&self) {
        self.state().set_alive(false);
        self.router
            .events()
            .forward_lifecycle(SessionEvent::Closed)
            .await;
    }
}
