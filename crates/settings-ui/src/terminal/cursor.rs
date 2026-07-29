//! Cursor settings group.

use gpui::{App, SharedString};
use gpui_component::setting::{SettingField, SettingGroup, SettingItem};
use oneterm_settings::terminal_settings::{hsla_to_hex, parse_hex_color};
use oneterm_settings::{TerminalBlink, TerminalCursorShape, TerminalSettings};

use crate::items_with_separators;

use super::persist;

/// Build the "Cursor" settings group.
pub(super) fn group() -> SettingGroup {
    let shape_options: Vec<(SharedString, SharedString)> = [
        ("block", "Block"),
        ("bar", "Bar"),
        ("underline", "Underline"),
    ]
    .iter()
    .map(|(k, label)| (SharedString::from(*k), SharedString::from(*label)))
    .collect();

    SettingGroup::new()
        .title("Cursor")
        .description("Shape, blink, and color.")
        .items(items_with_separators(vec![
            SettingItem::new(
                "Cursor Shape",
                SettingField::dropdown(
                    shape_options,
                    |cx: &App| {
                        let s = TerminalSettings::global(cx).read(cx).cursor_shape;
                        SharedString::from(match s {
                            TerminalCursorShape::Block => "block",
                            TerminalCursorShape::Bar => "bar",
                            TerminalCursorShape::Underline => "underline",
                        })
                    },
                    |val: SharedString, cx: &mut App| {
                        let shape = match val.as_ref() {
                            "bar" => TerminalCursorShape::Bar,
                            "underline" => TerminalCursorShape::Underline,
                            _ => TerminalCursorShape::Block,
                        };
                        TerminalSettings::global(cx).update(cx, |s, cx| {
                            s.set_cursor_shape(shape);
                            cx.notify();
                        });
                        persist(cx);
                    },
                ),
            )
            .description("Cursor shape."),
            SettingItem::new(
                "Cursor Blink",
                SettingField::switch(
                    |cx: &App| {
                        matches!(
                            TerminalSettings::global(cx).read(cx).cursor_blink,
                            TerminalBlink::On
                        )
                    },
                    |val: bool, cx: &mut App| {
                        let blink = if val {
                            TerminalBlink::On
                        } else {
                            TerminalBlink::Off
                        };
                        TerminalSettings::global(cx).update(cx, |s, cx| {
                            s.set_cursor_blink(blink);
                            cx.notify();
                        });
                        persist(cx);
                    },
                ),
            )
            .description("Blink when focused."),
            SettingItem::new(
                "Cursor Color",
                SettingField::input(
                    |cx: &App| {
                        TerminalSettings::global(cx)
                            .read(cx)
                            .cursor_color
                            .map(|c| SharedString::from(hsla_to_hex(c)))
                            .unwrap_or_default()
                    },
                    |val: SharedString, cx: &mut App| {
                        let color = if val.trim().is_empty() {
                            None
                        } else {
                            parse_hex_color(val.as_ref())
                        };
                        TerminalSettings::global(cx).update(cx, |s, cx| {
                            s.cursor_color = color;
                            cx.notify();
                        });
                        persist(cx);
                    },
                ),
            )
            .description("Hex color; blank uses theme."),
        ]))
}
