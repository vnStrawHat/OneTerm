//! Platform-native crash capture with compromised-context-safe file writes.

use std::{fs::File, path::Path};

use crash_handler::{CrashContext, CrashEventResult, CrashHandler};

const NATIVE_REPORT_HEADER: &[u8] = b"OneTerm Native Crash Report\n\
================================\n\
Version: ";
const NATIVE_REPORT_OS: &[u8] = b"\nOS: ";
const NATIVE_REPORT_ARCH: &[u8] = b"\nArchitecture: ";
const NATIVE_REPORT_CAPTURE: &[u8] = b"\nCapture: crash-handler 0.8.0\n";

pub(crate) fn install(path: &Path) -> Result<CrashHandler, String> {
    let file = File::options()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .map_err(|error| format!("failed to open native crash staging file: {error}"))?;
    let writer = NativeCrashWriter { file };

    // SAFETY: The callback only touches preallocated/stack data and invokes a direct OS write.
    // It performs no heap allocation, locking, logging, unwinding, or normal persistence work.
    let event = unsafe {
        crash_handler::make_crash_event(move |context| {
            writer.capture(context);
            CrashEventResult::Handled(false)
        })
    };

    CrashHandler::attach(event).map_err(|error| format!("failed to attach crash handler: {error}"))
}

struct NativeCrashWriter {
    file: File,
}

impl NativeCrashWriter {
    fn capture(&self, context: &CrashContext) {
        let mut details = FixedBuffer::<512>::new();
        format_context(context, &mut details);

        if !write_direct(&self.file, NATIVE_REPORT_HEADER)
            || !write_direct(&self.file, env!("ONETERM_VERSION").as_bytes())
            || !write_direct(&self.file, NATIVE_REPORT_OS)
            || !write_direct(&self.file, std::env::consts::OS.as_bytes())
            || !write_direct(&self.file, NATIVE_REPORT_ARCH)
            || !write_direct(&self.file, std::env::consts::ARCH.as_bytes())
            || !write_direct(&self.file, NATIVE_REPORT_CAPTURE)
            || !write_direct(&self.file, details.as_bytes())
        {
            crash_handler::write_stderr("OneTerm failed to write the native crash report\n");
        }
    }
}

struct FixedBuffer<const N: usize> {
    bytes: [u8; N],
    len: usize,
}

impl<const N: usize> FixedBuffer<N> {
    const fn new() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
        }
    }

    fn push_str(&mut self, value: &str) {
        let remaining = N.saturating_sub(self.len);
        let count = remaining.min(value.len());
        self.bytes[self.len..self.len + count].copy_from_slice(&value.as_bytes()[..count]);
        self.len += count;
    }

    fn push_u64(&mut self, mut value: u64) {
        let mut digits = [0_u8; 20];
        let mut start = digits.len();
        loop {
            start -= 1;
            digits[start] = b'0' + (value % 10) as u8;
            value /= 10;
            if value == 0 {
                break;
            }
        }
        self.push_bytes(&digits[start..]);
    }

    #[cfg(any(test, all(unix, not(target_os = "macos"))))]
    fn push_i64(&mut self, value: i64) {
        if value < 0 {
            self.push_str("-");
            self.push_u64(value.unsigned_abs());
        } else {
            self.push_u64(value as u64);
        }
    }

    #[cfg(any(test, windows, target_os = "macos"))]
    fn push_hex(&mut self, mut value: u64) {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        let mut digits = [0_u8; 16];
        let mut start = digits.len();
        loop {
            start -= 1;
            digits[start] = HEX[(value & 0x0f) as usize];
            value >>= 4;
            if value == 0 {
                break;
            }
        }
        self.push_str("0x");
        self.push_bytes(&digits[start..]);
    }

    fn push_bytes(&mut self, value: &[u8]) {
        let remaining = N.saturating_sub(self.len);
        let count = remaining.min(value.len());
        self.bytes[self.len..self.len + count].copy_from_slice(&value[..count]);
        self.len += count;
    }

    fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

