//! The Unix pseudo-terminal: `openpty`, a session leader, and a reaper thread.
//!
//! Child exit is **not** delivered through a process-global `SIGCHLD` handler.
//! A library crate that installs one fights every other runtime in the process
//! (tokio, the crash handler, the UI toolkit), so instead one thread per session
//! owns the `Child`, blocks in `wait`, and pokes a socket that the caller's
//! poller already watches. `wait` is as race-free as `SIGCHLD` and it reaps the
//! zombie on the way.

use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::mem::MaybeUninit;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::ptr;
use std::sync::{Arc, mpsc};

use polling::{Event, PollMode, Poller};

use crate::{
    ChildEvent, EventedPty, EventedReadWrite, OnResize, Options, PTY_CHILD_EVENT_TOKEN,
    PTY_READ_WRITE_TOKEN, WindowSize,
};

/// The signal mask a spawned child should start with.
///
/// Capture it on a thread where terminal signals are unblocked and hand it to
/// [`Options::child_signal_mask`]: a child forked from a worker thread otherwise
/// inherits that thread's blocked mask, and Ctrl-C never reaches it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SignalMask(libc::sigset_t);

impl SignalMask {
    /// The calling thread's current mask.
    pub fn current() -> io::Result<Self> {
        let mut set = MaybeUninit::<libc::sigset_t>::uninit();

        // `pthread_sigmask` only writes the kernel-relevant prefix of a
        // `sigset_t`, so the padding has to be zeroed first: reading it (in
        // `PartialEq`) would otherwise be undefined behaviour.
        //
        // SAFETY: `set` is a valid, writable `sigset_t`.
        if unsafe { libc::sigemptyset(set.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: as above; a null `set` argument means "only read".
        let result =
            unsafe { libc::pthread_sigmask(libc::SIG_SETMASK, ptr::null(), set.as_mut_ptr()) };
        if result != 0 {
            return Err(io::Error::from_raw_os_error(result));
        }
        // SAFETY: `pthread_sigmask` succeeded, so the set is initialized.
        Ok(Self(unsafe { set.assume_init() }))
    }

    /// Apply the mask to the calling (pre-exec) process.
    fn apply(&self) -> io::Result<()> {
        // SAFETY: `self.0` is an initialized `sigset_t`.
        let result = unsafe { libc::sigprocmask(libc::SIG_SETMASK, &self.0, ptr::null_mut()) };
        if result == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

impl fmt::Debug for SignalMask {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("SignalMask").finish()
    }
}

/// A child process running behind a pseudo-terminal.
pub struct PseudoConsole {
    /// The controlling side, non-blocking; both the reader and the writer.
    master: File,
    /// Readable once the child has exited, so the caller's poller wakes.
    exit_signal: UnixStream,
    exit_events: mpsc::Receiver<ChildEvent>,
    pid: u32,
}

impl PseudoConsole {
    /// Start `options.shell` (or `$SHELL`) on a new pseudo-terminal.
    pub fn spawn(options: &Options, size: WindowSize) -> io::Result<Self> {
        let (master, slave) = open_pty()?;
        set_utf8_input(&master);
        set_window_size(master.as_raw_fd(), size)?;

        let master_fd = master.as_raw_fd();
        let slave_fd = slave.as_raw_fd();

        let mut command = match options.shell.as_ref() {
            Some(shell) => {
                let mut command = Command::new(&shell.program);
                command.args(shell.args.as_slice());
                command
            }
            None => Command::new(default_shell()),
        };

        command.stdin(Stdio::from(slave.try_clone()?));
        command.stderr(Stdio::from(slave.try_clone()?));
        command.stdout(Stdio::from(slave.try_clone()?));

        for (key, value) in &options.env {
            command.env(key, value);
        }
        // Startup-notification tokens are single-use and belong to the process
        // that was launched, not to a shell it opens.
        command.env_remove("XDG_ACTIVATION_TOKEN");
        command.env_remove("DESKTOP_STARTUP_ID");

        let working_directory = options
            .working_directory
            .as_ref()
            .and_then(|path| std::ffi::CString::new(path.as_os_str().as_bytes()).ok());
        let signal_mask = options.child_signal_mask;
        // SAFETY: the closure runs between `fork` and `exec` and calls only
        // async-signal-safe functions.
        unsafe {
            command.pre_exec(move || {
                // Become a session leader, so the pty is a session of its own.
                if libc::setsid() == -1 {
                    return Err(io::Error::last_os_error());
                }

                // An invalid startup directory is not worth failing the spawn
                // over; the shell starts wherever it inherited.
                if let Some(directory) = working_directory.as_ref() {
                    libc::chdir(directory.as_ptr());
                }

                set_controlling_terminal(slave_fd)?;

                // The child talks to the pty through its stdio only.
                libc::close(slave_fd);
                libc::close(master_fd);

                if let Some(mask) = signal_mask {
                    mask.apply()?;
                }

                // The parent may have handlers installed; the shell wants the
                // defaults.
                for signal in [
                    libc::SIGCHLD,
                    libc::SIGHUP,
                    libc::SIGINT,
                    libc::SIGQUIT,
                    libc::SIGTERM,
                    libc::SIGALRM,
                ] {
                    libc::signal(signal, libc::SIG_DFL);
                }

                Ok(())
            });
        }

        let child = command.spawn().map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "cannot start '{}': {error}",
                    command.get_program().to_string_lossy()
                ),
            )
        })?;
        let pid = child.id();

