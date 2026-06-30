//! Custom icon enum — tương tự `gpui_component::IconName` nhưng cho riêng myTerm2.
//!
//! ## Thêm icon mới
//!
//! 1. Thả file `.svg` vào `crates/ui/assets/icons/` (ví dụ `my-icon.svg`)
//! 2. Build — variant `AppIcon::MyIcon` tự động sinh ra
//! 3. Sử dụng: `Icon::new(AppIcon::MyIcon)` hoặc `.icon(AppIcon::MyIcon)`
//!
//! Tên variant = PascalCase của filename:
//! - `arrow-right.svg` → `AppIcon::ArrowRight`
//! - `my_icon.svg`     → `AppIcon::MyIcon`
//!
//! File SVG phẳng cần có `width="16" height="16"` (hoặc 24) + `stroke="currentColor"`
//! để nhận text_color từ theme.
//!
//! ## Giữ màu sắc SVG (multi-color)
//!
//! `Icon::new(AppIcon::...)` render SVG qua **alpha mask** → tất cả màu bị mất,
//! chỉ giữ 1 màu `text_color`.
//!
//! Để giữ nguyên màu trong SVG file, dùng `.colored()` thay vì `Icon::new()`:
//! ```ignore
//! AppIcon::File3.colored().size(px(16.))   // full-color image
//! Icon::new(AppIcon::Terminal).small()       // monochrome (theme-aware)
//! ```

use gpui::{AnyElement, App, AssetSource, ImageSource, IntoElement, RenderOnce, Resource, Result, SharedString, Styled as _, Window, img};
use gpui_component::{Icon, IconNamed, icon_named};

// Generate `AppIcon` enum from SVG files in `assets/icons/`.
// The `$MYTERM2_UI_ICONS_DIR` env var is set by `build.rs`.
icon_named!(AppIcon, "$MYTERM2_UI_ICONS_DIR", [Debug, PartialEq, Eq]);

/// Cho phép `AppIcon` dùng trực tiếp như element: `div().child(AppIcon::Terminal)`.
impl RenderOnce for AppIcon {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        Icon::new(self)
    }
}

/// Cho phép `.into_any_element()`.
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
        img(ImageSource::Resource(Resource::Embedded(self.path())))
            .flex_none()
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