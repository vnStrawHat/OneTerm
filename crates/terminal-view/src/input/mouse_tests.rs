//! Mouse state machine tests. The session-facing ones drive a
//! `FakeTerminalSession` and read back the input calls its probe recorded.

use alacritty_terminal::selection::SelectionType;
use gpui::{
    AppContext as _, Bounds, Capslock, Edges, Modifiers, ModifiersChangedEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollDelta, ScrollWheelEvent,
    TestAppContext, TouchPhase, point, px, size,
};
use oneterm_terminal::test_support::{FakeInputCall, FakeSessionProbe, FakeTerminalSession};
use oneterm_terminal::url_policy::TargetDecision;
use oneterm_terminal::{TerminalMouseButton, TerminalSession};

use super::mouse::{
    Drag, MouseInputs, MouseOutcome, MouseState, selection_type, to_button, to_mods, wheel_lines,
};
use crate::render::metrics::{CellMetrics, GridGeometry};
use crate::url::UrlHover;

/// An 8 × 16 px cell grid whose origin is (100, 50).
fn geometry() -> GridGeometry {
    GridGeometry::new(
        Bounds {
            origin: point(px(100.0), px(50.0)),
            size: size(px(160.0), px(80.0)),
        },
        CellMetrics::snapped(8.0, 16.0, 12.0, 3.0, 5.0, 1.0),
        px(0.0),
        Edges::default(),
    )
}

fn inputs() -> MouseInputs {
    MouseInputs {
        geometry: Some(geometry()),
        over_scrollbar: false,
        show_context_menu: false,
        copy_on_select: false,
        scroll_multiplier: 1.0,
    }
}

/// Centre of cell `(row, col)`.
fn cell(row: usize, col: usize) -> Point<Pixels> {
    let g = geometry();
    let origin = g.cell_origin(row, col);
    point(origin.x + px(4.0), origin.y + px(8.0))
}

fn down(button: MouseButton, position: Point<Pixels>, mods: Modifiers) -> MouseDownEvent {
    MouseDownEvent {
        button,
        position,
        modifiers: mods,
        click_count: 1,
        first_mouse: false,
    }
}

fn up(button: MouseButton, position: Point<Pixels>) -> MouseUpEvent {
    MouseUpEvent {
        button,
        position,
        modifiers: Modifiers::default(),
        click_count: 1,
    }
}

fn moved(position: Point<Pixels>, pressed: Option<MouseButton>) -> MouseMoveEvent {
    MouseMoveEvent {
        position,
        pressed_button: pressed,
        modifiers: Modifiers::default(),
    }
}

fn ctrl() -> Modifiers {
    Modifiers {
        control: true,
        ..Default::default()
    }
}

/// A session entity plus its probe, usable inside `cx.update`.
fn session(
    cx: &mut TestAppContext,
    text: &str,
) -> (gpui::Entity<Box<dyn TerminalSession>>, FakeSessionProbe) {
    let (session, probe) = FakeTerminalSession::boxed(5, 40, text);
    let entity = cx.update(|cx| cx.new(|_| session));
    (entity, probe)
}

#[test]
fn click_count_selects_type() {
    assert_eq!(selection_type(1, false), SelectionType::Simple);
    assert_eq!(selection_type(2, false), SelectionType::Semantic);
    assert_eq!(selection_type(3, false), SelectionType::Lines);
    assert_eq!(selection_type(7, false), SelectionType::Lines);
}

#[test]
fn alt_click_is_block() {
    assert_eq!(selection_type(1, true), SelectionType::Block);
    assert_eq!(selection_type(2, true), SelectionType::Block);
}

#[test]
fn buttons_and_modifiers_convert() {
    assert_eq!(
        to_button(MouseButton::Middle),
        Some(TerminalMouseButton::Middle)
    );
    assert_eq!(
        to_button(MouseButton::Navigate(gpui::NavigationDirection::Back)),
        None
    );
    let mods = to_mods(&Modifiers {
        shift: true,
        control: true,
        ..Default::default()
    });
    assert!(mods.shift && mods.ctrl && !mods.alt);
}

