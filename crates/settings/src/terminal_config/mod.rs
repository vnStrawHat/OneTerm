//! Terminal config JSON — the `terminal.json` schema and its logical groups.
//!
//! Each group (font, cursor, layout, shell, scroll, mouse, bell, colors,
//! security, completion) lives in its own module. The aggregate [`TerminalConfig`]
//! document and its load/save/migrate logic live in [`document`].

mod bell;
mod colors;
mod completion;
mod cursor;
mod document;
mod font;
mod layout;
mod mouse;
mod scroll;
mod security;

pub use bell::BellConfig;
pub use colors::ColorsConfig;
pub use completion::{CompletionConfig, CompletionSources};
pub use cursor::CursorConfig;
pub use document::TerminalConfig;
pub use font::FontConfig;
pub use layout::{LayoutConfig, PaddingConfig, SemanticHighlightingMode, TabTitleMode};
pub use mouse::MouseConfig;
pub use scroll::ScrollConfig;
pub use security::SecurityConfig;
