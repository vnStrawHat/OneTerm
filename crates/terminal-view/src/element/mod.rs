//! `TerminalElement` — the custom `gpui::Element` that paints the terminal grid,
//! split across submodules by render phase.
//!
//! - [`terminal_element`] — the element type + `Element`/`IntoElement` impls
//! - [`prepaint`] — compute layout state
//! - [`paint`] — draw the grid
//! - [`measure`] — measure font / cell metrics
//! - [`gutter`] — compute gutter width / entries

pub(crate) mod gutter;
pub(crate) mod measure;
pub(crate) mod paint;
pub(crate) mod prepaint;
mod terminal_element;

pub(crate) use super::layout::{GridMetrics, RowLayoutCache};
pub(crate) use terminal_element::TerminalElement;
