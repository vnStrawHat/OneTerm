//! "Terminal" settings page — shell and font groups, plus the page assembly
//! and the persistence helper shared with [`super::terminal_options`].
//!
//! Every field reads from the global [`TerminalSettings`] and writes back to it,
//! then persists the full snapshot to `terminal.json` via
//! [`TerminalSettings::save`]. Live terminal sessions pick up the changes
//! through the `cx.notify()` observers wired in `apply.rs`.
//!
//! The cursor / layout / scroll / bell / security groups live in
//! [`super::terminal_options`] (split for the ~400-line file guideline).

use gpui::{
    App, AppContext as _, Entity, FontWeight, IntoElement, SharedString, Styled, Subscription,
    Window, prelude::FluentBuilder as _,
};
use gpui_component::{
    AxisExt, Disableable, Icon, IconName, Sizable,
    input::{InputEvent, InputState, NumberInput},
    setting::{
        NumberFieldOptions, RenderOptions, SettingField, SettingGroup, SettingItem, SettingPage,
    },
};
use oneterm_core::config::ShellKind;

use super::items_with_separators;
use crate::state::TerminalSettings;

/// Shell presets shown in the dropdown (label is used as both key and value).
const SHELL_KINDS: &[(ShellKind, &str)] = &[
    (ShellKind::Cmd, "cmd.exe (Windows)"),
    (ShellKind::PowerShell, "Windows PowerShell 5.x"),
    (ShellKind::Pwsh, "PowerShell 7+ (pwsh)"),
    (ShellKind::Bash, "Bash"),
    (ShellKind::Zsh, "Zsh"),
    (ShellKind::Sh, "Sh"),
    (ShellKind::Custom, "Custom"),
];

/// Curated monospace font families (plus a "Default" sentinel).
const FONT_FAMILIES: &[&str] = &[
    "Lilex",
    "Cascadia Mono",
    "JetBrains Mono",
    "Fira Code",
    "Menlo",
    "Consolas",
    "DejaVu Sans Mono",
    "Ubuntu Mono",
    "Courier New",
];

const DEFAULT_FONT_SENTINEL: &str = "Default (theme)";

/// Build the "Terminal" settings page.
pub(crate) fn page(_cx: &App) -> SettingPage {
    SettingPage::new("Terminal")
        .icon(Icon::new(IconName::SquareTerminal))
        .group(shell_group())
        .group(font_group())
        .group(super::terminal_options::cursor_group())
        .group(super::terminal_options::layout_group())
        .group(super::terminal_options::scroll_group())
        .group(super::terminal_options::bell_group())
        .group(super::terminal_options::security_group())
}

// ── Persistence helper (shared with `terminal_options`) ──────────────

/// Persist the live [`TerminalSettings`] to `terminal.json`.
pub(super) fn persist(cx: &mut App) {
    let entity = TerminalSettings::global(cx);
    if let Err(e) = entity.read(cx).save() {
        log::warn!("Failed to save terminal.json: {e}");
    }
}

// ── Shell + Font groups ───────────────────────────────────────────────

fn shell_group() -> SettingGroup {
    let options: Vec<(SharedString, SharedString)> = SHELL_KINDS
        .iter()
        .map(|(_, label)| (SharedString::from(*label), SharedString::from(*label)))
        .collect();

    SettingGroup::new()
        .title("Shell")
        .description("The default shell launched for new local terminals.")
        .items(items_with_separators(vec![
            SettingItem::new(
                "Shell",
                SettingField::dropdown(
                    options,
                    |cx: &App| {
                        let kind = TerminalSettings::global(cx).read(cx).shell.kind;
                        SHELL_KINDS
                            .iter()
                            .find(|(k, _)| *k == kind)
                            .map(|(_, label)| SharedString::from(*label))
                            .unwrap_or_else(|| "Custom".into())
                    },
                    |val: SharedString, cx: &mut App| {
                        let kind = SHELL_KINDS
                            .iter()
                            .find(|(_, label)| *label == val.as_ref())
                            .map(|(k, _)| *k)
                            .unwrap_or(ShellKind::Custom);
                        TerminalSettings::global(cx).update(cx, |s, cx| {
                            s.set_kind(kind);
                            cx.notify();
                        });
                        persist(cx);
                    },
                ),
            )
            .description("Choose the shell kind. For Custom, set the program path below."),
            SettingItem::new(
                "Custom Program",
                SettingField::input(
                    |cx: &App| {
                        TerminalSettings::global(cx)
                            .read(cx)
                            .shell
                            .program
                            .as_ref()
                            .map(|s| SharedString::from(s.to_string_lossy().to_string()))
                            .unwrap_or_default()
                    },
                    |val: SharedString, cx: &mut App| {
                        TerminalSettings::global(cx).update(cx, |s, cx| {
                            s.set_program(val.to_string());
                            cx.notify();
                        });
                        persist(cx);
                    },
                ),
            )
            .description("Used only when Shell is set to Custom (e.g. /usr/bin/fish)."),
        ]))
}

