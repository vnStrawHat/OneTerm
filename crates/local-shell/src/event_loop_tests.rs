//! Tests for the local-shell event loop notifier.

use super::*;

fn notifier(
    capacity: usize,
) -> (
    ShellNotifier,
    mpsc::Receiver<ShellMsg>,
    std::sync::Arc<ShellControl>,
) {
    let poller = std::sync::Arc::new(Poller::new().unwrap());
    let control = std::sync::Arc::new(ShellControl::default());
    let (sender, receiver) = mpsc::sync_channel(capacity);
    let notifier = ShellNotifier {
        sender,
        poller,
        control: control.clone(),
    };
    (notifier, receiver, control)
}

#[test]
fn input_queue_is_bounded_by_messages_and_bytes() {
    let (notifier, receiver, control) = notifier(1);
    notifier.send(ShellMsg::Input(Cow::Owned(vec![1]))).unwrap();
    let error = notifier
        .send(ShellMsg::Input(Cow::Owned(vec![2])))
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
    assert_eq!(control.queued_input_bytes.load(Ordering::Acquire), 1);
    assert!(matches!(
        receiver.try_recv().unwrap(),
        ShellMsg::Input(bytes) if bytes.as_ref() == [1]
    ));
}

#[test]
fn aggregate_local_input_bytes_are_bounded() {
    let (notifier, receiver, control) = notifier(2);
    notifier
        .send(ShellMsg::Input(Cow::Owned(vec![
            0;
            LOCAL_COMMAND_BYTE_BUDGET
        ])))
        .unwrap();
    let error = notifier
        .send(ShellMsg::Input(Cow::Owned(vec![1])))
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
    assert_eq!(receiver.try_iter().count(), 1);
    assert_eq!(
        control.queued_input_bytes.load(Ordering::Acquire),
        LOCAL_COMMAND_BYTE_BUDGET
    );
}

#[test]
fn local_input_queue_preserves_fifo_order() {
    let (notifier, receiver, _control) = notifier(2);
    notifier
        .send(ShellMsg::Input(Cow::Borrowed(b"first")))
        .unwrap();
    notifier
        .send(ShellMsg::Input(Cow::Borrowed(b"second")))
        .unwrap();
    assert!(matches!(
        receiver.try_recv().unwrap(),
        ShellMsg::Input(bytes) if bytes.as_ref() == b"first"
    ));
    assert!(matches!(
        receiver.try_recv().unwrap(),
        ShellMsg::Input(bytes) if bytes.as_ref() == b"second"
    ));
}

#[test]
fn resize_is_latest_value_and_shutdown_is_immediate() {
    let (notifier, receiver, control) = notifier(1);
    let first = WindowSize {
        num_lines: 24,
        num_cols: 80,
        cell_width: 0,
        cell_height: 0,
    };
    let latest = WindowSize {
        num_lines: 40,
        num_cols: 120,
        cell_width: 0,
        cell_height: 0,
    };
    notifier
        .send(ShellMsg::Input(Cow::Borrowed(b"queue is full")))
        .unwrap();
    notifier.send(ShellMsg::Resize(first)).unwrap();
    notifier.send(ShellMsg::Resize(latest)).unwrap();
    let pending = control.pending_resize.lock().unwrap().take().unwrap();
    assert_eq!((pending.num_lines, pending.num_cols), (40, 120));
    assert_eq!(receiver.try_iter().count(), 1);

    notifier.send(ShellMsg::Shutdown).unwrap();
    assert!(control.shutdown.load(Ordering::Acquire));
}

/// TEST-10 / CORR-05: an `Exited(None)` child event (Windows watcher could not
/// read the exit code) must still end the session: `alive` is cleared and both
/// `Exited` and `Closed` reach the UI.
#[test]
fn child_exit_without_status_still_ends_the_session() {
    let (event_tx, event_rx) = async_channel::bounded::<SessionEvent>(16);
    let state = crate::state::new_shared();
    state.lock().unwrap().alive = true;
    let listener = LocalListener::new(event_tx, state.clone());

    publish_child_exit(&listener, &state, None);

    {
        let st = state.lock().unwrap();
        assert!(!st.alive);
        assert_eq!(st.exit_code, None);
    }
    let events: Vec<SessionEvent> = std::iter::from_fn(|| event_rx.try_recv().ok()).collect();
    assert_eq!(
        events,
        vec![
            SessionEvent::Exited(None),
            SessionEvent::Output,
            SessionEvent::Closed
        ]
    );
}

/// A child exit with a status records the code and also emits `Closed`.
#[test]
fn child_exit_with_status_records_code_and_closes() {
    let (event_tx, event_rx) = async_channel::bounded::<SessionEvent>(16);
    let state = crate::state::new_shared();
    state.lock().unwrap().alive = true;
    let listener = LocalListener::new(event_tx, state.clone());

    publish_child_exit(&listener, &state, Some(exit_status(3)));

    {
        let st = state.lock().unwrap();
        assert!(!st.alive);
        assert_eq!(st.exit_code, Some(3));
    }
    let events: Vec<SessionEvent> = std::iter::from_fn(|| event_rx.try_recv().ok()).collect();
    assert_eq!(events.first(), Some(&SessionEvent::Exited(Some(3))));
    assert_eq!(events.last(), Some(&SessionEvent::Closed));
}

#[cfg(unix)]
fn exit_status(code: i32) -> std::process::ExitStatus {
    use std::os::unix::process::ExitStatusExt;

    std::process::ExitStatus::from_raw(code << 8)
}

#[cfg(windows)]
fn exit_status(code: u32) -> std::process::ExitStatus {
    use std::os::windows::process::ExitStatusExt;

    std::process::ExitStatus::from_raw(code)
}
