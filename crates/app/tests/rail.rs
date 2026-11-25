//! Close tab (specs/oneterm, "Rail"): the tab that shows has it at its end, in the rail and in the
//! strip; a press on it closes that tab and leaves the other one showing.

use std::rc::Rc;

use oneterm::{NewSession, OneTerm, keymap};
use quadra::{Key, Mods, Role, Settings, provide};
use quadra_term::echo;
use quadra_testing::Harness;

fn app(vertical: bool) -> Harness {
    let mut h = Harness::new();
    h.size = quadra::Size::new(1200.0, 700.0);
    h.tree.set_keymap(Rc::new(keymap()));
    let settings = Settings::memory();
    settings.value("tabs.vertical", true).set(vertical);
    let sessions: NewSession = Rc::new(|_| echo());
    h.mount(move || {
        provide(settings.clone());
        OneTerm::new(sessions).view()
    });
    h.settle();
    h
}

fn tabs(h: &Harness) -> Vec<(String, bool)> {
    let found = std::cell::RefCell::new(Vec::new());
    h.query(|n| {
        if n.semantics.role == Role::Tab {
            found.borrow_mut().push((n.semantics.name.clone(), n.semantics.selected));
        }
        false
    });
    found.into_inner()
}

#[test]
fn close_tab_closes_the_tab_that_shows_and_the_other_shows() {
    for vertical in [true, false] {
        let mut h = app(vertical);
        h.key(Key::T, Mods::CTRL);
        h.settle();
        assert_eq!(tabs(&h).len(), 2);
        let closes = h.query(|n| n.semantics.role == Role::Button && n.semantics.name == "Close tab");
        assert_eq!(closes.len(), 1, "only the tab that shows has it");
        // A press of the pointer, so the row under the button hears it too, unless stopped.
        let at = h.tree.window_rect(closes[0]).center();
        h.click(at.x, at.y);
        h.settle();
        assert_eq!(tabs(&h), [("Terminal".to_string(), true)], "vertical: {vertical}");
    }
}

#[test]
fn rail_tabs_have_no_line_between_them_folded_or_not_and_a_light_fill_under_the_pointer() {
    for folded in [false, true] {
        let mut h = app(true);
        h.key(Key::T, Mods::CTRL);
        h.settle();
        if folded {
            let fold =
                h.query(|n| n.semantics.role == Role::Button && n.semantics.name == "Collapse tabs to icons");
            h.tree.act(fold[0], &quadra::SemanticAction::Click);
            h.settle();
        }
        let rows = h.query(|n| n.semantics.role == Role::Tab);
        let (first, second) = (h.tree.window_rect(rows[0]), h.tree.window_rect(rows[1]));
        // No line under a tab, folded as open.
        assert_eq!(h.tree.layout_style(rows[0]).border.bottom, 0.0, "folded: {folded}");
        assert_eq!(second.y, first.bottom(), "the next starts where it ends");
        // The first does not show: under the pointer it fills panel_hover at half.
        let at = first.center();
        h.pointer(quadra_tree::PointerInput::Move, at.x, at.y);
        h.settle();
        let hover = h.tree.theme().resolve(quadra_design::token::PANEL_HOVER.alpha(0.5));
        assert_eq!(h.tree.paint_style(rows[0]).background, hover, "folded: {folded}");
        // New terminal is a box of a dashed line expanded, a plain `+` folded.
        let new = h.query(|n| n.semantics.role == Role::Button && n.semantics.name == "New terminal")[0];
        let dashed = std::iter::once(new).chain(h.tree.children(new).iter().copied());
        let dashed: Vec<_> =
            dashed.filter(|it| h.tree.paint_style(*it).border_style == quadra::BorderStyle::Dashed).collect();
        assert_eq!(dashed.len(), usize::from(!folded), "folded: {folded}");
    }
}

#[test]
fn two_clicks_in_the_rail_rename_in_a_dialog_and_cancel_keeps_the_name() {
    let mut h = app(true);
    let tab = h.get_by_role(Role::Tab, "Terminal").node();
    let at = h.tree.window_rect(tab).center();
    let twice = |h: &mut Harness| {
        for _ in 0..2 {
            for input in [quadra_tree::PointerInput::Down, quadra_tree::PointerInput::Up] {
                h.tree.pointer(input, quadra::MouseButton::Left, at, Mods::NONE, h.now());
            }
            h.settle();
        }
    };
    twice(&mut h);
    let named = |n: &quadra_tree::SemanticNode| {
        n.semantics.test_id.as_deref() == Some("rename-tab")
            && n.semantics.value.as_deref() == Some("Terminal")
    };
    assert_eq!(h.query(named).len(), 1, "the name as it is: {}", h.semantics());
    let field = h.get_by_test_id("rename-tab").node();
    assert_eq!(h.tree.window_rect(h.tree.parent(field).expect("its frame")).width, 334.0);
    h.get_by_test_id("rename-tab").set_value("logs");
    h.settle();
    h.get_by_role(Role::Button, "Cancel").click();
    h.settle();
    assert_eq!(tabs(&h), [("Terminal".to_string(), true)], "Cancel leaves it");
    // Not a third and a fourth click.
    h.advance(std::time::Duration::from_secs(1));
    twice(&mut h);
    h.get_by_test_id("rename-tab").set_value("logs");
    h.settle();
    h.get_by_role(Role::Button, "Rename").click();
    h.settle();
    assert_eq!(tabs(&h), [("logs".to_string(), true)]);
    assert!(h.query(|n| n.semantics.test_id.as_deref() == Some("rename-tab")).is_empty(), "closed");
}

#[test]
fn the_middle_button_closes_a_tab_of_the_folded_rail() {
    let mut h = app(true);
    h.key(Key::T, Mods::CTRL);
    h.settle();
    let fold = h.query(|n| n.semantics.role == Role::Button && n.semantics.name == "Collapse tabs to icons");
    h.tree.act(fold[0], &quadra::SemanticAction::Click);
    h.settle();
    let first = h.query(|n| n.semantics.role == Role::Tab)[0];
    let at = h.tree.window_rect(first).center();
    for input in [quadra_tree::PointerInput::Down, quadra_tree::PointerInput::Up] {
        h.tree.pointer(input, quadra::MouseButton::Middle, at, Mods::NONE, h.now());
    }
    h.settle();
    assert_eq!(tabs(&h).len(), 1);
}

#[test]
fn the_window_stays_with_no_tab_and_says_how_to_open_one() {
    for vertical in [true, false] {
        let mut h = app(vertical);
        h.key(Key::T, Mods::CTRL);
        h.settle();
        let button = h.get_by_role(Role::Button, "Close tab").node();
        let at = h.tree.window_rect(button).center();
        h.click(at.x, at.y);
        h.settle();
        // The last one by Ctrl+W.
        h.key(Key::W, Mods::CTRL);
        h.settle();
        assert!(tabs(&h).is_empty() && h.tree.take_window_requests().is_empty(), "vertical: {vertical}");
        assert!(h.has_text("Ctrl+T opens a terminal"));
        h.key(Key::T, Mods::CTRL);
        h.settle();
        assert_eq!(tabs(&h).len(), 1, "a new one opens");
    }
}