fn font_group() -> SettingGroup {
    // Build the font family options: the "Default" sentinel + the curated list.
    let mut family_options: Vec<(SharedString, SharedString)> =
        vec![(DEFAULT_FONT_SENTINEL.into(), DEFAULT_FONT_SENTINEL.into())];
    family_options.extend(
        FONT_FAMILIES
            .iter()
            .map(|f| (SharedString::from(*f), SharedString::from(*f))),
    );

    let weight_options: Vec<(SharedString, SharedString)> = [
        ("thin", "Thin"),
        ("extra_light", "Extra Light"),
        ("light", "Light"),
        ("normal", "Normal"),
        ("medium", "Medium"),
        ("semibold", "Semibold"),
        ("bold", "Bold"),
        ("extra_bold", "Extra Bold"),
        ("black", "Black"),
    ]
    .iter()
    .map(|(k, label)| (SharedString::from(*k), SharedString::from(*label)))
    .collect();

    SettingGroup::new()
        .title("Font")
        .description("Terminal font family, size, and weight.")
        .items(items_with_separators(vec![
            SettingItem::new(
                "Font Family",
                SettingField::dropdown(
                    family_options,
                    |cx: &App| {
                        TerminalSettings::global(cx)
                            .read(cx)
                            .font_family
                            .clone()
                            .unwrap_or_else(|| DEFAULT_FONT_SENTINEL.into())
                    },
                    |val: SharedString, cx: &mut App| {
                        let family = if val.as_ref() == DEFAULT_FONT_SENTINEL {
                            None
                        } else {
                            Some(val)
                        };
                        TerminalSettings::global(cx).update(cx, |s, cx| {
                            s.font_family = family;
                            cx.notify();
                        });
                        persist(cx);
                    },
                ),
            )
            .description(
                "The terminal text font. \"Default (theme)\" uses the active theme's mono font.",
            ),
            SettingItem::new(
                "Font Size",
                SettingField::number_input(
                    NumberFieldOptions {
                        min: 6.0,
                        max: 72.0,
                        ..Default::default()
                    },
                    |cx: &App| {
                        TerminalSettings::global(cx)
                            .read(cx)
                            .font_size
                            .unwrap_or(15.0) as f64
                    },
                    |val: f64, cx: &mut App| {
                        let size = val as f32;
                        TerminalSettings::global(cx).update(cx, |s, cx| {
                            s.font_size = Some(size);
                            s.base_font_size = Some(size);
                            cx.notify();
                        });
                        persist(cx);
                    },
                ),
            )
            .description("Terminal font size in px (6–72)."),
            SettingItem::new(
                "Font Weight",
                SettingField::dropdown(
                    weight_options,
                    |cx: &App| SharedString::from(weight_to_string(cx)),
                    |val: SharedString, cx: &mut App| {
                        let weight = parse_weight(val.as_ref());
                        TerminalSettings::global(cx).update(cx, |s, cx| {
                            s.font_weight = weight;
                            cx.notify();
                        });
                        persist(cx);
                    },
                ),
            )
            .description("Weight of the terminal font."),
            SettingItem::new("Line Height", line_height_field())
                .description("Line height multiplier (1.2 = 120% of the font size)."),
        ]))
}

// ── Weight helpers ───────────────────────────────────────────────────

