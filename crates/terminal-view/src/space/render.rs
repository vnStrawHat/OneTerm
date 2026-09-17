//! Rendering of the Space tree: nested resizable groups, the bordered frame
//! around each Space, the empty-Space placeholder (drop target + context menu),
//! and the [`DragTerminalTab`] payload dragged from a tab title onto a Space.

use gpui::{
    AnyElement, App, Axis, Context, Div, ElementId, Entity, Hsla, InteractiveElement as _,
    IntoElement, MouseButton, ParentElement as _, Render, Role, SharedString, Stateful,
    StatefulInteractiveElement as _, Styled as _, WeakEntity, Window, div,
    prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, Icon, Sizable as _,
    dock::ClosePanel,
    menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem},
    resizable::{h_resizable, resizable_panel, v_resizable},
    tooltip::Tooltip,
    v_flex,
};
use oneterm_state::InputChannelRegistry;
use oneterm_theme::icon::AppIcon;

use super::tree::{SpaceContent, SpaceId, SpaceLeaf, SpaceNode, SpaceTree};
use crate::input::menu::split_items;
use crate::panel::TerminalPanel;
use crate::theme::{channel_chip, channel_color};

/// Payload dragged from a Terminal Tab title onto an empty Space.
///
/// GPUI Kit 0.6 exposes its native `DragPanel`, but terminal Spaces use a
/// terminal-specific payload so the drop handler can move a Space-tree view and
/// render a terminal-title preview without coupling to dock internals.
#[derive(Clone)]
pub(crate) struct DragTerminalTab {
    /// The source terminal panel being dragged.
    pub panel: WeakEntity<TerminalPanel>,
    /// The tab label — shown in the small drag preview.
    pub title: SharedString,
}

impl Render for DragTerminalTab {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .bg(cx.theme().background)
            .border_1()
            .border_color(cx.theme().border)
            .text_color(cx.theme().foreground)
            .child(self.title.clone())
    }
}

impl SpaceTree {
    /// Render the whole tree for `panel`. `channels` is the panel's registry
    /// handle - passed in because the panel is being rendered and cannot be
    /// read back to reach its own dependencies.
    pub(crate) fn render(
        &self,
        panel: WeakEntity<TerminalPanel>,
        channels: Option<&Entity<InputChannelRegistry>>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        render_node(
            self.root(),
            self.active(),
            self.is_single(),
            panel,
            channels,
            window,
            cx,
        )
    }
}

/// Render `node`. `single` is true when this is the tree's sole leaf (the
/// plain-terminal fast path: no Space chrome).
fn render_node(
    node: &SpaceNode,
    active: SpaceId,
    single: bool,
    panel: WeakEntity<TerminalPanel>,
    channels: Option<&Entity<InputChannelRegistry>>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let split = match node {
        SpaceNode::Leaf(leaf) => {
            return render_leaf(leaf, active, single, panel, channels, window, cx);
        }
        SpaceNode::Split(split) => split,
    };

    // Stable element id derived from the split's first leaf id.
    let key = split
        .children
        .first()
        .map_or(0, |child| child.first_leaf_id().0) as usize;
    let mut group = if split.axis == Axis::Horizontal {
        h_resizable(("space-split-h", key))
    } else {
        v_resizable(("space-split-v", key))
    }
    .with_state(&split.state);

    for child in &split.children {
        let el = render_node(child, active, false, panel.clone(), channels, window, cx);
        group = group.child(resizable_panel().child(el));
    }
    group.into_any_element()
}

