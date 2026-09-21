//! Race-free child-exit notification on Windows — and, on drop, the last resort
//! that makes the exit happen.
//!
//! Watching is passive while the watcher lives. Dropping it is not: the
//! pseudo-console has closed by then, and a client that never noticed is
//! terminated after a bounded grace period. See
//! [`ChildExitWatcher::terminate_if_still_running`].
//!
//! `RegisterWaitForSingleObject` fires a thread-pool callback when the child's
//! process handle signals — including immediately, for a child that has already
//! exited by the time the watcher is installed. The callback records the exit
//! status and posts a completion packet, so the caller learns about the exit
//! through the same `poll.wait` it uses for output.
//!
//! Two orderings make that race-free, and both are load-bearing — a consumer
//! polls without a timeout and reads the child event only when the poll names
//! the child token, so a wake that is never posted is an exit that is never
//! reported:
//!
//! 1. **The exit is queued before the wake is posted.** A poll woken by the
//!    packet always finds the event waiting for it.
//! 2. **Whichever of the callback and [`ChildExitWatcher::register`] runs second
//!    posts the wake.** The callback fires on a thread-pool thread while the
//!    embedder is still building its session, so it can reach a watcher no
//!    poller is registered on yet; `register` then finds the exit already
//!    recorded and posts the packet itself.
//!
//! One wake per registration, not per call: re-registering after the exit has
//! been posted posts nothing more, and a poller registered after a
//! [`ChildExitWatcher::deregister`] is woken again.
//!
//! The shape — a wait callback feeding an `mpsc` channel plus an IOCP completion
//! packet, with the poll interest behind a mutex — follows Alacritty's
//! `alacritty_terminal/src/tty/windows/child.rs`
//! (<https://github.com/alacritty/alacritty>), Copyright
//! the Alacritty contributors, licensed under the Apache License 2.0, and is
//! modified here: the callback borrows an `Arc` the watcher owns and
//! `UnregisterWaitEx` fences it, instead of the original's `Box::into_raw` /
//! `UnregisterWait` pair.

use std::ffi::c_void;
use std::io;
use std::os::windows::io::{AsRawHandle, OwnedHandle};
use std::os::windows::process::ExitStatusExt;
use std::process::ExitStatus;
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use polling::os::iocp::{CompletionPacket, PollerIocpExt};
use polling::{Event, Poller};
use windows_sys::Win32::Foundation::{BOOLEAN, FALSE, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, GetProcessId, INFINITE, RegisterWaitForSingleObject, TerminateProcess,
    UnregisterWaitEx, WT_EXECUTEINWAITTHREAD, WT_EXECUTEONLYONCE, WaitForSingleObject,
};

use crate::pty::ChildEvent;

// The bound and the measurement below are `DEC-0016`.
/// How long the child is given to exit once its pseudo-console has closed,
/// before it is terminated.
///
/// Measured on `cmd.exe /K chcp 65001 >nul`: a client that had finished starting
/// exits within 20 ms of `ClosePseudoConsole`, while one that was still
/// initialising was still alive 15 s later in 8 of 9 runs. Two seconds is two
/// orders of magnitude above the normal path and still short enough that a
/// closed tab's console host does not linger visibly.
const CHILD_EXIT_GRACE: Duration = Duration::from_secs(2);

struct Interest {
    poller: Arc<Poller>,
    event: Event,
}

// The lost wake this lock closes is `BUG-0072`.
/// Where to post the wake, and whether there is one owed.
///
/// One lock over both, so that whichever of the exit callback and
/// [`ChildExitWatcher::register`] runs second is the one that posts: neither
/// can observe the other half-done, and the ordinary path still posts exactly
/// once.
#[derive(Default)]
struct Notify {
    interest: Option<Interest>,
    /// Set by the callback once the exit is in the channel — never before, so
    /// a watcher that can see this can already receive the event.
    exited: bool,
    /// Whether the exit's wake has been posted to the interest installed
    /// *now*. Registering is re-registering on Windows, so without this an
    /// embedder whose loop re-registers to toggle write interest would be
    /// handed a fresh wake on every pass once the child had exited.
    posted: bool,
}

