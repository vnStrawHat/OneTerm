//! `SessionEventSink` — delivery policy for `SessionEvent`s.
//!
//! Repaint hints (`SessionEvent::Output`) are coalescible: when the bounded
//! queue is full they are dropped, the UI will repaint on the next one. Every
//! other event is reliable — never dropped — but `forward` runs from `Term`
//! callbacks with the `Term` lock held and the UI thread needs that same lock
//! to drain the queue, so it must never block (CORR-01). Reliable events that
//! do not fit are kept in a FIFO and delivered by the pump's `flush_reliable`
//! after the parse batch, once the lock is released; a slow consumer applies
//! backpressure to the pump only *between* batches.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

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
/// all clones share the deferred queue and the counters.
#[derive(Clone)]
pub struct SessionEventSink {
    event_tx: Sender<SessionEvent>,
    /// Reliable events that did not fit in the event queue. FIFO order among
    /// reliable events is preserved across deferrals.
    deferred_reliable: Arc<Mutex<VecDeque<SessionEvent>>>,
    counters: Arc<EventQueueCounters>,
}

impl SessionEventSink {
    /// Wrap the UI-facing event sender.
    pub fn new(event_tx: Sender<SessionEvent>) -> Self {
        Self {
            event_tx,
            deferred_reliable: Arc::new(Mutex::new(VecDeque::new())),
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

    fn record_failure<T>(&self, error: &TrySendError<T>) {
        let counter = match error {
            TrySendError::Full(_) => &self.counters.event_full,
            TrySendError::Closed(_) => &self.counters.event_closed,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    /// Forward a session event according to its delivery policy. Never blocks —
    /// safe to call from `Term` callbacks with the `Term` lock held. Deferred
    /// reliable events are delivered by [`Self::flush_reliable_blocking`] /
    /// [`Self::flush_reliable`], which the pump calls after every parse batch.
    pub fn forward(&self, ev: SessionEvent) {
        // `Output` is a coalescible repaint hint; every other event is reliable.
        if matches!(ev, SessionEvent::Output) {
            if let Err(error) = self.event_tx.try_send(ev) {
                self.record_failure(&error);
                match error {
                    TrySendError::Full(_) => {
                        log::debug!("SessionEventSink: coalesced repaint event");
                    }
                    TrySendError::Closed(_) => {
                        warn!("SessionEventSink: event channel is closed");
                    }
                }
            }
            return;
        }

        let mut deferred = self
            .deferred_reliable
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // Keep FIFO order: once something is deferred, everything after
        // it queues behind it until the next flush.
        if !deferred.is_empty() {
            deferred.push_back(ev);
            return;
        }
        match self.event_tx.try_send(ev) {
            Ok(()) => {}
            Err(TrySendError::Full(ev)) => deferred.push_back(ev),
            Err(error @ TrySendError::Closed(_)) => {
                self.record_failure(&error);
                warn!("SessionEventSink: reliable event lost because channel is closed: {error:?}");
            }
        }
    }

    /// Whether reliable events are waiting for a flush.
    pub fn has_deferred_reliable(&self) -> bool {
        !self
            .deferred_reliable
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_empty()
    }

    fn pop_deferred_reliable(&self) -> Option<SessionEvent> {
        self.deferred_reliable
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
    }

    fn record_closed(&self, error: impl std::fmt::Debug) {
        self.counters.event_closed.fetch_add(1, Ordering::Relaxed);
        warn!("SessionEventSink: reliable event lost because channel is closed: {error:?}");
    }

    /// Deliver deferred reliable events, blocking until the UI makes room.
    /// Must be called **without** the `Term` lock held (thread-based pumps
    /// call it after each parse batch and before lifecycle events).
    pub fn flush_reliable_blocking(&self) {
        while let Some(ev) = self.pop_deferred_reliable() {
            if let Err(error) = self.event_tx.send_blocking(ev) {
                self.record_closed(error);
            }
        }
    }

    /// Async variant of [`Self::flush_reliable_blocking`] for tokio pumps.
    pub async fn flush_reliable(&self) {
        while let Some(ev) = self.pop_deferred_reliable() {
            if let Err(error) = self.event_tx.send(ev).await {
                self.record_closed(error);
            }
        }
    }

    /// Forward a lifecycle event (`Exited`/`Closed`) and flush every deferred
    /// reliable event so the transition reaches the UI in order. Call from
    /// the pump only, without the `Term` lock held.
    pub fn forward_lifecycle_blocking(&self, ev: SessionEvent) {
        debug_assert!(!matches!(ev, SessionEvent::Output));
        self.forward(ev);
        self.flush_reliable_blocking();
    }

    /// Async variant of [`Self::forward_lifecycle_blocking`].
    pub async fn forward_lifecycle(&self, ev: SessionEvent) {
        debug_assert!(!matches!(ev, SessionEvent::Output));
        self.forward(ev);
        self.flush_reliable().await;
    }
}
