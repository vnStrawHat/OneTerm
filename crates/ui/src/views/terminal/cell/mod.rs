//! Per-cell rendering helpers for `TerminalElement`.
//!
//! The original `terminal_element_cell.rs` module has been split into submodules.

pub(crate) mod batch;
pub(crate) mod blank;
pub(crate) mod color;
pub(crate) mod hash;
pub(crate) mod style;

pub(crate) use blank::is_blank;
pub(crate) use color::cell_colors;
pub(crate) use hash::line_hash;
pub(crate) use style::cell_style;
