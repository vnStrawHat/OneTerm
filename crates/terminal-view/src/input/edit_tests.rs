//! Edit-command tests. Paste runs on a real window because a rejected paste
//! must produce a toast, and that path needs the theme and the notification
//! layer.

use gpui::{
    AnyView, AppContext as _, ClipboardItem, Context, Entity, IntoElement, Render, TestAppContext,
    VisualTestContext, Window, div,
};
use gpui_component::Root;
use oneterm_terminal::TerminalSession;
use oneterm_terminal::test_support::{FakeInputCall, FakeSessionProbe, FakeTerminalSession};

use super::edit::{copy_selection, paste_clipboard, select_all};

struct Host {
    session: Entity<Box<dyn TerminalSession>>,
}

impl Render for Host {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// A window whose first layer is a `Root`, which is what `push_notification`
/// requires. Returns the session entity and the probe watching it.
fn window<'a>(
    cx: &'a mut TestAppContext,
    text: &str,
) -> (
    Entity<Box<dyn TerminalSession>>,
    FakeSessionProbe,
    &'a mut VisualTestContext,
) {
    cx.update(gpui_component::init);
    let (session, probe) = FakeTerminalSession::boxed(4, 20, text);
    let session = cx.update(|cx| cx.new(|_| session));
    let child = session.clone();
    let (_root, cx) = cx.add_window_view(move |window, cx| {
        let host = cx.new(|_| Host { session: child });
        Root::new(AnyView::from(host), window, cx)
    });
    (session, probe, cx)
}

#[gpui::test]
fn copy_noop_without_selection(cx: &mut TestAppContext) {
    let (session, probe, cx) = window(cx, "hello");
    cx.update(|window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("untouched".into()));
        copy_selection(&session, window, cx);
        assert_eq!(
            cx.read_from_clipboard().and_then(|i| i.text()).as_deref(),
            Some("untouched"),
            "copying without a selection must not clear the clipboard"
        );
        probe.set_selection(Some("picked".into()));
        copy_selection(&session, window, cx);
        assert_eq!(
            cx.read_from_clipboard().and_then(|i| i.text()).as_deref(),
            Some("picked")
        );
        // Select All reaches the session unconditionally.
        select_all(&session, window, cx);
        assert!(probe.input_calls().contains(&FakeInputCall::SelectAll));
    });
}

#[gpui::test]
fn paste_scrolls_to_bottom_then_pastes(cx: &mut TestAppContext) {
    let (session, probe, cx) = window(cx, "");
    cx.update(|window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("ls -al".into()));
        paste_clipboard(&session, window, cx);
    });
    // The viewport snaps back to the live screen before the bytes go out.
    assert_eq!(probe.input_calls(), vec![FakeInputCall::ScrollToBottom]);
    assert_eq!(probe.writes(), vec![b"ls -al".to_vec()]);
}

#[gpui::test]
fn paste_too_large_notifies(cx: &mut TestAppContext) {
    let (session, probe, cx) = window(cx, "");
    // One byte over `oneterm_terminal`'s 1 MiB paste policy, which is not
    // re-exported for the view to read.
    let huge = "x".repeat(1024 * 1024 + 1);
    cx.update(|window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(huge));
        // Pushes the Warning notification; nothing may reach the PTY.
        paste_clipboard(&session, window, cx);
    });
    assert!(probe.writes().is_empty());
}
