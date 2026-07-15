//! Recursive rendering of the Space tree → nested `h/v_resizable` groups with
//! 4px borders and the active-Space highlight.
//!
//! See `docs/terminal-split/05-rendering-theme.md`.

use gpui::{
    AnyElement, App, Axis, ElementId, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, Styled as _, WeakEntity, Window, div, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _,
    resizable::{h_resizable, resizable_panel, v_resizable},
};

use super::super::panel::TerminalPanel;
use super::node::{SpaceContent, SpaceId, SpaceLeaf, SpaceNode};

/// Render `node` into an element. `single` is true when this is the tree's sole
/// leaf (the plain-terminal fast path: no Space chrome).
pub(crate) fn render_node(
    node: &SpaceNode,
    active: SpaceId,
    single: bool,
    panel: WeakEntity<TerminalPanel>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    match node {
        SpaceNode::Leaf(leaf) => render_leaf(leaf, active, single, panel, window, cx),
        SpaceNode::Split(split) => {
            let axis = split.axis;
            // Stable element id derived from the split's first leaf id.
            let key = split.children[0].first_leaf_id().0 as usize;
            let group = if axis == Axis::Horizontal {
                h_resizable(("space-split-h", key))
            } else {
                v_resizable(("space-split-v", key))
            }
            .with_state(&split.state);

            let mut group = group;
            for child in &split.children {
                let el = render_node(child, active, false, panel.clone(), window, cx);
                group = group.child(resizable_panel().child(el));
            }
            group.into_any_element()
        }
    }
}

/// Render a single leaf Space.
fn render_leaf(
    leaf: &SpaceLeaf,
    active: SpaceId,
    single: bool,
    panel: WeakEntity<TerminalPanel>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let id = leaf.id;
    let content: AnyElement = match &leaf.content {
        SpaceContent::Terminal(view) => view.clone().into_any_element(),
        SpaceContent::Empty => {
            super::placeholder::render_placeholder(leaf, panel.clone(), window, cx)
        }
    };

    // Fast path: the tab's only Space renders with no border / no activation
    // wrapper — visually identical to the pre-split single terminal.
    if single {
        return content;
    }

    let is_active = id == active;
    let active_border = cx.theme().table_active_border;

    div()
        .id(ElementId::from(("space", id.0 as usize)))
        .relative()
        .size_full()
        // Uniform 1px neutral separator on every Space. On the edge shared with a
        // sibling, gpui-component's resize handle paints its own 1px line on top
        // in the same neutral color, so this reads as a single 1px border.
        .border_1()
        .border_color(cx.theme().border)
        // Clicking anywhere in the Space makes it the active Space. Bubble phase:
        // the terminal view handles its own selection first, then this fires.
        .on_mouse_down(MouseButton::Left, {
            let panel = panel.clone();
            move |_, window, cx| {
                let _ = panel.update(cx, |p, cx| p.set_active_space(id, window, cx));
            }
        })
        .child(content)
        // Active-Space highlight: an inset 1px ring painted on top of the content.
        // It sits 1px inside the edge so it clears the resize handle's 1px bar
        // (which would otherwise hide the highlight on the shared edge) and is
        // therefore visible on all four sides.
        .when(is_active, |this| {
            this.child(
                div()
                    .absolute()
                    .top(px(1.))
                    .left(px(1.))
                    .right(px(1.))
                    .bottom(px(1.))
                    .border_1()
                    .border_color(active_border),
            )
        })
        .into_any_element()
}
