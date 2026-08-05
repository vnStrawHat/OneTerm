//! `LocalTerminalView` — the GPUI view that renders one terminal session
//! (local or ssh), split across submodules by concern.
//!
//! - [`local_view`] — the view type, event loop, lifecycle, gutter timestamps
//! - [`completion`] — auto-completion wiring (feeds the gpui-free controller)
//! - [`cursor`], [`font`], [`grid`], [`key`] — small per-concern helpers
//! - [`scrollbar_overlay`] — the custom scrollbar overlay

mod local_view;

pub(crate) mod completion;
pub(crate) mod cursor;
pub(crate) mod font;
pub(crate) mod grid;
pub(crate) mod key;
pub(crate) mod scrollbar_overlay;
#[cfg(test)]
mod tests;

pub use local_view::{LocalTerminalView, TerminalViewEvent};
