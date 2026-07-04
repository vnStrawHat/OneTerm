//! "Terminal" settings page — the cursor, layout, scroll, bell, and security
//! groups.
//!
//! Split from [`super::terminal`] to keep files under the ~400-line guideline.
//! Each group reads from the global [`TerminalSettings`] and writes back to it,
//! then persists the full snapshot to `terminal.json` via
//! [`super::terminal::persist`].

use gpui::{App, SharedString};
use gpui_component::setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem};

use crate::state::terminal_settings::{hsla_to_hex, parse_hex_color};
use crate::state::{TerminalBlink, TerminalCursorShape, TerminalSettings};

/// "Cursor" group — shape, blink, and color.
pub(super) fn cursor_group() -> SettingGroup {
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
        .description("Cursor shape, blink, and color.")
        .item(
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
                        super::terminal::persist(cx);
                    },
                ),
            )
            .description("The terminal cursor shape."),
        )
        .item(
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
                        super::terminal::persist(cx);
                    },
                ),
            )
            .description("Blink the cursor when the terminal is focused."),
        )
        .item(
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
                        super::terminal::persist(cx);
                    },
                ),
            )
            .description("Override the cursor color as #RRGGBB (blank = theme caret)."),
        )
}

/// "Layout" group — gutter, dock auto-hide, and scrollback size.
pub(super) fn layout_group() -> SettingGroup {
    SettingGroup::new()
        .title("Layout")
        .description("Gutter, dock auto-hide, and scrollback size.")
        .item(
            SettingItem::new(
                "Show Gutter",
                SettingField::switch(
                    |cx: &App| TerminalSettings::global(cx).read(cx).show_gutter,
                    |val: bool, cx: &mut App| {
                        TerminalSettings::global(cx).update(cx, |s, cx| {
                            s.show_gutter = val;
                            cx.notify();
                        });
                        super::terminal::persist(cx);
                    },
                ),
            )
            .description("Show the timestamp + line number column on the left of the terminal."),
        )
        .item(
            SettingItem::new(
                "Auto-hide Right Dock",
                SettingField::switch(
                    |cx: &App| {
                        TerminalSettings::global(cx)
                            .read(cx)
                            .auto_hide_right_dock_on_local
                    },
                    |val: bool, cx: &mut App| {
                        TerminalSettings::global(cx).update(cx, |s, cx| {
                            s.auto_hide_right_dock_on_local = val;
                            cx.notify();
                        });
                        super::terminal::persist(cx);
                    },
                ),
            )
            .description(
                "Collapse the Right Dock (Session/SFTP) when a local shell tab is active.",
            ),
        )
        .item(
            SettingItem::new(
                "Scrollback History",
                SettingField::number_input(
                    NumberFieldOptions {
                        min: 0.0,
                        max: 1_000_000.0,
                        step: 1000.0,
                    },
                    |cx: &App| TerminalSettings::global(cx).read(cx).scrollback_history as f64,
                    |val: f64, cx: &mut App| {
                        TerminalSettings::global(cx).update(cx, |s, cx| {
                            s.scrollback_history = val as usize;
                            cx.notify();
                        });
                        super::terminal::persist(cx);
                    },
                ),
            )
            .description("Maximum number of scrollback history lines."),
        )
}

/// "Scroll" group — mouse wheel speed and alternate-screen scroll mode.
pub(super) fn scroll_group() -> SettingGroup {
    SettingGroup::new()
        .title("Scroll")
        .description("Mouse wheel speed and alternate-screen scroll mode.")
        .item(
            SettingItem::new(
                "Scroll Multiplier",
                SettingField::number_input(
                    NumberFieldOptions {
                        min: 0.0,
                        max: 20.0,
                        step: 0.5,
                    },
                    |cx: &App| TerminalSettings::global(cx).read(cx).scroll_multiplier as f64,
                    |val: f64, cx: &mut App| {
                        TerminalSettings::global(cx).update(cx, |s, cx| {
                            s.scroll_multiplier = val as f32;
                            cx.notify();
                        });
                        super::terminal::persist(cx);
                    },
                ),
            )
            .description("Mouse wheel scroll speed (1.0 = default)."),
        )
        .item(
            SettingItem::new(
                "Alternate Scroll",
                SettingField::switch(
                    |cx: &App| TerminalSettings::global(cx).read(cx).alternate_scroll,
                    |val: bool, cx: &mut App| {
                        TerminalSettings::global(cx).update(cx, |s, cx| {
                            s.alternate_scroll = val;
                            cx.notify();
                        });
                        super::terminal::persist(cx);
                    },
                ),
            )
            .description(
                "In alt-screen (vim/less/htop), send arrow keys instead of scrolling scrollback.",
            ),
        )
}

/// "Bell" group — enable/disable the bell indicator.
pub(super) fn bell_group() -> SettingGroup {
    SettingGroup::new().title("Bell").item(
        SettingItem::new(
            "Bell Enabled",
            SettingField::switch(
                |cx: &App| TerminalSettings::global(cx).read(cx).bell_enabled,
                |val: bool, cx: &mut App| {
                    TerminalSettings::global(cx).update(cx, |s, cx| {
                        s.bell_enabled = val;
                        cx.notify();
                    });
                    super::terminal::persist(cx);
                },
            ),
        )
        .description("Show a 🔔 indicator when the terminal receives \\x07."),
    )
}

/// "Security" group — gates for privacy-sensitive terminal features.
pub(super) fn security_group() -> SettingGroup {
    SettingGroup::new().title("Security").item(
        SettingItem::new(
            "Allow Clipboard Read (OSC 52)",
            SettingField::switch(
                |cx: &App| TerminalSettings::global(cx).read(cx).allow_clipboard_read,
                |val: bool, cx: &mut App| {
                    TerminalSettings::global(cx).update(cx, |s, cx| {
                        s.allow_clipboard_read = val;
                        cx.notify();
                    });
                    super::terminal::persist(cx);
                },
            ),
        )
        .description(
            "Let programs read the system clipboard via OSC 52. Disabled by default — \
             enabling exposes the clipboard to remote programs over SSH.",
        ),
    )
}