#[gpui::test]
fn ctrl_click_on_url_opens_not_selects(cx: &mut TestAppContext) {
    let (session, probe) = session(cx, "see https://example.com/ok now");
    let mut state = MouseState::default();
    let outcome = cx.update(|cx| {
        state.down(
            &down(MouseButton::Left, cell(0, 6), ctrl()),
            &session,
            &inputs(),
            cx,
        )
    });
    let MouseOutcome::OpenUrl(open) = outcome else {
        panic!("Ctrl+click on a URL must return an open request, got {outcome:?}");
    };
    assert_eq!(open.url.url, "https://example.com/ok");
    assert_eq!(open.decision, TargetDecision::Allow);
    // No selection was started behind the link.
    assert!(probe.input_calls().is_empty());
    assert_eq!(state.drag(), Drag::None);
}

#[gpui::test]
fn middle_click_forwards_to_session(cx: &mut TestAppContext) {
    let (session, probe) = session(cx, "hello");
    let mut state = MouseState::default();
    cx.update(|cx| {
        state.down(
            &down(MouseButton::Middle, cell(1, 2), Modifiers::default()),
            &session,
            &inputs(),
            cx,
        )
    });
    // Middle click is mouse-mode input, never a primary-selection paste.
    assert!(matches!(
        probe.input_calls().as_slice(),
        [FakeInputCall::MouseDown {
            button: TerminalMouseButton::Middle,
            selection: SelectionType::Simple,
            ..
        }]
    ));
    assert!(probe.writes().is_empty());
    assert_eq!(state.drag(), Drag::None);
}

#[gpui::test]
fn right_click_forwards_when_menu_disabled(cx: &mut TestAppContext) {
    let (session, probe) = session(cx, "hello");
    let mut state = MouseState::default();
    let with_menu = MouseInputs {
        show_context_menu: true,
        ..inputs()
    };
    let event = down(MouseButton::Right, cell(0, 1), Modifiers::default());
    let outcome = cx.update(|cx| state.down(&event, &session, &with_menu, cx));
    assert_eq!(outcome, MouseOutcome::Ignored);
    assert!(probe.input_calls().is_empty());

    cx.update(|cx| state.down(&event, &session, &inputs(), cx));
    assert!(matches!(
        probe.input_calls().as_slice(),
        [FakeInputCall::MouseDown {
            button: TerminalMouseButton::Right,
            ..
        }]
    ));
}

#[gpui::test]
fn copy_on_select_on_left_up(cx: &mut TestAppContext) {
    let (session, probe) = session(cx, "hello");
    let mut state = MouseState::default();
    let copying = MouseInputs {
        copy_on_select: true,
        ..inputs()
    };
    // Nothing selected → no copy.
    let outcome =
        cx.update(|cx| state.up(&up(MouseButton::Left, cell(0, 0)), &session, &copying, cx));
    assert_eq!(outcome, MouseOutcome::Handled);

    probe.set_selection(Some("hello".into()));
    let outcome =
        cx.update(|cx| state.up(&up(MouseButton::Left, cell(0, 0)), &session, &copying, cx));
    assert_eq!(outcome, MouseOutcome::CopySelection);
    // The setting gates it.
    let outcome =
        cx.update(|cx| state.up(&up(MouseButton::Left, cell(0, 0)), &session, &inputs(), cx));
    assert_eq!(outcome, MouseOutcome::Handled);
    // Every release still reached the session.
    assert_eq!(
        probe
            .input_calls()
            .iter()
            .filter(|c| matches!(c, FakeInputCall::MouseUp { .. }))
            .count(),
        3
    );
}

#[gpui::test]
fn scrollbar_drag_precedes_selection(cx: &mut TestAppContext) {
    let (session, probe) = session(cx, "hello");
    let mut state = MouseState::default();
    let over_bar = MouseInputs {
        over_scrollbar: true,
        ..inputs()
    };
    let outcome = cx.update(|cx| {
        state.down(
            &down(MouseButton::Left, cell(2, 3), Modifiers::default()),
            &session,
            &over_bar,
            cx,
        )
    });
    // 50 (grid top) + 2 rows × 16 + 8 = 82; the element top is 50.
    assert_eq!(outcome, MouseOutcome::ScrollbarDrag { track_y: 40.0 });
    assert_eq!(state.drag(), Drag::Scrollbar);
    assert!(probe.input_calls().is_empty());

    // Moves keep dragging the bar, not the selection.
    let mut hover = UrlHover::default();
    let outcome = cx.update(|cx| {
        state.moved(
            &moved(cell(3, 3), Some(MouseButton::Left)),
            &session,
            &over_bar,
            &mut hover,
            cx,
        )
    });
    assert_eq!(outcome, MouseOutcome::ScrollbarDrag { track_y: 56.0 });
    assert!(probe.input_calls().is_empty());

    // Any release ends it, and nothing is forwarded for that release.
    let outcome =
        cx.update(|cx| state.up(&up(MouseButton::Right, cell(3, 3)), &session, &over_bar, cx));
    assert_eq!(outcome, MouseOutcome::Handled);
    assert_eq!(state.drag(), Drag::None);
    assert!(probe.input_calls().is_empty());
}

