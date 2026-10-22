#!/usr/bin/env python3
"""Check that text in the built-in themes clears the WCAG AA contrast floor, in order.

Secondary text (`muted.foreground`, `tab.foreground`, `table.head.foreground`) carries the
most data-bearing values on a row -- host addresses, SFTP dates, key-binding chips,
placeholders -- so it has to be readable, not decorative. Primary text (`foreground`) is
every label, name and value beside them. This script computes the WCAG relative-luminance
contrast ratio of each of those tokens against every surface `SURFACES` says it is drawn on,
for every variant of every theme in `crates/theme/themes/`, and applies two rules:

* **Floor** -- no token drops below 4.5:1 on any of its surfaces. The one exception is a
  non-text glyph, which WCAG 1.4.11 holds to 3:1: `GLYPH_SURFACES` lists those (the OSC 7501
  "waiting" bell) and every one of them is measured at `GLYPH_FLOOR` on every surface it is
  drawn on, one rule for all of them (owner ruling, 2026-10-21).
* **Hierarchy** -- on every surface both are drawn on, `foreground` reads *more* strongly
  than `muted.foreground` (`HIERARCHY`). Raising only the secondary token clears the floor
  and inverts the page: secondary text louder than the primary text beside it is not a
  readable page, it is a differently broken one (`US-0127`).

`SURFACES` is the whole contract: a foreground paired with a surface the application never
paints under it proves nothing, and a surface left out of the list is simply not checked.
Every entry below therefore cites the source line that draws that pair. When a component
starts drawing one of these tokens on a background that is not listed, add the surface here
(and its parent in `PARENTS`, and its fallback in `FALLBACKS` if the kit gives it one)
rather than assuming an existing entry covers it.

Stdlib only, offline, no dependencies.

Usage:
    python scripts/check-theme-contrast.py              # fail on any variant below the floor
    python scripts/check-theme-contrast.py --report     # print the full ratio table
    python scripts/check-theme-contrast.py --self-test  # check the ratio maths itself
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
THEMES_DIR = REPO_ROOT / "crates" / "theme" / "themes"

# WCAG 2.1 AA for body text.
FLOOR = 4.5
# WCAG 2.1 1.4.11 for a graphic that carries information (a status icon): 3:1 against its
# neighbouring colours. Only the tokens in `GLYPH_SURFACES` get it.
GLYPH_FLOOR = 3.0

# The surfaces each checked foreground token is composited over in OneTerm, with the line
# that draws the pair. A token that sits on more than one surface must clear the floor on all
# of them, so the reported ratio is the worst of the set.
SURFACES: dict[str, tuple[str, ...]] = {
    # `foreground` -- primary text. Most of it carries no colour of its own: the kit sets the
    # window's default text colour once (`reference/gpui-kit/crates/component/src/root.rs:593`)
    # and every label that does not override it inherits that. The distinct surfaces:
    #   background            the window body under that default, the tab-rename dialog's
    #                         field label (`crates/terminal-view/src/panel/tab_title.rs:543`)
    #                         on a dialog whose panel is the body colour (kit
    #                         `dialog/dialog.rs:574`), and the drag preview of a tab
    #                         (`crates/terminal-view/src/space/render.rs:46`)
    #   popover.background    a menu row's label (kit `menu/menu_item.rs:105`) and the
    #                         completion overlay's own rows, both popover-backed
    #   title_bar.background  the app menu bar's top-level names: `menu/app_menu_bar.rs` sets
    #                         no text colour, so they inherit the root's `foreground`
    #                         (`crates/workspace/src/layout/title_bar.rs:77`)
    #   tab_bar.background    the dock panel headers, "Session" and "SFTP Browser": the kit's
    #                         `render_title` colours the header only
    #                         `when_some(title_style, ...)` (kit
    #                         `dock/tab_panel.rs:356-425`, the `when_some` at `:380-382`) and
    #                         `Panel::title_style` defaults to `None` (kit
    #                         `dock/panel.rs:87-89`), which neither `SessionPanel` nor
    #                         `SftpPanel` overrides -- so the header inherits the root's
    #                         `foreground` on the strip's own fill. An *inactive tab* label on
    #                         the same strip is `tab.foreground`, measured under that token;
    #                         both are drawn there, so both are listed.
    #   status_bar.background a status segment in the `Foreground` tone
    #                         (`crates/workspace/src/widgets/status_text.rs:42,343`)
    #   list.background       the session tree's session and group labels
    #                         (`crates/session-ui/src/tree_render.rs:170`, kit
    #                         `list/list_item.rs:192`)
    #   list.even.background  its alternating row
    #   list.hover.background the hovered *and selected* tree row: `list_item.rs:192` keeps
    #                         `foreground` while only the fill changes, and OneTerm overrides
    #                         the selection style to equal hover
    #                         (`crates/theme/src/theme.rs:69-77`)
    #   table.background      the SFTP file-name cell
    #                         (`crates/sftp-ui/src/table_delegate.rs:54`)
    #   table.even.background its alternating row
    #   table.hover.background the hovered and selected SFTP row
    #   table.head.background the local pane's column headers, which unlike the remote
    #                         table's use the primary colour
    #                         (`crates/sftp-ui/src/local_pane.rs:276`)
    #
    # Deliberately absent, because primary text is not what is drawn there:
    #   accent.background     a hovered or selected menu row swaps to `accent.foreground`
    #                         (kit `menu/menu_item.rs:115-120`), measured under that token
    #   muted.background      the key-binding chip's text is the secondary token
    #   sidebar.background / tab.active.background
    #                         each has its own primary token below, so measuring `foreground`
    #                         there as well would score a colour the kit does not paint on
    #                         that surface in the themes that set the more specific token.
    "foreground": (
        "background",
        "popover.background",
        "title_bar.background",
        "status_bar.background",
        "tab_bar.background",
        "list.background",
        "list.even.background",
        "list.hover.background",
        "table.background",
        "table.even.background",
        "table.hover.background",
        "table.head.background",
    ),
    # `popover.foreground` -- primary text on a popover that the kit routes through the more
    # specific token: a popup menu's title and empty label (kit `menu/popup_menu.rs:1441`),
    # a tooltip's body (kit `tooltip.rs:115`) and the command palette (kit
    # `command/state.rs:828`), and the FPS HUD's row values
    # (`crates/workspace/src/widgets/fps_hud.rs`). It falls back to `foreground` (kit `theme/schema.rs:970`), so
    # this row measures the primary colour in the variants that leave it unset and the
    # override in the 20 that set it.
    "popover.foreground": ("popover.background",),
    # `sidebar.foreground` -- the Settings sidebar's page rows, the only colour drawn on that
    # surface as primary text (kit `sidebar/mod.rs:414`; group headings use the same value at
    # 70 % opacity, `sidebar/group.rs:69`). Falls back to `foreground` (kit
    # `theme/schema.rs:984`), which is what 36 of the 39 variants take.
    "sidebar.foreground": ("sidebar.background",),
    # `tab.active.foreground` -- the active tab's label, the strip's primary text (kit
    # `tab/tab.rs:175`, labelled by `crates/terminal-view/src/panel/tab_title.rs:244`).
    # Falls back to `foreground` (kit `theme/schema.rs:997`). Two surfaces:
    #   tab.active.background  the selected tab or rail row
    #   tab_bar.background     a *hovered* inactive tab or rail row, which swaps only the
    #                          text colour over the bar's own fill (kit `tab/tab.rs:172-184`;
    #                          the vertical rail, `crates/workspace/src/layout/workspace/
    #                          tab_rail.rs`, draws the same pair). Added by `IN-0055`; it also
    #                          turns on the `HIERARCHY` comparison against `muted.foreground`
    #                          on that fill, because `muted.foreground` is listed there too.
    #   tab_bar.hover.background  the same rail row under the pointer, which now also gets the
    #                          list hover fill over the bar (owner rework 5 of `US-0196`;
    #                          `tab_rail.rs`, the `group_hover` fill), at half strength since
    #                          owner rework 7 so that it never outranks the selected row
    #   tab_bar.selected.background  the selected rail row since owner rework 7: `Theme::list_active`
    #                          (`tab.active.background` is the terminal's own colour, which cannot
    #                          make the row stand out), and OneTerm's list-style override
    #                          (`crates/theme/src/theme.rs:69-77`) makes that `list.hover.background`
    #   tab.active.background  stays for the horizontal strip's active tab and the rail's drag
    #                          preview chip
    "tab.active.foreground": (
        "tab.active.background",
        "tab_bar.background",
        "tab_bar.hover.background",
        "tab_bar.selected.background",
    ),
    # `accent.foreground` -- the label of a hovered or selected menu row, which swaps both
    # fill and text (kit `menu/menu_item.rs:115-120`): every OneTerm popup menu -- the tab
    # strip's `+` menu, the terminal and session context menus, the app menu's dropdowns.
    # Falls back to `foreground` (kit `theme/schema.rs:904`). `muted.foreground` is drawn on
    # the same row (the shortcut hint), so this pair is in `HIERARCHY` too.
    "accent.foreground": ("accent.background",),
    # `secondary.foreground` -- text on a secondary fill: a `Secondary` button, whose own
    # tokens fall back to this pair (`button.secondary.background` -> `secondary.background`,
    # kit `theme/schema.rs:828`; `button.secondary.foreground` -> `secondary.foreground`,
    # `:830-832`; only 1 of the 39 variants overrides either), and a secondary `Tag` (kit
    # `tag.rs:30` fill, `tag.rs:78` text). `secondary.foreground` falls back to `foreground`
    # (kit `theme/schema.rs:819`). OneTerm reaches this pair through the kit's components
    # rather than a call site of its own, which is why it has no OneTerm citation; it is
    # listed because the colours are the theme's and a theme author has to keep them legible.
    # `muted.foreground` is not drawn on `secondary.background`, so there is no hierarchy
    # comparison here.
    "secondary.foreground": ("secondary.background",),
    # `muted.foreground` -- 21 call sites across the feature crates. The distinct surfaces:
    #   background            the window body and the empty-Space placeholder
    #                         (`crates/terminal-view/src/space/render.rs:233`), and the
    #                         Settings pages, whose GroupBox is the `Outline` variant and so
    #                         paints no fill of its own
    #                         (`crates/settings-ui/src/panel.rs:29`,
    #                         `reference/gpui-kit/crates/component/src/group_box.rs:133-135`)
    #   popover.background    the completion overlay
    #                         (`crates/terminal-view/src/completion/overlay.rs:98`), menu
    #                         rows and their shortcut hints
    #                         (`crates/terminal-view/src/panel/terminal_panel.rs:76`,
    #                         kit `menu/menu_item.rs:130`, `menu/popup_menu.rs:1341`),
    #                         tooltips and notification toasts (kit `tooltip.rs`,
    #                         `notification.rs`, both popover-backed), and the FPS HUD's
    #                         row labels (`crates/workspace/src/widgets/fps_hud.rs`)
    #   accent.background     a hovered menu row, under the same shortcut hint: the `Kbd`
    #                         is drawn transparent over the row
    #                         (kit `menu/popup_menu.rs:1114`, `menu/menu_item.rs:115`)
    #   muted.background      the key-binding chip, `Kbd` without `outline`
    #                         (`crates/settings-ui/src/key_bindings/key_bindings_ui.rs:166`,
    #                         kit `kbd.rs:223-225`, `kbd.rs:35`)
    #   sidebar.background    the Settings sidebar and dock surfaces
    #   title_bar.background  /  status_bar.background   the two strips
    #                         (`crates/workspace/src/widgets/status_text.rs:39`)
    #   list.background       the session tree row
    #                         (`crates/session-ui/src/tree_render.rs:136`)
    #   list.even.background  its alternating row
    #   list.hover.background the hovered *and selected* tree row: the selection style is
    #                         overridden to equal hover
    #                         (`crates/theme/src/theme.rs:69-77`)
    #   table.background      the SFTP table row
    #                         (`crates/sftp-ui/src/table_delegate.rs:317`)
    #   table.even.background its alternating row
    #   table.hover.background the hovered and selected SFTP row
    #                         (`crates/sftp-ui/src/table_delegate.rs:284-285`,
    #                         `crates/sftp-ui/src/local_pane.rs:236`)
    #   tab_bar.background / tab.active.background  the tab strip's subtitle and channel
    #                         chips, on an inactive and on the active tab
    #                         (`crates/terminal-view/src/panel/tab_title.rs:110`), the `x`
    #                         close button, and the vertical rail's reorder grip
    #                         (`crates/workspace/src/layout/workspace/tab_rail.rs`)
    "muted.foreground": (
        "background",
        "popover.background",
        "accent.background",
        "muted.background",
        "sidebar.background",
        "title_bar.background",
        "status_bar.background",
        "list.background",
        "list.even.background",
        "list.hover.background",
        "table.background",
        "table.even.background",
        "table.hover.background",
        "tab_bar.background",
        "tab_bar.hover.background",
        "tab_bar.selected.background",
        "tab.active.background",
    ),
    # `tab.foreground` -- an inactive tab label, drawn on the strip; `tab.background` is
    # usually the body colour at zero alpha, so both surfaces are measured (kit
    # `tab/tab.rs`, theme `tab.background` / `tab_bar.background`).
    "tab.foreground": ("tab_bar.background", "tab.background"),
    # `table.head.foreground` -- SFTP and session column headers
    # (`crates/sftp-ui/src/table_delegate.rs:440`).
    "table.head.foreground": ("table.head.background",),
    # `warning.background` -- despite the name, OneTerm draws this token as **text**, and
    # has for longer than `IN-0043`. `Theme::warning` is what `cx.theme().warning` returns,
    # and the sites are:
    #   title_bar.background  the elevation suffix, "(Administrator)" or "(elevation
    #                         unknown)", beside the plain app name
    #                         (`crates/workspace/src/layout/title_bar.rs`). A *marker*, so it
    #                         has to be read at a glance, not merely noticed
    #                         (`IN-0043`, `DEC-0019` M5)
    #   background            the settings window's read-only note
    #                         (`crates/settings-ui/src/panel.rs`), the key-bindings notice
    #                         (`crates/settings-ui/src/key_bindings/key_bindings_ui.rs:216`),
    #                         the port-forwarding row's warning
    #                         (`crates/session-ui/src/forward_rows.rs:292`), the terminal's
    #                         own warning line and `Alert::warning`
    #                         (`crates/terminal-view/src/terminal_view/render.rs:510,603`)
    #   popover.background    a `Warning` notification's whole body
    #                         (`crates/theme/src/notif_ext.rs:86`), which is popover-backed
    #
    # The tab title's "waiting for you" bell (OSC 7501 `blocked`,
    # `crates/terminal-view/src/panel/tab_title.rs`) also uses this token, but it is a glyph,
    # not text, so it is measured at the 3:1 of `GLYPH_SURFACES` below and not here.
    #
    # **The name is the trap this entry exists to close.** `ThemeConfigColors` renames the
    # field to `warning.background` (`gpui-component/src/theme/schema.rs:617`) while
    # `HighlightThemeStyle` accepts a bare `warning` -- and the struct has no
    # `deny_unknown_fields`, so the wrong spelling inside `colors` is dropped in silence and
    # this script happily measures a value the window never draws. `IN-0043` did exactly
    # that for one round. `oneterm-theme`'s `warning_is_the_value_the_theme_file_names`
    # drives the kit's real `apply_config` and is what makes this row honest.
    #
    # Every variant now sets it explicitly. Without that the kit falls back to
    # `self.yellow` (`schema.rs:878`) -- not a fixed amber but each theme's own base yellow,
    # which is not in this JSON and so is not measurable here at all. 16 variants were below
    # the floor as rendered and took a lightness-only correction of their own hue; the other
    # 23 have their existing colour written out verbatim, which changes nothing on screen
    # and everything about what this gate can prove.
    "warning.background": (
        "title_bar.background",
        "background",
        "popover.background",
    ),
}

# Tokens drawn as a non-text glyph, held to `GLYPH_FLOOR` (3:1) on every surface they are drawn
# on -- the same rule everywhere, not a per-surface exception (owner ruling, 2026-10-21).
#   warning.background   the tab title's "waiting for you" bell (OSC 7501 `blocked`,
#                        `crates/terminal-view/src/panel/tab_title.rs`): on an inactive tab
#                        (`tab_bar.background`, `tab.background`), the selected tab
#                        (`tab.active.background`; in the rail `tab_bar.selected.background`)
#                        and a vertical-rail row under the pointer
#                        (`tab_bar.hover.background`, the list hover fill over the bar,
#                        `crates/workspace/src/layout/workspace/tab_rail.rs`). Its text uses
#                        are in `SURFACES` above, at 4.5:1.
GLYPH_SURFACES: dict[str, tuple[str, ...]] = {
    "warning.background": (
        "tab_bar.background",
        "tab.background",
        "tab.active.background",
        "tab_bar.hover.background",
        "tab_bar.selected.background",
    ),
}

# The collapsed tab cell's glyph on its status fill (`US-0198` rework 6, `collapsed_fill` in
# `crates/terminal-view/src/panel/tab_title.rs`): the fill is the theme's `primary`, `warning`,
# `success` or `danger`, and the glyph (and the recording dot on it) is whichever of the theme's
# `background` and `foreground` reads better on it (`oneterm_theme::legible_on`, owner ruling
# 2026-10-22; the themes' own `*_foreground` tokens were below 3:1 in 23 of 39 variants). So the
# measure is the better of the two against each fill, held to `GLYPH_FLOOR`, and the row is
# `background|foreground (glyph)` on the fill.
PICKED_GLYPH_FILLS: tuple[str, ...] = (
    "primary.background",
    "warning.background",
    "success.background",
    "danger.background",
)
PICKED_GLYPH = "background|foreground (glyph)"
# The picked glyph still reads below `GLYPH_FLOOR` on these (variant, fill) pairs: the better of
# the theme's own `background` and `foreground` cannot reach 3:1 on that fill. Recorded so the
# check gates every other pair and every other variant; they print on each run. The owner
# accepted them on 2026-10-22 and no theme is retuned. Delete an entry when the theme is fixed
# (a stale entry fails the run).
KNOWN_BELOW: frozenset[tuple[str, str]] = frozenset({
    ("Catppuccin Latte", "primary.background"),      # 2.61:1
    ("Everforest Light", "primary.background"),      # 2.81:1
    ("Everforest Light", "success.background"),      # 2.84:1
    ("Everforest Light", "danger.background"),       # 2.66:1
    ("Solarized Light", "success.background"),       # 2.99:1
})

# The hierarchy rule, as (primary, secondary) pairs. On every surface both tokens are drawn
# on -- the intersection of their `SURFACES` entries -- the primary token's ratio must be
# strictly greater than the secondary's. Equal is a failure too: text that reads exactly as
# loud as the label beside it has no hierarchy left. `tab.foreground` is not a primary token
# (an inactive tab label is secondary by design) and so is not compared here.
HIERARCHY: tuple[tuple[str, str], ...] = (
    ("foreground", "muted.foreground"),
    ("popover.foreground", "muted.foreground"),
    ("sidebar.foreground", "muted.foreground"),
    ("tab.active.foreground", "muted.foreground"),
    ("accent.foreground", "muted.foreground"),
)

# The surface each surface is painted on, so a translucent one is composited over what is
# actually behind it rather than over the window body. Anything absent sits on `background`.
PARENTS: dict[str, str] = {
    "tab.background": "tab_bar.background",
    "tab.active.background": "tab_bar.background",
    "tab_bar.hover.background": "tab_bar.background",
    "tab_bar.selected.background": "tab_bar.background",
    "accent.background": "popover.background",
    "list.even.background": "list.background",
    "list.hover.background": "list.background",
    "list.head.background": "list.background",
    "table.even.background": "table.background",
    "table.hover.background": "table.background",
    "table.head.background": "table.background",
}

# Token fallbacks, mirroring `ThemeColor::apply_config` in
# `reference/gpui-kit/crates/component/src/theme/schema.rs:180-1020`: a theme that omits a
# token inherits what is named here, so the check measures what the application actually
# draws. A plain string is another token; `("blend", base, over, alpha)` is the kit's
# `base.blend(over.opacity(alpha))`; `("alpha", token, a)` is `token.opacity(a)`.
#
# Tokens with no entry and no value in the theme are an error rather than a guess: the kit
# falls back to its own built-in palette for those, which is not expressible in terms of the
# theme's own colours. Every built-in theme defines `background`, `foreground`, `border`,
# `muted.background` and `accent.background`, which is what the entries below rest on.
FALLBACKS: dict[str, str | tuple] = {
    "popover.background": "background",                                  # schema.rs:969
    "sidebar.background": ("blend", "background", "border", 0.15),       # schema.rs:977-980
    "title_bar.background": "background",                                # schema.rs:1011
    "status_bar.background": "title_bar.background",                     # schema.rs:1013
    "list.background": "background",                                     # schema.rs:957
    "list.even.background": "list.background",                           # schema.rs:966
    "list.hover.background": ("alpha", "accent.background", 0.6),        # schema.rs:968
    "list.head.background": "list.background",                           # schema.rs:967
    "table.background": "list.background",                               # schema.rs:1001
    "table.even.background": "list.even.background",                     # schema.rs:1004
    "table.hover.background": "list.hover.background",                   # schema.rs:1009
    "table.head.background": "list.head.background",                     # schema.rs:1005
    "tab_bar.background": "background",                                  # schema.rs:998
    # Not kit tokens. The rail row's hover fill is `list.hover.background` at half alpha, drawn
    # over the bar; its selected fill is `Theme::list_active`, which OneTerm sets to
    # `list.hover.background` after every theme application (owner rework 7 of `US-0196`).
    "tab_bar.hover.background": ("alpha", "list.hover.background", 0.5),
    "tab_bar.selected.background": "list.hover.background",
    "tab.background": "background",                                      # schema.rs:995
    "tab.active.background": "background",                               # schema.rs:996
    "tab.foreground": "foreground",                                      # schema.rs:1000
    "tab.active.foreground": "foreground",                               # schema.rs:997
    "popover.foreground": "foreground",                                  # schema.rs:970
    "sidebar.foreground": "foreground",                                  # schema.rs:984
    "accent.foreground": "foreground",                                   # schema.rs:904
    "secondary.foreground": "foreground",                                # schema.rs:819
    "table.head.foreground": "muted.foreground",                         # schema.rs:1006
    "muted.foreground": ("blend", "muted.background", "foreground", 0.7),  # schema.rs:777-779
}

# What the kit shows for a status background a theme omits: the built-in palette of the
# theme's mode (`default-theme.json`: `success.background` green-500 / green-400,
# `danger.background` red-500 / red-400, resolved through `default-colors.json`). It has no
# expression in the theme's own colours, so `variants()` writes it into the variant. 35 of the
# 39 variants omit `success.background`, 25 omit `danger.background`; `warning.background` is
# set by every one (see its `SURFACES` entry), `primary.background` too.
KIT_DEFAULTS: dict[str, dict[str, str]] = {
    "light": {"success.background": "#22c55e", "danger.background": "#ef4444"},
    "dark": {"success.background": "#4ade80", "danger.background": "#f87171"},
}

HEX = re.compile(r"#[0-9a-fA-F]{3,8}\b")


def parse_hex(value: str) -> tuple[float, float, float, float]:
    """Return (r, g, b, a) in 0..1 from a `#rgb` / `#rgba` / `#rrggbb` / `#rrggbbaa` string."""
    text = value.lstrip("#")
    if len(text) in (3, 4):
        text = "".join(ch * 2 for ch in text)
    if len(text) == 6:
        text += "ff"
    if len(text) != 8:
        raise ValueError(f"not a hex color: {value!r}")
    r, g, b, a = (int(text[i : i + 2], 16) / 255 for i in (0, 2, 4, 6))
    return r, g, b, a


def luminance(rgb: tuple[float, float, float]) -> float:
    """WCAG 2.1 relative luminance."""

    def channel(c: float) -> float:
        return c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4

    r, g, b = (channel(c) for c in rgb)
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def contrast(fg: tuple[float, float, float], bg: tuple[float, float, float]) -> float:
    a, b = luminance(fg), luminance(bg)
    lo, hi = min(a, b), max(a, b)
    return (hi + 0.05) / (lo + 0.05)


def composite(
    top: tuple[float, float, float, float], under: tuple[float, float, float]
) -> tuple[float, float, float]:
    """Source-over of a translucent colour onto an opaque one."""
    r, g, b, a = top
    return tuple(c * a + u * (1 - a) for c, u in zip((r, g, b), under))  # type: ignore[return-value]


def stops(colors: dict[str, str], token: str, seen: frozenset[str] = frozenset()) -> list:
    """Every colour a token resolves to: one per `linear-gradient(...)` stop, alpha kept."""
    if token in seen:
        raise ValueError(f"fallback cycle at {token}")
    value = colors.get(token)
    if value is not None:
        found = HEX.findall(value)
        if not found:
            raise ValueError(f"no colour in token value {value!r} for {token}")
        return [parse_hex(hit) for hit in found]

    rule = FALLBACKS.get(token)
    if rule is None:
        raise ValueError(f"{token}: the theme does not define it and the kit has no fallback")
    if isinstance(rule, str):
        return stops(colors, rule, seen | {token})
    kind = rule[0]
    if kind == "alpha":
        _, other, alpha = rule
        return [(r, g, b, a * alpha) for r, g, b, a in stops(colors, other, seen | {token})]
    if kind == "blend":
        _, base_token, over_token, alpha = rule
        base = stops(colors, base_token, seen | {token})[0][:3]
        return [
            (*composite((r, g, b, a * alpha), base), 1.0)
            for r, g, b, a in stops(colors, over_token, seen | {token})
        ]
    raise ValueError(f"unknown fallback rule {rule!r} for {token}")


def surface(colors: dict[str, str], token: str, depth: int = 0) -> list:
    """The opaque colours a background token really shows, composited over its parent."""
    own = stops(colors, token)
    if token == "background" or depth > 8:
        # The window body is the bottom of the stack; a theme that gives it alpha shows the
        # desktop, which is not a colour this check can know.
        return [c[:3] for c in own]
    under = surface(colors, PARENTS.get(token, "background"), depth + 1)[0]
    return [composite(c, under) for c in own]


def measure(colors: dict[str, str], fg_token: str, bg_token: str) -> float:
    """Worst-case contrast of `fg_token` over `bg_token`, across gradient stops."""
    worst = None
    for bg in surface(colors, bg_token):
        for fg in stops(colors, fg_token):
            ratio = contrast(composite(fg, bg), bg)
            worst = ratio if worst is None else min(worst, ratio)
    assert worst is not None
    return worst


def with_kit_defaults(mode: str, colors: dict[str, str]) -> dict[str, str]:
    """A variant's colours with the kit's built-in status backgrounds under the ones it omits."""
    return {**KIT_DEFAULTS[mode], **colors}


def variants() -> list[tuple[str, str, str, dict[str, str]]]:
    """(file name, variant name, mode, colors) for every variant of every built-in theme."""
    out = []
    for path in sorted(THEMES_DIR.glob("*.json")):
        document = json.loads(path.read_text(encoding="utf-8"))
        for theme in document.get("themes", []):
            mode = theme.get("mode", "dark")
            out.append((path.name, theme["name"], mode, with_kit_defaults(mode, theme["colors"])))
    return out


def shared_surfaces(primary: str, secondary: str) -> tuple[str, ...]:
    """The surfaces both tokens are drawn on -- where the hierarchy rule can be applied."""
    return tuple(bg for bg in SURFACES[primary] if bg in SURFACES[secondary])


def pairings() -> int:
    """How many foreground/surface pairs one pass actually measures."""
    return len(variants()) * sum(
        len(surfaces)
        for surfaces in (
            *SURFACES.values(),
            *GLYPH_SURFACES.values(),
        )
    ) + len(variants()) * len(PICKED_GLYPH_FILLS)


def comparisons() -> int:
    """How many primary/secondary surface comparisons one pass makes."""
    return len(variants()) * sum(len(shared_surfaces(*pair)) for pair in HIERARCHY)


def rows(
    only: list[tuple[str, str, str, dict[str, str]]] | None = None,
) -> list[tuple[str, str, str, str, str, float, float]]:
    """(file, variant, mode, fg token, worst surface, worst ratio, its floor) per checked token.

    A glyph token is its own row (`token (glyph)`), measured at `GLYPH_FLOOR`.
    """
    out = []
    for file_name, name, mode, colors in variants() if only is None else only:
        for table, floor, suffix in (
            (SURFACES, FLOOR, ""),
            (GLYPH_SURFACES, GLYPH_FLOOR, " (glyph)"),
        ):
            for fg_token, surfaces in table.items():
                measured = [(bg, measure(colors, fg_token, bg)) for bg in surfaces]
                bg, ratio = min(measured, key=lambda item: item[1])
                out.append((file_name, name, mode, fg_token + suffix, bg, ratio, floor))
        for fill in PICKED_GLYPH_FILLS:
            ratio = max(measure(colors, token, fill) for token in ("background", "foreground"))
            out.append((file_name, name, mode, PICKED_GLYPH, fill, ratio, GLYPH_FLOOR))
    return out


def inversions(
    only: list[tuple[str, str, str, dict[str, str]]] | None = None,
) -> list[tuple[str, str, str, str, str, float, float, int]]:
    """(file, variant, mode, primary, surface, primary ratio, secondary ratio, count).

    One row per variant and pair, for the surface with the smallest margin: a primary token
    that is too quiet is usually too quiet on every surface it is drawn on, and ten
    near-identical rows say it ten times. `count` is how many of that pair's shared surfaces
    are actually inverted, so a caller can report comparisons as well as rows -- the two are
    different units and 10 rows was 37 comparisons when this rule was written.
    """
    out = []
    for file_name, name, mode, colors in variants() if only is None else only:
        for primary, secondary in HIERARCHY:
            measured = [
                (bg, measure(colors, primary, bg), measure(colors, secondary, bg))
                for bg in shared_surfaces(primary, secondary)
            ]
            worst = min(measured, key=lambda item: item[1] - item[2])
            if worst[1] <= worst[2]:
                count = sum(1 for _, hi, lo in measured if hi <= lo)
                out.append((file_name, name, mode, primary, *worst, count))
    return out


def self_test(quiet: bool = False) -> int:
    """The ratio maths, against values whose contrast is known independently.

    Cheap enough that `main` runs it on every invocation: a check whose arithmetic is wrong
    passes every theme silently, which is worse than no check at all.
    """
    white, black = (1.0, 1.0, 1.0), (0.0, 0.0, 0.0)
    assert abs(contrast(white, black) - 21.0) < 1e-6, "black on white is 21:1"
    assert abs(contrast(white, white) - 1.0) < 1e-6, "a colour on itself is 1:1"
    # #767676 on white is the canonical "just clears AA" grey.
    grey = parse_hex("#767676")[:3]
    assert 4.5 <= contrast(grey, white) < 4.6, "#767676 on #ffffff clears AA by a hair"
    assert contrast(parse_hex("#777777")[:3], white) < 4.5, "one step lighter no longer does"
    # The finding this check exists for: #5c6370 on #23272e measured 2.48:1.
    assert abs(contrast(parse_hex("#5c6370")[:3], parse_hex("#23272e")[:3]) - 2.48) < 0.01
    # Pairing a foreground with the wrong surface passes text that is unreadable: the same
    # grey over white looks fine, over the panel it does not.
    assert contrast(parse_hex("#5c6370")[:3], white) > FLOOR
    # Alpha is composited, not ignored: a 60% panel over the body sits between the two.
    blended = composite(parse_hex("#1c1b1a99"), parse_hex("#100f0f")[:3])
    assert luminance(parse_hex("#100f0f")[:3]) < luminance(blended) < luminance(
        parse_hex("#1c1b1a")[:3]
    )
    # A gradient is measured at its worst stop, not its first.
    colors = {"background": "#ffffff", "muted.foreground": "#767676",
              "title_bar.background": "linear-gradient(180deg, #ffffff, #cccccc)"}
    assert measure(colors, "muted.foreground", "title_bar.background") < 4.5
    # A translucent surface is composited over its own parent, not over the window body: a
    # half-transparent white tab over a white strip stays white, however dark the body is.
    colors = {"background": "#000000", "tab_bar.background": "#ffffff",
              "tab.background": "#ffffff80", "tab.foreground": "#767676"}
    assert abs(measure(colors, "tab.foreground", "tab.background") - 4.54) < 0.01
    # The kit's derived fallbacks are applied, not skipped: a theme with no sidebar, no
    # list.hover and no muted.foreground still resolves, and to the kit's values.
    colors = {"background": "#000000", "foreground": "#ffffff", "border": "#ffffff",
              "muted.background": "#000000", "accent.background": "#ffffff"}
    assert abs(surface(colors, "sidebar.background")[0][0] - 0.15) < 0.01, "border at 15%"
    assert abs(surface(colors, "list.hover.background")[0][0] - 0.6) < 0.01, "accent at 60%"
    assert abs(stops(colors, "muted.foreground")[0][0] - 0.7) < 0.01, "foreground at 70%"
    # The floor rule catches primary text that is too quiet. `#8a8a8a` on white is 3.05:1.
    faint = {"background": "#ffffff", "foreground": "#8a8a8a", "muted.foreground": "#767676",
             "border": "#000000", "muted.background": "#ffffff", "accent.background": "#767676"}
    assert measure(faint, "foreground", "background") < FLOOR, "primary below the floor"
    # The hierarchy rule catches the `US-0127` shape: both tokens clear 4.5:1, but the
    # secondary one reads more strongly, which is the inversion `US-0111` left behind.
    for pair in HIERARCHY:
        assert shared_surfaces(*pair), f"{pair} share no surface, so the rule never fires"
    inverted = {"background": "#ffffff", "foreground": "#767676", "muted.foreground": "#595959",
                "border": "#000000", "muted.background": "#ffffff",
                "accent.background": "#ffffff", "secondary.background": "#ffffff"}
    assert measure(inverted, "foreground", "background") >= FLOOR, "primary clears the floor"
    assert measure(inverted, "muted.foreground", "background") >= FLOOR, "so does secondary"
    found = inversions([("fixture.json", "Fixture", "light", inverted)])
    # Every primary token falls back to `foreground` here, so every pair reports, and each
    # row names how many of that pair's shared surfaces are inverted.
    assert len(found) == len(HIERARCHY) and all(row[5] < row[6] for row in found), found
    assert sum(row[7] for row in found) == sum(
        len(shared_surfaces(*pair)) for pair in HIERARCHY
    ), "every shared surface of every pair is inverted in this fixture"
    # ...and passes once the primary is the darker of the two.
    upright = dict(inverted, foreground="#404040")
    assert not inversions([("fixture.json", "Fixture", "light", upright)])
    # Equal is not "above": a tie has no hierarchy left either.
    tied = dict(inverted, foreground=inverted["muted.foreground"])
    assert inversions([("fixture.json", "Fixture", "light", tied)]), "a tie is a failure"
    # A theme that overrides one primary token is judged on the override, not on `foreground`:
    # the popover row alone fails when only `popover.foreground` is lowered.
    only_popover = dict(upright, **{"popover.foreground": "#767676"})
    found = inversions([("fixture.json", "Fixture", "light", only_popover)])
    assert [row[3] for row in found] == ["popover.foreground"], found
    # The glyph tier is a lower floor, not a skipped check, and `rows()` applies it to the glyph
    # row alone: a bell at about 3.4:1 on its worst surface passes its own row and would fail the text floor, about 2.4:1 fails it.
    fixture = {"background": "#ffffff", "foreground": "#000000", "border": "#cccccc",
               "muted.background": "#eeeeee", "accent.background": "#dddddd",
               "secondary.background": "#eeeeee", "primary.background": "#000000",
               "primary.foreground": "#ffffff", "success.background": "#008000",
               "danger.background": "#c00000"}
    for bell, passes in (("#808080", True), ("#a8a8a8", False)):
        measured = rows([("fixture.json", "Fixture", "light", dict(fixture, **{"warning.background": bell}))])
        glyph = [row for row in measured if row[3] == "warning.background (glyph)"]
        text = [row for row in measured if row[3] == "warning.background"]
        assert len(glyph) == 1 and len(text) == 1, measured
        assert glyph[0][6] == GLYPH_FLOOR and text[0][6] == FLOOR, "each tier carries its floor"
        assert (glyph[0][5] >= glyph[0][6]) is passes, glyph
        assert glyph[0][5] < FLOOR, "a 3:1-class bell would fail the text floor"
    # The picked-glyph tier (the collapsed cell's glyph on a status fill) is the better of
    # `background` and `foreground` against each fill, held to the glyph floor.
    fixture = dict(fixture, **{"warning.background": "#808080"})
    fixture_rows = rows([("fixture.json", "Fixture", "light", fixture)])
    picked = [r for r in fixture_rows if r[3] == PICKED_GLYPH]
    assert [r[4] for r in picked] == list(PICKED_GLYPH_FILLS), picked
    assert all(r[6] == GLYPH_FLOOR and r[5] >= GLYPH_FLOOR for r in picked), picked
    # ...white on the black primary fill is 21:1, the better token, not the worse one.
    assert abs(picked[0][5] - 21.0) < 0.01, picked[0]
    # When neither token can reach it the row fails, and only `KNOWN_BELOW` can exempt it.
    muddy = dict(fixture, background="#808080", foreground="#909090", **{"warning.background": "#888888"})
    muddy_rows = rows([("fixture.json", "Muddy", "light", muddy)])
    low = [r for r in muddy_rows if r[3] == PICKED_GLYPH and r[4] == "warning.background"]
    assert len(low) == 1 and low[0][5] < GLYPH_FLOOR, low
    known, failures, stale = split_known(muddy_rows, frozenset())
    assert low[0] in failures and not known and not stale
    known, failures, stale = split_known(muddy_rows, frozenset({("Muddy", "warning.background")}))
    assert low[0] in known and low[0] not in failures and not stale
    # A stale exemption -- a pair that reads, or one that never matched -- is reported, so it
    # fails the run instead of being kept for ever.
    fine = ("Fixture", "primary.background")
    gone = ("Nowhere", "success.background")
    assert split_known(fixture_rows, frozenset({fine, gone}))[2] == sorted({fine, gone})
    assert not split_known(fixture_rows, frozenset())[2]
    # The summary counts exempt rows instead of claiming every glyph clears the floor.
    clean = summary(10, 5, 0, "muted.foreground", 3)
    assert f"glyphs >= {GLYPH_FLOOR}:1;" in clean and "known" not in clean, clean
    exempt = summary(10, 5, 5, "muted.foreground", 3)
    assert "5 known glyph rows below the floor" in exempt, exempt
    assert f"glyphs >= {GLYPH_FLOOR}:1;" not in exempt, exempt
    # The kit's status backgrounds fill in what a theme omits and never override what it sets.
    assert with_kit_defaults("light", {})["success.background"] == KIT_DEFAULTS["light"]["success.background"]
    assert with_kit_defaults("dark", {})["danger.background"] == KIT_DEFAULTS["dark"]["danger.background"]
    assert with_kit_defaults("dark", {"success.background": "#123456"})["success.background"] == "#123456"
    assert KIT_DEFAULTS["light"] != KIT_DEFAULTS["dark"], "the defaults are per mode"
    # A token with no value and no kit fallback is reported, not a traceback.
    try:
        stops({"background": "#000000"}, "muted.foreground")
    except ValueError as err:
        assert "muted.background" in str(err), err
    else:
        raise AssertionError("a theme missing every fallback source must be reported")
    if not quiet:
        print("check-theme-contrast: self-test passed")
    return 0


def split_known(
    measured: list, known_below: frozenset[tuple[str, str]] | None = None
) -> tuple[list, list, list]:
    """(known, failures, stale) of the rows below their floor.

    `known` are the picked-glyph rows `KNOWN_BELOW` exempts, `failures` every other row below
    its floor, and `stale` the `KNOWN_BELOW` entries no row below the floor matches -- the pair
    now reads, or the variant or fill is gone -- which must be deleted, not kept for ever.
    """
    exempt = KNOWN_BELOW if known_below is None else known_below
    below = [row for row in measured if row[5] < row[6]]
    known = [row for row in below if row[3] == PICKED_GLYPH and (row[1], row[4]) in exempt]
    failures = [row for row in below if row not in known]
    stale = sorted(exempt - {(row[1], row[4]) for row in known})
    return known, failures, stale


def summary(pairs: int, row_count: int, known: int, secondary: str, compared: int) -> str:
    """The closing line of a passing run; it counts the exempt glyph rows instead of hiding them."""
    glyphs = (
        f"glyphs >= {GLYPH_FLOOR}:1"
        if not known
        else f"glyphs >= {GLYPH_FLOOR}:1 except {known} known glyph rows below the floor"
    )
    return (
        f"check-theme-contrast: {pairs} foreground/surface pairings across {row_count} "
        f"token/variant rows, text >= {FLOOR}:1, {glyphs}; primary text out-reads "
        f"{secondary} on all {compared} shared-surface comparisons"
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", action="store_true", help="print every variant, not just failures")
    parser.add_argument("--self-test", action="store_true", help="check the ratio maths and exit")
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    self_test(quiet=True)
    measured = rows()
    known, failures, stale = split_known(measured)
    inverted = inversions()
    inverted_comparisons = sum(row[7] for row in inverted)
    secondary = HIERARCHY[0][1]

    if args.report:
        width = max(len(row[1]) for row in measured)
        for file_name, name, mode, fg_token, bg_token, ratio, floor in measured:
            mark = "FAIL" if ratio < floor else "ok  "
            print(f"{mark} {name:<{width}} {mode:<5} {fg_token:<22} on {bg_token:<24} {ratio:5.2f}:1")
        print(
            f"\n{len(measured)} measurements, {len(failures)} below their floor "
            f"({FLOOR}:1 text, {GLYPH_FLOOR}:1 glyph)"
        )
        for file_name, name, mode, fg_token, bg_token, hi, lo, count in inverted:
            print(
                f"INVERTED {name:<{width}} {mode:<5} {fg_token:<22} worst on "
                f"{bg_token:<24} {hi:5.2f}:1 <= {lo:5.2f}:1 ({count} surface(s))"
            )
        print(
            f"{comparisons()} primary/{secondary} comparisons, {inverted_comparisons} "
            f"inverted across {len(inverted)} token/variant row(s)"
        )

    if failures or inverted or stale:
        for name, fill in stale:
            print(
                f"check-theme-contrast: STALE KNOWN_BELOW entry ({name}, {fill}): that pair is "
                "not below the glyph floor (or the variant or fill is gone); delete the entry"
            )
        if not args.report:
            for file_name, name, mode, fg_token, bg_token, ratio, floor in failures:
                print(
                    f"{file_name}: {name} ({mode}): {fg_token} on {bg_token} is {ratio:.2f}:1 "
                    f"(floor {floor}:1)"
                )
            for file_name, name, mode, fg_token, bg_token, hi, lo, count in inverted:
                print(
                    f"{file_name}: {name} ({mode}): on {bg_token} {fg_token} is {hi:.2f}:1 but "
                    f"{secondary} is {lo:.2f}:1 -- secondary text out-reads primary "
                    f"({count} of its surface(s))"
                )
        if failures:
            print(
                f"check-theme-contrast: {len(failures)} token(s) below their floor "
                f"({FLOOR}:1 text, {GLYPH_FLOOR}:1 glyph)",
                file=sys.stderr,
            )
        if stale:
            print(
                f"check-theme-contrast: {len(stale)} stale KNOWN_BELOW entr(y/ies)",
                file=sys.stderr,
            )
        if inverted:
            print(
                f"check-theme-contrast: {inverted_comparisons} comparison(s) in "
                f"{len(inverted)} token/variant row(s) where {secondary} reads at least "
                "as strongly as the primary token beside it",
                file=sys.stderr,
            )
        return 1

    for file_name, name, mode, fg_token, bg_token, ratio, floor in known:
        print(
            f"check-theme-contrast: KNOWN {name} ({mode}): status glyph on {bg_token} is "
            f"{ratio:.2f}:1 (floor {floor}:1), accepted by the owner 2026-10-22"
        )
    print(summary(pairings(), len(measured), len(known), secondary, comparisons()))
    return 0


if __name__ == "__main__":
    sys.exit(main())
