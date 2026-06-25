//! Custom AssetSource — wrap gpui-component Assets + serve terminal icon.
//!
//! `svg().path("icons/terminal.svg")` tra SVG từ AssetSource.
//! gpui-component Assets chỉ serve icons riêng, nên cần wrapper thêm.

use gpui::{AssetSource, SharedString};
use gpui_component_assets::Assets;

const TERMINAL_SVG: &[u8] = include_bytes!("../../ui/assets/icons/terminal.svg");

pub struct CustomAssets;

impl AssetSource for CustomAssets {
    fn load(&self, path: &str) -> Result<Option<std::borrow::Cow<'static, [u8]>>, anyhow::Error> {
        if path == "icons/terminal.svg" {
            return Ok(Some(std::borrow::Cow::Borrowed(TERMINAL_SVG)));
        }
        Assets.load(path)
    }

    fn list(&self, prefix: &str) -> Result<Vec<SharedString>, anyhow::Error> {
        let mut entries = Assets.list(prefix)?;
        if prefix.is_empty() || "icons/terminal.svg".starts_with(prefix) {
            entries.push("icons/terminal.svg".into());
        }
        Ok(entries)
    }
}