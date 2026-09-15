//! Blocking Windows pipes made pollable.
//!
//! An anonymous pipe cannot be attached to an IOCP, so each end gets a thread
//! that does the blocking `ReadFile`/`WriteFile` and a bounded byte ring in
//! between. The ring posts a completion packet to the caller's `Poller` when it
//! has something for the caller, which is what makes the pseudo-console look
//! like any other pollable source.
//!
//! One `Mutex<VecDeque<u8>>` guards each ring. A pseudo-console delivers tens of
//! MiB/s; a single uncontended lock per 64 KiB chunk is not the limiter, and the
//! alternative (a lock-free ring crate) is a dependency this crate does not need.
//!
//! A ring is **level-triggered**: it posts a packet whenever it is usable and
//! the caller is not already looking at it, and `read`/`write` re-post when they
//! leave the ring usable. A caller may therefore stop reading with bytes still
//! buffered and be woken again, which is what [`PollMode::Level`] promises and
//! what an embedder's read loop relies on.
//!
//! The registration semantics — readable/writable gating, clearing a one-shot
//! interest after posting, and the priming packet on first registration — follow
//! Alacritty's `alacritty_terminal/src/tty/windows/blocking.rs`
//! (<https://github.com/alacritty/alacritty>), Copyright the Alacritty
//! contributors, licensed under the Apache License 2.0, and are modified here:
//! the `piper` ring and its wakers are replaced by `std` synchronisation.

use std::collections::VecDeque;
use std::io;
use std::os::windows::io::{AsRawHandle, OwnedHandle};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

use polling::os::iocp::{CompletionPacket, PollerIocpExt};
use polling::{Event, PollMode, Poller};
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Storage::FileSystem::{ReadFile, WriteFile};

/// Bytes moved between the pipe thread and the caller in one go.
const CHUNK: usize = 64 * 1024;

/// Which side of the ring the caller sits on, and therefore which `Event` flag
/// makes a completion packet worth posting.
#[derive(Clone, Copy)]
enum Side {
    /// The caller reads; wake it when bytes arrive.
    Read,
    /// The caller writes; wake it when room appears.
    Write,
}

struct Interest {
    event: Event,
    poller: Arc<Poller>,
    mode: PollMode,
}

#[derive(Default)]
struct Bytes {
    queue: VecDeque<u8>,
    /// The caller found the ring unusable (empty when reading, full when
    /// writing) and is waiting for a completion packet.
    caller_waiting: bool,
    /// The owning end was dropped; the pipe thread must stop.
    closed: bool,
}

/// The bounded ring plus its poll registration.
struct Ring {
    bytes: Mutex<Bytes>,
    /// Signals the pipe thread that the ring changed.
    changed: Condvar,
    interest: Mutex<Option<Interest>>,
    capacity: usize,
    side: Side,
}

impl Ring {
    fn new(capacity: usize, side: Side) -> Arc<Self> {
        Arc::new(Self {
            bytes: Mutex::new(Bytes::default()),
            changed: Condvar::new(),
            interest: Mutex::new(None),
            capacity,
            side,
        })
    }

    fn lock(&self) -> MutexGuard<'_, Bytes> {
        // A poisoned ring still holds valid bytes: the pipe thread only ever
        // panics through a caller-supplied `Read`/`Write`, which these two are
        // not. Recovering keeps a panic in one session from wedging the loop.
        self.bytes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn interest(&self) -> MutexGuard<'_, Option<Interest>> {
        self.interest
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Post a completion packet so the caller's poll wakes up.
    fn wake(&self) {
        let mut interest = self.interest();
        let Some(registered) = interest.as_ref() else {
            return;
        };
        let wanted = match self.side {
            Side::Read => registered.event.readable,
            Side::Write => registered.event.writable,
        };
        if !wanted {
            return;
        }
        // Best effort: a closed poller means the caller is gone, and there is
        // nothing left to wake.
        let _ = registered
            .poller
            .post(CompletionPacket::new(registered.event));
        if matches!(registered.mode, PollMode::Oneshot | PollMode::EdgeOneshot) {
            *interest = None;
        }
    }

