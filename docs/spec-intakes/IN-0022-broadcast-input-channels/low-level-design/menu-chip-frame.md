# Low-Level Design: Input Channel menu, tab chips, and Space badge

Intake: IN-0022
HLD: ../high-level-design.md
Topic: menu-chip-frame
Date: 2026-09-09

## Concern

How a Space joins, leaves, or closes a channel from the UI, and how membership is painted on
the tab strip and on the member Space itself.

## Design

### Actions and handlers

`crates/actions`: `JoinInputChannel(pub InputChannel)`, `LeaveInputChannel`,
`CloseInputChannel`, all `#[action(namespace = oneterm, no_json)]`. Handlers sit on the
panel root `div` in `TerminalPanel::render` next to `DuplicateSession`; each resolves
`self.tree.active_terminal()` and calls `view.join_channel` / `view.leave_channel`, or reads
the view's channel and calls `registry.close(channel)`.

`crates/settings-ui/src/key_bindings/key_bindings_actions.rs`: seven `BindableAction`
entries, ids `join_input_channel_a`..`_e`, `leave_input_channel`, `close_input_channel`,
group `"Input Channel"`, `default: None`, context `None`.

### Context menu (`input/menu.rs`)

`MenuContext` gains:

```text
pub channel: Option<InputChannel>,   // this Space's channel
pub tab_spaces: usize,               // terminal Spaces in the tab (1 when not in a tree)
pub tab_has_member: bool,            // any Space in the tab is a member
```

`build_menu` inserts an "Input Channel" submenu after the Split items:

1. `Channel A`..`Channel E`: item label prefixed with `* ` when equal to `channel` (the
   popup menu has no radio mark; the prefix is the cheapest honest marker). `on_click`
   dispatches `JoinInputChannel(ch)` and re-focuses the terminal, like every other item.
2. When `channel.is_some()`: separator, `Leave Channel` (dispatches `LeaveInputChannel`),
   `Close Channel <X>` (dispatches `CloseInputChannel`).
3. When `tab_spaces > 1`: separator, then `Join All Spaces In Tab To <X>` (only when
   `channel.is_some()`) and `Leave With All Spaces In Tab` (only when `tab_has_member`).
   Both call panel methods directly through the existing `MenuSplitContext` panel handle:
   `panel.join_tab_to_channel(ch, cx)` and `panel.leave_tab_channels(cx)`, which iterate
   `self.tree.terminal_views()`.

### Tab chips (`panel/tab_title.rs`)

`render_tab_strip` reads `channels_in` for the ids of `panel.tree.terminal_views()` and
renders, before the recording dot, one chip per channel:

```text
channel_chip(ch, ("tab-channel", index), cx)   // crates/terminal-view/src/theme
// = div().id(id).flex_shrink_0().size(px(16.))
//       .text_xs().line_height(px(16.)).text_center().font_weight(FontWeight::BOLD)
//       .bg(channel_color(ch, cx)).text_color(cx.theme().background)
//       .child(ch.label())
```

The chip is a square with square corners, and the letter is centred by the text node, not by
flex. A flex `justify_center` centres the text node's *rounded* box: Taffy rounds that box to
whole pixels, so a 8.4375 px advance becomes a 9 px box at offset 3 instead of 3.78, the glyph
is painted at the box's left edge, and the letter lands about 1 px left of centre. Giving the
text node the whole square instead — block layout (no `.flex()`), `line_height(px(16.))`, and
`text_center()` — makes GPUI align the shaped run inside the text bounds in floating point at
paint time, with no integer rounding between the box and the glyph. Measured ink must be
centred within 1 px in both directions, in the tab chip and in the Space badge alike.

`channel_chip` is the one chip builder, used by the tab strip and by the Space badge below;
its label/colour triple comes from the pure `channel_chip_style`, which is what the test
checks. `channel_color(ch, cx)` returns `[chart_1, chart_2, chart_3, chart_4,
chart_5][ch.index()]` from `cx.theme()`; it lives in `crates/terminal-view/src/theme/` so the
chip and the badge share it. Chip text uses `cx.theme().background` for contrast on the saturated chart colour;
if a theme's chart colour is too light for that, the theme JSON owns the fix, not the chip.

