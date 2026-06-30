//! Custom AssetSource — gộp gpui-component Assets + OneTerm UiAssets.
//!
//! `UiAssets` (từ `oneterm_ui::icon`) serve SVG từ `crates/ui/assets/icons/`,
//! cho phép `Icon::new(AppIcon::Terminal)` render đúng icon.
//!
//! Thứ tự load: UiAssets (custom icons) → gpui-component Assets (built-in icons).

use gpui::{AssetSource, SharedString};
use gpui_component_assets::Assets;
use oneterm_ui::icon::UiAssets;

pub struct CustomAssets;

impl AssetSource for CustomAssets {
    fn load(&self, path: &str) -> Result<Option<std::borrow::Cow<'static, [u8]>>, anyhow::Error> {
        // Ưu tiên custom icons từ UiAssets, fallback sang gpui-component Assets.
        if let Ok(Some(data)) = UiAssets.load(path) {
            return Ok(Some(data));
        }
        Assets.load(path)
    }

    fn list(&self, prefix: &str) -> Result<Vec<SharedString>, anyhow::Error> {
        let mut entries = Assets.list(prefix)?;
        // Merge custom icons vào list.
        for p in UiAssets.list(prefix)? {
            if !entries.contains(&p) {
                entries.push(p);
            }
        }
        Ok(entries)
    }
}