        // SAFETY: `master` is an open, owned descriptor.
        unsafe { set_nonblocking(master_fd)? };

        let (exit_signal, waker) = UnixStream::pair()?;
        exit_signal.set_nonblocking(true)?;
        let (events_tx, exit_events) = mpsc::channel();
        reap_in_background(child, events_tx, waker)?;

        Ok(Self {
            master: File::from(master),
            exit_signal,
            exit_events,
            pid,
        })
    }

    /// The child's process id, on both platforms.
    pub fn child_pid(&self) -> Option<u32> {
        Some(self.pid)
    }
}

/// Wait for `child` off-thread and report its exit through `events` + `waker`.
///
/// A failure to spawn is fatal to the session: nothing else ever reports the
/// child's exit, so the caller would own a tab that can never close.
fn reap_in_background(
    mut child: std::process::Child,
    events: mpsc::Sender<ChildEvent>,
    mut waker: UnixStream,
) -> io::Result<()> {
    std::thread::Builder::new()
        .name("oneterm-pty-reaper".to_owned())
        .spawn(move || {
            let status = child.wait().ok();
            // Both sends are best effort: a torn-down session has already
            // dropped the receiving ends.
            let _ = events.send(ChildEvent::Exited(status));
            let _ = io::Write::write_all(&mut waker, &[1]);
        })
        .map(drop)
}

// No `Drop` sends a signal, and that is the invariant: the reaper thread owns
// the `Child` and its `wait()` reaps the zombie, which releases the pid for
// reuse. A `kill(self.pid, …)` here would therefore be aimed at whatever the
// kernel handed the pid to next — and the common reason to drop a session is
// that the child already exited. Closing `master` instead is both safe and
// sufficient: the last close of the controlling terminal makes the line
// discipline send `SIGHUP` to the child's foreground process group, which is
// exactly the hang-up that was wanted.

impl EventedReadWrite for PseudoConsole {
    type Reader = File;
    type Writer = File;

    unsafe fn register(
        &mut self,
        poller: &Arc<Poller>,
        mut interest: Event,
        mode: PollMode,
    ) -> io::Result<()> {
        interest.key = PTY_READ_WRITE_TOKEN;
        // SAFETY: both sources are owned by `self` and deregistered on drop of
        // the loop that registered them, as the trait requires.
        unsafe {
            poller.add_with_mode(&self.master, interest, mode)?;
            poller.add_with_mode(
                &self.exit_signal,
                Event::readable(PTY_CHILD_EVENT_TOKEN),
                PollMode::Level,
            )
        }
    }

