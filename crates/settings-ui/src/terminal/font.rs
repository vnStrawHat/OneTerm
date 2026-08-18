//! Font settings group and custom font-related fields.

use gpui::{
    App, AppContext as _, Entity, FontWeight, IntoElement, ParentElement, SharedString, Styled,
    Subscription, Window, div, prelude::FluentBuilder as _, px, rems,
};
use gpui_component::{
    AxisExt, Disableable, IndexPath, Sizable,
    input::{InputEvent, InputState, NumberInput},
    select::{SearchableVec, Select, SelectEvent, SelectState},
    setting::{NumberFieldOptions, RenderOptions, SettingField, SettingGroup, SettingItem},
};
use oneterm_settings::TerminalSettings;

use crate::items_with_separators;

use super::set;

const DEFAULT_FONT_SENTINEL: &str = "Default (theme)";

/// Build the "Font" settings group.
pub(super) fn group() -> SettingGroup {
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
        .description("Family, size, and weight.")
        .items(items_with_separators(vec![
            SettingItem::new("Font Family", font_family_field())
                .description("Use \"Default\" for theme font."),
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
                        set(cx, |s| {
                            s.font_size = Some(size);
                            s.base_font_size = Some(size);
                        });
                    },
                ),
            )
            .description("Size in px (6–72)."),
            SettingItem::new(
                "Font Weight",
                SettingField::dropdown(
                    weight_options,
                    |cx: &App| SharedString::from(weight_to_string(cx)),
                    |val: SharedString, cx: &mut App| {
                        let weight = parse_weight(val.as_ref());
                        set(cx, |s| s.font_weight = weight);
                    },
                ),
            )
            .description("Font weight."),
            SettingItem::new("Line Height", line_height_field())
                .description("Line height multiplier."),
        ]))
}

// ── Font Family Select field ─────────────────────────────────────────────
//
// The font family list can be very long (hundreds of OS fonts). Instead of
// a plain `SettingField::dropdown` that renders every item, we use a
// searchable, scrollable `Select` from gpui-component with `menu_max_h` set
// to show roughly five items at a time. The `SelectState` entity is created
// once via `window::use_keyed_state` (same pattern as `line_height_field`)
// and reused across renders.

/// Build the font family list shown in the Select: the "Default (theme)" sentinel
/// followed by all OS-available font names. When font enumeration returns
/// nothing, only the sentinel is shown.
fn font_name_list(cx: &App) -> Vec<SharedString> {
    let mut list = vec![SharedString::from(DEFAULT_FONT_SENTINEL)];
    list.extend(
        cx.text_system()
            .all_font_names()
            .into_iter()
            .map(SharedString::from),
    );
    list
}

/// Find the index in `list` that corresponds to the currently-selected font
/// family (or the "Default" sentinel when `font_family` is `None`).
fn initial_font_index(list: &[SharedString], cx: &App) -> Option<usize> {
    let current = TerminalSettings::global(cx)
        .read(cx)
        .font_family
        .clone()
        .unwrap_or_else(|| DEFAULT_FONT_SENTINEL.into());
    list.iter().position(|f| *f == current)
}

/// State held across renders for the font-family `Select`.
struct FontSelectState {
    select: Entity<SelectState<SearchableVec<SharedString>>>,
    /// Last value applied to `TerminalSettings` — used to detect external
    /// changes (config reload / reset) that need to be synced back into the
    /// `Select`.
    initial_value: Option<SharedString>,
    _subscription: Subscription,
}

/// Build the "Font Family" setting field using a searchable, scrollable
/// `Select` from gpui-component.
///
/// The dropdown shows ~5 items at a time (`menu_max_h(rems(10.))`) with a
/// search input for filtering the full OS font list.
fn font_family_field() -> SettingField<SharedString> {
    SettingField::element(
        move |options: &RenderOptions, window: &mut Window, cx: &mut App| {
            let key = SharedString::from(format!(
                "font-family-select-{}-{}-{}",
                options.page_ix, options.group_ix, options.item_ix
            ));

            let state_entity = window.use_keyed_state(key, cx, |window, cx| {
                let list = font_name_list(cx);
                let selected_ix = initial_font_index(&list, cx);

                let select = cx.new(|cx| {
                    SelectState::new(
                        SearchableVec::new(list),
                        selected_ix.map(|i| IndexPath::default().row(i)),
                        window,
                        cx,
                    )
                    .searchable(true)
                });

                let _subscription = cx.subscribe_in(&select, window, {
                    move |state: &mut FontSelectState,
                          _,
                          event: &SelectEvent<SearchableVec<SharedString>>,
                          _,
                          cx| {
                        match event {
                            SelectEvent::Confirm(value) => {
                                let family = value.as_ref().and_then(|v| {
                                    if v.as_ref() == DEFAULT_FONT_SENTINEL {
                                        None
                                    } else {
                                        Some(v.clone())
                                    }
                                });
                                state.initial_value = family.clone();
                                set(cx, |s| s.font_family = family);
                            }
                        }
                    }
                });

                FontSelectState {
                    select,
                    initial_value: TerminalSettings::global(cx).read(cx).font_family.clone(),
                    _subscription,
                }
            });

            // Sync external changes (e.g. config file reload or reset).
            let current = TerminalSettings::global(cx).read(cx).font_family.clone();
            state_entity.update(cx, |state, cx| {
                if state.initial_value != current {
                    state.initial_value = current.clone();
                    let select_value = current
                        .clone()
                        .unwrap_or_else(|| DEFAULT_FONT_SENTINEL.into());
                    state.select.update(cx, |select, cx| {
                        select.set_selected_value(&select_value, window, cx);
                    });
                }
            });

            let select = state_entity.read(cx).select.clone();

            div()
                .map(|this| {
                    if options.layout.is_horizontal() {
                        this.w(px(240.))
                    } else {
                        this.w_full()
                    }
                })
                .child(
                    Select::new(&select)
                        .search_placeholder("Search fonts...")
                        .menu_max_h(rems(10.)),
                )
                .into_any_element()
        },
    )
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
    oneterm_settings::terminal_settings::parse_weight(s)
}

// ── Line Height custom number field ─────────────────────────────────────
//
// `SettingField::number_input` from gpui-component does NOT propagate
// `NumberFieldOptions.step` to the internal `InputState` (it defaults to
// 1.0). The increment/decrement buttons therefore step by 1 instead of the
// configured 0.1.
//
// To fix this we use `SettingField::element` with a custom `NumberInput` that
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
    SettingField::element(
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
                                set(cx, |s| s.line_height_factor = clamped as f32);
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
