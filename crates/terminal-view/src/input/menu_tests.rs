//! `PopupMenu` keeps its item list private, so what is testable here are the
//! guards and labels the builder feeds it; the item order itself is reviewed
//! against `low-level-design/input.md` §"Context menu".

use gpui::WeakEntity;
use gpui_component::menu::PopupMenuItem;

use crate::panel::DuplicateDestination;
use crate::space::{SpaceId, SplitDir};

use super::menu::{can_close_space, dead_duplicate_item, duplicate_label, split_item};

#[test]
fn close_space_only_with_siblings() {
    assert!(!can_close_space(0));
    assert!(
        !can_close_space(1),
        "closing the only Space is what Close Terminal Tab does"
    );
    assert!(can_close_space(2));
}

#[test]
fn duplicate_destination_labels() {
    assert_eq!(
        duplicate_label(DuplicateDestination::NewTab),
        "In New Tab".to_string()
    );
    assert_eq!(
        duplicate_label(DuplicateDestination::ExistingSpace(SpaceId(3))),
        format!("Into Space #{}", SpaceId(3).display_number())
    );
    assert_eq!(
        duplicate_label(DuplicateDestination::Split(SplitDir::Right)),
        "Split Right".to_string()
    );
    assert_eq!(
        duplicate_label(DuplicateDestination::Split(SplitDir::Down)),
        "Split Down".to_string()
    );
}

/// `(label, disabled)` of a plain item. The `Item` variant's fields are public and the menu's
/// own list is not, so the rows are checked through the items the builder helpers return.
fn item_state(item: &PopupMenuItem) -> (String, bool) {
    match item {
        PopupMenuItem::Item {
            label, disabled, ..
        } => (label.to_string(), *disabled),
        _ => panic!("expected a plain item"),
    }
}

#[test]
fn split_items_follow_the_session_liveness() {
    let panel = WeakEntity::new_invalid();
    for dir in [
        SplitDir::Right,
        SplitDir::Left,
        SplitDir::Up,
        SplitDir::Down,
    ] {
        let live = item_state(&split_item(dir, &panel, SpaceId(0), None, false));
        let dead = item_state(&split_item(dir, &panel, SpaceId(0), None, true));
        assert!(!live.1, "{} is enabled while the session runs", live.0);
        assert!(dead.1, "{} is disabled once the session ended", dead.0);
        assert_eq!(live.0, dead.0, "liveness changes the state, not the label");
    }
}

#[test]
fn the_dead_duplicate_row_is_a_plain_disabled_row() {
    assert_eq!(
        item_state(&dead_duplicate_item()),
        ("Duplicate Session".to_string(), true),
        "a disabled row, not a submenu"
    );
}

// ---------------------------------------------------------------------------
// US-0201 advisor review: pinned defect 1 (`evidence/US-0201-verify.md`).
//
// A `PopupMenuItem` marked `disabled` keeps its click handler and its action, and
// the kit's `confirm` (keyboard Enter) does not re-check `disabled`: hovering a
// greyed row still sets the menu's selected index (`MenuItemElement` registers
// `on_hover` whether or not the row is disabled), so hover + Enter runs the row.
// On a dead terminal that splits the tab or dispatches `DuplicateSession` through
// rows the menu shows as disabled.
//
// Both tests are `should_panic` pins: when the rows are fixed (no handler and no
// action on a disabled row, or a kit `confirm` that honours `disabled`), drop the
// `should_panic` attribute and the `defect_1_` prefix.
// ---------------------------------------------------------------------------

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, MouseButton,
    MouseDownEvent, MouseMoveEvent, Render, Styled as _, TestAppContext, VisualTestContext, Window,
    div, point, px,
};
use gpui_component::menu::ContextMenuExt as _;
use oneterm_terminal::test_support::FakeTerminalSession;

use crate::panel::{PanelSpec, TerminalPanel};

/// A right-clickable surface whose menu is one greyed "Split Right" row of a tab
/// whose only terminal is dead (what `build_menu` emits once `session_alive` is
/// false).
struct DeadTerminalHost {
    panel: Entity<TerminalPanel>,
}

impl Render for DeadTerminalHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let panel = self.panel.downgrade();
        div()
            .id("dead-terminal")
            .size_full()
            .context_menu(move |menu, _, _| {
                menu.item(split_item(SplitDir::Right, &panel, SpaceId(0), None, true))
            })
    }
}

#[gpui::test]
#[should_panic(expected = "a disabled Split row must not split the tab")]
fn defect_1_hover_then_enter_splits_through_a_disabled_split_row(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    cx.update(crate::init);
    cx.update(oneterm_settings::TerminalSettings::init);

    let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
    probe.set_alive(false);
    let panel_slot: Rc<RefCell<Option<Entity<TerminalPanel>>>> = Rc::new(RefCell::new(None));
    let slot = panel_slot.clone();
    let (_host, cx) = cx.add_window_view(move |window, cx| {
        let panel = cx.new(|cx| {
            TerminalPanel::from_spec(
                PanelSpec::Session {
                    session,
                    title: "Terminal".to_string(),
                    duplicate_config: None,
                },
                window,
                cx,
            )
        });
        *slot.borrow_mut() = Some(panel.clone());
        DeadTerminalHost { panel }
    });
    let panel = panel_slot.borrow().clone().expect("panel built");
    let cx: &mut VisualTestContext = cx;
    let draw = |cx: &mut VisualTestContext| {
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
    };
    draw(cx);
    assert_eq!(panel.read_with(cx, |p, _| p.leaf_count()), 1);

    // Right-click opens the menu (built in a deferred callback, drawn next frame).
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Right,
        position: point(px(40.), px(40.)),
        modifiers: Default::default(),
        click_count: 1,
        first_mouse: false,
    });
    draw(cx);

    // The menu is anchored at the click point; its one row is the first 26 px
    // band below the edge padding. The kit's element snapshots need a feature
    // this crate does not enable, so sweep the band instead of reading bounds:
    // point at the greyed row, press Enter, next step. A split ends the sweep.
    for y in (44..=74).step_by(6) {
        cx.simulate_event(MouseMoveEvent {
            position: point(px(70.), px(y as f32)),
            pressed_button: None,
            modifiers: Default::default(),
        });
        draw(cx);
        cx.simulate_keystrokes("enter");
        draw(cx);
        if panel.read_with(cx, |p, _| p.leaf_count()) > 1 {
            break;
        }
    }

    let leaves = panel.read_with(cx, |p, _| p.leaf_count());
    assert_eq!(
        leaves, 1,
        "a disabled Split row must not split the tab (hover + Enter left {leaves} Spaces)"
    );
}

/// The row model behind defect 1: a disabled row that carries no handler and no
/// action has nothing for the kit's `confirm` to run, whatever the selection.
#[gpui::test]
#[should_panic(expected = "carries a click path while disabled")]
fn defect_1_disabled_rows_still_carry_their_click_path(_cx: &mut TestAppContext) {
    let click_path = |item: &PopupMenuItem| match item {
        PopupMenuItem::Item {
            handler, action, ..
        } => handler.is_some() || action.is_some(),
        _ => panic!("expected a plain item"),
    };
    let panel = WeakEntity::new_invalid();
    for dir in [
        SplitDir::Right,
        SplitDir::Left,
        SplitDir::Up,
        SplitDir::Down,
    ] {
        let item = split_item(dir, &panel, SpaceId(0), None, true);
        assert!(
            !click_path(&item),
            "{} carries a click path while disabled",
            item_state(&item).0
        );
    }
    assert!(
        !click_path(&dead_duplicate_item()),
        "Duplicate Session carries a click path while disabled"
    );
}
