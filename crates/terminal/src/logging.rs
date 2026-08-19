//! Per-session printable-output logging.

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use alacritty_terminal::vte::{Parser, Perform};
use chrono::Local;
use oneterm_core::{LogWriteMode, TerminalLogConfig};

/// Current state of one terminal logger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalLogState {
    /// No file is open.
    Stopped,
    /// Printable output is being written to this file.
    Running { path: PathBuf },
    /// Logging stopped after a setup or write failure.
    Failed { message: String },
}

/// A start/stop or write failure.
#[derive(Debug)]
pub struct TerminalLogError(String);

impl std::fmt::Display for TerminalLogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TerminalLogError {}

/// Thread-safe controller shared by the session, pump, and UI.
#[derive(Clone)]
pub struct TerminalLogController(Arc<Mutex<ControllerState>>);

impl std::fmt::Debug for TerminalLogController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("TerminalLogController")
            .field(&self.state())
            .finish()
    }
}

struct ControllerState {
    identity: String,
    state: TerminalLogState,
    active: Option<ActiveLogger>,
    pending_error: Option<String>,
}

struct ActiveLogger {
    path: PathBuf,
    writer: BufWriter<File>,
    parser: Parser,
    collector: LineCollector,
}

#[derive(Default)]
struct LineCollector {
    current: String,
    completed: Vec<String>,
}

impl Perform for LineCollector {
    fn print(&mut self, character: char) {
        if !character.is_control() {
            self.current.push(character);
        }
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' | b'\r' => self.commit(),
            0x08 => {
                self.current.pop();
            }
            _ => {}
        }
    }
}

impl LineCollector {
    fn commit(&mut self) {
        if !self.current.is_empty() {
            self.completed.push(std::mem::take(&mut self.current));
        }
    }

    fn take_completed(&mut self) -> Vec<String> {
        std::mem::take(&mut self.completed)
    }
}

impl Default for TerminalLogController {
    fn default() -> Self {
        Self::new("terminal")
    }
}

impl TerminalLogController {
    /// Create a stopped logger with an initial filename identity.
    pub fn new(identity: impl Into<String>) -> Self {
        Self(Arc::new(Mutex::new(ControllerState {
            identity: identity.into(),
            state: TerminalLogState::Stopped,
            active: None,
            pending_error: None,
        })))
    }

    /// Replace the identity before automatic or manual logging starts.
    pub fn set_identity(&self, identity: impl Into<String>) {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if state.active.is_none() {
            state.identity = identity.into();
        }
    }

    /// Open a log file and begin capturing output.
    pub fn start(&self, config: &TerminalLogConfig) -> Result<PathBuf, TerminalLogError> {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(active) = &state.active {
            return Ok(active.path.clone());
        }

        let active = ActiveLogger::open(&state.identity, config).map_err(|error| {
            let message = format!("Failed to start terminal logging: {error}");
            state.state = TerminalLogState::Failed {
                message: message.clone(),
            };
            state.pending_error = Some(message.clone());
            TerminalLogError(message)
        })?;
        let path = active.path.clone();
        state.active = Some(active);
        state.state = TerminalLogState::Running { path: path.clone() };
        state.pending_error = None;
        Ok(path)
    }

    /// Flush the final partial line and close the current file.
    pub fn stop(&self) -> Result<(), TerminalLogError> {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        let Some(mut active) = state.active.take() else {
            state.state = TerminalLogState::Stopped;
            state.pending_error = None;
            return Ok(());
        };
        if let Err(error) = active.finish() {
            return Err(state.fail(format!("Failed to stop terminal logging: {error}")));
        }
        state.state = TerminalLogState::Stopped;
        state.pending_error = None;
        Ok(())
    }

    /// Return the current logging state.
    pub fn state(&self) -> TerminalLogState {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .state
            .clone()
    }

    /// Take a new failure message once for user notification.
    pub fn take_error(&self) -> Option<String> {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .pending_error
            .take()
    }

    /// Feed one transport read through the printable-output parser.
    pub(crate) fn process(&self, bytes: &[u8]) {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        let Some(active) = state.active.as_mut() else {
            return;
        };
        if let Err(error) = active.process(bytes) {
            state.active = None;
            let _ = state.fail(format!("Terminal logging stopped: {error}"));
        }
    }
}

