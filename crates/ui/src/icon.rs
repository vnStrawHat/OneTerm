//! Custom icon enum — similar to `gpui_component::IconName` but specific to OneTerm.
//!
//! ## Adding a new icon
//!
//! 1. Drop a `.svg` file into `crates/ui/assets/icons/` (e.g. `my-icon.svg`)
//! 2. Build — the `AppIcon::MyIcon` variant is generated automatically
//! 3. Use it: `Icon::new(AppIcon::MyIcon)` or `.icon(AppIcon::MyIcon)`
//!
//! The variant name is the PascalCase of the filename:
//! - `arrow-right.svg` → `AppIcon::ArrowRight`
//! - `my_icon.svg`     → `AppIcon::MyIcon`
//!
//! A flat SVG file needs `width="16" height="16"` (or 24) plus `stroke="currentColor"`
//! to pick up `text_color` from the theme.
//!
//! ## Preserving SVG colors (multi-color)
//!
//! `Icon::new(AppIcon::...)` renders the SVG through an **alpha mask**, so all colors
//! are lost and only the single `text_color` remains.
//!
//! To keep the colors in the SVG file, use `.colored()` instead of `Icon::new()`:
//! ```ignore
//! AppIcon::File3.colored().size(px(16.))   // full-color image
//! Icon::new(AppIcon::Terminal).small()       // monochrome (theme-aware)
//! ```

use gpui::{
    AnyElement, App, AssetSource, ImageSource, IntoElement, RenderOnce, Resource, Result,
    SharedString, Styled as _, Window, img,
};
use gpui_component::{Icon, IconNamed, icon_named};

// Generate `AppIcon` enum from SVG files in `assets/icons/`.
// The `$ONETERM_UI_ICONS_DIR` env var is set by `build.rs`.
icon_named!(AppIcon, "$ONETERM_UI_ICONS_DIR", [Debug, PartialEq, Eq]);

/// Allows `AppIcon` to be used directly as an element: `div().child(AppIcon::Terminal)`.
impl RenderOnce for AppIcon {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        Icon::new(self)
    }
}

/// Enables `.into_any_element()`.
impl From<AppIcon> for AnyElement {
    fn from(val: AppIcon) -> Self {
        Icon::new(val).into_any_element()
    }
}

impl AppIcon {
    /// Render as a full-color image — **preserves SVG colors** from the file.
    ///
    /// Use this instead of `Icon::new(self)` for multi-color SVGs (e.g. file, folder icons).
    ///
    /// `Icon::new()` renders via alpha-mask (monochrome), losing all SVG colors.
    /// `colored()` renders via `render_single_frame()` (full RGBA), keeping original colors.
    ///
    /// Chain `.size(px(16.))` or `.w(px(16.)).h(px(16.))` to set dimensions.
    pub fn colored(self) -> gpui::Img {
        img(ImageSource::Resource(Resource::Embedded(self.path()))).flex_none()
    }
}

/// Embedded assets — serve SVG files from `crates/ui/assets/` via RustEmbed.
///
/// Paths stored relative to `assets/`, e.g. `"icons/terminal.svg"`.
/// This matches the `"icons/<name>.svg"` path format that `icon_named!` generates.
#[derive(rust_embed::RustEmbed)]
#[folder = "assets"]
#[include = "icons/*.svg"]
pub struct UiAssets;

impl AssetSource for UiAssets {
    fn load(&self, path: &str) -> Result<Option<std::borrow::Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }
        UiAssets::get(path)
            .map(|f| Some(f.data))
            .ok_or_else(|| anyhow::anyhow!("could not find asset at path \"{}\"", path))
    }

    fn list(&self, prefix: &str) -> Result<Vec<SharedString>> {
        Ok(UiAssets::iter()
            .filter_map(|p| p.starts_with(prefix).then(|| p.into()))
            .collect())
    }
}
