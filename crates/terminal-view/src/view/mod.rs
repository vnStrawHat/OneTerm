//! `LocalTerminalView` — the GPUI view that renders one terminal session
//! (local or ssh), split across submodules by concern.
//!
//! - [`local_view`] — the view type, event loop, lifecycle
//! - [`render`] — the `Render`/`Focusable` impls + overlays
//! - [`search`], [`scrollbar`], [`gutter_timestamps`], [`completion`] — the
//!   cohesive sub-states the view owns, each with its own methods
//! - [`grid`], [`key`], [`ime`] — coordinate mapping, key mapping, IME

mod local_view;

pub(crate) mod completion;
pub(crate) mod grid;
pub(crate) mod gutter_timestamps;
mod ime;
pub(crate) mod key;
mod render;
pub(crate) mod scrollbar;
pub(crate) mod search;
#[cfg(test)]
mod tests;

pub(crate) use local_view::{LocalTerminalView, TerminalViewEvent};
pub(crate) use search::SearchHighlight;