impl Notify {
    /// Wake the registered poller, if there is one. Best effort: a dead poller
    /// means the caller is already gone.
    fn post(&mut self) {
        if let Some(interest) = self.interest.as_ref() {
            let _ = interest.poller.post(CompletionPacket::new(interest.event));
            self.posted = true;
        }
    }
}

/// The callback's view of the watcher. Reached through a raw pointer, so it
/// lives in an `Arc` whose last owner is the watcher itself.
struct ChildExitSender {
    events: mpsc::Sender<ChildEvent>,
    notify: Mutex<Notify>,
    process: AtomicPtr<c_void>,
}

impl ChildExitSender {
    fn notify(&self) -> std::sync::MutexGuard<'_, Notify> {
        self.notify
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Runs on a thread-pool wait thread once the child's handle signals.
extern "system" fn child_exited(context: *mut c_void, timed_out: BOOLEAN) {
    if timed_out != 0 {
        return;
    }

    // SAFETY: `context` is the `Arc<ChildExitSender>`'s address. `UnregisterWaitEx`
    // with `INVALID_HANDLE_VALUE` blocks until any running callback has
    // returned, and the watcher only drops its `Arc` after that call, so the
    // pointee is alive for the whole body.
    let sender = unsafe { &*(context as *const ChildExitSender) };

    let mut code = 0u32;
    let process = sender.process.load(Ordering::Relaxed) as HANDLE;
    // SAFETY: the process handle is owned by the watcher, which outlives this
    // callback for the reason above.
    let read = unsafe { GetExitCodeProcess(process, &mut code) };
    let status = (read != FALSE).then(|| ExitStatus::from_raw(code));

    // A closed receiver means the session was torn down first; nothing to report.
    // Queued *before* the wake is posted: a poll the packet wakes always finds
    // the event already there.
    let _ = sender.events.send(ChildEvent::Exited(status));

    let mut notify = sender.notify();
    notify.exited = true;
    notify.post();
}

pub(super) struct ChildExitWatcher {
    wait: AtomicPtr<c_void>,
    events: mpsc::Receiver<ChildEvent>,
    sender: Arc<ChildExitSender>,
    pid: Option<u32>,
    /// The child's process handle: kept alive for the callback, then used by
    /// [`ChildExitWatcher::terminate_if_still_running`] on drop, and closed
    /// after both.
    process: OwnedHandle,
}

impl ChildExitWatcher {
    pub(super) fn new(process: OwnedHandle) -> io::Result<Self> {
        let (tx, events) = mpsc::channel();
        let raw = process.as_raw_handle();
        let sender = Arc::new(ChildExitSender {
            events: tx,
            notify: Mutex::new(Notify::default()),
            process: AtomicPtr::new(raw),
        });

        let mut wait: HANDLE = ptr::null_mut();
        // SAFETY: `raw` is a live process handle, and the context pointer is the
        // address of an `Arc` allocation this struct keeps alive until after
        // `UnregisterWaitEx` (see `Drop`).
        let ok = unsafe {
            RegisterWaitForSingleObject(
                &mut wait,
                raw,
                Some(child_exited),
                Arc::as_ptr(&sender) as *mut c_void,
                INFINITE,
                WT_EXECUTEINWAITTHREAD | WT_EXECUTEONLYONCE,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }

        // SAFETY: `raw` is live.
        let pid = unsafe { GetProcessId(raw) };

        Ok(Self {
            wait: AtomicPtr::new(wait),
            events,
            sender,
            pid: (pid != 0).then_some(pid),
            process,
        })
    }

    pub(super) fn pid(&self) -> Option<u32> {
        self.pid
    }

    pub(super) fn next_event(&self) -> Option<ChildEvent> {
        self.events.try_recv().ok()
    }

    /// Record where to post the child's exit — and post it at once when the
    /// child has already exited, because the callback that fired before this
    /// call had nowhere to post to.
    ///
    /// At most one wake per registration: re-registering an interest that has
    /// already been woken posts nothing, so a loop that re-registers every
    /// iteration does not spin once the child is gone.
    pub(super) fn register(&self, poller: &Arc<Poller>, event: Event) {
        let mut notify = self.sender.notify();
        notify.interest = Some(Interest {
            poller: poller.clone(),
            event,
        });
        if notify.exited && !notify.posted {
            notify.post();
        }
    }

    /// Stop waking a poller that no longer cares.
    ///
    /// The recorded exit stays, and so does the wake that goes with it: a
    /// poller registered after this one is owed it as much as the first was,
    /// because nothing here knows whether the event was ever read.
    pub(super) fn deregister(&self) {
        let mut notify = self.sender.notify();
        notify.interest = None;
        notify.posted = false;
    }

    // `DEC-0016`, and the client that provoked it is `BUG-0055`.
    /// Give the child [`CHILD_EXIT_GRACE`] to notice that its pseudo-console is
    /// gone, then terminate it.
    ///
    /// This runs while the watcher drops, which is **after**
    /// `ClosePseudoConsole`: the watcher is the last field of `PseudoConsole`
    /// and the pseudo-console is the first. Closing the pseudo-console asks the
    /// host to end the session, and the host asks its client to exit — but a
    /// client that had not finished initialising never processes that request
    /// and then stays forever, holding its console host alive with it.
    ///
    /// Only this child is ever touched, and only through the handle
    /// `CreateProcessW` returned. Never matching a process by name is a hard
    /// rule; reaching no further than this module's own child is stricter
    /// still, and is what this drop promises.
    fn terminate_if_still_running(&self) {
        let handle = self.process.as_raw_handle() as HANDLE;
        // SAFETY: the handle is owned by `self` and live until after this call.
        let waited = unsafe { WaitForSingleObject(handle, CHILD_EXIT_GRACE.as_millis() as u32) };
        if waited == WAIT_OBJECT_0 {
            return;
        }

        log::warn!(
            "oneterm-vt-pty: the shell (pid {}) did not exit within {CHILD_EXIT_GRACE:?} of its \
             pseudo-console closing; terminating it",
            self.pid.unwrap_or(0)
        );
        // SAFETY: as above; the handle carries `PROCESS_TERMINATE` because this
        // process created the child.
        if unsafe { TerminateProcess(handle, 1) } == 0 {
            log::warn!(
                "oneterm-vt-pty: terminating the shell (pid {}) failed: {}",
                self.pid.unwrap_or(0),
                io::Error::last_os_error()
            );
        }
    }
}

impl Drop for ChildExitWatcher {
    fn drop(&mut self) {
        // SAFETY: `wait` came from `RegisterWaitForSingleObject`.
        //
        // `INVALID_HANDLE_VALUE` means "wait until any running callback has
        // returned", which is the invariant the callback's raw `Arc` pointer
        // relies on. It runs before `sender` and `process` are dropped, because
        // `Drop::drop` precedes field destruction.
        unsafe { UnregisterWaitEx(self.wait.load(Ordering::Relaxed), INVALID_HANDLE_VALUE) };
        self.terminate_if_still_running();
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;
    use std::time::Duration;

    use super::*;
    use crate::pty::PTY_CHILD_EVENT_TOKEN;

    fn watcher_for(child: std::process::Child) -> (ChildExitWatcher, u32) {
        use std::os::windows::io::{FromRawHandle, IntoRawHandle};

        let pid = child.id();
        // SAFETY: `Child` hands over its own process handle exactly once.
        let handle = unsafe { OwnedHandle::from_raw_handle(child.into_raw_handle()) };
        (ChildExitWatcher::new(handle).expect("watch the child"), pid)
    }

    /// Block until the callback has recorded the exit, so that whatever the
    /// test does next is provably second.
    fn wait_until_recorded(watcher: &ChildExitWatcher) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !watcher.sender.notify().exited {
            assert!(
                std::time::Instant::now() < deadline,
                "the wait callback never ran for an already-exited child"
            );
            std::thread::yield_now();
        }
    }