#[gpui::test]
fn wheel_delta_uses_multiplier_and_threshold(cx: &mut TestAppContext) {
    // Pure math first: pixels ÷ line height × multiplier.
    assert_eq!(
        wheel_lines(ScrollDelta::Pixels(point(px(0.0), px(48.0))), px(16.0), 1.0),
        3.0
    );
    assert_eq!(
        wheel_lines(ScrollDelta::Pixels(point(px(0.0), px(48.0))), px(16.0), 2.5),
        7.5
    );
    assert_eq!(
        wheel_lines(ScrollDelta::Lines(point(0.0, 2.0)), px(16.0), 1.0),
        2.0
    );

    let (session, probe) = session(cx, "hello");
    let state = MouseState::default();
    let wheel = |delta: ScrollDelta| ScrollWheelEvent {
        position: cell(1, 1),
        delta,
        modifiers: Modifiers::default(),
        touch_phase: TouchPhase::Moved,
    };
    let outcome = cx.update(|cx| {
        state.wheel(
            &wheel(ScrollDelta::Pixels(point(px(0.0), px(48.0)))),
            &session,
            &inputs(),
            cx,
        )
    });
    assert_eq!(outcome, MouseOutcome::Handled);
    assert!(matches!(
        probe.take_input_calls().as_slice(),
        [FakeInputCall::Wheel { delta_y, .. }] if (*delta_y - 3.0).abs() < 1e-6
    ));

    // Below the 0.001-line threshold nothing is forwarded.
    let outcome = cx.update(|cx| {
        state.wheel(
            &wheel(ScrollDelta::Pixels(point(px(0.0), px(0.0001)))),
            &session,
            &inputs(),
            cx,
        )
    });
    assert_eq!(outcome, MouseOutcome::Ignored);
    assert!(probe.input_calls().is_empty());
}

#[gpui::test]
fn hover_redetects_only_on_cell_or_ctrl_change(cx: &mut TestAppContext) {
    let (session, probe) = session(cx, "go https://example.com/a here");
    let mut state = MouseState::default();
    let mut hover = UrlHover::default();
    let move_to =
        |position: Point<Pixels>, mods: Modifiers, state: &mut MouseState, hover: &mut UrlHover| {
            cx.update(|cx| {
                state.moved(
                    &MouseMoveEvent {
                        position,
                        pressed_button: None,
                        modifiers: mods,
                    },
                    &session,
                    &inputs(),
                    hover,
                    cx,
                )
            })
        };
    move_to(cell(0, 5), Modifiers::default(), &mut state, &mut hover);
    assert!(hover.is_hovering(), "the URL under the pointer is detected");

    // A sub-cell move does not re-query the grid.
    let before = probe.input_calls().len();
    let inside_same_cell = point(cell(0, 5).x + px(1.0), cell(0, 5).y + px(1.0));
    move_to(
        inside_same_cell,
        Modifiers::default(),
        &mut state,
        &mut hover,
    );
    assert_eq!(
        probe.input_calls().len() - before,
        1,
        "only the mouse_move forward, no re-detection"
    );
    assert!(!hover.needs_detection(inside_same_cell, (0, 5), false));
    // Ctrl alone forces a fresh detection.
    assert!(hover.needs_detection(inside_same_cell, (0, 5), true));
    // Leaving the grid drops the highlight.
    let outcome = cx.update(|cx| {
        state.moved(
            &moved(point(px(0.0), px(0.0)), None),
            &session,
            &inputs(),
            &mut hover,
            cx,
        )
    });
    assert_eq!(outcome, MouseOutcome::Handled);
    assert!(!hover.is_hovering());
}