    fn reregister(
        &mut self,
        poller: &Arc<Poller>,
        mut interest: Event,
        mode: PollMode,
    ) -> io::Result<()> {
        interest.key = PTY_READ_WRITE_TOKEN;
        poller.modify_with_mode(&self.master, interest, mode)?;
        poller.modify_with_mode(
            &self.exit_signal,
            Event::readable(PTY_CHILD_EVENT_TOKEN),
            PollMode::Level,
        )
    }

    fn deregister(&mut self, poller: &Arc<Poller>) -> io::Result<()> {
        poller.delete(&self.master)?;
        poller.delete(&self.exit_signal)
    }

    fn reader(&mut self) -> &mut File {
        &mut self.master
    }

    fn writer(&mut self) -> &mut File {
        &mut self.master
    }
}

impl EventedPty for PseudoConsole {
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        // Clear the level-triggered wake-up before reporting, so the caller does
        // not spin on a socket that stays readable.
        let mut byte = [0u8; 1];
        if let Err(error) = self.exit_signal.read(&mut byte)
            && error.kind() != io::ErrorKind::WouldBlock
        {
            log::error!("oneterm-pty: cannot read the child-exit signal: {error}");
        }
        self.exit_events.try_recv().ok()
    }
}

impl OnResize for PseudoConsole {
    fn on_resize(&mut self, size: WindowSize) -> io::Result<()> {
        set_window_size(self.master.as_raw_fd(), size)
    }
}

/// `openpty`, with the size applied afterwards so the call is identical on every
/// Unix (the `termios`/`winsize` arguments differ in constness by platform).
fn open_pty() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut master: libc::c_int = -1;
    let mut slave: libc::c_int = -1;
    // SAFETY: both out-parameters are valid `c_int`s and every optional
    // argument is null.
    let result = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            ptr::null_mut(),
            ptr::null_mut::<libc::termios>(),
            ptr::null_mut::<libc::winsize>(),
        )
    };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `openpty` succeeded, so both descriptors are open and owned here.
    unsafe { Ok((OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave))) }
}

