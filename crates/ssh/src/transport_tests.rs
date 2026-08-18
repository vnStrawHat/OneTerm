//! Bounded-queue and secret-safety tests for `SshTransport`.

use std::sync::{Mutex, Once};

use log::{LevelFilter, Log, Metadata, Record};

use super::*;

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

fn make_transport(
    command_capacity: usize,
) -> (SshTransport, Sender<Cmd>, async_channel::Receiver<Cmd>) {
    let (tx, rx) = async_channel::bounded(command_capacity);
    (SshTransport::new(tx.clone()), tx, rx)
}

#[test]
fn phase1_ssh_input_is_not_logged() {
    capture_logs();
    let (transport, _sender, commands) = make_transport(4);
    let sentinel = b"PHASE0_DO_NOT_LOG_SECRET_7fd65c";

    assert_eq!(transport.pty_write(sentinel), Ok(()));

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
fn ssh_command_failures_are_counted_when_saturated_and_closed() {
    let (transport, sender, commands) = make_transport(1);
    sender.try_send(Cmd::Close).unwrap();

    assert_eq!(
        transport.pty_write(b"dropped command"),
        Err(TerminalError::QueueFull)
    );
    assert_eq!(transport.diagnostics().command_full, 1);
    assert_eq!(commands.len(), 1);

    commands.close();
    assert_eq!(transport.pty_resize(24, 80), Err(TerminalError::Closed));
    assert_eq!(transport.diagnostics().command_closed, 1);
}

#[test]
fn ssh_writes_are_bounded_by_payload_bytes() {
    let (transport, _sender, commands) = make_transport(4);
    let budget = vec![0; SSH_COMMAND_BYTE_BUDGET];
    assert_eq!(transport.pty_write(&budget), Ok(()));
    assert_eq!(transport.pty_write(&[1]), Err(TerminalError::QueueFull));
    assert_eq!(commands.len(), 1);
    assert_eq!(
        transport.diagnostics().queued_write_bytes,
        SSH_COMMAND_BYTE_BUDGET
    );
    assert!(matches!(
        commands.try_recv(),
        Ok(Cmd::Write(bytes)) if bytes.len() == SSH_COMMAND_BYTE_BUDGET
    ));
    transport.release_write_bytes(SSH_COMMAND_BYTE_BUDGET);
    assert_eq!(transport.diagnostics().queued_write_bytes, 0);
}

#[test]
fn ssh_resizes_coalesce_to_the_latest_dimensions() {
    let (transport, _sender, commands) = make_transport(4);
    transport.pty_resize(24, 80).unwrap();
    transport.pty_resize(40, 120).unwrap();
    assert_eq!(commands.len(), 1);
    assert!(matches!(commands.try_recv(), Ok(Cmd::Resize)));
    assert_eq!(transport.take_pending_resize(), Some((40, 120)));
    assert_eq!(transport.take_pending_resize(), None);
}

#[test]
fn ssh_write_queue_preserves_fifo_order() {
    let (transport, _sender, commands) = make_transport(4);
    transport.pty_write(b"first").unwrap();
    transport.pty_write(b"second").unwrap();
    assert!(matches!(
        commands.try_recv(),
        Ok(Cmd::Write(bytes)) if bytes == b"first"
    ));
    assert!(matches!(
        commands.try_recv(),
        Ok(Cmd::Write(bytes)) if bytes == b"second"
    ));
    transport.release_write_bytes(11);
}

#[test]
fn phase1_close_is_honored_even_when_command_queue_is_full() {
    let (transport, sender, commands) = make_transport(1);
    // Fill the command queue to capacity.
    sender.try_send(Cmd::Write(b"x".to_vec())).unwrap();
    assert_eq!(commands.len(), 1);

    // A regular write would be dropped (queue full)...
    assert_eq!(
        transport.pty_write(b"dropped"),
        Err(TerminalError::QueueFull)
    );
    assert_eq!(commands.len(), 1);

    // ...but close sets the closing flag even if Cmd::Close is dropped.
    // The tokio task checks is_closing() to ensure it exits.
    assert_eq!(transport.pty_close(), Ok(()));
    assert!(transport.is_closing());
    assert_eq!(commands.len(), 1);
    // Once closing, writes and resizes are refused outright.
    assert_eq!(transport.pty_write(b"late"), Err(TerminalError::Closed));
    assert_eq!(transport.pty_resize(1, 1), Err(TerminalError::Closed));

    // Now drain the queue and try again — Cmd::Close fits.
    assert!(matches!(commands.try_recv(), Ok(Cmd::Write(_))));
    assert_eq!(transport.pty_close(), Ok(()));
    assert!(matches!(commands.try_recv(), Ok(Cmd::Close)));
}
