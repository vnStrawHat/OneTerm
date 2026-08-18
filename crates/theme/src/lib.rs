//! OneTerm theme registry + icon set.
//!
//! Owns the embedded theme JSON files (loaded into gpui-component's
//! `ThemeRegistry`), the `SwitchTheme` / `SwitchThemeMode` action handlers, and
//! the `AppIcon` icon enum generated from `assets/icons/*.svg` (via `build.rs` +
//! the `icon_named!` macro). `UiAssets` serves those SVGs to GPUI.

pub mod brand;
pub mod icon;
pub mod theme;

pub use icon::{AppIcon, UiAssets};
pub use theme::{apply_list_style_override, init};