    /// Register (or re-register) the caller's interest.
    ///
    /// `prime` forces one packet even when the ring has nothing to say, so the
    /// caller's first `poll.wait` cannot block forever on a pipe that has not
    /// spoken yet.
    fn register(&self, poller: &Arc<Poller>, event: Event, mode: PollMode, prime: bool) {
        *self.interest() = Some(Interest {
            event,
            poller: poller.clone(),
            mode,
        });
        let usable = {
            let bytes = self.lock();
            match self.side {
                Side::Read => !bytes.queue.is_empty(),
                Side::Write => bytes.queue.len() < self.capacity,
            }
        };
        if usable || prime {
            self.wake();
        }
    }

    fn deregister(&self) {
        *self.interest() = None;
    }

    /// Stop the pipe thread; called when the caller's end is dropped.
    fn close(&self) {
        self.lock().closed = true;
        self.changed.notify_all();
    }
}

// ── The reading side: pipe thread fills, caller drains ───────────────────────

/// The caller's end of the conout pipe.
pub struct PipeReader {
    ring: Arc<Ring>,
    first_register: bool,
}

impl PipeReader {
    /// Spawn the thread that drains `pipe` into a `capacity`-byte ring.
    pub(super) fn new(pipe: OwnedHandle, capacity: usize) -> io::Result<Self> {
        let ring = Ring::new(capacity, Side::Read);
        let thread_ring = ring.clone();
        spawn_pipe_thread("oneterm-vt-pty-conout", move || {
            let mut chunk = [0u8; CHUNK];
            loop {
                let read = match read_pipe(&pipe, &mut chunk) {
                    Ok(0) => return,
                    Ok(read) => read,
                    Err(error) => {
                        log::debug!("oneterm-vt-pty: conout read ended: {error}");
                        return;
                    }
                };
                if !push(&thread_ring, &chunk[..read]) {
                    return;
                }
            }
        })?;
        Ok(Self {
            ring,
            first_register: true,
        })
    }

    pub(super) fn register(&mut self, poller: &Arc<Poller>, event: Event, mode: PollMode) {
        let prime = std::mem::take(&mut self.first_register);
        self.ring.register(poller, event, mode, prime);
    }

    pub(super) fn deregister(&self) {
        self.ring.deregister();
    }
}

/// Append to the ring, blocking while it is full. `false` once the caller is gone.
fn push(ring: &Ring, data: &[u8]) -> bool {
    let mut bytes = ring.lock();
    while bytes.queue.len() >= ring.capacity && !bytes.closed {
        bytes = ring
            .changed
            .wait(bytes)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
    }
    if bytes.closed {
        return false;
    }
    bytes.queue.extend(data);
    let wake = std::mem::take(&mut bytes.caller_waiting);
    drop(bytes);
    if wake {
        ring.wake();
    }
    true
}

impl io::Read for PipeReader {
    /// Never blocks, and honours [`PollMode::Level`]: the caller is woken again
    /// whenever the ring is still readable, so it may stop reading with bytes
    /// buffered — to yield, or because its buffer filled — and be told there is
    /// more. `Ok(0)` means "nothing buffered".
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut bytes = self.ring.lock();
        let was_full = bytes.queue.len() >= self.ring.capacity;
        let taken = bytes.queue.len().min(buf.len());
        for (slot, byte) in buf[..taken].iter_mut().zip(bytes.queue.drain(..taken)) {
            *slot = byte;
        }
        // Left empty: the next `push` owes the caller its packet. Left readable:
        // post now, because nothing else will — `push` only fires on an armed
        // ring, so an edge missed here is a session that stalls with output in
        // hand and no diagnostic.
        let left = bytes.queue.len();
        bytes.caller_waiting = left == 0;
        drop(bytes);
        if was_full {
            self.ring.changed.notify_all();
        }
        if left > 0 {
            self.ring.wake();
        }
        Ok(taken)
    }
}

impl Drop for PipeReader {
    fn drop(&mut self) {
        self.ring.close();
    }
}

// ── The writing side: caller fills, pipe thread drains ───────────────────────

/// The caller's end of the conin pipe.
pub struct PipeWriter {
    ring: Arc<Ring>,
}

