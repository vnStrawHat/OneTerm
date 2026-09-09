//! Shared fixtures for the `terminal_view` tests: the global init every
//! window needs and a view hosted as a window root.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{AnyView, AppContext as _, Entity, TestAppContext, VisualTestContext};
use gpui_component::Root;
use oneterm_terminal::TerminalSession;

use super::{TerminalDeps, TerminalView};

/// Initialise the globals a `TerminalView` reads (theme, panel registry,
/// settings, completion history).
pub(super) fn init(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    cx.update(crate::init);
    cx.update(oneterm_settings::TerminalSettings::init);
    cx.update(oneterm_state::GlobalCompletionHistory::init);
}

/// Open a window showing a `TerminalView` over `session` under a
/// `gpui_component::Root` (which `push_notification` requires).
pub(super) fn open_view(
    cx: &mut TestAppContext,
    session: Box<dyn TerminalSession>,
) -> (Entity<TerminalView>, &mut VisualTestContext) {
    init(cx);
    let slot: Rc<RefCell<Option<Entity<TerminalView>>>> = Rc::default();
    let created = slot.clone();
    let (_root, cx) = cx.add_window_view(move |window, cx| {
        let session = cx.new(|_| session);
        let view =
            cx.new(|cx| TerminalView::new(session, TerminalDeps::from_globals(cx), window, cx));
        *created.borrow_mut() = Some(view.clone());
        Root::new(AnyView::from(view), window, cx)
    });
    cx.run_until_parked();
    let view = slot.borrow().clone().expect("the window closure ran");
    (view, cx)
}
