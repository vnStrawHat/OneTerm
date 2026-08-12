//! Recursive rendering of the Space tree → nested `h/v_resizable` groups with
//! one-pixel frames and the active-Space highlight.
//!
//! See `docs/terminal-split/05-rendering-theme.md`.

use gpui::{
    AnyElement, App, Axis, ElementId, Hsla, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, Styled as _, WeakEntity, Window, div, px,
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

fn space_border_color(is_active: bool, active: Hsla, inactive: Hsla) -> Hsla {
    if is_active { active } else { inactive }
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

    let frame_color = space_border_color(
        id == active,
        cx.theme().table_active_border,
        cx.theme().border,
    );

    div()
        .id(ElementId::from(("space", id.0 as usize)))
        .relative()
        .size_full()
        // Keep a neutral outer border for separation and reserve a one-pixel
        // inner gutter for selection. The resize handle may paint over the outer
        // shared edge, but cannot erase this gutter; padding keeps it outside the
        // terminal's content bounds instead of overlaying terminal cells.
        .border_1()
        .border_color(cx.theme().border)
        .p(px(1.))
        .bg(frame_color)
        // Clicking anywhere in the Space makes it the active Space. Bubble phase:
        // the terminal view handles its own selection first, then this fires.
        .on_mouse_down(MouseButton::Left, {
            let panel = panel.clone();
            move |_, window, cx| {
                let _ = panel.update(cx, |p, cx| p.set_active_space(id, window, cx));
            }
        })
        .child(content)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use gpui::hsla;

    use super::space_border_color;

    #[test]
    fn selected_space_uses_active_gutter_color() {
        let active = hsla(0.1, 0.8, 0.5, 1.0);
        let inactive = hsla(0.0, 0.0, 0.2, 1.0);

        assert_eq!(space_border_color(true, active, inactive), active);
        assert_eq!(space_border_color(false, active, inactive), inactive);
    }
}