/// Render a single leaf Space.
fn render_leaf(
    leaf: &SpaceLeaf,
    active: SpaceId,
    single: bool,
    panel: WeakEntity<TerminalPanel>,
    channels: Option<&Entity<InputChannelRegistry>>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let id = leaf.id;
    let channel = match (&leaf.content, channels) {
        (SpaceContent::Terminal(view), Some(registry)) => {
            registry.read(cx).channel_of(view.entity_id())
        }
        _ => None,
    };
    // The badge names the channel; the Space border below repeats its colour.
    let badge = channel.map(|channel| channel_chip(channel, ("space-channel", id.0 as usize), cx));
    // ...and beside it, in a split, the Space's number — but only on the Spaces
    // the 2px cue does *not* mark (`US-0117`, rework). A single Space stays
    // unmarked, an empty Space is already named by its placeholder, and the
    // active Space needs no chip because the ring already answers "where does my
    // typing go". What a Space holds moves to the chip's tooltip, because the
    // top-right of a terminal is not chrome: it is wherever the output is.
    let chip = match (&leaf.content, single) {
        (SpaceContent::Terminal(view), false) if id != active => Some(space_number_chip(
            id.display_number(),
            space_chip_tooltip(view.read(cx).session.read(cx).title().as_deref()),
            ("space-number", id.0 as usize),
            cx,
        )),
        _ => None,
    };
    // One corner slot, 5 px from both edges (the owner's placement), holding
    // both in a row so the chip can never hide the badge; it overlaps the
    // scrollbar track, which is empty at the top unless the view is scrolled up.
    let corner = (badge.is_some() || chip.is_some()).then(|| {
        gpui_component::h_flex()
            .absolute()
            .top(px(5.))
            .right(px(5.))
            .items_center()
            .gap_1()
            .children(chip)
            .children(badge)
    });
    let content: AnyElement = match &leaf.content {
        SpaceContent::Terminal(view) => view.clone().into_any_element(),
        SpaceContent::Empty => render_placeholder(leaf, panel.clone(), window, cx),
    };

    // Fast path: the tab's only Space renders with no border / no activation
    // wrapper — visually identical to the pre-split single terminal. A lone
    // member still gets its badge, so it needs a positioned wrapper.
    if single {
        let Some(corner) = corner else {
            return content;
        };
        return div()
            .id(ElementId::from(("space", id.0 as usize)))
            .relative()
            .size_full()
            .child(content)
            .child(corner)
            .into_any_element();
    }

    let cue = space_border_color(
        id == active,
        channel.map(|ch| channel_color(ch, cx)),
        cx.theme().table_active_border,
        cx.theme().border,
    );
    div()
        .id(ElementId::from(("space", id.0 as usize)))
        .relative()
        .size_full()
        // Keep a neutral outer border for separation and reserve a one-pixel
        // inner gutter for the channel colour or the selection. The resize
        // handle may paint over the outer shared edge, but cannot erase this
        // gutter; padding keeps it outside the terminal's content bounds
        // instead of overlaying terminal cells.
        .border_1()
        .border_color(cx.theme().border)
        .p(px(1.))
        .bg(cue)
        // Clicking anywhere in the Space makes it the active Space. Bubble phase:
        // the terminal view handles its own selection first, then this fires.
        .on_mouse_down(MouseButton::Left, activate_space(panel, id))
        .child(content)
        // The cue last, so it paints over the terminal's own edge pixels.
        .children((id == active).then(|| active_cue_ring(cue)))
        .children(corner)
        .into_any_element()
}

/// The active Space's cue: a 2px ring in the cue colour, painted over the
/// Space's own 1px gutter and the outermost pixel of the terminal's content
/// area (`US-0117`, `P20`).
///
/// An absolutely-positioned child is laid out against the **padding box**, so
/// `inset_0` starts *inside* the 1px outer border: the border is never
/// overpainted and stays the neutral separator decision 8 fixed, while the
/// 2px ring covers the 1px gutter plus one pixel of terminal edge.
///
/// An overlay rather than a wider border or a wider padding, because
/// `terminal-split.md` decision 8 fixes the Space frame at 1px outer border +
/// 1px inner gutter and this strengthens the *cue*, not the frame — and because
/// geometry that changed with focus would move the terminal's content box by a
/// pixel on every focus switch, which can cost the grid a column. The ring has
/// no id and no mouse handler, so it creates no hitbox and the terminal below
/// still receives every click.
fn active_cue_ring(color: Hsla) -> Div {
    div().absolute().inset_0().border_2().border_color(color)
}