    /// One `poller.wait` with a short timeout: how many child-token wakes are
    /// pending right now.
    fn pending_wakes(poller: &Arc<Poller>) -> usize {
        let mut events = polling::Events::new();
        poller
            .wait(&mut events, Some(Duration::from_millis(200)))
            .expect("poll");
        events
            .iter()
            .filter(|e| e.key == PTY_CHILD_EVENT_TOKEN)
            .count()
    }

    /// Poll until the child token wakes us, and require the exit to be there on
    /// that same wake — the "queued before posted" half of the contract.
    ///
    /// An iteration that sees no token is not a failure even if the event is
    /// already in the channel: the callback queues it a few instructions before
    /// it posts, and a poll that expires inside that gap is a timeout, not a
    /// lost wake. The packet is on its way, so the next poll returns at once.
    fn wait_for_exit(watcher: &ChildExitWatcher, poller: &Arc<Poller>) -> ChildEvent {
        let mut events = polling::Events::new();
        for _ in 0..50 {
            events.clear();
            poller
                .wait(&mut events, Some(Duration::from_millis(200)))
                .expect("poll");
            if events.iter().any(|e| e.key == PTY_CHILD_EVENT_TOKEN) {
                return watcher
                    .next_event()
                    .expect("a wake on the child token must carry the exit");
            }
        }
        panic!("the child exit was never reported on the child token");
    }

