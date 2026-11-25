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
    let (views, cx) = open_views(cx, vec![session]);
    let view = views.into_iter().next().expect("one view was requested");
    (view, cx)
}

/// Open one window holding a `TerminalView` per session — what a split tab
/// looks like to everything below the Space tree. The first view is the window
/// root; the others exist as entities, which is all an input test needs.
pub(super) fn open_views(
    cx: &mut TestAppContext,
    sessions: Vec<Box<dyn TerminalSession>>,
) -> (Vec<Entity<TerminalView>>, &mut VisualTestContext) {
    init(cx);
    let slot: Rc<RefCell<Vec<Entity<TerminalView>>>> = Rc::default();
    let created = slot.clone();
    let (_root, cx) = cx.add_window_view(move |window, cx| {
        let views: Vec<_> = sessions
            .into_iter()
            .map(|session| {
                let session = cx.new(|_| session);
                cx.new(|cx| TerminalView::new(session, TerminalDeps::from_globals(cx), window, cx))
            })
            .collect();
        *created.borrow_mut() = views.clone();
        let root = AnyView::from(views.first().expect("at least one session").clone());
        Root::new(root, window, cx)
    });
    cx.run_until_parked();
    let views = slot.borrow().clone();
    (views, cx)
}