/// The number chip of an inactive Space in a split: `#N` and nothing else,
/// with the same 16px footprint as the channel badge beside it (`US-0117`).
///
/// `#N` only, because the wider label this replaced sat over the top-right of a
/// running shell and washed out live output. `background` under
/// `muted_foreground` is the pairing `scripts/check-theme-contrast.py` already
/// holds at >= 4.5:1 in every built-in theme, so the chip stays legible without
/// a private colour — and it is opaque, so the few cells it does cover are
/// covered honestly rather than smeared.
fn space_number_chip(
    number: u64,
    tooltip: Option<SharedString>,
    id: impl Into<ElementId>,
    cx: &App,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex_shrink_0()
        .h(px(16.))
        .min_w(px(16.))
        .px(px(3.))
        .line_height(px(16.))
        .text_center()
        .text_xs()
        .bg(cx.theme().background)
        .text_color(cx.theme().muted_foreground)
        .when_some(tooltip, |this, title| {
            this.tooltip(move |window, cx| Tooltip::new(title.clone()).build(window, cx))
        })
        .child(format!("#{number}"))
}

/// What the number chip says on hover: what the Space holds, or nothing when
/// the session names itself nothing. A title that is only an absolute path is
/// shortened to its last component, exactly as the tab label does it —
/// `cmd.exe` announces itself as `C:\WINDOWS\system32\cmd.exe`, which is a
/// path, not a name.
fn space_chip_tooltip(session_title: Option<&str>) -> Option<SharedString> {
    session_title
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(crate::panel::trim_path_title)
        .map(|title| SharedString::from(title.to_string()))
}

/// The 1px frame color of a Space. Only the active Space is highlighted: in
/// its `channel` colour when it is a member, in the theme's active colour
/// otherwise. Every inactive Space keeps the plain border; the badge alone
/// says which channel it is in.
fn space_border_color(
    is_active: bool,
    channel: Option<Hsla>,
    active: Hsla,
    inactive: Hsla,
) -> Hsla {
    if is_active {
        channel.unwrap_or(active)
    } else {
        inactive
    }
}

/// A mouse-down handler that makes Space `id` of `panel` the active Space.
fn activate_space(
    panel: WeakEntity<TerminalPanel>,
    id: SpaceId,
) -> impl Fn(&gpui::MouseDownEvent, &mut Window, &mut App) + 'static {
    move |_, window, cx| {
        let _ = panel.update(cx, |p, cx| p.set_active_space(id, window, cx));
    }
}

/// Render the empty-Space placeholder: centered hint text, a drop target for
/// [`DragTerminalTab`], and its own context menu.
fn render_placeholder(
    leaf: &SpaceLeaf,
    panel: WeakEntity<TerminalPanel>,
    _window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let id = leaf.id;
    let number = id.display_number();

    v_flex()
        .id(ElementId::from(("space-empty", id.0 as usize)))
        .role(Role::Pane)
        .aria_label(format!("Empty terminal space {number}"))
        .track_focus(&leaf.focus)
        .size_full()
        .items_center()
        .justify_center()
        .gap_2()
        .bg(cx.theme().background)
        .text_color(cx.theme().muted_foreground)
        .child(
            Icon::new(AppIcon::Terminal)
                .large()
                .text_color(oneterm_theme::brand_accent()),
        )
        .child(format!("Space #{number}"))
        // The likeliest action first, worded exactly as the context-menu row it
        // points at; the two rarer ones keep their mention (`US-0115`, `F25`).
        // "New Terminal Here" spawns the default shell, with no picker: the
        // empty Space is a placement action, and a user who wants a specific
        // shell opens it from the "+" menu and drags the tab in.
        .child("Right-click \u{2192} New Terminal Here")
        .child("or split, or drag a terminal tab here")
        // Clicking the placeholder activates this Space.
        .on_mouse_down(MouseButton::Left, activate_space(panel.clone(), id))
        // Visual affordance while a valid tab-drag hovers this empty Space.
        .drag_over::<DragTerminalTab>(|this, _, _, cx| this.bg(cx.theme().tokens.drop_target))
        // Drop → move the dragged tab's terminal into this Space.
        .on_drop({
            let panel = panel.clone();
            move |drag: &DragTerminalTab, window, cx| {
                let _ = panel.update(cx, |p, cx| p.handle_tab_drop(id, drag, window, cx));
            }
        })
        .context_menu(move |menu, _window, cx| placeholder_menu(menu, panel.clone(), id, cx))
        .into_any_element()
}

