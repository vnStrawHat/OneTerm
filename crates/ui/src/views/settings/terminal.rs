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

use gpui::{App, FontWeight, SharedString};
use gpui_component::{
    Icon, IconName,
    setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage},
};
use oneterm_core::config::ShellKind;

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
        .item(
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
        )
        .item(
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
        )
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
        .item(
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
        )
        .item(
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
        )
        .item(
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
        )
        .item(
            SettingItem::new(
                "Line Height",
                SettingField::number_input(
                    NumberFieldOptions {
                        min: 1.0,
                        max: 3.0,
                        step: 0.1,
                    },
                    |cx: &App| TerminalSettings::global(cx).read(cx).line_height_factor as f64,
                    |val: f64, cx: &mut App| {
                        TerminalSettings::global(cx).update(cx, |s, cx| {
                            s.line_height_factor = val as f32;
                            cx.notify();
                        });
                        persist(cx);
                    },
                ),
            )
            .description("Line height multiplier (1.2 = 120% of the font size)."),
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
    crate::state::terminal_settings::parse_weight(s)
}
