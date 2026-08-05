//! Terminal-view auto-completion: the gpui-free [`CompletionController`] state
//! machine plus the [`overlay`] that renders its suggestions.
//!
//! History lives in the process-global `Entity<CompletionHistory>` (docs 01 §4);
//! the controller receives a `&`/`&mut CompletionHistory` when it needs it so its
//! logic stays free of gpui.

mod controller;
pub mod overlay;

pub use controller::CompletionController;

#[cfg(test)]
mod tests;
