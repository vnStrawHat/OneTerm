//! Empty-Space placeholder: centered hint text, a drop target for
//! [`DragTerminalTab`], and its own context menu (New Terminal Here, Split
//! R/L/U/D, Close Terminal Tab, Close Space).
//!
//! See `docs/terminal-split/04-context-menu.md` §3 and `05-rendering-theme.md` §5.

use gpui::{
    App, ElementId, InteractiveElement, IntoElement, MouseButton, ParentElement, Styled,
    WeakEntity, Window,
};
use gpui_component::{
    ActiveTheme as _, Icon, Sizable as _,
    dock::ClosePanel,
    menu::{ContextMenu, ContextMenuExt as _, PopupMenu, PopupMenuItem},
    v_flex,
};

use crate::icon::AppIcon;

use super::super::panel::TerminalPanel;
use super::SplitDir;
use super::drag::DragTerminalTab;
use super::node::{SpaceId, SpaceLeaf};

/// Render the empty-Space placeholder for `leaf`.
pub(crate) fn render_placeholder(
    leaf: &SpaceLeaf,
    panel: WeakEntity<TerminalPanel>,
    _window: &mut Window,
    cx: &mut App,
) -> gpui::AnyElement {
    let id = leaf.id;

    let base = v_flex()
        .id(ElementId::from(("space-empty", id.0 as usize)))
        .track_focus(&leaf.focus)
        .size_full()
        .items_center()
        .justify_center()
        .gap_2()
        .bg(cx.theme().background)
        .text_color(cx.theme().muted_foreground)
        .child(Icon::new(AppIcon::Terminal).large())
        .child("Drag a terminal tab here")
        .child("or right-click to split")
        // Clicking the placeholder activates this Space.
        .on_mouse_down(MouseButton::Left, {
            let panel = panel.clone();
            move |_, window, cx| {
                let _ = panel.update(cx, |p, cx| p.set_active_space(id, window, cx));
            }
        })
        // Visual affordance while a valid tab-drag hovers this empty Space.
        .drag_over::<DragTerminalTab>(|this, _, _, cx| this.bg(cx.theme().tokens.drop_target))
        // Drop → move the dragged tab's terminal into this Space.
        .on_drop({
            let panel = panel.clone();
            move |drag: &DragTerminalTab, window, cx| {
                let _ = panel.update(cx, |p, cx| p.handle_tab_drop(id, drag, window, cx));
            }
        });

    attach_empty_menu(base, panel, id).into_any_element()
}

/// A "Split <dir>" menu item that dispatches to the panel.
fn split_item(
    label: &'static str,
    dir: SplitDir,
    panel: WeakEntity<TerminalPanel>,
    space_id: SpaceId,
) -> PopupMenuItem {
    PopupMenuItem::new(label).on_click(move |_, window, cx| {
        let _ = panel.update(cx, |p, cx| p.split_active_at(space_id, dir, window, cx));
    })
}

/// Attach the empty-Space context menu to `el`.
fn attach_empty_menu<E>(
    el: E,
    panel: WeakEntity<TerminalPanel>,
    space_id: SpaceId,
) -> ContextMenu<E>
where
    E: InteractiveElement + ParentElement + Styled,
{
    el.context_menu(
        move |menu: PopupMenu, _window: &mut Window, cx: &mut gpui::Context<PopupMenu>| {
            let can_close_space = panel
                .upgrade()
                .map(|p| p.read(cx).leaf_count() > 1)
                .unwrap_or(false);

            let mut menu = menu
                // New Terminal Here — spawn a local shell into this empty Space.
                .item(PopupMenuItem::new("New Terminal Here").on_click({
                    let panel = panel.clone();
                    move |_, window, cx| {
                        let _ = panel.update(cx, |p, cx| p.new_terminal_here(space_id, window, cx));
                    }
                }))
                .item(split_item(
                    "Split Right",
                    SplitDir::Right,
                    panel.clone(),
                    space_id,
                ))
                .item(split_item(
                    "Split Left",
                    SplitDir::Left,
                    panel.clone(),
                    space_id,
                ))
                .item(split_item(
                    "Split Up",
                    SplitDir::Up,
                    panel.clone(),
                    space_id,
                ))
                .item(split_item(
                    "Split Down",
                    SplitDir::Down,
                    panel.clone(),
                    space_id,
                ))
                .separator()
                // Close Terminal Tab — closes the whole tab (all Spaces).
                .item(
                    PopupMenuItem::new("Close Terminal Tab").on_click(move |_, window, cx| {
                        window.dispatch_action(Box::new(ClosePanel), cx);
                    }),
                );

            // Close Space — only when the tab has more than one Space.
            if can_close_space {
                menu = menu.item(PopupMenuItem::new("Close Space").on_click({
                    let panel = panel.clone();
                    move |_, window, cx| {
                        let _ = panel.update(cx, |p, cx| p.close_space(space_id, window, cx));
                    }
                }));
            }

            menu
        },
    )
}
