//! Bounded-queue delivery and secret-safety tests for `SshListener`.

use std::sync::{Mutex, Once};

use log::{LevelFilter, Log, Metadata, Record};
use oneterm_terminal::SessionEvent;
use oneterm_terminal::test_support::FakeTransport;

use super::*;
use crate::state::new_shared;

struct CaptureLogger {
    records: Mutex<Vec<String>>,
}

impl Log for CaptureLogger {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &Record<'_>) {
        self.records.lock().unwrap().push(format!(
            "{} {} {}",
            record.level(),
            record.target(),
            record.args()
        ));
    }

    fn flush(&self) {}
}

static LOGGER: CaptureLogger = CaptureLogger {
    records: Mutex::new(Vec::new()),
};
static INSTALL_LOGGER: Once = Once::new();

fn capture_logs() {
    INSTALL_LOGGER.call_once(|| {
        log::set_logger(&LOGGER).expect("test logger should install once");
        log::set_max_level(LevelFilter::Trace);
    });
    LOGGER.records.lock().unwrap().clear();
}

fn make_listener(
    event_capacity: usize,
    command_capacity: usize,
) -> (SshListener, FakeTransport<SessionEvent>, FakeTransport<Cmd>) {
    let events = FakeTransport::bounded(event_capacity);
    let commands = FakeTransport::bounded(command_capacity);
    let listener = SshListener::new(events.sender(), commands.sender(), new_shared());
    (listener, events, commands)
}

#[test]
fn phase1_ssh_input_is_not_logged() {
    capture_logs();
    let (listener, _events, commands) = make_listener(4, 4);
    let sentinel = b"PHASE0_DO_NOT_LOG_SECRET_7fd65c";

    assert_eq!(listener.pty_write(sentinel), Ok(()));

    let records = LOGGER.records.lock().unwrap().clone();
    // No log record may contain the sentinel secret.
    assert!(
        records
            .iter()
            .all(|record| !record.contains("PHASE0_DO_NOT_LOG_SECRET_7fd65c")),
        "sentinel secret leaked into log records: {records:?}"
    );
    // The write itself must still be delivered.
    assert!(matches!(
        commands.try_recv(),
        Ok(Cmd::Write(bytes)) if bytes == sentinel
    ));
    // Byte count may be logged, but not content.
    assert!(
        records.iter().any(|record| record.contains("31 bytes")),
        "expected byte-count log, got: {records:?}"
    );
}

#[test]
fn coalescible_ssh_repaint_events_are_counted_when_saturated() {
    let (listener, events, commands) = make_listener(1, 1);
    commands.try_send(Cmd::Close).unwrap();
    events.try_send(SessionEvent::Output).unwrap();

    assert_eq!(
        listener.pty_write(b"dropped command"),
        Err(TerminalError::QueueFull)
    );
    listener.forward(SessionEvent::Output);

    let diagnostics = listener.queue_diagnostics();
    assert_eq!(diagnostics.command_full, 1);
    assert_eq!(diagnostics.event_full, 1);
    assert_eq!(commands.len(), 1);
    assert_eq!(events.len(), 1);

    commands.close();
    events.close();
    assert_eq!(listener.pty_resize(24, 80), Err(TerminalError::Closed));
    listener.forward(SessionEvent::Output);
    let diagnostics = listener.queue_diagnostics();
    assert_eq!(diagnostics.command_closed, 1);
    assert_eq!(diagnostics.event_closed, 1);
}

#[test]
fn reliable_ssh_events_wait_for_queue_capacity() {
    let (listener, events, _commands) = make_listener(1, 1);
    events.try_send(SessionEvent::Output).unwrap();
    let (started_tx, started_rx) = std::sync::mpsc::sync_channel(1);
    let (finished_tx, finished_rx) = std::sync::mpsc::sync_channel(1);

    let sender = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        listener.forward(SessionEvent::Bell);
        finished_tx.send(()).unwrap();
    });

    started_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .unwrap();
    assert!(
        finished_rx
            .recv_timeout(std::time::Duration::from_millis(20))
            .is_err()
    );
    assert_eq!(events.try_recv().unwrap(), SessionEvent::Output);
    finished_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .unwrap();
    sender.join().unwrap();
    assert_eq!(events.try_recv().unwrap(), SessionEvent::Bell);
}

#[test]
fn ssh_writes_are_bounded_by_payload_bytes() {
    let (listener, events, commands) = make_listener(4, 4);
    let budget = vec![0; SSH_COMMAND_BYTE_BUDGET];
    assert_eq!(listener.pty_write(&budget), Ok(()));
    assert_eq!(listener.pty_write(&[1]), Err(TerminalError::QueueFull));
    assert_eq!(commands.len(), 1);
    assert_eq!(
        listener.queue_diagnostics().queued_write_bytes,
        SSH_COMMAND_BYTE_BUDGET
    );
    assert!(matches!(
        commands.try_recv(),
        Ok(Cmd::Write(bytes)) if bytes.len() == SSH_COMMAND_BYTE_BUDGET
    ));
    listener.release_write_bytes(SSH_COMMAND_BYTE_BUDGET);
    events.close();
}

#[test]
fn ssh_resizes_coalesce_to_the_latest_dimensions() {
    let (listener, events, commands) = make_listener(4, 4);
    listener.pty_resize(24, 80).unwrap();
    listener.pty_resize(40, 120).unwrap();
    assert_eq!(commands.len(), 1);
    assert!(matches!(commands.try_recv(), Ok(Cmd::Resize)));
    assert_eq!(listener.take_pending_resize(), Some((40, 120)));
    events.close();
}

#[test]
fn ssh_write_queue_preserves_fifo_order() {
    let (listener, events, commands) = make_listener(4, 4);
    listener.pty_write(b"first").unwrap();
    listener.pty_write(b"second").unwrap();
    assert!(matches!(
        commands.try_recv(),
        Ok(Cmd::Write(bytes)) if bytes == b"first"
    ));
    assert!(matches!(
        commands.try_recv(),
        Ok(Cmd::Write(bytes)) if bytes == b"second"
    ));
    listener.release_write_bytes(11);
    events.close();
}

#[test]
fn phase1_close_is_honored_even_when_command_queue_is_full() {
    let (listener, _events, commands) = make_listener(2, 1);
    // Fill the command queue to capacity.
    commands.try_send(Cmd::Write(b"x".to_vec())).unwrap();
    assert_eq!(commands.len(), 1);

    // A regular write would be dropped (queue full)...
    assert_eq!(
        listener.pty_write(b"dropped"),
        Err(TerminalError::QueueFull)
    );
    assert_eq!(commands.len(), 1);

    // ...but close sets the closing flag even if Cmd::Close is dropped.
    // The tokio task checks is_closing() to ensure it exits.
    assert_eq!(listener.pty_close(), Ok(()));
    assert!(listener.is_closing());
    // Cmd::Close was dropped (queue full), but the flag is set.
    assert_eq!(commands.len(), 1);

    // Now drain the queue and try again — Cmd::Close fits.
    assert!(matches!(commands.try_recv(), Ok(Cmd::Write(_))));
    assert_eq!(listener.pty_close(), Ok(()));
    assert!(matches!(commands.try_recv(), Ok(Cmd::Close)));
}
