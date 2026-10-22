//! `PopupMenu` keeps its item list private, so what is testable here are the
//! guards and labels the builder feeds it; the item order itself is reviewed
//! against `low-level-design/input.md` §"Context menu".

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, Render, Styled as _, TestAppContext,
    VisualTestContext, WeakEntity, Window, div, point, px,
};
use gpui_component::menu::{ContextMenuExt as _, PopupMenuItem};
use oneterm_actions::DuplicateSession;
use oneterm_terminal::TerminalSession;
use oneterm_terminal::test_support::FakeTerminalSession;

use crate::panel::{DuplicateDestination, PanelSpec, TerminalPanel};
use crate::space::{SpaceId, SplitContext, SplitDir};
use crate::terminal_view::BroadcastOrigin;

use super::menu::{
    MenuContext, MenuSplitContext, build_menu, can_close_space, dead_duplicate_item,
    duplicate_label, split_item,
};

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

/// The row model behind the hover + Enter hole: the kit's `confirm` runs a row's handler or
/// action without checking `disabled`, and hovering a greyed row selects it, so a disabled row
/// must carry neither (`US-0201`, advisor review).
#[gpui::test]
fn disabled_rows_carry_no_click_path(_cx: &mut TestAppContext) {
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
        assert!(
            click_path(&split_item(dir, &panel, SpaceId(0), None, false)),
            "an enabled split row still acts"
        );
    }
    assert!(
        !click_path(&dead_duplicate_item()),
        "Duplicate Session carries a click path while disabled"
    );
}

// ---------------------------------------------------------------------------
// The whole menu in a window: `build_menu` over a `TerminalView` of a `TerminalPanel`, driven
// through `gpui_base::test_support::snapshots` (the drawn rows with labels and bounds).
// ---------------------------------------------------------------------------

/// A right-clickable surface whose menu is the product's `build_menu` for the panel's active
/// terminal. It counts the `DuplicateSession` actions that reach it.
struct MenuHost {
    session: Entity<Box<dyn TerminalSession>>,
    origin: BroadcastOrigin,
    focus: FocusHandle,
    split: SplitContext,
    duplicates: Rc<Cell<usize>>,
}

impl Render for MenuHost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session.clone();
        let origin = self.origin.clone();
        let focus = self.focus.clone();
        let split = self.split.clone();
        let panel = split.panel.clone();
        let duplicates = self.duplicates.clone();
        let _ = cx;
        div()
            .id("menu-host")
            .size_full()
            .on_action(move |_: &DuplicateSession, _, _| duplicates.set(duplicates.get() + 1))
            .context_menu(move |menu, window, cx| {
                // The same reads the terminal's own `context_menu` closure makes.
                let (has_selection, session_alive) = {
                    let s = session.read(cx);
                    (s.has_selection(), s.alive())
                };
                let (empty_destinations, leaf_count) = panel
                    .upgrade()
                    .map(|p| {
                        let p = p.read(cx);
                        (p.empty_space_destinations(), p.leaf_count())
                    })
                    .unwrap_or_default();
                let ctx = MenuContext {
                    session: session.clone(),
                    origin: origin.clone(),
                    focus: focus.clone(),
                    has_selection,
                    session_alive,
                    split: Some(MenuSplitContext {
                        ctx: split.clone(),
                        empty_destinations,
                        leaf_count,
                    }),
                    logging: None,
                };
                build_menu(menu, &ctx, window, cx)
            })
    }
}

struct Harness {
    panel: Entity<TerminalPanel>,
    duplicates: Rc<Cell<usize>>,
}

fn open_host(cx: &mut TestAppContext, alive: bool) -> (Harness, &mut VisualTestContext) {
    cx.update(gpui_component::init);
    cx.update(crate::init);
    cx.update(oneterm_settings::TerminalSettings::init);

    let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
    probe.set_alive(alive);
    let slot: Rc<RefCell<Option<Entity<TerminalPanel>>>> = Rc::new(RefCell::new(None));
    let duplicates = Rc::new(Cell::new(0));
    let (slot_in, duplicates_in) = (slot.clone(), duplicates.clone());
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
        *slot_in.borrow_mut() = Some(panel.clone());
        let view = panel.read(cx).active_view().expect("the session's view");
        let origin = view.update(cx, |view, cx| view.broadcast_origin(cx));
        let (session, focus, split) = {
            let view = view.read(cx);
            (
                view.session.clone(),
                view.focus.clone(),
                view.split_ctx
                    .clone()
                    .expect("a panel terminal has a Space"),
            )
        };
        MenuHost {
            session,
            origin,
            focus,
            split,
            duplicates: duplicates_in,
        }
    });
    let panel = slot.borrow().clone().expect("panel built");
    (Harness { panel, duplicates }, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
}

