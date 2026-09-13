//! `TerminalPump` — the parse-batch driver shared by the local event loop and
//! the SSH task.
//!
//! A backend read loop owns one pump and, per chunk of transport bytes:
//!
//! 1. locks the engine and calls [`TerminalPump::advance`], which feeds the
//!    chunk through `Terminal::feed` and drains the returned `EventBatch`
//!    through the router — reply bytes to the transport, colour queries queued,
//!    the state caches updated, and every UI-facing event **collected** rather
//!    than sent;
//! 2. answers OSC colour queries collected during the drain — either through
//!    [`TerminalPump::process_chunk`] (lock managed here) or the split
//!    `take_color_queries` / `color_replies` / `write_color_replies` steps when
//!    the loop manages the guard itself;
//! 3. releases the lock and calls [`TerminalPump::finish_batch_blocking`] /
//!    [`TerminalPump::finish_batch`], which publishes the line count, **sends**
//!    the collected events (waiting for the UI if needed) and posts the repaint
//!    hint — so events emitted during the batch are seen before that batch's
//!    `Output`.
//!
//! Lifecycle: `publish_exit*` / `publish_closed*` record the state and forward
//! `Exited` / `Closed` after everything queued before them.
//!
//! Two properties are the contract, and both have tests:
//!
//! * **Replies leave first** (R-37), inside `advance`, before anything else in
//!   the batch and before any yield: conhost blocks for up to a second waiting
//!   for the DA1 answer at session start, which is exactly when a burst of
//!   output is arriving.
//! * **One repaint hint per chunk, last.** The engine appends a `Repaint` to
//!   every batch that dispatched something; the router drops it, because the
//!   hint has one owner — `finish_batch*`, after the reliable events.

use std::sync::{Mutex, PoisonError};
use std::time::Instant;

use oneterm_vt::{EventBatch, Terminal};

use crate::handle::SharedTerminal;
use crate::osc_color::{PendingColorQuery, default_color_for_key};
use crate::session::SessionEvent;

use super::{OscRouter, PtyTransport, SharedState};

/// Grid dimensions for [`crate::handle::new_shared_terminal`].
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
    /// UI-facing events collected under the engine lock, sent once it is
    /// released. **Pump-owned**, lent to [`OscRouter::drain`] per `advance`.
    ///
    /// Several `advance` calls can share one batch boundary — the local read
    /// loop feeds until the pipe is empty before it unlocks — so the events
    /// accumulate here rather than in the `EventBatch`, which `feed` clears.
    /// Behind a mutex because the backends hold the pump by `&` at the
    /// lifecycle call sites (`publish_child_exit`), which `US-0083` owns.
    pending: Mutex<Vec<SessionEvent>>,
    /// The gutter's absolute line number, published once per batch.
    absolute_lines: usize,
}

impl<T: PtyTransport> TerminalPump<T> {
    /// Create a pump around the router the session also holds.
    pub fn new(router: OscRouter<T>) -> Self {
        Self {
            router,
            batch: EventBatch::new(),
            pending: Mutex::new(Vec::new()),
            absolute_lines: 0,
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

    /// The gutter's absolute line number: lines output since spawn, floored at
    /// the rows the grid currently holds.
    ///
    /// `Terminal::lines_produced()` counts **output lines** — line feeds, never
    /// implicit wraps and never rows a reflow created (R-05) — which is the
    /// number `LineAccounting`'s three-branch heuristic over `total_lines`
    /// approximated, without its scan of every chunk for `\n` under the lock and
    /// without its reset on a clear. The floor is load-bearing rather than
    /// cosmetic: the gutter labels display row `i` with
    /// `absolute - display_offset - rows + i`, so a count below the viewport
    /// height would number the top rows from below zero.
    pub fn absolute_line_count(&self) -> usize {
        self.absolute_lines
    }

    /// Feed one chunk into the engine. The caller holds the lock.
    pub fn advance(&mut self, term: &mut Terminal, bytes: &[u8]) {
        self.router.logging().process(bytes);
        term.feed(bytes, &mut self.batch, Instant::now());
        self.router.drain(&self.batch, &mut self.lock_pending());
        let screen = term.screen();
        let rows = screen.history_len() as usize + usize::from(screen.rows());
        self.absolute_lines = (term.lines_produced() as usize).max(rows);
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
    pub fn color_replies(&self, term: &Terminal, queries: Vec<PendingColorQuery>) -> Vec<String> {
        let defaults = self.state().default_colors();
        // The live override when the program set one via OSC, otherwise the
        // theme default; queries with no answer are skipped.
        queries
            .into_iter()
            .filter_map(|query| {
                let color = term
                    .color(query.key)
                    .or_else(|| default_color_for_key(query.key, &defaults));
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
        self.state()
            .set_absolute_line_count(self.absolute_line_count());
    }

    /// End a parse batch (lock released): publish the line count, deliver the
    /// events collected during the batch (blocking on UI backpressure), then
    /// post the repaint hint when `repaint` is set.
    pub fn finish_batch_blocking(&self, repaint: bool) {
        self.publish_line_count();
        self.flush_blocking();
        if repaint {
            self.router.events().post_repaint();
        }
    }

    /// Async variant of [`Self::finish_batch_blocking`] for tokio pumps.
    pub async fn finish_batch(&self, repaint: bool) {
        self.publish_line_count();
        self.flush().await;
        if repaint {
            self.router.events().post_repaint();
        }
    }

    /// Record process exit and forward `Exited(code)` in order.
    pub fn publish_exit_blocking(&self, code: Option<i32>) {
        self.state().record_exit(code);
        self.flush_blocking();
        self.router
            .events()
            .send_blocking(SessionEvent::Exited(code));
    }

    /// Async variant of [`Self::publish_exit_blocking`].
    pub async fn publish_exit(&self, code: Option<i32>) {
        self.state().record_exit(code);
        self.flush().await;
        self.router.events().send(SessionEvent::Exited(code)).await;
    }

    /// Mark the session dead and forward `Closed` in order.
    pub fn publish_closed_blocking(&self) {
        self.state().set_alive(false);
        self.flush_blocking();
        self.router.events().send_blocking(SessionEvent::Closed);
    }

    /// Async variant of [`Self::publish_closed_blocking`].
    pub async fn publish_closed(&self) {
        self.state().set_alive(false);
        self.flush().await;
        self.router.events().send(SessionEvent::Closed).await;
    }

    /// Deliver everything the batch collected, in order, waiting for the UI.
    /// Must run **without** the engine lock held.
    fn flush_blocking(&self) {
        for event in self.take_pending() {
            self.router.events().send_blocking(event);
        }
    }

    /// Async variant of [`Self::flush_blocking`].
    async fn flush(&self) {
        for event in self.take_pending() {
            self.router.events().send(event).await;
        }
    }

    fn lock_pending(&self) -> std::sync::MutexGuard<'_, Vec<SessionEvent>> {
        self.pending.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn take_pending(&self) -> Vec<SessionEvent> {
        std::mem::take(&mut *self.lock_pending())
    }
}
