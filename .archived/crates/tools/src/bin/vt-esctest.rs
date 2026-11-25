//! Run an outside conformance harness against `oneterm-vt` (diagnostic).
//!
//! `esctest` talks to a terminal the way a program does: it writes escape
//! sequences to its stdout and reads the terminal's answers from its stdin. It
//! reads the screen back with `DECRQCRA` rectangle checksums, which is why it
//! could not run against this engine at all until `US-0106`.
//!
//! This binary is the bridge. It opens a pty, runs the harness on the slave
//! end, feeds everything the harness writes into a `Terminal`, and writes every
//! `VtEvent::Reply` back. There is no window, no renderer and no OneTerm
//! anywhere in the loop -- the engine is the terminal.
//!
//! ```text
//! cargo run -p oneterm-tools --bin vt-esctest -- \
//!     python3 esctest.py --expected-terminal xterm --xterm-checksum 334 \
//!                        --logfile esctest.log --max-vt-level 4
//! ```
//!
//! `--xterm-checksum 334` is not cosmetic. It is what makes `esc.py`'s
//! `empty()` return a space rather than `NUL` and what stops `escutil.py`
//! applying the pre-patch-279 negation, which together are the only
//! configuration that matches the single checksum variant this engine
//! implements. Pass anything else and every rectangle assertion fails.
//!
//! **Unix only.** `esctest` drives a pty, and the harness is the one place in
//! this repository that opens `Config::allow_screen_readback`. On Windows the
//! binary compiles to a stub that explains itself and exits non-zero, so
//! `cargo check -p oneterm-tools` still covers the file.
//!
//! It exits with the harness's own status, so the CI job that runs it must be
//! `continue-on-error`: conformance here is a report, never a gate
//! (`IN-0029/low-level-design/testing-and-bench.md` section 6).

#[cfg(not(unix))]
fn main() {
    eprintln!(
        "vt-esctest: Unix only -- esctest drives a pty, and this bridge is built \
         on `oneterm_vt::pty`'s openpty half. Run it on the Linux CI job."
    );
    std::process::exit(2);
}

#[cfg(unix)]
fn main() {
    unix::run();
}

#[cfg(unix)]
mod unix {
    use std::io::{Read, Write};
    use std::num::NonZeroUsize;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use oneterm_vt::pty::{
        ChildEvent, EventedPty, EventedReadWrite, Options, PTY_CHILD_EVENT_TOKEN,
        PTY_READ_WRITE_TOKEN, PseudoConsole, Shell, WindowSize,
    };
    use oneterm_vt::{Config, EventBatch, Size, Terminal, VtEvent};
    use polling::{Event as PollEvent, Events, PollMode, Poller};

    /// What `esctest` expects of a terminal it was not told the size of, and
    /// what xterm starts at.
    const ROWS: u16 = 24;
    const COLS: u16 = 80;

    /// A wall-clock ceiling, so a harness that waits forever for an answer the
    /// engine does not give cannot hold a CI runner until the job times out.
    /// Every unanswered query in `esctest` has its own short timeout, so the
    /// whole run is minutes; this is an order of magnitude above that.
    const DEADLINE: Duration = Duration::from_secs(900);

    /// How long to wait for the child to be reaped after the pty reports
    /// end of file, and how long each wait between polls is.
    ///
    /// Two seconds is far longer than the reaper thread needs -- it is already
    /// blocked in `waitpid` when the child exits -- and short enough that a
    /// child which somehow never reports still lets the job finish.
    const REAP_POLLS: u32 = 100;
    const REAP_INTERVAL: Duration = Duration::from_millis(20);