/// `(label, centre)` of every menu row drawn now, in the order the kit drew them.
fn menu_rows(cx: &mut VisualTestContext) -> Vec<(String, gpui::Point<gpui::Pixels>)> {
    cx.update(|window, _| {
        gpui_base::test_support::snapshots(window)
            .into_iter()
            .filter(|element| element.role() == Some(gpui::Role::MenuItem))
            .filter_map(|element| {
                let label = element.label()?.to_string();
                Some((label, element.bounds().center()))
            })
            .collect()
    })
}

fn right_click_to_open_the_menu(cx: &mut VisualTestContext) {
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Right,
        position: point(px(40.), px(40.)),
        modifiers: Default::default(),
        click_count: 1,
        first_mouse: false,
    });
    draw(cx);
}

fn centre_of(
    rows: &[(String, gpui::Point<gpui::Pixels>)],
    label: &str,
) -> gpui::Point<gpui::Pixels> {
    rows.iter()
        .find(|(l, _)| l == label)
        .unwrap_or_else(|| panic!("no menu row {label:?} among {:?}", rows))
        .1
}

const SPLIT_LABELS: [&str; 4] = ["Split Right", "Split Left", "Split Up", "Split Down"];

#[gpui::test]
fn a_dead_terminals_menu_greys_split_and_duplicate_and_they_do_nothing(cx: &mut TestAppContext) {
    let (host, cx) = open_host(cx, false);
    draw(cx);
    assert_eq!(host.panel.read_with(cx, |p, _| p.leaf_count()), 1);

    right_click_to_open_the_menu(cx);
    let labels: Vec<String> = menu_rows(cx).into_iter().map(|(l, _)| l).collect();
    for expected in [
        "New Terminal",
        "Duplicate Session",
        "Split Right",
        "Split Left",
        "Split Up",
        "Split Down",
        "Copy",
        "Paste",
        "Select All",
        "Clear",
        "Close Terminal Tab",
    ] {
        assert!(
            labels.iter().any(|l| l == expected),
            "menu row {expected:?} missing from {labels:?}"
        );
    }

    for label in std::iter::once("Duplicate Session").chain(SPLIT_LABELS) {
        // Reopen the menu if the previous row's Enter dismissed it.
        if !menu_rows(cx).iter().any(|(l, _)| l == label) {
            right_click_to_open_the_menu(cx);
        }
        let at = centre_of(&menu_rows(cx), label);

        // Hover the greyed row and press Enter: the kit's confirm path.
        cx.simulate_event(MouseMoveEvent {
            position: at,
            pressed_button: None,
            modifiers: Default::default(),
        });
        draw(cx);
        cx.simulate_keystrokes("enter");
        draw(cx);

        // And a plain click on it.
        if !menu_rows(cx).iter().any(|(l, _)| l == label) {
            right_click_to_open_the_menu(cx);
        }
        let at = centre_of(&menu_rows(cx), label);
        cx.simulate_click(at, Default::default());
        draw(cx);

        assert_eq!(
            host.panel.read_with(cx, |p, _| p.leaf_count()),
            1,
            "the disabled {label:?} row split the tab"
        );
        assert_eq!(
            host.duplicates.get(),
            0,
            "the disabled {label:?} row dispatched DuplicateSession"
        );
    }
}

/// The control: the same drive on a running terminal does act, so the test above fails for
/// the right reason and not because the harness cannot reach the rows.
#[gpui::test]
fn a_live_terminals_split_row_splits_on_hover_and_enter(cx: &mut TestAppContext) {
    let (host, cx) = open_host(cx, true);
    draw(cx);
    right_click_to_open_the_menu(cx);
    let at = centre_of(&menu_rows(cx), "Split Right");
    cx.simulate_event(MouseMoveEvent {
        position: at,
        pressed_button: None,
        modifiers: Default::default(),
    });
    draw(cx);
    cx.simulate_keystrokes("enter");
    draw(cx);
    assert_eq!(host.panel.read_with(cx, |p, _| p.leaf_count()), 2);
}