/// Ask the line discipline to treat input as UTF-8. Best effort: a terminal that
/// does not support the flag still works.
fn set_utf8_input(master: &OwnedFd) {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let mut termios = MaybeUninit::<libc::termios>::uninit();
        // SAFETY: `master` is an open terminal descriptor and `termios` is
        // writable.
        unsafe {
            if libc::tcgetattr(master.as_raw_fd(), termios.as_mut_ptr()) != 0 {
                return;
            }
            let mut termios = termios.assume_init();
            termios.c_iflag |= libc::IUTF8;
            libc::tcsetattr(master.as_raw_fd(), libc::TCSANOW, &termios);
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let _ = master;
}

/// Make the pty the child's controlling terminal. Really only needed on BSD,
/// harmless elsewhere.
fn set_controlling_terminal(fd: RawFd) -> io::Result<()> {
    // SAFETY: `fd` is the open slave side of a pty.
    //
    // `TIOCSCTTY` is typed differently per platform and architecture, so the
    // request is cast generically.
    #[allow(clippy::cast_lossless)]
    let result = unsafe { libc::ioctl(fd, libc::TIOCSCTTY as _, 0) };
    if result == 0 {
        return Ok(());
    }
    Err(io::Error::last_os_error())
}

fn set_window_size(fd: RawFd, size: WindowSize) -> io::Result<()> {
    let winsize = libc::winsize {
        ws_row: size.rows,
        ws_col: size.cols,
        ws_xpixel: size.cols.saturating_mul(size.cell_width),
        ws_ypixel: size.rows.saturating_mul(size.cell_height),
    };
    // SAFETY: `fd` is an open terminal descriptor and `winsize` outlives the call.
    let result = unsafe { libc::ioctl(fd, libc::TIOCSWINSZ, &winsize as *const libc::winsize) };
    if result == 0 {
        return Ok(());
    }
    Err(io::Error::other(format!(
        "ioctl TIOCSWINSZ to {}x{} failed: {}",
        size.cols,
        size.rows,
        io::Error::last_os_error()
    )))
}

/// # Safety
///
/// `fd` must be open and owned by the caller.
unsafe fn set_nonblocking(fd: RawFd) -> io::Result<()> {
    // SAFETY: guaranteed by the caller.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL, 0) };
    if flags == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: as above.
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// The shell to run when the caller did not name one.
fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_owned())
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::Shell;

    fn size(rows: u16, cols: u16) -> WindowSize {
        WindowSize {
            rows,
            cols,
            cell_width: 8,
            cell_height: 16,
        }
    }

    fn options(program: &str, args: &[&str]) -> Options {
        Options {
            shell: Some(Shell::new(
                program.to_owned(),
                args.iter().map(|arg| (*arg).to_owned()).collect(),
            )),
            ..Options::default()
        }
    }

    fn wait_for_exit(console: &mut PseudoConsole, timeout: Duration) -> Option<ChildEvent> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Some(event) = console.next_child_event() {
                return Some(event);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        None
    }

    #[test]
    fn child_exit_is_reported_once_with_its_code() {
        let mut console =
            PseudoConsole::spawn(&options("/bin/sh", &["-c", "exit 3"]), size(24, 80))
                .expect("spawn a pty child");

        let event = wait_for_exit(&mut console, Duration::from_secs(5)).expect("a child exit");
        let ChildEvent::Exited(Some(status)) = event else {
            panic!("expected an exit status, got {event:?}");
        };
        assert_eq!(status.code(), Some(3));
        assert!(console.next_child_event().is_none());
    }

    #[test]
    fn the_child_pid_is_available() {
        let console = PseudoConsole::spawn(&options("/bin/sh", &["-c", "exit 0"]), size(24, 80))
            .expect("spawn a pty child");
        assert!(console.child_pid().is_some_and(|pid| pid > 0));
    }

    #[test]
    fn output_reaches_the_reader() {
        let mut console =
            PseudoConsole::spawn(&options("/bin/sh", &["-c", "printf hi"]), size(24, 80))
                .expect("spawn a pty child");

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut output = Vec::new();
        while Instant::now() < deadline && !output.windows(2).any(|pair| pair == b"hi") {
            let mut buffer = [0u8; 256];
            match console.reader().read(&mut buffer) {
                Ok(0) => std::thread::sleep(Duration::from_millis(10)),
                Ok(read) => output.extend_from_slice(&buffer[..read]),
                Err(_) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        assert!(
            output.windows(2).any(|pair| pair == b"hi"),
            "the child's output never arrived: {output:?}"
        );
    }

    #[test]
    fn resizing_a_live_pty_succeeds() {
        let mut console =
            PseudoConsole::spawn(&options("/bin/sh", &["-c", "sleep 5"]), size(24, 80))
                .expect("spawn a pty child");
        console.on_resize(size(40, 120)).expect("resize the pty");
    }

    #[test]
    fn a_rejected_resize_is_an_error_not_a_panic() {
        let mut console =
            PseudoConsole::spawn(&options("/bin/sh", &["-c", "sleep 5"]), size(24, 80))
                .expect("spawn a pty child");
        // Whatever the kernel answers, the answer is a value: the code this
        // replaces aborted the process on a failed ioctl.
        if let Err(error) = console.on_resize(size(0, 0)) {
            assert!(error.to_string().contains("TIOCSWINSZ"));
        }
        console.on_resize(size(30, 100)).expect("a valid resize");
    }

    #[test]
    fn input_reaches_the_child() {
        let mut console = PseudoConsole::spawn(&options("/bin/cat", &[]), size(24, 80))
            .expect("spawn a pty child");
        console.writer().write_all(b"ping\n").expect("write input");

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut output = Vec::new();
        while Instant::now() < deadline && !contains(&output, b"ping") {
            let mut buffer = [0u8; 256];
            match console.reader().read(&mut buffer) {
                Ok(0) => std::thread::sleep(Duration::from_millis(10)),
                Ok(read) => output.extend_from_slice(&buffer[..read]),
                Err(_) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        assert!(contains(&output, b"ping"), "the echo never arrived");
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    }
}
