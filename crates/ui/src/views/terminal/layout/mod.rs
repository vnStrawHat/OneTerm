//! Layout state + cache + per-row layout cho `TerminalElement`.
//!
//! File gốc `terminal_element_layout.rs` đã được tách thành `terminal/layout/`.

pub(crate) mod cache;
pub(crate) mod row;
pub(crate) mod selection;
pub(crate) mod types;

pub(crate) use cache::update_row_cache;
pub(crate) use selection::{build_selection_set, layout_selection};
pub(crate) use types::{
    BatchedTextRun, CursorPaint, GridMetrics, GutterEntry, LayoutPoint, LayoutState, RowLayoutCache,
};
