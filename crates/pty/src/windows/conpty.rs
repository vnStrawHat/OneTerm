//! ConPTY: which host serves the pseudo-console, and how the child is started.
//!
//! Portions of the environment-block builder below are adapted from
//! `alacritty_terminal` (<https://github.com/alacritty/alacritty>), Copyright
//! the Alacritty contributors, licensed under the Apache License 2.0, and
//! modified for this crate.

use std::collections::{HashMap, HashSet};
use std::ffi::{OsStr, c_void};
use std::io;
use std::mem;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::Once;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, S_OK};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::System::Console::{
    COORD, ClosePseudoConsole, CreatePseudoConsole, HPCON, ResizePseudoConsole,
};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::System::Pipes::CreatePipe;
use windows_sys::Win32::System::Threading::{
    CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DeleteProcThreadAttributeList,
    EXTENDED_STARTUPINFO_PRESENT, InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
    PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOEXW,
    STARTUPINFOW, UpdateProcThreadAttribute,
};
use windows_sys::core::{HRESULT, PWSTR};
use windows_sys::s;

use crate::windows::child::ChildExitWatcher;
use crate::windows::{PIPE_CAPACITY, PseudoConsole, cmdline, win32_string};
use crate::{GlyphWidth, Options, WindowSize};

use super::pipe::{PipeReader, PipeWriter};

/// `PSEUDOCONSOLE_GLYPH_WIDTH_GRAPHEMES` — measure whole clusters.
const GLYPH_WIDTH_GRAPHEMES: u32 = 0x08;
/// `PSEUDOCONSOLE_GLYPH_WIDTH_WCSWIDTH` — `wcswidth` semantics.
///
/// Note for the record: there is no live `PSEUDOCONSOLE_PASSTHROUGH_MODE`. It
/// was experimental up to about host 1.18 and `0x8` has since been reused for
/// the graphemes bit above, so advice to pass `0x8` for "passthrough" is wrong.
const GLYPH_WIDTH_WCSWIDTH: u32 = 0x10;

type CreatePseudoConsoleFn =
    unsafe extern "system" fn(COORD, HANDLE, HANDLE, u32, *mut HPCON) -> HRESULT;
type ResizePseudoConsoleFn = unsafe extern "system" fn(HPCON, COORD) -> HRESULT;
type ClosePseudoConsoleFn = unsafe extern "system" fn(HPCON);

/// Which console host is serving this pseudo-console.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ConptyBackend {
    /// The `conpty.dll` OneTerm ships next to its executable, which launches the
    /// bundled `x64\OpenConsole.exe`.
    Bundled,
    /// `kernel32`, served by the inbox `conhost.exe`.
    System,
}

impl ConptyBackend {
    /// The name a bug report should carry, so a host-dependent defect can be
    /// attributed to the host that produced it.
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Bundled => "conpty: bundled",
            Self::System => "conpty: system",
        }
    }
}

/// The three pseudo-console entry points, bound to one host.
pub(super) struct ConptyApi {
    create: CreatePseudoConsoleFn,
    resize: ResizePseudoConsoleFn,
    close: ClosePseudoConsoleFn,
    backend: ConptyBackend,
}

impl ConptyApi {
    /// Resolve the bundled host first, `kernel32` second.
    ///
    /// That order is [`DEC-0013`] and it must not be simplified away: the inbox
    /// `conhost.exe` swallows Sixel DCS payloads, so images only survive the
    /// round trip through the bundled `OpenConsole.exe`. A build without the
    /// bundled files still runs — without Sixel passthrough.
    ///
    /// [`DEC-0013`]: ../../../../docs/decisions/DEC-0013-bundled-conpty-host-and-bump-script.md
    pub(super) fn resolve() -> Self {
        let api = match executable_directory() {
            Some(directory) => Self::resolve_in(&directory),
            None => Self::system(),
        };

        static LOGGED: Once = Once::new();
        LOGGED.call_once(|| log::info!("{}", api.backend.name()));

        api
    }

