//! Layout settings group.

use gpui::{App, SharedString};
use gpui_component::setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem};
use oneterm_settings::{SemanticHighlightingMode, TabTitleMode, TerminalSettings};

use crate::items_with_separators;

use super::set;

/// Build the "Layout" settings group.
pub(super) fn group() -> SettingGroup {
    SettingGroup::new()
        .title("Layout")
        .description("Gutter and scrollback.")
        .items(items_with_separators(vec![
            SettingItem::new(
                "Show Gutter",
                SettingField::switch(
                    |cx: &App| TerminalSettings::global(cx).read(cx).show_gutter,
                    |val: bool, cx: &mut App| {
                        set(cx, |s| s.show_gutter = val);
                    },
                ),
            )
            .description("Show line gutter."),
            SettingItem::new(
                "Semantic Highlighting",
                SettingField::dropdown(
                    vec![
                        (SharedString::from("auto"), SharedString::from("Auto")),
                        (SharedString::from("on"), SharedString::from("On")),
                        (SharedString::from("off"), SharedString::from("Off")),
                    ],
                    |cx: &App| {
                        SharedString::from(
                            match TerminalSettings::global(cx).read(cx).semantic_highlighting {
                                SemanticHighlightingMode::Auto => "auto",
                                SemanticHighlightingMode::On => "on",
                                SemanticHighlightingMode::Off => "off",
                            },
                        )
                    },
                    |val: SharedString, cx: &mut App| {
                        let mode = match val.as_ref() {
                            "on" => SemanticHighlightingMode::On,
                            "off" => SemanticHighlightingMode::Off,
                            _ => SemanticHighlightingMode::Auto,
                        };
                        set(cx, |s| s.semantic_highlighting = mode);
                    },
                ),
            )
            .description("Highlight paths, commands, and URLs."),
            SettingItem::new(
                "Tab Title",
                SettingField::dropdown(
                    // (key, label) — key is the config value, label is shown.
                    vec![
                        (
                            SharedString::from("default"),
                            SharedString::from("Default (label)"),
                        ),
                        (
                            SharedString::from("osc"),
                            SharedString::from("OSC 0/2 (shell title)"),
                        ),
                    ],
                    |cx: &App| {
                        SharedString::from(
                            match TerminalSettings::global(cx).read(cx).tab_title_mode {
                                TabTitleMode::Osc => "osc",
                                TabTitleMode::Default => "default",
                            },
                        )
                    },
                    |val: SharedString, cx: &mut App| {
                        let mode = match val.as_ref() {
                            "osc" => TabTitleMode::Osc,
                            _ => TabTitleMode::Default,
                        };
                        set(cx, |s| s.tab_title_mode = mode);
                    },
                ),
            )
            .description("Choose static or shell title."),
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
                        set(cx, |s| s.scrollback_history = val as usize);
                    },
                ),
            )
            .description("Max scrollback lines."),
        ]))
}
