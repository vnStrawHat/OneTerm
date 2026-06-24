//! Build terminal GPUI font từ settings.

use gpui::SharedString;

use crate::state::TerminalSettings;

impl super::LocalTerminalView {
    /// Build GPUI Font từ terminal settings.
    pub(crate) fn font(
        &self,
        settings: &TerminalSettings,
        font_family: &SharedString,
    ) -> gpui::Font {
        let fallbacks = if settings.font_fallbacks.is_empty() {
            None
        } else {
            Some(gpui::FontFallbacks::from_fonts(
                settings
                    .font_fallbacks
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            ))
        };
        let mut features: Vec<(String, u32)> = vec![("calt".to_string(), 0)];
        for f in &settings.font_features {
            features.retain(|(tag, _)| tag != f);
            features.push((f.to_string(), 1u32));
        }
        gpui::Font {
            family: font_family.clone().into(),
            weight: settings.font_weight,
            style: gpui::FontStyle::Normal,
            fallbacks,
            features: gpui::FontFeatures(std::sync::Arc::new(features)),
        }
    }
}
