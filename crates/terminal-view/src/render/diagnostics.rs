//! Per-frame counters and latency samples for the render engine.
//!
//! `FrameStats` counters are always compiled: a dozen `u32` increments per
//! frame cost nothing and the tests read them. Timing samples and the
//! throttled log line are gated behind `terminal-diagnostics` (or `cfg(test)`).

#[cfg(any(test, feature = "terminal-diagnostics"))]
use std::collections::VecDeque;
#[cfg(feature = "terminal-diagnostics")]
use std::time::{Duration, Instant};

/// What one frame did. Reset at the start of prepaint.
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FrameStats {
    pub snapshot_calls: u32,
    pub rows_total: u32,
    pub rows_candidate: u32,
    pub rows_planned: u32,
    pub shape_calls: u32,
    pub glyph_hits: u32,
    pub url_scans: u32,
    pub quads: u32,
    pub glyphs: u32,
    /// `paint_glyph` failures (missing glyph); counted, never propagated.
    pub glyph_errors: u32,
    /// Sixel images painted this frame (one call per visible image).
    pub images: u32,
    /// Sixel images uploaded to the store this frame.
    pub images_uploaded: u32,
    pub layers: u32,
    pub prepaint_us: u32,
    pub paint_us: u32,
}

/// A ring of the last frame durations, for p95 / p99 in the diagnostics log.
#[cfg(any(test, feature = "terminal-diagnostics"))]
pub(crate) struct LatencySamples {
    samples: VecDeque<u32>,
    sorted: Vec<u32>,
}

#[cfg(any(test, feature = "terminal-diagnostics"))]
impl LatencySamples {
    const CAPACITY: usize = 512;

    pub(crate) fn new() -> Self {
        Self {
            samples: VecDeque::with_capacity(Self::CAPACITY),
            sorted: Vec::with_capacity(Self::CAPACITY),
        }
    }

    pub(crate) fn push(&mut self, micros: u32) {
        if self.samples.len() == Self::CAPACITY {
            self.samples.pop_front();
        }
        self.samples.push_back(micros);
    }

    pub(crate) fn len(&self) -> usize {
        self.samples.len()
    }

    /// The `q`-th percentile (`0.0..=1.0`) of the retained samples, or 0.
    pub(crate) fn percentile(&mut self, q: f32) -> u32 {
        if self.samples.is_empty() {
            return 0;
        }
        self.sorted.clear();
        self.sorted.extend(self.samples.iter().copied());
        self.sorted.sort_unstable();
        let last = self.sorted.len() - 1;
        let index = ((last as f32) * q.clamp(0.0, 1.0)).round() as usize;
        self.sorted[index.min(last)]
    }
}

#[cfg(any(test, feature = "terminal-diagnostics"))]
impl Default for LatencySamples {
    fn default() -> Self {
        Self::new()
    }
}

/// Emits one `log::debug!` line with the last frame's counters at most every
/// five seconds.
#[cfg(feature = "terminal-diagnostics")]
pub(crate) struct DiagnosticsLog {
    last: Option<Instant>,
}

#[cfg(feature = "terminal-diagnostics")]
impl DiagnosticsLog {
    const INTERVAL: Duration = Duration::from_secs(5);

    pub(crate) fn new() -> Self {
        Self { last: None }
    }

    pub(crate) fn maybe_log(&mut self, stats: &FrameStats, latency: &mut LatencySamples) {
        let now = Instant::now();
        if self
            .last
            .is_some_and(|last| now.duration_since(last) < Self::INTERVAL)
        {
            return;
        }
        self.last = Some(now);
        let p95 = latency.percentile(0.95);
        let p99 = latency.percentile(0.99);
        log::debug!(
            "terminal render: rows {}/{} candidate, {} planned, {} shaped, {} glyph hits, \
             {} url scans, {} quads, {} glyphs, {} layers, prepaint {} us, paint {} us, \
             p95 {} us, p99 {} us over {} frames",
            stats.rows_candidate,
            stats.rows_total,
            stats.rows_planned,
            stats.shape_calls,
            stats.glyph_hits,
            stats.url_scans,
            stats.quads,
            stats.glyphs,
            stats.layers,
            stats.prepaint_us,
            stats.paint_us,
            p95,
            p99,
            latency.len(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_percentiles_track_the_ring() {
        let mut l = LatencySamples::new();
        assert_eq!(l.percentile(0.95), 0);
        for v in 1..=100 {
            l.push(v);
        }
        assert_eq!(l.percentile(0.0), 1);
        assert_eq!(l.percentile(1.0), 100);
        assert_eq!(l.percentile(0.95), 95);
        for v in 0..600 {
            l.push(v);
        }
        assert_eq!(l.len(), LatencySamples::CAPACITY);
        assert_eq!(l.percentile(0.0), 600 - LatencySamples::CAPACITY as u32);
    }
}