    /// The same resolution against an explicit directory, so the preference is
    /// testable in both directions.
    fn resolve_in(directory: &Path) -> Self {
        Self::load_bundled(&directory.join("conpty.dll")).unwrap_or_else(Self::system)
    }

    fn system() -> Self {
        Self {
            create: CreatePseudoConsole,
            resize: ResizePseudoConsole,
            close: ClosePseudoConsole,
            backend: ConptyBackend::System,
        }
    }

    /// Bind to `conpty.dll` at `path`, or report why it could not be used.
    ///
    /// A half-loaded bundled host is never fatal: a missing export logs at
    /// `warn` and falls through to `kernel32`.
    fn load_bundled(path: &Path) -> Option<Self> {
        type AnyProc = unsafe extern "system" fn() -> isize;

        let wide = win32_string(path);
        // SAFETY: `wide` is a NUL-terminated UTF-16 path that outlives the call.
        // The module is intentionally never freed: the returned function
        // pointers stay valid for the life of the process.
        let module = unsafe { LoadLibraryW(wide.as_ptr()) };
        if module.is_null() {
            return None;
        }

        let export = |name: windows_sys::core::PCSTR, label: &str| {
            // SAFETY: `module` is a live module handle and `name` is a
            // NUL-terminated literal.
            let proc = unsafe { GetProcAddress(module, name) };
            if proc.is_none() {
                log::warn!(
                    "oneterm-pty: {} has no {label}; falling back to the system ConPTY",
                    path.display()
                );
            }
            proc
        };
        let create = export(s!("CreatePseudoConsole"), "CreatePseudoConsole")?;
        let resize = export(s!("ResizePseudoConsole"), "ResizePseudoConsole")?;
        let close = export(s!("ClosePseudoConsole"), "ClosePseudoConsole")?;

        // SAFETY: each address was resolved by name from conpty.dll, whose
        // exports have exactly these signatures (they mirror the kernel32 ones).
        unsafe {
            Some(Self {
                create: mem::transmute::<AnyProc, CreatePseudoConsoleFn>(create),
                resize: mem::transmute::<AnyProc, ResizePseudoConsoleFn>(resize),
                close: mem::transmute::<AnyProc, ClosePseudoConsoleFn>(close),
                backend: ConptyBackend::Bundled,
            })
        }
    }
}

/// The directory OneTerm's executable lives in, where the bundled pair is
/// installed (`crates/app/build.rs` copies it there).
fn executable_directory() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(Path::to_path_buf)
}

/// An owned pseudo-console.
pub(super) struct Conpty {
    handle: HPCON,
    api: ConptyApi,
}

// SAFETY: the pseudo-console handle is just a token; the host owns the state it
// names and every call below is thread-safe.
unsafe impl Send for Conpty {}

impl Conpty {
    pub(super) fn on_resize(&mut self, size: WindowSize) -> io::Result<()> {
        // SAFETY: `self.handle` is a live pseudo-console for the life of `self`.
        let result = unsafe { (self.api.resize)(self.handle, coord(size)) };
        if result == S_OK {
            return Ok(());
        }
        Err(io::Error::other(format!(
            "ResizePseudoConsole to {}x{} failed (HRESULT {result:#x})",
            size.cols, size.rows
        )))
    }
}

impl Drop for Conpty {
    fn drop(&mut self) {
        // SAFETY: `self.handle` is live and dropped exactly once.
        //
        // This blocks until the conout pipe is drained, which is why `Conpty` is
        // the FIRST field of `PseudoConsole`: the conout pipe must still exist
        // while this runs. Reordering those fields deadlocks on close.
        unsafe { (self.api.close)(self.handle) }
    }
}

fn coord(size: WindowSize) -> COORD {
    COORD {
        X: size.cols as i16,
        Y: size.rows as i16,
    }
}

fn glyph_width_flag(width: GlyphWidth) -> u32 {
    match width {
        GlyphWidth::WcsWidth => GLYPH_WIDTH_WCSWIDTH,
        GlyphWidth::Graphemes => GLYPH_WIDTH_GRAPHEMES,
    }
}