#[cfg(target_os = "windows")]
fn format_context(context: &CrashContext, output: &mut FixedBuffer<512>) {
    output.push_str("Exception code: ");
    output.push_hex(context.exception_code as u32 as u64);
    output.push_str("\nProcess ID: ");
    output.push_u64(context.process_id.into());
    output.push_str("\nThread ID: ");
    output.push_u64(context.thread_id.into());
    output.push_str("\n");
}

#[cfg(all(unix, not(target_os = "macos")))]
fn format_context(context: &CrashContext, output: &mut FixedBuffer<512>) {
    output.push_str("Signal: ");
    output.push_u64(context.siginfo.ssi_signo.into());
    output.push_str("\nSignal code: ");
    output.push_i64(context.siginfo.ssi_code.into());
    output.push_str("\nProcess ID: ");
    output.push_i64(context.pid.into());
    output.push_str("\nThread ID: ");
    output.push_i64(context.tid.into());
    output.push_str("\n");
}

#[cfg(target_os = "macos")]
fn format_context(context: &CrashContext, output: &mut FixedBuffer<512>) {
    output.push_str("Task: ");
    output.push_u64(context.task.into());
    output.push_str("\nThread: ");
    output.push_u64(context.thread.into());
    if let Some(exception) = context.exception {
        output.push_str("\nException kind: ");
        output.push_u64(exception.kind.into());
        output.push_str("\nException code: ");
        output.push_hex(exception.code);
        if let Some(subcode) = exception.subcode {
            output.push_str("\nException subcode: ");
            output.push_hex(subcode);
        }
    }
    output.push_str("\n");
}

#[cfg(unix)]
fn write_direct(file: &File, mut bytes: &[u8]) -> bool {
    use std::ffi::c_void;
    use std::os::fd::AsRawFd as _;

    unsafe extern "C" {
        fn write(file_descriptor: i32, buffer: *const c_void, count: usize) -> isize;
    }

    while !bytes.is_empty() {
        // SAFETY: `file` remains alive for the callback lifetime, and `bytes` points to readable
        // memory for exactly `bytes.len()` bytes. POSIX `write` is async-signal-safe.
        let written = unsafe { write(file.as_raw_fd(), bytes.as_ptr().cast(), bytes.len()) };
        if written <= 0 {
            return false;
        }
        bytes = &bytes[written as usize..];
    }
    true
}

#[cfg(target_os = "windows")]
fn write_direct(file: &File, mut bytes: &[u8]) -> bool {
    use std::os::windows::io::AsRawHandle as _;

    use windows_sys::Win32::Storage::FileSystem::WriteFile;

    while !bytes.is_empty() {
        let mut written = 0;
        // SAFETY: The file handle remains owned by `file`, the input slice is valid for its stated
        // length, `written` is a valid output pointer, and no overlapped operation is requested.
        let succeeded = unsafe {
            WriteFile(
                file.as_raw_handle(),
                bytes.as_ptr().cast(),
                bytes.len() as u32,
                &mut written,
                std::ptr::null_mut(),
            )
        };
        if succeeded == 0 || written == 0 {
            return false;
        }
        bytes = &bytes[written as usize..];
    }
    true
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "windows")]
    use std::fs;

    use super::*;

    #[test]
    fn fixed_buffer_formats_signed_and_hex_values_without_allocation() {
        let mut buffer = FixedBuffer::<64>::new();
        buffer.push_i64(-42);
        buffer.push_str(" ");
        buffer.push_hex(0xc0000005);

        assert_eq!(buffer.as_bytes(), b"-42 0xC0000005");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn simulated_native_exception_writes_staging_report() {
        let path = std::env::temp_dir().join(format!(
            "oneterm-native-crash-test-{}.txt",
            std::process::id()
        ));
        drop(fs::remove_file(&path));
        let handler = install(&path).expect("handler should attach");

        handler.simulate_exception(Some(crash_handler::ExceptionCode::User as i32));
        drop(handler);

        let report = fs::read_to_string(&path).expect("native report should be readable");
        assert!(report.contains("OneTerm Native Crash Report"));
        assert!(report.contains("Exception code: 0xCCA11ED"));
        fs::remove_file(path).expect("fixture should be deleted");
    }
}
