//! `TerminalPump` — the parse-batch driver shared by the local event loop and
//! the SSH task.
//!
//! A backend read loop owns one pump and, per chunk of transport bytes:
//!
//! 1. locks the `Term` and calls [`TerminalPump::advance`] (parse + line
//!    accounting; router callbacks run inside, never blocking);
//! 2. answers OSC colour queries collected during the parse — either through
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

use std::sync::Arc;

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;
use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};

use crate::osc_color::PendingColorQuery;
use crate::session::SessionEvent;

use super::{ColorQueryReplier, LineAccounting, OscRouter, PtyTransport, SharedState};

/// Grid dimensions for `Term::new` / `Term::resize`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridSize {
    /// Columns.
    pub cols: usize,
    /// Visible lines.
    pub lines: usize,
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.lines
    }
    fn screen_lines(&self) -> usize {
        self.lines
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// Parse-batch driver for one session (see module docs).
pub struct TerminalPump<T: PtyTransport> {
    router: OscRouter<T>,
    processor: Processor<StdSyncHandler>,
    lines: LineAccounting,
}

impl<T: PtyTransport> TerminalPump<T> {
    /// Create a pump around the router that is also installed in the `Term`.
    pub fn new(router: OscRouter<T>) -> Self {
        Self {
            router,
            processor: Processor::new(),
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

    /// Feed one chunk into `term`. The caller holds the `Term` lock.
    pub fn advance(&mut self, term: &mut Term<OscRouter<T>>, bytes: &[u8]) {
        self.processor.advance(term, bytes);
        self.lines.observe(term, bytes);
    }

    /// Whether colour queries are waiting for an answer.
    pub fn has_color_queries(&self) -> bool {
        self.router.color_queries().has_pending()
    }

    /// Drain the colour queries collected during `advance`.
    pub fn take_color_queries(&self) -> Vec<PendingColorQuery> {
        self.router.take_color_queries()
    }

    /// Format replies for `queries` against the live `Term` colours (caller
    /// holds the lock) with the theme defaults as fallback.
    pub fn color_replies(
        &self,
        term: &Term<OscRouter<T>>,
        queries: Vec<PendingColorQuery>,
    ) -> Vec<String> {
        let defaults = self.state().default_colors();
        ColorQueryReplier::replies(term, &defaults, queries)
    }

    /// Send colour replies back through the transport (no lock needed).
    pub fn write_color_replies(&self, replies: Vec<String>) {
        for reply in replies {
            if let Err(error) = self.router.transport().pty_write(reply.as_bytes()) {
                log::warn!("TerminalPump: OSC colour reply delivery failed: {error}");
            }
        }
    }

    /// Lock `term`, feed `bytes`, answer colour queries, unlock, and write the
    /// replies. Call `finish_batch*` afterwards.
    pub fn process_chunk(&mut self, term: &Arc<FairMutex<Term<OscRouter<T>>>>, bytes: &[u8]) {
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
