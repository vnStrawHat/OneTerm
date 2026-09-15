//! The ConPTY pseudo-console and the command line it starts.
//!
//! `push_escaped_arg` and its test table are adapted from Alacritty
//! (<https://github.com/alacritty/alacritty>), Copyright the Alacritty
//! contributors, licensed under the Apache License 2.0 — which in turn adapted
//! it from the Rust standard library — and are modified for this crate.

use std::ffi::OsStr;
use std::io;
use std::iter::once;
use std::os::windows::ffi::OsStrExt;
use std::sync::Arc;

use polling::{Event, PollMode, Poller};

mod child;
mod conpty;
mod pipe;

use child::ChildExitWatcher;
use conpty::Conpty;
pub use pipe::{PipeReader, PipeWriter};

use crate::pty::{
    ChildEvent, EventedPty, EventedReadWrite, OnResize, Options, PTY_CHILD_EVENT_TOKEN,
    PTY_READ_WRITE_TOKEN, Shell, WindowSize,
};

/// Bytes buffered per pipe direction (1 MiB), matching the caller's read buffer.
const PIPE_CAPACITY: usize = 0x10_0000;

/// A child process running inside a Windows pseudo-console.
pub struct PseudoConsole {
    /// INVARIANT: first field, so it drops first.
    ///
    /// `ClosePseudoConsole` blocks until the conout pipe is drained, which means
    /// `conout` must still be alive while the pseudo-console closes. Moving this
    /// field down deadlocks on drop.
    conpty: Conpty,
    conout: PipeReader,
    conin: PipeWriter,
    child: ChildExitWatcher,
}

impl PseudoConsole {
    /// Start `options.shell` (or `powershell`) in a new pseudo-console.
    pub fn spawn(options: &Options, size: WindowSize) -> io::Result<Self> {
        conpty::spawn(options, size)
    }

    /// The child's process id, on both platforms.
    pub fn child_pid(&self) -> Option<u32> {
        self.child.pid()
    }
}

fn with_key(mut event: Event, key: usize) -> Event {
    event.key = key;
    event
}

impl EventedReadWrite for PseudoConsole {
    type Reader = PipeReader;
    type Writer = PipeWriter;

    unsafe fn register(
        &mut self,
        poller: &Arc<Poller>,
        interest: Event,
        mode: PollMode,
    ) -> io::Result<()> {
        self.reregister(poller, interest, mode)
    }

    fn reregister(
        &mut self,
        poller: &Arc<Poller>,
        interest: Event,
        mode: PollMode,
    ) -> io::Result<()> {
        // Nothing here is a real OS source: every wake-up is a completion packet
        // posted by one of the pipe threads or by the child-exit callback, so
        // registering is just recording where to post.
        self.conin
            .register(poller, with_key(interest, PTY_READ_WRITE_TOKEN), mode);
        self.conout
            .register(poller, with_key(interest, PTY_READ_WRITE_TOKEN), mode);
        self.child
            .register(poller, with_key(interest, PTY_CHILD_EVENT_TOKEN));
        Ok(())
    }

    fn deregister(&mut self, _poller: &Arc<Poller>) -> io::Result<()> {
        self.conin.deregister();
        self.conout.deregister();
        self.child.deregister();
        Ok(())
    }

    fn reader(&mut self) -> &mut Self::Reader {
        &mut self.conout
    }

    fn writer(&mut self) -> &mut Self::Writer {
        &mut self.conin
    }
}

impl EventedPty for PseudoConsole {
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        self.child.next_event()
    }
}

impl OnResize for PseudoConsole {
    fn on_resize(&mut self, size: WindowSize) -> io::Result<()> {
        self.conpty.on_resize(size)
    }
}