### Space badge (`space/render.rs`)

`render_leaf` reads the leaf's channel once and, for a member, renders
`channel_chip(ch, ("space-channel", id), cx).absolute().top(px(5.)).right(px(5.))` as the
last child of the Space wrapper (both the split path and the single-Space fast path, which
therefore becomes `.relative()`). The chip is a fixed 16 px square, so the tab chip and the
badge have the same footprint; 5 px from both edges is the owner's placement. It overlaps
the scrollbar track, which is empty at the top unless the view is scrolled up.

The badge says *which* channel a Space is in; the border repeats it. With the badge in place
the colour clash that once ruled the frame out (in the dark theme `chart_1` is close to
`table_active_border`) no longer misleads, so:

```text
space_border_color(is_active: bool, channel: Option<Hsla>, active: Hsla, inactive: Hsla)
// Some(colour) -> colour        a member, active or not
// None         -> is_active ? active : inactive
```

A member Space is framed in its channel colour at full strength whether or not it is the
active Space; a Space in no channel keeps the theme's active/inactive rule. The split path
passes `channel.map(|ch| channel_color(ch, cx))`. The single-Space fast path stays
borderless: a lone Space is marked by its badge alone, so an unsplit tab keeps its pixel
layout. The badge takes no focus (a plain `div` with an id, no `track_focus`) and carries no
click handler; a click on it reaches the Space's own activation handler like any other click
inside the Space.

### Registry observer (`panel/terminal_panel.rs`)

`TerminalPanel` observes the registry entity (`cx.observe`) once at construction so a join
from another tab (for example `Close Channel` elsewhere) repaints this tab's chips and
badges.

## Interfaces

```text
// crates/terminal-view/src/theme
pub(crate) fn channel_color(channel: InputChannel, cx: &App) -> Hsla;
pub(crate) fn channel_chip_style(channel: InputChannel, cx: &App) -> (&'static str, Hsla, Hsla);
pub(crate) fn channel_chip(channel: InputChannel, id: impl Into<ElementId>, cx: &App)
    -> Stateful<Div>;

// panel/terminal_panel.rs
pub(crate) fn join_tab_to_channel(&mut self, channel: InputChannel, cx: &mut Context<Self>);
pub(crate) fn leave_tab_channels(&mut self, cx: &mut Context<Self>);
pub(crate) fn tab_channels(&self, cx: &App) -> Vec<InputChannel>;
```

## Edge Cases and Failure Modes

- [ ] Right-click on an empty Space: no `MenuContext`, no submenu.
- [ ] Terminal outside a Space tree (`split: None`): the submenu shows items 1 and 2 only.
- [ ] Tab chip row with five channels: five chips; the tab keeps its `min_w(px(100.))` and
  the title truncates as it does for a long title today.
- [ ] Theme switch: chips and badges recolour on the next render; no cached colours.
- [ ] `JoinInputChannel` dispatched by key binding while the active Space is empty: handler
  finds no terminal and does nothing.
- [ ] `Close Channel` from a tab that is not focused (via another tab's menu): the observer
  repaints every panel.

## Verification

- [ ] `panel/tests.rs`: `tab_channels` returns `[A]` for one member Space, `[A, C]` for a
  mixed split, `[]` for none; `join_tab_to_channel` joins every Space; `leave_tab_channels`
  clears them.
- [ ] `space/render.rs` tests: the `space_border_color` truth table — a member returns its
  channel colour whether active or not; a non-member returns the active/inactive colour.
- [ ] `theme/input_channel.rs`: `channel_chip_style` returns the channel's letter, its chart
  colour and the theme background for every channel.
- [ ] `input/menu` tests: items 2 and 3 appear only under their conditions.
- [ ] GUI evidence (PrintWindow screenshot): split tab with two members and one non-member,
  plus a lone member tab, both light and dark theme.