    #[test]
    fn child_exit_is_reported_once_with_its_code() {
        let poller = Arc::new(Poller::new().unwrap());
        let child = Command::new("cmd.exe")
            .args(["/c", "exit 3"])
            .spawn()
            .unwrap();
        let (watcher, _) = watcher_for(child);
        watcher.register(&poller, Event::readable(PTY_CHILD_EVENT_TOKEN));

        assert_eq!(
            wait_for_exit(&watcher, &poller),
            ChildEvent::Exited(Some(ExitStatus::from_raw(3)))
        );
        assert!(
            watcher.next_event().is_none(),
            "the exit must be reported exactly once"
        );
    }

    /// `RegisterWaitForSingleObject` fires immediately for an already-signalled
    /// handle, so a child that is gone before the watcher exists is not missed.
    #[test]
    fn instant_exit_is_not_missed() {
        let poller = Arc::new(Poller::new().unwrap());
        let mut child = Command::new("cmd.exe")
            .args(["/c", "exit 0"])
            .spawn()
            .unwrap();
        child.wait().expect("reap the child first");
        let (watcher, _) = watcher_for(child);
        watcher.register(&poller, Event::readable(PTY_CHILD_EVENT_TOKEN));

        assert_eq!(
            wait_for_exit(&watcher, &poller),
            ChildEvent::Exited(Some(ExitStatus::from_raw(0)))
        );
    }