/// The empty-Space context menu: New Terminal Here, the four Split items,
/// Close Terminal Tab, and Close Space (only with a sibling Space).
fn placeholder_menu(
    menu: PopupMenu,
    panel: WeakEntity<TerminalPanel>,
    space_id: SpaceId,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let can_close_space = panel
        .upgrade()
        .map(|p| p.read(cx).leaf_count() > 1)
        .unwrap_or(false);

    let menu = menu.item(PopupMenuItem::new("New Terminal Here").on_click({
        let panel = panel.clone();
        move |_, window, cx| {
            let _ = panel.update(cx, |p, cx| p.new_terminal_here(space_id, window, cx));
        }
    }));
    let mut menu = split_items(menu, panel.clone(), space_id, None)
        .separator()
        // Close Terminal Tab — closes the whole tab (all Spaces).
        .item(
            PopupMenuItem::new("Close Terminal Tab").on_click(move |_, window, cx| {
                window.dispatch_action(Box::new(ClosePanel), cx);
            }),
        );

    if can_close_space {
        menu = menu.item(PopupMenuItem::new("Close Space").on_click({
            move |_, window, cx| {
                let _ = panel.update(cx, |p, cx| p.close_space(space_id, window, cx));
            }
        }));
    }
    menu
}

#[cfg(test)]
mod tests {
    use gpui::hsla;

    use super::{space_border_color, space_chip_tooltip};

    #[test]
    fn the_number_chips_tooltip_names_what_the_space_holds() {
        assert_eq!(
            space_chip_tooltip(Some("dev@127.0.0.1: ~")).as_deref(),
            Some("dev@127.0.0.1: ~")
        );
        // A shell that announces itself as a path is named, not pathed.
        assert_eq!(
            space_chip_tooltip(Some("C:\\WINDOWS\\system32\\cmd.exe")).as_deref(),
            Some("cmd.exe")
        );
        // Nothing to say rather than an empty tooltip: the chip still shows
        // `#N`, which is the part the cue cannot carry.
        assert_eq!(space_chip_tooltip(None), None);
        assert_eq!(space_chip_tooltip(Some("   ")), None);
        assert_eq!(space_chip_tooltip(Some("")), None);
    }

    #[test]
    fn selected_space_uses_active_gutter_color() {
        let active = hsla(0.1, 0.8, 0.5, 1.0);
        let inactive = hsla(0.0, 0.0, 0.2, 1.0);

        assert_eq!(space_border_color(true, None, active, inactive), active);
        assert_eq!(space_border_color(false, None, active, inactive), inactive);
    }

    #[test]
    fn member_space_uses_its_channel_color_only_while_selected() {
        let active = hsla(0.1, 0.8, 0.5, 1.0);
        let inactive = hsla(0.0, 0.0, 0.2, 1.0);
        let channel = hsla(0.6, 0.7, 0.6, 1.0);

        assert_eq!(
            space_border_color(true, Some(channel), active, inactive),
            channel
        );
        assert_eq!(
            space_border_color(false, Some(channel), active, inactive),
            inactive
        );
    }
}