/// Create the pseudo-console and start the child inside it.
pub(super) fn spawn(options: &Options, size: WindowSize) -> io::Result<PseudoConsole> {
    let api = ConptyApi::resolve();

    let (conout, conout_host) = anonymous_pipe()?;
    let (conin_host, conin) = anonymous_pipe()?;

    let mut handle: HPCON = 0;
    // SAFETY: both host handles are live for the call and `handle` is a valid
    // out-pointer.
    let result = unsafe {
        (api.create)(
            coord(size),
            conin_host.as_raw_handle() as HANDLE,
            conout_host.as_raw_handle() as HANDLE,
            glyph_width_flag(options.glyph_width),
            &mut handle,
        )
    };
    if result != S_OK {
        return Err(io::Error::other(format!(
            "CreatePseudoConsole failed (HRESULT {result:#x})"
        )));
    }
    // The host duplicated both handles into itself, so our copies are dead
    // weight. Closing them is what lets the conout reader see end-of-file when
    // the pseudo-console goes away, instead of blocking forever.
    drop(conin_host);
    drop(conout_host);

    // The pseudo-console must be owned before the first `?` below, so a failed
    // `CreateProcessW` still closes it.
    let conpty = Conpty { handle, api };

    let mut startup: STARTUPINFOEXW = unsafe { mem::zeroed() };
    startup.StartupInfo.cb = mem::size_of::<STARTUPINFOEXW>() as u32;
    // Set the flag but leave every standard handle null: the child then
    // inherits no handle from this process.
    startup.StartupInfo.dwFlags |= STARTF_USESTDHANDLES;

    let mut attributes = ProcThreadAttributeList::with_capacity(1)?;
    attributes.set_pseudoconsole(handle)?;
    startup.lpAttributeList = attributes.as_mut_ptr();

    let command_line = win32_string(&cmdline(options));
    let working_directory = options.working_directory.as_deref().map(win32_string);
    let environment = environment_block(&options.env);
    let mut creation_flags = EXTENDED_STARTUPINFO_PRESENT;
    let environment_pointer = match &environment {
        Some(block) => {
            creation_flags |= CREATE_UNICODE_ENVIRONMENT;
            block.as_ptr() as *mut c_void
        }
        None => ptr::null_mut(),
    };

    let mut process: PROCESS_INFORMATION = unsafe { mem::zeroed() };
    // SAFETY: every pointer is either null or valid for the duration of the
    // call; `command_line` is a writable NUL-terminated UTF-16 buffer as
    // `CreateProcessW` requires.
    let started = unsafe {
        CreateProcessW(
            ptr::null(),
            command_line.as_ptr() as PWSTR,
            ptr::null_mut(),
            ptr::null_mut(),
            0,
            creation_flags,
            environment_pointer,
            working_directory
                .as_ref()
                .map_or_else(ptr::null, |path| path.as_ptr()),
            &mut startup.StartupInfo as *mut STARTUPINFOW,
            &mut process,
        )
    };
    if started == 0 {
        let error = io::Error::last_os_error();
        return Err(io::Error::new(
            error.kind(),
            format!(
                "cannot start '{}': {error}",
                options
                    .shell
                    .as_ref()
                    .map_or("the default shell", |shell| shell.program.as_str())
            ),
        ));
    }

    // SAFETY: `CreateProcessW` succeeded, so both handles are live and owned by
    // this process. The thread handle has no use here.
    unsafe { CloseHandle(process.hThread) };
    // SAFETY: as above; ownership of the process handle moves to the watcher.
    let process_handle = unsafe { OwnedHandle::from_raw_handle(process.hProcess) };

    // Bound to locals first, not built inside the struct literal: a `?` inside
    // the literal would drop the already-evaluated fields in reverse order of
    // *evaluation*, closing the conout reader before `ClosePseudoConsole` runs —
    // the deadlock the field order in `PseudoConsole` exists to prevent. Locals
    // drop in reverse declaration order, which is the right one.
    let conout = PipeReader::new(conout, PIPE_CAPACITY)?;
    let conin = PipeWriter::new(conin, PIPE_CAPACITY)?;
    let child = ChildExitWatcher::new(process_handle)?;

    Ok(PseudoConsole {
        conpty,
        conout,
        conin,
        child,
    })
}