/// Join the program and its arguments into one `CreateProcessW` command line.
///
/// `CreateProcessW` is called with a null application name, so the program is
/// part of the command line and the caller is responsible for quoting it (see
/// `crates/local-shell/src/session.rs`).
fn cmdline(options: &Options) -> String {
    let default_shell = Shell::new("powershell".to_owned(), Vec::new());
    let shell = options.shell.as_ref().unwrap_or(&default_shell);

    let mut command = String::new();
    command.push_str(&shell.program);
    for argument in &shell.args {
        command.push(' ');
        if options.escape_args {
            push_escaped_arg(&mut command, argument);
        } else {
            command.push_str(argument);
        }
    }
    command
}

/// Append one argument quoted by the C-runtime rules every `CreateProcessW`
/// consumer re-parses with.
fn push_escaped_arg(command: &mut String, argument: &str) {
    let bytes = argument.as_bytes();
    let quote = bytes.is_empty() || bytes.iter().any(|byte| *byte == b' ' || *byte == b'\t');
    if quote {
        command.push('"');
    }

    let mut backslashes: usize = 0;
    for character in argument.chars() {
        if character == '\\' {
            backslashes += 1;
        } else {
            if character == '"' {
                // n+1 backslashes total 2n+1 before an embedded quote.
                command.extend((0..=backslashes).map(|_| '\\'));
            }
            backslashes = 0;
        }
        command.push(character);
    }

    if quote {
        // n backslashes total 2n before the closing quote.
        command.extend((0..backslashes).map(|_| '\\'));
        command.push('"');
    }
}

/// NUL-terminated UTF-16, as every `…W` entry point wants it.
fn win32_string<S: AsRef<OsStr> + ?Sized>(value: &S) -> Vec<u16> {
    value.as_ref().encode_wide().chain(once(0)).collect()
}

#[cfg(test)]
mod pseudo_console_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn options(program: &str, args: &[&str], escape_args: bool) -> Options {
        Options {
            shell: Some(Shell::new(
                program.to_owned(),
                args.iter().map(|arg| (*arg).to_owned()).collect(),
            )),
            escape_args,
            ..Options::default()
        }
    }

    #[test]
    fn windows_argument_escaping_table() {
        let table = [
            // No escaping needed.
            ("abc", "abc"),
            // Whitespace, or empty, needs quotes.
            ("", "\"\""),
            (" ", "\" \""),
            ("ab c", "\"ab c\""),
            ("ab\tc", "\"ab\tc\""),
            // Backslashes alone are literal.
            ("ab\\c", "ab\\c"),
            // Quotes are escaped even without surrounding quotes.
            ("ab\"c", "ab\\\"c"),
            ("\"", "\\\""),
            ("a\"b\"c", "a\\\"b\\\"c"),
            // Both at once.
            ("ab \"c", "\"ab \\\"c\""),
            ("a \"b\" c", "\"a \\\"b\\\" c\""),
            // Trailing backslashes double before the closing quote.
            ("C:\\Program Files\\", "\"C:\\Program Files\\\\\""),
            ("C:\\Program Files\\a.txt", "\"C:\\Program Files\\a.txt\""),
            (
                r#"sh -c "cd /home/user; ARG='abc' \""'${SHELL:-sh}" -i -c '"'echo hello'""#,
                r#""sh -c \"cd /home/user; ARG='abc' \\\"\"'${SHELL:-sh}\" -i -c '\"'echo hello'\"""#,
            ),
        ];

        for (input, expected) in table {
            let mut escaped = String::new();
            push_escaped_arg(&mut escaped, input);
            assert_eq!(escaped, expected, "failed for {input:?}");
        }
    }

    #[test]
    fn the_command_line_escapes_only_when_asked() {
        assert_eq!(
            cmdline(&options("echo", &["hello world"], false)),
            "echo hello world"
        );
        assert_eq!(
            cmdline(&options("echo", &["hello world"], true)),
            "echo \"hello world\""
        );
    }

    #[test]
    fn the_default_shell_is_powershell() {
        assert_eq!(cmdline(&Options::default()), "powershell");
    }
}
