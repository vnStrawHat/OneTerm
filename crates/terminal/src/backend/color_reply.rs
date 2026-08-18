//! `ColorQueryReplier` — OSC 10/11/12 (and OSC 4) colour *query* replies.
//!
//! alacritty routes `OSC 1x;?` out as `Event::ColorRequest(index, format)`
//! while the `Term` lock is held. The router enqueues the request here; the
//! pump answers after the parse batch by reading the live colour from the
//! `Term` colour table, falling back to the theme default the UI registered.

use std::sync::PoisonError;

use alacritty_terminal::event::EventListener;
use alacritty_terminal::term::Term;

use crate::osc_color::{
    ColorFormatter, PendingColorQuery, SharedColorQueries, default_color_for_index,
    new_color_queries,
};

use super::DefaultColors;

/// Queue of pending colour queries shared by the router clones and the pump.
#[derive(Clone, Default)]
pub struct ColorQueryReplier {
    pending: SharedColorQueries,
}

impl ColorQueryReplier {
    /// Create an empty queue.
    pub fn new() -> Self {
        Self {
            pending: new_color_queries(),
        }
    }

    /// Enqueue one query (from `Event::ColorRequest`).
    pub fn enqueue(&self, index: usize, format: ColorFormatter) {
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(PendingColorQuery { index, format });
    }

    /// Whether any query is waiting for an answer.
    pub fn has_pending(&self) -> bool {
        !self
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_empty()
    }

    /// Drain every pending query.
    pub fn take(&self) -> Vec<PendingColorQuery> {
        std::mem::take(&mut *self.pending.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Format the reply sequences for `queries`: the live `Term` colour when the
    /// program set one via OSC, otherwise the theme default; queries with no
    /// answer are skipped. The caller holds the `Term` lock.
    pub fn replies<EP: EventListener>(
        term: &Term<EP>,
        defaults: &DefaultColors,
        queries: Vec<PendingColorQuery>,
    ) -> Vec<String> {
        queries
            .into_iter()
            .filter_map(|query| {
                let color = term.colors()[query.index].or_else(|| {
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
}
