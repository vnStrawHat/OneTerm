//! Semantic highlighting — per-view [`SemanticOverlay`] + color bridge.
//!
//! The overlay produces per-cell `Class` for the visible viewport; the bridge
//! converts `oneterm_highlight` colors into `gpui::Hsla` and loads the default
//! semantic style asset. The shared `RuleSet` is global (built once via
//! `LazyLock`).

pub mod bridge;
mod overlay;

pub use bridge::{load_default_styles, to_gpui_hsla};
pub use overlay::SemanticOverlay;
