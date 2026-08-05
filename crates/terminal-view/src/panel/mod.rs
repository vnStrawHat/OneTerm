//! [`TerminalPanel`] — a Terminal Tab hosting a tree of resizable **Spaces**.
//!
//! A panel used to wrap exactly one `LocalTerminalView`; it now owns a
//! [`SpaceTree`](super::space::SpaceTree) whose leaves are terminals or empty
//! placeholders. A tree with a single leaf renders exactly like the old
//! single-terminal panel. See `docs/terminal-split/`.
//!
//! The [`TerminalPanel`] type and its dock trait impls live in
//! [`terminal_panel`], Space operations in [`ops`], the context-menu action
//! handlers + [`Render`](gpui::Render) impl in [`actions`], and tab-title
//! resolution + the rename dialog in [`title`].

#[cfg(test)]
mod tests;

mod actions;
mod ops;
mod terminal_panel;
mod title;

pub use terminal_panel::TerminalPanel;
