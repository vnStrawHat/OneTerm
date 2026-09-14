//! Race-free child-exit notification on Windows.
//!
//! `RegisterWaitForSingleObject` fires a thread-pool callback when the child's
//! process handle signals — including immediately, for a child that has already
//! exited by the time the watcher is installed. The callback records the exit
//! status and posts a completion packet, so the caller learns about the exit
//! through the same `poll.wait` it uses for output.
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

use crate::ChildEvent;

/// How long the child is given to exit once its pseudo-console has closed,
/// before it is terminated (`DEC-0016`).
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

/// The callback's view of the watcher. Reached through a raw pointer, so it
/// lives in an `Arc` whose last owner is the watcher itself.
struct ChildExitSender {
    events: mpsc::Sender<ChildEvent>,
    interest: Mutex<Option<Interest>>,
    process: AtomicPtr<c_void>,
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
    let _ = sender.events.send(ChildEvent::Exited(status));

    let interest = sender
        .interest
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(interest) = interest.as_ref() {
        // Best effort: a dead poller means the caller is already gone.
        let _ = interest.poller.post(CompletionPacket::new(interest.event));
    }
}

pub(super) struct ChildExitWatcher {
    wait: AtomicPtr<c_void>,
    events: mpsc::Receiver<ChildEvent>,
    sender: Arc<ChildExitSender>,
    pid: Option<u32>,
    /// Kept alive for the callback; closed after `UnregisterWaitEx`.
    process: OwnedHandle,
}

impl ChildExitWatcher {
    pub(super) fn new(process: OwnedHandle) -> io::Result<Self> {
        let (tx, events) = mpsc::channel();
        let raw = process.as_raw_handle();
        let sender = Arc::new(ChildExitSender {
            events: tx,
            interest: Mutex::new(None),
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

    pub(super) fn register(&self, poller: &Arc<Poller>, event: Event) {
        *self
            .sender
            .interest
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Interest {
            poller: poller.clone(),
            event,
        });
    }

    pub(super) fn deregister(&self) {
        *self
            .sender
            .interest
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }

    /// Give the child [`CHILD_EXIT_GRACE`] to notice that its pseudo-console is
    /// gone, then terminate it (`DEC-0016`).
    ///
    /// This runs while the watcher drops, which is **after**
    /// `ClosePseudoConsole`: the watcher is the last field of `PseudoConsole`
    /// and the pseudo-console is the first. Closing the pseudo-console asks the
    /// host to end the session, and the host asks its client to exit — but a
    /// client that had not finished initialising never processes that request
    /// and then stays forever, holding its console host alive with it
    /// (`BUG-0055`).
    ///
    /// Only this child is ever touched, and only through the handle
    /// `CreateProcessW` returned — never a process matched by name (`DEC-0005`).
    fn terminate_if_still_running(&self) {
        let handle = self.process.as_raw_handle() as HANDLE;
        // SAFETY: the handle is owned by `self` and live until after this call.
        let waited = unsafe { WaitForSingleObject(handle, CHILD_EXIT_GRACE.as_millis() as u32) };
        if waited == WAIT_OBJECT_0 {
            return;
        }

        log::warn!(
            "oneterm-pty: the shell (pid {}) did not exit within {CHILD_EXIT_GRACE:?} of its \
             pseudo-console closing; terminating it",
            self.pid.unwrap_or(0)
        );
        // SAFETY: as above; the handle carries `PROCESS_TERMINATE` because this
        // process created the child.
        if unsafe { TerminateProcess(handle, 1) } == 0 {
            log::warn!(
                "oneterm-pty: terminating the shell (pid {}) failed: {}",
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
    use crate::PTY_CHILD_EVENT_TOKEN;

    fn watcher_for(child: std::process::Child) -> (ChildExitWatcher, u32) {
        use std::os::windows::io::{FromRawHandle, IntoRawHandle};

        let pid = child.id();
        // SAFETY: `Child` hands over its own process handle exactly once.
        let handle = unsafe { OwnedHandle::from_raw_handle(child.into_raw_handle()) };
        (ChildExitWatcher::new(handle).expect("watch the child"), pid)
    }

    fn wait_for_exit(watcher: &ChildExitWatcher, poller: &Arc<Poller>) -> ChildEvent {
        let mut events = polling::Events::new();
        for _ in 0..50 {
            events.clear();
            poller
                .wait(&mut events, Some(Duration::from_millis(200)))
                .expect("poll");
            if let Some(event) = watcher.next_event() {
                assert!(
                    events.iter().any(|e| e.key == PTY_CHILD_EVENT_TOKEN),
                    "the exit must arrive on the child token"
                );
                return event;
            }
        }
        panic!("the child exit was never reported");
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
