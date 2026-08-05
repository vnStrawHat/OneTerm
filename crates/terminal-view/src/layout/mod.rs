//! Layout state + cache + per-row layout for `TerminalElement`.
//!
//! The original `terminal_element_layout.rs` was split into `terminal/layout/`.

pub(crate) mod cache;
pub(crate) mod row;
pub(crate) mod selection;
pub(crate) mod types;

pub(crate) use cache::{RowCacheFrame, RowCacheStyle, update_row_cache};
pub(crate) use selection::layout_selection;
pub(crate) use types::{
    BatchedTextRun, CursorPaint, GridMetrics, GutterEntry, LayoutPoint, LayoutRect, LayoutState,
    RenderStyleKey, RowLayoutCache,
};
