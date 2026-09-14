//! The ring's poll contract: `PollMode::Level` means "usable" is re-announced,
//! not announced once.
//!
//! Both tests fail against a ring that only posts from `push`/`pull`, which is
//! the stall `US-0083`'s verification found: a caller that stops reading with
//! bytes buffered is never woken again and the session hangs with output in
//! hand.

use std::io::{Read, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};

use polling::{Event, Events, PollMode, Poller};

use super::*;

const KEY: usize = 7;
const RING: usize = 4096;

/// One `poll.wait`; `false` if nothing arrived before the timeout.
fn woken(poller: &Poller, events: &mut Events) -> bool {
    events.clear();
    poller
        .wait(events, Some(Duration::from_secs(2)))
        .expect("poll");
    events.iter().next().is_some()
}

/// The pipe thread runs on its own schedule; wait for it to hand the ring the
/// whole chunk so the partial read below is deterministic.
fn wait_for_buffered(reader: &PipeReader, wanted: usize) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while reader.ring.lock().queue.len() < wanted {
        assert!(
            Instant::now() < deadline,
            "{wanted} bytes never reached the ring"
        );
        std::thread::yield_now();
    }
}

#[test]
fn a_reader_left_with_bytes_buffered_is_woken_again() {
    let (pipe_read, mut child) = std::io::pipe().expect("a pipe");
    let mut reader = PipeReader::new(OwnedHandle::from(pipe_read), RING).expect("conout thread");
    let poller = Arc::new(Poller::new().expect("poller"));
    let mut events = Events::new();

    reader.register(&poller, Event::readable(KEY), PollMode::Level);
    assert!(
        woken(&poller, &mut events),
        "the first registration primes one packet"
    );

    // Arm the ring the way any caller does: read until it reports empty.
    let mut sink = [0u8; 64];
    while reader.read(&mut sink).expect("read") > 0 {}

    child.write_all(&[b'x'; 32]).expect("child output");
    assert!(
        woken(&poller, &mut events),
        "arriving bytes must wake a waiting caller"
    );
    wait_for_buffered(&reader, 32);

    // Stop reading with 24 bytes still buffered — the defect this test pins.
    let mut small = [0u8; 8];
    assert_eq!(reader.read(&mut small).expect("read"), 8);
    assert!(
        woken(&poller, &mut events),
        "a level-triggered reader was not woken with bytes still buffered"
    );

    // Draining exactly must re-arm the push-side wake, not swallow it.
    let mut rest = [0u8; 64];
    assert_eq!(reader.read(&mut rest).expect("read"), 24);
    child.write_all(b"more").expect("child output");
    assert!(
        woken(&poller, &mut events),
        "an exactly drained reader was not re-armed"
    );
}

#[test]
fn a_writer_with_room_left_is_woken_again() {
    let (_pipe_read, pipe_write) = std::io::pipe().expect("a pipe");
    let mut writer = PipeWriter::new(OwnedHandle::from(pipe_write), RING).expect("conin thread");
    let poller = Arc::new(Poller::new().expect("poller"));
    let mut events = Events::new();

    writer.register(&poller, Event::writable(KEY), PollMode::Level);
    assert!(
        woken(&poller, &mut events),
        "an empty ring is writable at registration"
    );

    assert_eq!(writer.write(b"typed").expect("write"), 5);
    assert!(
        woken(&poller, &mut events),
        "a level-triggered writer was not woken with room still left"
    );
}