/// Map the live [`FontWeight`] to its config string.
fn weight_to_string(cx: &App) -> String {
    let w = TerminalSettings::global(cx).read(cx).font_weight;
    match w {
        FontWeight::THIN => "thin",
        FontWeight::EXTRA_LIGHT => "extra_light",
        FontWeight::LIGHT => "light",
        FontWeight::NORMAL => "normal",
        FontWeight::MEDIUM => "medium",
        FontWeight::SEMIBOLD => "semibold",
        FontWeight::BOLD => "bold",
        FontWeight::EXTRA_BOLD => "extra_bold",
        FontWeight::BLACK => "black",
        _ => "normal",
    }
    .into()
}

/// Parse a weight config string into [`FontWeight`] (delegates to the shared
/// helper in `terminal_settings::font`).
fn parse_weight(s: &str) -> FontWeight {
    crate::state::terminal_settings::parse_weight(s)
}

// ── Line Height custom number field ─────────────────────────────────────
//
// `SettingField::number_input` from gpui-component does NOT propagate
// `NumberFieldOptions.step` to the internal `InputState` (it defaults to
// 1.0).  The increment/decrement buttons therefore step by 1 instead of the
// configured 0.1.
//
// To fix this we use `SettingField::render` with a custom `NumberInput` that
// calls `.step(0.1).min(1.0).max(3.0)` directly on the `InputState`.

/// State held across renders for the custom Line Height number input.
struct LineHeightInputState {
    input: Entity<InputState>,
    initial_value: f64,
    _subscription: Subscription,
}

/// Build the "Line Height" setting field using a custom-rendered `NumberInput`
/// whose increment/decrement buttons step by 0.1 (not the library default of
/// 1.0).
///
/// The displayed value is rounded to 1 decimal place to avoid f32→f64
/// precision artifacts (e.g. `1.2f32` → `1.2000000476837158f64`).
fn line_height_field() -> SettingField<SharedString> {
    SettingField::render(
        move |options: &RenderOptions, window: &mut Window, cx: &mut App| {
            // Current value from settings, rounded to 1 decimal place.
            let value = {
                let v = TerminalSettings::global(cx).read(cx).line_height_factor as f64;
                (v * 10.0).round() / 10.0
            };

            let key = SharedString::from(format!(
                "line-height-input-{}-{}-{}",
                options.page_ix, options.group_ix, options.item_ix
            ));

            let state_entity = window.use_keyed_state(key, cx, |window, cx| {
                let input = cx.new(|cx| {
                    InputState::new(window, cx)
                        .default_value(value.to_string())
                        .step(0.1)
                        .min(1.0)
                        .max(3.0)
                });

                let _subscription = cx.subscribe_in(&input, window, {
                    move |state: &mut LineHeightInputState,
                          input,
                          event: &InputEvent,
                          window,
                          cx| {
                        if !matches!(event, InputEvent::Change) {
                            return;
                        }
                        input.update(cx, |input, cx| {
                            let val_str = input.value();
                            if val_str == state.initial_value.to_string() {
                                return;
                            }
                            if let Ok(val) = val_str.parse::<f64>() {
                                let rounded = (val * 10.0).round() / 10.0;
                                let clamped = rounded.clamp(1.0, 3.0);
                                TerminalSettings::global(cx).update(cx, |s, cx| {
                                    s.line_height_factor = clamped as f32;
                                    cx.notify();
                                });
                                persist(cx);
                                state.initial_value = clamped;
                                if clamped.to_string() != val_str {
                                    input.set_value(
                                        SharedString::from(clamped.to_string()),
                                        window,
                                        cx,
                                    );
                                }
                            }
                        });
                    }
                });

                LineHeightInputState {
                    input,
                    initial_value: value,
                    _subscription,
                }
            });

            // Sync external changes (e.g. config file reload).
            state_entity.update(cx, |state, cx| {
                if state.initial_value != value {
                    state.initial_value = value;
                    state.input.update(cx, |input, cx| {
                        input.set_value(SharedString::from(value.to_string()), window, cx);
                    });
                }
            });

            let state = state_entity.read(cx);

            NumberInput::new(&state.input)
                .disabled(options.disabled)
                .with_size(options.size)
                .map(|this| {
                    if options.layout.is_horizontal() {
                        this.w_32()
                    } else {
                        this.w_full()
                    }
                })
                .into_any_element()
        },
    )
}