    // The lost wake, and the load-dependent failure that exposed it, are
    // `BUG-0072`.
    /// The callback fires on a thread-pool thread and can reach the watcher
    /// before the embedder has registered a poller on it — in the real session
    /// the two are separated by a channel send and by opening the session log
    /// file. The wake is owed all the same, so `register` posts it.
    ///
    /// Deterministic, not timed: the exit is observed through the watcher's own
    /// state before `register` is called at all, so the callback is provably
    /// first.
    #[test]
    fn an_exit_before_registration_still_wakes_the_poller() {
        let poller = Arc::new(Poller::new().unwrap());
        let mut child = Command::new("cmd.exe")
            .args(["/c", "exit 5"])
            .spawn()
            .unwrap();
        child.wait().expect("reap the child first");
        let (watcher, _) = watcher_for(child);
        wait_until_recorded(&watcher);

        watcher.register(&poller, Event::readable(PTY_CHILD_EVENT_TOKEN));

        let mut events = polling::Events::new();
        poller
            .wait(&mut events, Some(Duration::from_secs(5)))
            .expect("poll");
        assert!(
            events.iter().any(|e| e.key == PTY_CHILD_EVENT_TOKEN),
            "registering after the exit must still wake the poller"
        );
        assert_eq!(
            watcher.next_event(),
            Some(ChildEvent::Exited(Some(ExitStatus::from_raw(5))))
        );
    }

    /// The first half of the contract, with no poller in it: the callback
    /// queues the exit **before** it records it, and it records it under the
    /// same lock it posts the wake from — so a watcher that can see the
    /// recorded exit can already receive the event, and a wake can never
    /// arrive ahead of it.
    ///
    /// Repeated, because a reversed order leaves a window only a few
    /// instructions wide: 30 children make it land.
    #[test]
    fn the_exit_is_queued_before_it_is_recorded() {
        for _ in 0..30 {
            let mut child = Command::new("cmd.exe")
                .args(["/c", "exit 0"])
                .spawn()
                .unwrap();
            child.wait().expect("reap the child first");
            let (watcher, _) = watcher_for(child);
            wait_until_recorded(&watcher);

            assert!(
                watcher.next_event().is_some(),
                "the exit must be in the channel before anything can observe it, \
                 because the wake is posted from there on"
            );
        }
    }

    /// One wake per registration. Re-registering is what an embedder's loop
    /// does to toggle write interest, and on Windows it lands on the very same
    /// `register`; posting again each time would spin such a loop at 100 % CPU
    /// for as long as the exited session stayed open.
    #[test]
    fn a_recorded_exit_is_posted_once_per_registration() {
        let poller = Arc::new(Poller::new().unwrap());
        let mut child = Command::new("cmd.exe")
            .args(["/c", "exit 0"])
            .spawn()
            .unwrap();
        child.wait().expect("reap the child first");
        let (watcher, _) = watcher_for(child);
        wait_until_recorded(&watcher);

        let interest = Event::readable(PTY_CHILD_EVENT_TOKEN);
        watcher.register(&poller, interest);
        assert_eq!(pending_wakes(&poller), 1, "the exit is owed one wake");
        assert!(watcher.next_event().is_some(), "and that wake carries it");

        watcher.register(&poller, interest);
        watcher.register(&poller, interest);
        assert_eq!(
            pending_wakes(&poller),
            0,
            "re-registering must not post the same exit again"
        );

        // A new poller has not been woken yet, and nothing here knows whether
        // the event was read, so the wake is owed again.
        watcher.deregister();
        watcher.register(&poller, interest);
        assert_eq!(
            pending_wakes(&poller),
            1,
            "a registration after a deregistration is owed the wake"
        );
    }

    #[test]
    fn the_child_pid_is_available() {
        let child = Command::new("cmd.exe")
            .args(["/c", "exit 0"])
            .spawn()
            .unwrap();
        let (watcher, pid) = watcher_for(child);
        assert_eq!(watcher.pid(), Some(pid));
    }

    /// A deregistered watcher still records the exit; it just stops waking a
    /// poller that no longer cares.
    #[test]
    fn deregistering_keeps_the_exit_observable() {
        let child = Command::new("cmd.exe")
            .args(["/c", "exit 7"])
            .spawn()
            .unwrap();
        let (watcher, _) = watcher_for(child);
        watcher.deregister();

        for _ in 0..50 {
            if let Some(event) = watcher.next_event() {
                assert_eq!(event, ChildEvent::Exited(Some(ExitStatus::from_raw(7))));
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("the child exit was never recorded");
    }
}
