//! The FPS HUD (`IN-0048`, `US-0151`): GPUI Kit's `gpui-fps` monitor, plus a strip under it
//! naming the GPU and the graphics API, pinned to the window's top-right below the tab strip.
//!
//! The kit HUD measures everything that moves (frame cost, p95, dropped frames, GPU %, CPU,
//! memory) and keeps its own fixed palette, which it does not let an application change
//! because its near-opaque backdrop is what keeps it legible over any content. The strip is
//! OneTerm's and uses theme tokens. Nothing here exists while the HUD is off: the workspace
//! holds an `Option<FpsHud>` and drops it on the first frame the setting is off, which drops
//! the kit's readout clock and its frame-trace guard with it.

use gpui::{
    App, AppContext as _, Entity, GpuSpecs, IntoElement, ParentElement, Pixels, SharedString,
    Styled, Window, div, px,
};
use gpui_component::{ActiveTheme as _, TITLE_BAR_HEIGHT};
use gpui_fps::FpsMonitor;

/// The kit HUD's own width (`HUD_WIDTH` in gpui-fps `monitor.rs`, private there), so the
/// strip lines up under it.
const HUD_WIDTH: Pixels = px(172.);
/// The tab strip under the title bar. The HUD starts below it so the strip's `+` and `...`
/// stay clickable, as do the title bar's caption buttons and right-dock toggles.
const TAB_STRIP_HEIGHT: Pixels = px(32.);
/// The kit overlay's margin (`MARGIN` in gpui-fps `overlay.rs`).
const MARGIN: Pixels = px(12.);
/// The kit HUD's text size (`TEXT_SIZE` in gpui-fps `monitor.rs`).
const TEXT_SIZE: Pixels = px(10.);

/// The renderer gpui-pre 0.3.7 compiles for this platform. A constant because gpui does not
/// report it at run time: gpui-pre-windows creates only a Direct3D 11 device and logs, but
/// does not expose, the feature level it got; gpui-pre-wgpu asks wgpu for Vulkan or GL and
/// `GpuSpecs` does not say which; macOS has only Metal.
#[cfg(target_os = "windows")]
const GRAPHICS_API: &str = "Direct3D 11";
#[cfg(target_os = "macos")]
const GRAPHICS_API: &str = "Metal";
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const GRAPHICS_API: &str = "Vulkan/GL (wgpu)";

/// The HUD of one window: the kit monitor and the strip's rows, read once when shown.
pub struct FpsHud {
    monitor: Entity<FpsMonitor>,
    rows: [(&'static str, SharedString); 2],
}

impl FpsHud {
    /// Called on the first frame the HUD is shown. `gpu_specs` may load a vendor library to
    /// read the driver version, so it is asked once here and not per frame.
    pub fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            monitor: cx.new(|cx| FpsMonitor::new(window, cx)),
            rows: device_rows(window.gpu_specs().as_ref()),
        }
    }

    /// The HUD, absolutely positioned; the parent must be `relative()`.
    pub fn render(&self, cx: &App) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let (muted, value) = (theme.muted_foreground, theme.popover_foreground);
        div()
            .absolute()
            .top(TITLE_BAR_HEIGHT + TAB_STRIP_HEIGHT + MARGIN)
            .right(MARGIN)
            .flex()
            .flex_col()
            // Children keep their own width, so the kit's collapsed tag stays a tag.
            .items_end()
            .gap_1()
            .child(self.monitor.clone())
            .child(
                div()
                    .w(HUD_WIDTH)
                    .px_2()
                    .py_1()
                    .rounded(px(4.))
                    .bg(theme.popover)
                    .border_1()
                    .border_color(theme.border)
                    .font_family(theme.mono_font_family.clone())
                    .text_size(TEXT_SIZE)
                    .children(self.rows.iter().map(|(label, text)| {
                        div()
                            .flex()
                            .justify_between()
                            .gap_2()
                            .child(div().flex_none().text_color(muted).child(*label))
                            .child(
                                div()
                                    .min_w_0()
                                    .text_right()
                                    .text_color(value)
                                    .child(text.clone()),
                            )
                    })),
            )
    }
}

/// The strip's rows, in display order: the GPU gpui renders with, then the graphics API.
fn device_rows(specs: Option<&GpuSpecs>) -> [(&'static str, SharedString); 2] {
    let device = match specs {
        Some(specs) if specs.device_name.is_empty() => "n/a".to_string(),
        Some(specs) if specs.is_software_emulated => format!("{} (software)", specs.device_name),
        Some(specs) => specs.device_name.clone(),
        None => "n/a".to_string(),
    };
    [("DEVICE", device.into()), ("API", GRAPHICS_API.into())]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specs(name: &str, software: bool) -> GpuSpecs {
        GpuSpecs {
            is_software_emulated: software,
            device_name: name.into(),
            driver_name: "NVIDIA Corporation".into(),
            driver_info: "581.29".into(),
        }
    }

    #[test]
    fn the_strip_names_the_device_then_the_api() {
        let rows = device_rows(Some(&specs("NVIDIA GeForce RTX 4060", false)));
        assert_eq!(
            rows.map(|(label, text)| format!("{label} {text}")),
            [
                "DEVICE NVIDIA GeForce RTX 4060".to_string(),
                format!("API {GRAPHICS_API}"),
            ]
        );
        assert_eq!(
            device_rows(Some(&specs("Microsoft Basic Render Driver", true)))[0].1,
            "Microsoft Basic Render Driver (software)"
        );
        // gpui-pre-macos 0.3.7 answers `None`; the row says so instead of vanishing.
        assert_eq!(device_rows(None)[0].1, "n/a");
        assert_eq!(device_rows(Some(&specs("", false)))[0].1, "n/a");
    }
}
