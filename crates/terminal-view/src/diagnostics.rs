//! Opt-in terminal renderer diagnostics.
//!
//! This module is compiled only for tests or with the `terminal-diagnostics`
//! feature. Normal builds keep collecting the existing internal frame counters,
//! but do not expose or periodically log them.

use crate::layout::types::FrameStats;

/// Snapshot of renderer work performed by the most recently painted frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerminalRenderDiagnostics {
    /// Number of visible terminal rows.
    pub total_lines: usize,
    /// Number of rows identified as dirty before hash validation.
    pub dirty_lines: usize,
    /// Number of rows whose layout artifacts were rebuilt.
    pub row_layout_calls: usize,
    /// Number of GPUI text-shaping calls.
    pub shape_line_calls: usize,
    /// Number of terminal-cell hash calls.
    pub hash_calls: usize,
    /// Number of paint quads emitted.
    pub paint_quad_calls: usize,
    /// Number of background rectangles painted.
    pub background_rects: usize,
    /// Number of shaped text runs painted.
    pub text_run_paints: usize,
    /// Number of known allocation-capable temporary buffers created by the
    /// terminal row-cache pass. This is a deterministic lower-bound proxy, not
    /// a process-wide allocator count.
    pub allocation_buffer_sites: usize,
    /// Number of completed paint frames.
    pub frame_count: u64,
    /// Wall-clock duration of the complete prepaint pass, in microseconds.
    pub prepaint_us: u128,
    /// Wall-clock duration spent in `terminal_info`, including backend locking,
    /// in microseconds.
    pub terminal_info_us: u128,
    /// Wall-clock duration spent acquiring and cloning the render snapshot,
    /// including backend locking, in microseconds.
    pub snapshot_us: u128,
    /// Wall-clock duration of the paint pass, in microseconds.
    pub paint_us: u128,
}

impl From<FrameStats> for TerminalRenderDiagnostics {
    fn from(stats: FrameStats) -> Self {
        Self {
            total_lines: stats.total_lines,
            dirty_lines: stats.dirty_lines,
            row_layout_calls: stats.row_layout_calls,
            shape_line_calls: stats.shape_line_calls,
            hash_calls: stats.hash_calls,
            paint_quad_calls: stats.paint_quad_calls,
            background_rects: stats.bg_rect_count,
            text_run_paints: stats.text_run_paints,
            allocation_buffer_sites: stats.allocation_buffer_sites,
            frame_count: stats.frame_count,
            prepaint_us: stats.prepaint_us,
            terminal_info_us: stats.terminal_info_us,
            snapshot_us: stats.snapshot_us,
            paint_us: stats.paint_us,
        }
    }
}
