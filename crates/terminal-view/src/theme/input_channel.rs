//! The colour that marks a broadcast input channel, shared by the tab chips
//! and the Space frame so both name the same channel with the same colour.

use gpui::{App, Hsla};
use gpui_component::ActiveTheme as _;

use oneterm_core::InputChannel;

/// The colour of `channel`: the theme's `chart_1..chart_5` in A..E order. The
/// chart colours are the theme's own five-way distinct set, so the channels
/// stay readable in light and dark themes without a private palette.
pub(crate) fn channel_color(channel: InputChannel, cx: &App) -> Hsla {
    let theme = cx.theme();
    [
        theme.chart_1,
        theme.chart_2,
        theme.chart_3,
        theme.chart_4,
        theme.chart_5,
    ][channel.index()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn every_channel_maps_to_its_own_chart_color(cx: &mut gpui::TestAppContext) {
        cx.update(gpui_component::init);
        cx.update(|cx| {
            let expected = [
                cx.theme().chart_1,
                cx.theme().chart_2,
                cx.theme().chart_3,
                cx.theme().chart_4,
                cx.theme().chart_5,
            ];
            for (channel, color) in InputChannel::ALL.into_iter().zip(expected) {
                assert_eq!(channel_color(channel, cx), color);
            }
        });
    }
}
