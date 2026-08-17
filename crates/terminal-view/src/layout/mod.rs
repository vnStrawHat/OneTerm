//! Layout state + cache + per-row layout for `TerminalElement`.

pub(crate) mod cache;
pub(crate) mod row;
pub(crate) mod selection;
pub(crate) mod types;

pub(crate) use cache::{RowCacheFrame, RowCacheStyle, update_row_cache};
pub(crate) use row::is_blank;
pub(crate) use selection::layout_selection;
pub(crate) use types::{
    CursorPaint, GridMetrics, GutterCache, GutterEntry, LayoutPoint, LayoutRect, LayoutState,
    RenderCache, RenderStyleKey,
};
