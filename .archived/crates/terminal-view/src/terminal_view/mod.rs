//! [`TerminalView`] — the GPUI entity that shows one terminal session (local
//! or SSH) through the `render/` engine and the `input/` decision layer.
//!
//! - [`view`] — the entity, its dependencies, the events pump and lifecycle
//! - [`render`] — `Render`/`Focusable`: inputs refresh, wrapper div, overlays
//! - [`input`] — the wrapper's key / mouse / wheel listeners
//! - [`ime`] — `EntityInputHandler`
//! - [`search`], [`scrollbar`], [`gutter_timestamps`], [`completion`] — the
//!   sub-states the view owns

mod completion;
mod gutter_timestamps;
mod ime;
mod input;
mod render;
mod scrollbar;
mod search;
mod view;

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod view_tests;

pub(crate) use view::{BroadcastOrigin, TerminalDeps, TerminalView, TerminalViewEvent};
