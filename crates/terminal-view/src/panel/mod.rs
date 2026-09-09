//! [`TerminalPanel`] — a Terminal Tab hosting a tree of resizable **Spaces**.
//!
//! A tab owns a [`SpaceTree`](crate::space::SpaceTree) whose leaves are
//! terminals or empty placeholders; a tree with a single leaf renders exactly
//! like a plain single-terminal panel.
//!
//! - [`terminal_panel`] — the type, its construction, accessors and dock traits
//! - [`spaces`] — split / close / fill / drag-drop over the Space tree
//! - [`duplicate`] — duplicating a session into a tab, Space, or split
//! - [`tab_title`] — tab label resolution, the rename dialog, the tab strip
//!
//! The current owning design is
//! `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/high-level-design.md`.

mod duplicate;
mod spaces;
mod tab_title;
mod terminal_panel;
#[cfg(test)]
mod tests;

pub(crate) use duplicate::DuplicateDestination;
pub use terminal_panel::{PanelSpec, TerminalPanel};
