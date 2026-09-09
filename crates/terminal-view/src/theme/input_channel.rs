//! The colour that marks a broadcast input channel and the chip that names it,
//! shared by the tab strip and the badge of a member Space so both name the
//! same channel with the same colour and the same look.

use gpui::{
    App, Div, ElementId, FontWeight, Hsla, InteractiveElement as _, ParentElement as _, Stateful,
    Styled as _, div,
};
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

/// The letter, background and text colour of the chip that names `channel`.
/// Pure, so the tab chip and the Space badge can be checked without a frame.
pub(crate) fn channel_chip_style(channel: InputChannel, cx: &App) -> (&'static str, Hsla, Hsla) {
    (
        channel.label(),
        channel_color(channel, cx),
        cx.theme().background,
    )
}

/// The chip that names `channel`: its bold letter on the channel colour.
pub(crate) fn channel_chip(
    channel: InputChannel,
    id: impl Into<ElementId>,
    cx: &App,
) -> Stateful<Div> {
    let (label, background, foreground) = channel_chip_style(channel, cx);
    div()
        .id(id)
        .flex_shrink_0()
        .px_1()
        .rounded_sm()
        .text_xs()
        .font_weight(FontWeight::BOLD)
        .bg(background)
        .text_color(foreground)
        .child(label)
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

    #[gpui::test]
    fn every_channel_chip_shows_its_letter_on_its_channel_color(cx: &mut gpui::TestAppContext) {
        cx.update(gpui_component::init);
        cx.update(|cx| {
            for channel in InputChannel::ALL {
                assert_eq!(
                    channel_chip_style(channel, cx),
                    (
                        channel.label(),
                        channel_color(channel, cx),
                        cx.theme().background
                    )
                );
            }
        });
    }
}
