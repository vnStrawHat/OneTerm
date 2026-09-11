//! Rendering internals of the terminal view.
//!
//! Bottom-up: `frame` (the only alacritty-typed file) → `metrics` → `glyphs` →
//! `row_plan` → `plan_cache` → `overlay` / `cursor` → `state` → `element`.
//! `shapes` is the pure geometry of the code points drawn as quads/paths.

pub(crate) mod cursor;
pub(crate) mod diagnostics;
pub(crate) mod element;
pub(crate) mod frame;
pub(crate) mod glyphs;
pub(crate) mod graphics;
pub(crate) mod metrics;
pub(crate) mod overlay;
pub(crate) mod plan_cache;
pub(crate) mod row_plan;
pub(crate) mod shapes;
pub(crate) mod state;
