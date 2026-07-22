//! Domain-level dock placement for actions and composition boundaries.

use serde::{Deserialize, Serialize};

/// Placement where a panel should be added to the application dock.
///
/// This type intentionally has no dependency on the GPUI dock implementation.
/// The workspace maps it to the UI crate's placement type at the composition
/// boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DockPlacement {
    /// The central terminal area.
    Center,
    /// The left side dock.
    Left,
    /// The bottom dock.
    Bottom,
    /// The right side dock.
    Right,
}