#[gpui::test]
fn modifiers_changed_redetects_at_the_last_cell(cx: &mut TestAppContext) {
    let (session, _probe) = session(cx, "go https://example.com/a here");
    let state = MouseState::default();
    let mut hover = UrlHover::default();
    // Without a known position there is nothing to re-detect.
    let event = ModifiersChangedEvent {
        modifiers: ctrl(),
        capslock: Capslock { on: false },
    };
    let outcome =
        cx.update(|cx| state.modifiers_changed(&event, &session, &inputs(), &mut hover, cx));
    assert_eq!(outcome, MouseOutcome::Ignored);

    hover.needs_detection(cell(0, 5), (0, 5), false);
    let outcome =
        cx.update(|cx| state.modifiers_changed(&event, &session, &inputs(), &mut hover, cx));
    assert_eq!(outcome, MouseOutcome::Handled);
    assert!(hover.is_hovering());
}

#[test]
fn pixel_to_grid_fractional_and_bounds() {
    let g = geometry();
    let (row, col) = g
        .pixel_to_grid(point(px(100.0 + 8.0 * 3.5), px(50.0 + 16.0 * 2.0)))
        .expect("inside the grid");
    assert!((row - 2.0).abs() < 1e-5 && (col - 3.5).abs() < 1e-5);
    // Outside on every side.
    assert_eq!(g.pixel_to_grid(point(px(99.0), px(60.0))), None);
    assert_eq!(g.pixel_to_grid(point(px(110.0), px(49.0))), None);
    assert_eq!(
        g.pixel_to_grid(point(px(100.0 + 8.0 * 20.0), px(60.0))),
        None
    );
    assert_eq!(
        g.pixel_to_grid(point(px(110.0), px(50.0 + 16.0 * 5.0))),
        None
    );
    assert_eq!(g.cell_at(point(px(115.9), px(81.9))), Some((1, 1)));
}

#[gpui::test]
fn events_before_first_paint_are_ignored(cx: &mut TestAppContext) {
    let (session, probe) = session(cx, "hello");
    let mut state = MouseState::default();
    let mut hover = UrlHover::default();
    let blind = MouseInputs {
        geometry: None,
        ..inputs()
    };
    cx.update(|cx| {
        assert_eq!(
            state.down(
                &down(MouseButton::Left, cell(0, 0), Modifiers::default()),
                &session,
                &blind,
                cx
            ),
            MouseOutcome::Ignored
        );
        assert_eq!(
            state.moved(&moved(cell(0, 0), None), &session, &blind, &mut hover, cx),
            MouseOutcome::Ignored
        );
        assert_eq!(
            state.up(&up(MouseButton::Left, cell(0, 0)), &session, &blind, cx),
            MouseOutcome::Ignored
        );
        assert_eq!(
            state.wheel(
                &ScrollWheelEvent {
                    position: cell(0, 0),
                    delta: ScrollDelta::Lines(point(0.0, 3.0)),
                    modifiers: Modifiers::default(),
                    touch_phase: TouchPhase::Moved,
                },
                &session,
                &blind,
                cx
            ),
            MouseOutcome::Ignored
        );
    });
    assert!(probe.input_calls().is_empty());
}

#[gpui::test]
fn selection_drag_continues_outside_the_grid(cx: &mut TestAppContext) {
    let (session, probe) = session(cx, "hello");
    let mut state = MouseState::default();
    let mut hover = UrlHover::default();
    cx.update(|cx| {
        state.down(
            &down(MouseButton::Left, cell(1, 1), Modifiers::default()),
            &session,
            &inputs(),
            cx,
        )
    });
    assert_eq!(state.drag(), Drag::Selecting);
    probe.take_input_calls();
    // Far below the grid: the drag keeps going with clamped coordinates.
    cx.update(|cx| {
        state.moved(
            &moved(point(px(140.0), px(400.0)), Some(MouseButton::Left)),
            &session,
            &inputs(),
            &mut hover,
            cx,
        )
    });
    assert!(matches!(
        probe.input_calls().as_slice(),
        [FakeInputCall::MouseDrag { row, .. }] if (*row - 4.0).abs() < 1e-6
    ));
}