    pub fn run() {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let Some((program, harness_args)) = args.split_first() else {
            eprintln!("usage: vt-esctest <program> [args...]");
            eprintln!("   eg: vt-esctest python3 esctest.py --expected-terminal xterm \\");
            eprintln!("                 --xterm-checksum 334 --max-vt-level 4");
            std::process::exit(2);
        };

        let options = Options {
            shell: Some(Shell::new(program.clone(), harness_args.to_vec())),
            // `esctest` keys some expectations off `TERM`. The engine answers
            // `XTGETTCAP`'s `TN` with the same name, which is why
            // `product_name` is deliberately **not** set below: setting it
            // overrides `TN`, and a terminal that says `xterm-256color` in the
            // environment and something else over `XTGETTCAP` is contradicting
            // itself in front of a conformance harness.
            env: [
                ("TERM".to_owned(), "xterm-256color".to_owned()),
                ("COLORTERM".to_owned(), "truecolor".to_owned()),
            ]
            .into_iter()
            .collect(),
            ..Options::default()
        };
        let mut pty = match PseudoConsole::spawn(
            &options,
            WindowSize {
                rows: ROWS,
                cols: COLS,
                cell_width: 0,
                cell_height: 0,
            },
        ) {
            Ok(pty) => pty,
            Err(error) => {
                eprintln!("vt-esctest: cannot spawn {program}: {error}");
                std::process::exit(2);
            }
        };

        // The one place in this repository that opens the readback gate. Every
        // other `Config` in the workspace leaves it false, which is why the
        // engine's default behaviour is unchanged by `US-0106`.
        let mut term = Terminal::new(
            Size {
                rows: ROWS,
                cols: COLS,
            },
            Config {
                allow_screen_readback: true,
                // No `product_name`: it would override `XTGETTCAP`'s `TN` away
                // from the `TERM` set above. `esctest`'s own `escutil.py` never
                // reads `TN`, so nothing here depends on the choice -- but the
                // terminal should not contradict itself whether or not anyone
                // is checking. `XTVERSION` and `DA2` then report the engine's
                // own identity, which is the truth about what is running.
                ..Config::default()
            },
        );
        let mut batch = EventBatch::new();

        let poller = Arc::new(Poller::new().expect("a poller"));
        // SAFETY: `pty` outlives the poller loop below and is deregistered
        // before either is dropped.
        unsafe {
            pty.register(
                &poller,
                PollEvent::readable(PTY_READ_WRITE_TOKEN),
                PollMode::Level,
            )
            .expect("register the pty");
        }
        let mut events = Events::with_capacity(NonZeroUsize::new(16).unwrap());
        let mut buffer = vec![0u8; 64 * 1024];
        let started = Instant::now();
        let mut status = None;
        let mut end_of_file = false;

        'outer: while started.elapsed() < DEADLINE {
            events.clear();
            if poller
                .wait(&mut events, Some(Duration::from_millis(100)))
                .is_err()
            {
                break;
            }
            for event in events.iter() {
                if event.key == PTY_CHILD_EVENT_TOKEN {
                    if let Some(ChildEvent::Exited(code)) = pty.next_child_event() {
                        status = Some(code);
                        // One last drain: the harness's final output is often
                        // still in the pty buffer when it exits.
                        drain(&mut pty, &mut term, &mut batch, &mut buffer);
                        break 'outer;
                    }
                    continue;
                }
                if event.readable && !drain(&mut pty, &mut term, &mut batch, &mut buffer) {
                    end_of_file = true;
                    break 'outer;
                }
            }
        }

        // **The race this closes.** End of file on the pty master is how the
        // kernel says the last slave descriptor closed, and on Linux it arrives
        // as `EIO` rather than as a clean `Ok(0)`. That happens at the same
        // instant the child exits, so the master's readability and the
        // child-exit socket both become ready at once and `events.iter()` may
        // hand back either first. Leaving the loop through the end-of-file arm
        // therefore says nothing about whether the child has been reaped -- and
        // the first version of this file fell straight through to the timeout
        // branch and reported exit 2 for a harness that had finished normally.
        //
        // So: having seen end of file, wait for the reaper. `waitpid` is not
        // called here on purpose -- `PseudoConsole` owns a reaper thread that is
        // already blocked in it, and a second waiter would race it for the
        // status. `next_child_event` is how that thread hands the status over.
        if end_of_file && status.is_none() {
            for _ in 0..REAP_POLLS {
                if let Some(ChildEvent::Exited(code)) = pty.next_child_event() {
                    status = Some(code);
                    break;
                }
                std::thread::sleep(REAP_INTERVAL);
            }
        }

        let stats = term.stats();
        eprintln!(
            "vt-esctest: {} bytes fed, {} unhandled sequences, {} aborted DCS",
            stats.bytes, stats.unhandled_sequences, stats.aborted_dcs
        );

        let code = match status {
            Some(Some(status)) => status.code().unwrap_or(1),
            Some(None) => 1,
            None if end_of_file => {
                // The pty closed but the reaper never reported. The harness did
                // run, so this is not a timeout and must not be reported as
                // one; there is simply no status to pass on.
                eprintln!("vt-esctest: the pty closed but the child status never arrived");
                1
            }
            None => {
                eprintln!("vt-esctest: the harness did not exit within {DEADLINE:?}");
                2
            }
        };
        // Deregister before the poller and the console are dropped, as
        // `EventedReadWrite` requires.
        let _ = pty.deregister(&poller);
        std::process::exit(code);
    }

    /// Read everything the harness has written, feed it to the engine, and
    /// write every reply back. `false` once the pty is at end of file.
    fn drain(
        pty: &mut PseudoConsole,
        term: &mut Terminal,
        batch: &mut EventBatch,
        buffer: &mut [u8],
    ) -> bool {
        loop {
            let read = match pty.reader().read(buffer) {
                Ok(0) => return false,
                Ok(read) => read,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return true,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => return false,
            };
            batch.clear();
            term.feed(&buffer[..read], batch, Instant::now());
            let mut replies = Vec::new();
            for event in batch.iter() {
                if let VtEvent::Reply(span) = event {
                    replies.extend_from_slice(batch.bytes(*span));
                }
            }
            if !replies.is_empty() {
                let writer = pty.writer();
                // A failed write means the harness is gone; the child-exit
                // event is what ends the loop, so there is nothing to do here.
                let _ = writer.write_all(&replies);
                let _ = writer.flush();
            }
        }
    }
}