impl ControllerState {
    fn fail(&mut self, message: String) -> TerminalLogError {
        log::error!("{message}");
        self.state = TerminalLogState::Failed {
            message: message.clone(),
        };
        self.pending_error = Some(message.clone());
        TerminalLogError(message)
    }
}

impl ActiveLogger {
    fn open(identity: &str, config: &TerminalLogConfig) -> std::io::Result<Self> {
        std::fs::create_dir_all(&config.directory)?;
        let identity = sanitize_identity(identity);
        let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S");
        let path = config.directory.join(format!("{identity}_{timestamp}.log"));
        let writer = BufWriter::new(open_log_file(&path, config.write_mode)?);
        Ok(Self {
            path,
            writer,
            parser: Parser::new(),
            collector: LineCollector::default(),
        })
    }

    fn process(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.parser.advance(&mut self.collector, bytes);
        self.write_completed()
    }

    fn write_completed(&mut self) -> std::io::Result<()> {
        for message in self.collector.take_completed() {
            writeln!(
                self.writer,
                "[{}] {message}",
                Local::now().format("%Y-%m-%d %H:%M:%S")
            )?;
        }
        self.writer.flush()
    }

    fn finish(&mut self) -> std::io::Result<()> {
        self.collector.commit();
        self.write_completed()
    }
}

impl Drop for ControllerState {
    fn drop(&mut self) {
        if let Some(active) = &mut self.active
            && let Err(error) = active.finish()
        {
            log::warn!("Failed to flush terminal log during drop: {error}");
        }
    }
}

fn open_log_file(path: &Path, write_mode: LogWriteMode) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).write(true);
    match write_mode {
        LogWriteMode::Append => {
            options.append(true);
        }
        LogWriteMode::Overwrite => {
            options.truncate(true);
        }
    }
    options.open(path)
}

fn sanitize_identity(identity: &str) -> String {
    let sanitized: String = identity
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect();
    let sanitized = sanitized.trim_matches('.');
    if sanitized.is_empty() {
        "terminal".to_string()
    } else {
        sanitized.to_string()
    }
}

/// Return the executable basename used in a local log identity.
pub fn local_log_identity(program: &Path, pid: u32) -> String {
    let name = program
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("shell");
    format!("{name}_{pid}")
}

/// Return the SSH log identity.
pub fn ssh_log_identity(username: &str, host: &str, port: u16) -> String {
    format!("{username}_{host}_{port}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("oneterm-logging-{name}-{}", std::process::id()))
    }

    #[test]
    fn parser_keeps_printable_lines_and_strips_terminal_sequences() {
        let mut parser = Parser::new();
        let mut collector = LineCollector::default();
        parser.advance(&mut collector, b"hello \x1b[31mred\x1b[0m\nprogress\r");
        parser.advance(&mut collector, "héllo\n".as_bytes());

        assert_eq!(
            collector.take_completed(),
            ["hello red", "progress", "héllo"]
        );
    }

    #[test]
    fn parser_excludes_tabs_and_tracks_backspace_and_partial_chunks() {
        let mut parser = Parser::new();
        let mut collector = LineCollector::default();
        parser.advance(&mut collector, b"ab");
        parser.advance(&mut collector, b"c\x08\td\n");

        assert_eq!(collector.take_completed(), ["abd"]);
    }

    #[test]
    fn sanitizes_identity_without_path_escape() {
        assert_eq!(sanitize_identity("../a/b:c"), "_a_b_c");
        assert_eq!(sanitize_identity("..."), "terminal");
    }

    #[test]
    fn write_modes_preserve_or_truncate_an_existing_file() {
        let directory = test_dir("write-mode");
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("existing.log");
        std::fs::write(&path, "old\n").unwrap();

        writeln!(open_log_file(&path, LogWriteMode::Append).unwrap(), "new").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "old\nnew\n");

        writeln!(
            open_log_file(&path, LogWriteMode::Overwrite).unwrap(),
            "replacement"
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "replacement\n");
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn stop_flushes_unterminated_message() {
        let directory = test_dir("partial");
        let controller = TerminalLogController::new("shell_42");
        let path = controller
            .start(&TerminalLogConfig {
                enabled: true,
                directory: directory.clone(),
                write_mode: LogWriteMode::Overwrite,
            })
            .unwrap();
        controller.process(b"partial");
        controller.stop().unwrap();

        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.ends_with("] partial\n"));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
