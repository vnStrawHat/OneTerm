//! Custom AssetSource — merges the gpui-component Assets with OneTerm's UiAssets.
//!
//! `UiAssets` (from `oneterm_theme::icon`) serves SVGs from
//! `crates/theme/assets/icons/`, letting `Icon::new(AppIcon::Terminal)` render
//! the correct icon.
//!
//! Load order: UiAssets (custom icons) → gpui-component Assets (built-in icons).

use gpui::{AssetSource, SharedString};
use gpui_component_assets::Assets;
use oneterm_theme::icon::UiAssets;

pub struct CustomAssets;

impl AssetSource for CustomAssets {
    fn load(&self, path: &str) -> Result<Option<std::borrow::Cow<'static, [u8]>>, anyhow::Error> {
        // Prefer custom icons from UiAssets, fall back to gpui-component Assets.
        if let Ok(Some(data)) = UiAssets.load(path) {
            return Ok(Some(data));
        }
        Assets.load(path)
    }

    fn list(&self, prefix: &str) -> Result<Vec<SharedString>, anyhow::Error> {
        let mut entries = Assets.list(prefix)?;
        // Merge custom icons into the list.
        for p in UiAssets.list(prefix)? {
            if !entries.contains(&p) {
                entries.push(p);
            }
        }
        Ok(entries)
    }
}