/// A pipe with the system default buffer size, as `(read, write)`.
fn anonymous_pipe() -> io::Result<(OwnedHandle, OwnedHandle)> {
    let mut read: HANDLE = ptr::null_mut();
    let mut write: HANDLE = ptr::null_mut();
    let attributes = SECURITY_ATTRIBUTES {
        nLength: mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: ptr::null_mut(),
        // The pseudo-console duplicates what it needs; nothing is inherited.
        bInheritHandle: 0,
    };
    // SAFETY: both out-pointers are valid and `attributes` outlives the call.
    let ok = unsafe { CreatePipe(&mut read, &mut write, &attributes, 0) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `CreatePipe` succeeded, so both handles are live and owned here.
    unsafe {
        Ok((
            OwnedHandle::from_raw_handle(read),
            OwnedHandle::from_raw_handle(write),
        ))
    }
}

/// The `STARTUPINFOEXW` attribute list, sized and freed by RAII.
struct ProcThreadAttributeList {
    storage: Box<[u8]>,
}

impl ProcThreadAttributeList {
    fn with_capacity(attributes: u32) -> io::Result<Self> {
        let mut size = 0usize;
        // SAFETY: the documented way to ask for the required size; this call is
        // expected to fail and to write `size`.
        let unexpected_success =
            unsafe { InitializeProcThreadAttributeList(ptr::null_mut(), attributes, 0, &mut size) };
        if unexpected_success != 0 {
            return Err(io::Error::last_os_error());
        }

        let mut list = Self {
            storage: vec![0u8; size].into_boxed_slice(),
        };
        // SAFETY: the buffer is exactly the size the call above asked for.
        let ok = unsafe {
            InitializeProcThreadAttributeList(list.as_mut_ptr(), attributes, 0, &mut size)
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(list)
    }

    /// Point the list at `handle`, which makes the child a client of it.
    fn set_pseudoconsole(&mut self, handle: HPCON) -> io::Result<()> {
        // SAFETY: the list is initialized, the attribute takes an `HPCON` by
        // value, and `handle` outlives the `CreateProcessW` that consumes it.
        let ok = unsafe {
            UpdateProcThreadAttribute(
                self.as_mut_ptr(),
                0,
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
                handle as *mut c_void,
                mem::size_of::<HPCON>(),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn as_mut_ptr(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.storage.as_mut_ptr().cast()
    }
}

impl Drop for ProcThreadAttributeList {
    fn drop(&mut self) {
        // SAFETY: the list was initialized by `with_capacity` and is deleted
        // exactly once, before its backing buffer is freed. `CreateProcessW` has
        // already consumed it by this point.
        unsafe { DeleteProcThreadAttributeList(self.as_mut_ptr()) };
    }
}

/// Deduplicate `custom` **case-insensitively** with the user's entries winning,
/// then append the parent environment, as one `name=value\0…\0\0` UTF-16 block.
///
/// `None` keeps the parent environment: that is what `CreateProcessW` does when
/// no block is supplied, and Windows will not eliminate duplicate variables for
/// us.
fn environment_block(custom: &HashMap<String, String>) -> Option<Vec<u16>> {
    if custom.is_empty() {
        return None;
    }

    let mut block = Vec::new();
    let mut seen = HashSet::new();
    for (key, value) in custom {
        let key = OsStr::new(key);
        if seen.insert(key.to_ascii_uppercase()) {
            push_entry(&mut block, key, OsStr::new(value));
        } else {
            log::warn!(
                "oneterm-pty: dropping the duplicate environment key '{}'",
                key.display()
            );
        }
    }
    for (key, value) in std::env::vars_os() {
        if seen.insert(key.to_ascii_uppercase()) {
            push_entry(&mut block, &key, &value);
        }
    }

    block.push(0);
    Some(block)
}

fn push_entry(block: &mut Vec<u16>, key: &OsStr, value: &OsStr) {
    block.extend(key.encode_wide());
    block.push(u16::from(b'='));
    block.extend(value.encode_wide());
    block.push(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `conpty.dll` OneTerm ships, so the preference is tested against the
    /// real binary rather than a stand-in.
    fn bundled_dll() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../app/assets/conpty.dll")
            .canonicalize()
            .expect("the bundled conpty.dll must exist (DEC-0013)")
    }

    fn scratch_directory(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("oneterm-pty-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("scratch directory");
        directory
    }

    /// DEC-0013: this is the test that stops a refactor from quietly turning
    /// Sixel passthrough off by dropping the bundled-DLL path.
    #[test]
    fn conpty_api_prefers_the_bundled_host() {
        let directory = scratch_directory("bundled");
        std::fs::copy(bundled_dll(), directory.join("conpty.dll")).expect("stage conpty.dll");

        assert_eq!(
            ConptyApi::resolve_in(&directory).backend,
            ConptyBackend::Bundled
        );
    }

    #[test]
    fn conpty_api_falls_back_to_the_system_host() {
        let directory = scratch_directory("system");
        let _ = std::fs::remove_file(directory.join("conpty.dll"));

        assert_eq!(
            ConptyApi::resolve_in(&directory).backend,
            ConptyBackend::System
        );
    }

    #[test]
    fn the_resolved_backend_is_named_for_the_log() {
        assert_eq!(ConptyBackend::Bundled.name(), "conpty: bundled");
        assert_eq!(ConptyBackend::System.name(), "conpty: system");
    }

    #[test]
    fn glyph_width_selects_the_documented_flag() {
        assert_eq!(glyph_width_flag(GlyphWidth::WcsWidth), 0x10);
        assert_eq!(glyph_width_flag(GlyphWidth::Graphemes), 0x08);
        assert_eq!(glyph_width_flag(GlyphWidth::default()), 0x10);
    }

    #[test]
    fn an_empty_environment_inherits_the_parent_block() {
        assert!(environment_block(&HashMap::new()).is_none());
    }

    #[test]
    fn custom_environment_deduplicates_case_insensitively() {
        let mut custom = HashMap::new();
        custom.insert("ONETERM_PTY_TEST".to_owned(), "1".to_owned());
        let block = environment_block(&custom).expect("a custom block");

        let text = String::from_utf16_lossy(&block);
        let entries: Vec<&str> = text.split('\0').filter(|entry| !entry.is_empty()).collect();
        let ours: Vec<&&str> = entries
            .iter()
            .filter(|entry| entry.to_ascii_uppercase().starts_with("ONETERM_PTY_TEST="))
            .collect();

        assert_eq!(ours, [&"ONETERM_PTY_TEST=1"]);
        assert!(
            text.ends_with("\0\0"),
            "the block must be double-terminated"
        );
        assert!(
            entries.len() > 1,
            "the parent environment must be appended after the custom entries"
        );
    }

    /// The parent's own value must lose to the caller's, whatever its case.
    #[test]
    fn custom_entries_win_over_the_inherited_ones() {
        // SAFETY: single-threaded setup for this test process only.
        unsafe { std::env::set_var("ONETERM_PTY_OVERRIDE", "parent") };

        let mut custom = HashMap::new();
        custom.insert("oneterm_pty_override".to_owned(), "child".to_owned());
        let block = environment_block(&custom).expect("a custom block");

        let text = String::from_utf16_lossy(&block).to_ascii_uppercase();
        assert!(text.contains("ONETERM_PTY_OVERRIDE=CHILD"));
        assert!(!text.contains("ONETERM_PTY_OVERRIDE=PARENT"));
    }
}