impl PipeWriter {
    /// Spawn the thread that drains a `capacity`-byte ring into `pipe`.
    pub(super) fn new(pipe: OwnedHandle, capacity: usize) -> io::Result<Self> {
        let ring = Ring::new(capacity, Side::Write);
        let thread_ring = ring.clone();
        spawn_pipe_thread("oneterm-vt-pty-conin", move || {
            let mut chunk = Vec::with_capacity(CHUNK);
            loop {
                if !pull(&thread_ring, &mut chunk) {
                    return;
                }
                if let Err(error) = write_pipe(&pipe, &chunk) {
                    log::debug!("oneterm-vt-pty: conin write ended: {error}");
                    return;
                }
            }
        })?;
        Ok(Self { ring })
    }

    pub(super) fn register(&self, poller: &Arc<Poller>, event: Event, mode: PollMode) {
        self.ring.register(poller, event, mode, false);
    }

    pub(super) fn deregister(&self) {
        self.ring.deregister();
    }
}

/// Take everything buffered, blocking while the ring is empty. `false` once the
/// caller is gone.
fn pull(ring: &Ring, out: &mut Vec<u8>) -> bool {
    let mut bytes = ring.lock();
    while bytes.queue.is_empty() && !bytes.closed {
        bytes = ring
            .changed
            .wait(bytes)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
    }
    if bytes.closed {
        return false;
    }
    let take = bytes.queue.len().min(CHUNK);
    out.clear();
    out.extend(bytes.queue.drain(..take));
    let wake = std::mem::take(&mut bytes.caller_waiting);
    drop(bytes);
    if wake {
        ring.wake();
    }
    true
}

impl io::Write for PipeWriter {
    /// Never blocks. A short count (`0` included) leaves the remainder with the
    /// caller, which is how the local-shell loop already handles partial writes.
    ///
    /// Level semantics, the mirror of `PipeReader`'s `read`: while the ring has
    /// room the caller is woken again, so it may stop writing and come back.
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut bytes = self.ring.lock();
        let room = self.ring.capacity.saturating_sub(bytes.queue.len());
        let taken = room.min(buf.len());
        bytes.queue.extend(&buf[..taken]);
        let room_left = room - taken;
        bytes.caller_waiting = room_left == 0;
        drop(bytes);
        if taken > 0 {
            self.ring.changed.notify_all();
        }
        if room_left > 0 {
            self.ring.wake();
        }
        Ok(taken)
    }

    fn flush(&mut self) -> io::Result<()> {
        // The pipe thread drains continuously; there is no buffer to push.
        Ok(())
    }
}

impl Drop for PipeWriter {
    fn drop(&mut self) {
        self.ring.close();
    }
}

// ── Platform glue ────────────────────────────────────────────────────────────

/// A pipe thread that cannot be joined: it is parked in a blocking `ReadFile` or
/// `WriteFile` and only returns when the pipe breaks.
///
// `docs/agents/error-policy.md`, transport row.
/// A failure to spawn is fatal to the session — without this thread the
/// pseudo-console can never move a byte in that direction — so it is reported
/// rather than logged.
fn spawn_pipe_thread(name: &str, body: impl FnOnce() + Send + 'static) -> io::Result<()> {
    std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(body)
        .map(drop)
}

fn read_pipe(pipe: &OwnedHandle, buf: &mut [u8]) -> io::Result<usize> {
    let mut read = 0u32;
    // SAFETY: `buf` is a valid writable slice for `buf.len()` bytes and the
    // handle is owned by this thread for the whole call.
    let ok = unsafe {
        ReadFile(
            pipe.as_raw_handle() as HANDLE,
            buf.as_mut_ptr(),
            buf.len() as u32,
            &mut read,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(read as usize)
}

fn write_pipe(pipe: &OwnedHandle, mut buf: &[u8]) -> io::Result<()> {
    while !buf.is_empty() {
        let mut written = 0u32;
        // SAFETY: `buf` is a valid readable slice for `buf.len()` bytes and the
        // handle is owned by this thread for the whole call.
        let ok = unsafe {
            WriteFile(
                pipe.as_raw_handle() as HANDLE,
                buf.as_ptr(),
                buf.len() as u32,
                &mut written,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        if written == 0 {
            return Err(io::Error::from(io::ErrorKind::WriteZero));
        }
        buf = &buf[written as usize..];
    }
    Ok(())
}

#[cfg(test)]
#[path = "pipe_tests.rs"]
mod pipe_tests;
