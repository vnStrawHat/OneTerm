//! `SessionEventSink` — delivery policy for `SessionEvent`s.
//!
//! `SessionEvent::Output` is a coalescible repaint hint: when the bounded queue
//! is full it is dropped and counted, because the UI will repaint on the next
//! one. Every other event is reliable and applies backpressure.
//!
//! **The deferred tier is gone (`US-0082`).** It existed because the engine
//! called back *during* parsing with the terminal lock held, so a blocking send
//! could deadlock against the UI thread that needed the same lock to drain the
//! queue (CORR-01). Events are values now: [`super::OscRouter::drain`] collects
//! them and [`super::TerminalPump`] sends them after the guard is dropped, so
//! the sink can simply block. Deleted with the tier: `deferred_reliable`,
//! `flush_reliable[_blocking]`, `forward_lifecycle*` and
//! `has_deferred_reliable`.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use async_channel::{Sender, TrySendError};
use log::warn;

use crate::session::SessionEvent;

/// Snapshot of event-queue failures (diagnostics and tests).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EventQueueDiagnostics {
    /// Repaint hints coalesced because the event queue was full.
    pub event_full: u64,
    /// Events lost because the event queue was closed.
    pub event_closed: u64,
}

#[derive(Default)]
struct EventQueueCounters {
    event_full: AtomicU64,
    event_closed: AtomicU64,
}

/// Bounded, policy-aware sender of `SessionEvent`s to the UI. Clone-friendly:
/// all clones share the counters.
#[derive(Clone)]
pub struct SessionEventSink {
    event_tx: Sender<SessionEvent>,
    counters: Arc<EventQueueCounters>,
}

impl SessionEventSink {
    /// Wrap the UI-facing event sender.
    pub fn new(event_tx: Sender<SessionEvent>) -> Self {
        Self {
            event_tx,
            counters: Arc::new(EventQueueCounters::default()),
        }
    }

    /// Return the event-queue failure counters.
    pub fn diagnostics(&self) -> EventQueueDiagnostics {
        EventQueueDiagnostics {
            event_full: self.counters.event_full.load(Ordering::Relaxed),
            event_closed: self.counters.event_closed.load(Ordering::Relaxed),
        }
    }

    /// Post the coalescible repaint hint. Never blocks and never waits: a hint
    /// that does not fit is dropped, because the next one carries the same
    /// information.
    pub fn post_repaint(&self) {
        if let Err(error) = self.event_tx.try_send(SessionEvent::Output) {
            match error {
                TrySendError::Full(_) => {
                    self.counters.event_full.fetch_add(1, Ordering::Relaxed);
                    log::debug!("SessionEventSink: coalesced repaint event");
                }
                error @ TrySendError::Closed(_) => self.record_closed(error),
            }
        }
    }

    /// Deliver a reliable event, waiting for the UI to make room.
    ///
    /// Call **without** the terminal lock held — which is where the pump calls
    /// it, and the reason the deferred tier could be deleted.
    pub fn send_blocking(&self, event: SessionEvent) {
        debug_assert!(!matches!(event, SessionEvent::Output));
        if let Err(error) = self.event_tx.send_blocking(event) {
            self.record_closed(error);
        }
    }

    /// Async variant of [`Self::send_blocking`] for tokio pumps.
    pub async fn send(&self, event: SessionEvent) {
        debug_assert!(!matches!(event, SessionEvent::Output));
        if let Err(error) = self.event_tx.send(event).await {
            self.record_closed(error);
        }
    }

    fn record_closed(&self, error: impl std::fmt::Debug) {
        self.counters.event_closed.fetch_add(1, Ordering::Relaxed);
        warn!("SessionEventSink: event lost because the channel is closed: {error:?}");
    }
}
