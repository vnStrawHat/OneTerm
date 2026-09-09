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
// = div().id(id).flex_shrink_0().px_1().rounded_sm()
//       .text_xs().font_weight(FontWeight::BOLD)
//       .bg(channel_color(ch, cx)).text_color(cx.theme().background)
//       .child(ch.label())
```

`channel_chip` is the one chip builder, used by the tab strip and by the Space badge below;
its label/colour triple comes from the pure `channel_chip_style`, which is what the test
checks. `channel_color(ch, cx)` returns `[chart_1, chart_2, chart_3, chart_4,
chart_5][ch.index()]` from `cx.theme()`; it lives in `crates/terminal-view/src/theme/` so the
chip and the badge share it. Chip text uses `cx.theme().background` for contrast on the saturated chart colour;
if a theme's chart colour is too light for that, the theme JSON owns the fix, not the chip.

### Space badge (`space/render.rs`)

`render_leaf` reads the leaf's channel once and, for a member, renders
`channel_chip(ch, ("space-channel", id), cx).absolute().top(px(3.)).right(px(15.))` as the
last child of the Space wrapper (both the split path and the single-Space fast path, which
therefore becomes `.relative()`). The 15 px inset clears the terminal's 12 px scrollbar
track (`TRACK_WIDTH_PX` in `terminal_view/scrollbar.rs`).

The badge is the only per-Space marker. A frame in the channel colour was tried and
dropped: in the dark theme the channel-A colour (`chart_1`) is close to
`table_active_border`, so a frame cannot say which Space of a split joined, and it costs the
active-Space cue it overwrites. Space borders therefore keep the theme's rule unchanged
(`space_border_color(is_active, active, inactive)`): `table_active_border` for the active
Space, `border` otherwise, and no border at all on the single-Space fast path. The badge
takes no focus (a plain `div` with an id, no `track_focus`) and
carries no click handler; a click on it reaches the Space's own activation handler like any
other click inside the Space.

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
- [ ] `space/render.rs` tests: the `space_border_color` truth table — membership is not
  one of its inputs.
- [ ] `theme/input_channel.rs`: `channel_chip_style` returns the channel's letter, its chart
  colour and the theme background for every channel.
- [ ] `input/menu` tests: items 2 and 3 appear only under their conditions.
- [ ] GUI evidence (PrintWindow screenshot): split tab with two members and one non-member,
  plus a lone member tab, both light and dark theme.
