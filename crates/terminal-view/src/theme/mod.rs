//! Terminal theme: maps the gpui-component `Theme` → `core::TerminalPalette`,
//! resolves `Color` → `gpui::Hsla`, and `ensure_minimum_contrast`.
//!
//! Pure utilities (no GPUI Element).

pub mod contrast;
pub mod palette;
pub mod resolve;
mod terminal_theme;
#[cfg(test)]
mod tests;

pub use contrast::ensure_minimum_contrast;
pub use palette::{hsla_from_vte, vte_from_rgba};
pub use resolve::resolve_cell_color;
pub use terminal_theme::{TerminalTheme, build_terminal_theme};
