//! ConPTY: which host serves the pseudo-console, and how the child is started.
//!
//! Portions of the environment-block builder below are adapted from
//! Alacritty (<https://github.com/alacritty/alacritty>), Copyright
//! the Alacritty contributors, licensed under the Apache License 2.0, and
//! modified for this crate.

use std::collections::{HashMap, HashSet};
use std::ffi::{OsStr, OsString, c_void};
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

use crate::pty::windows::child::ChildExitWatcher;
use crate::pty::windows::{PIPE_CAPACITY, PseudoConsole, cmdline, win32_string};
use crate::pty::{DROPPED_PARENT_ENV, GlyphWidth, Options, WindowSize};

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
    /// That order is [a recorded decision](https://github.com/vnStrawHat/OneTerm/blob/main/docs/decisions/DEC-0013-bundled-conpty-host-and-bump-script.md) and it must not be
    /// simplified away: the inbox `conhost.exe` swallows Sixel DCS payloads, so
    /// images only survive the round trip through a bundled `OpenConsole.exe`.
    /// A build without the bundled files still runs — without Sixel
    /// passthrough.
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
                    "oneterm-vt-pty: {} has no {label}; falling back to the system ConPTY",
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

// OneTerm stages the pair from `crates/app/build.rs`; an embedder's own build
// script is what puts it there for them.
/// The directory the running executable lives in, where a bundled ConPTY pair
/// is looked for.
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
    let environment = environment_block(&options.env, std::env::vars_os());
    let creation_flags = EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT;
    let environment_pointer = environment.as_ptr() as *mut c_void;

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
/// then append `parent` minus [`DROPPED_PARENT_ENV`], as one
/// `name=value\0…\0\0` UTF-16 block.
///
/// Always a block, never the null pointer that makes `CreateProcessW` copy the
/// parent environment whole: the dropped variables must not reach the child
/// even when there is no custom entry. Windows will not eliminate duplicate
/// variables for us, hence the dedup.
fn environment_block(
    custom: &HashMap<String, String>,
    parent: impl IntoIterator<Item = (OsString, OsString)>,
) -> Vec<u16> {
    let mut block = Vec::new();
    let mut seen = HashSet::new();
    for (key, value) in custom {
        let key = OsStr::new(key);
        if seen.insert(key.to_ascii_uppercase()) {
            push_entry(&mut block, key, OsStr::new(value));
        } else {
            log::warn!(
                "oneterm-vt-pty: dropping the duplicate environment key '{}'",
                key.display()
            );
        }
    }
    // After the custom entries, so an embedder's own value for one of these
    // names is kept while the parent's is not.
    seen.extend(
        DROPPED_PARENT_ENV
            .iter()
            .map(|name| OsStr::new(name).to_ascii_uppercase()),
    );
    for (key, value) in parent {
        if seen.insert(key.to_ascii_uppercase()) {
            push_entry(&mut block, &key, &value);
        }
    }

    // A block with no entries is still two NULs: `CreateProcessW` reads a
    // single one as the start of an unterminated block.
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    block
}

fn push_entry(block: &mut Vec<u16>, key: &OsStr, value: &OsStr) {
    block.extend(key.encode_wide());
    block.push(u16::from(b'='));
    block.extend(value.encode_wide());
    block.push(0);
}

#[cfg(test)]
mod tests {
    use windows_sys::Win32::Foundation::{ERROR_INVALID_PARAMETER, GetLastError, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    use super::*;

    /// The `conpty.dll` OneTerm ships, so the preference is tested against the
    /// real binary rather than a stand-in.
    fn bundled_dll() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../app/assets/conpty.dll")
            .canonicalize()
            .expect("the bundled conpty.dll must exist (DEC-0013)")
    }

    // Removes the scratch directory on drop, including when an assertion
    // panics (`Drop` runs during unwind), so a test run never leaves one
    // behind in the OS temp directory.
    //
    // `conpty_api_prefers_the_bundled_host` stages `conpty.dll` here and
    // loads it with `LoadLibraryW`, which `load_bundled` never frees: the
    // module stays mapped for the rest of this test process, and on
    // Windows a directory holding a currently-loaded DLL cannot be
    // removed (verified: this `Drop` alone left one new directory behind
    // per run). It is harmless to defer: `scratch_directory` sweeps stale
    // siblings from earlier, provably-exited processes on its way in, so
    // the directory this Drop cannot remove today is removed on the next
    // test run instead. Only that one directory (the current process's)
    // is ever left standing at a time.
    struct ScratchDirectory(PathBuf);

    impl Drop for ScratchDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    impl std::ops::Deref for ScratchDirectory {
        type Target = Path;

        fn deref(&self) -> &Path {
            &self.0
        }
    }

    // Whether a same-prefix sibling directory is safe to remove: its trailing
    // PID no longer names a live process. `alive` is injected so this
    // decision is unit-tested without touching real OS processes; production
    // code passes `process_is_alive` below.
    //
    // A name that is not `<prefix><u32>` is left alone -- it is not a
    // directory this code owns, so it is never a removal candidate. Neither
    // is `current_pid`'s own name: it is definitionally alive for the whole
    // time this function can run against it, so it is always kept regardless
    // of what `alive` answers (guards a PID-reuse edge case in tests that
    // inject a liveness function returning `false` unconditionally).
    fn is_stale(name: &str, prefix: &str, current_pid: u32, alive: impl Fn(u32) -> bool) -> bool {
        let Some(suffix) = name.strip_prefix(prefix) else {
            return false;
        };
        let Ok(pid) = suffix.parse::<u32>() else {
            return false;
        };
        pid != current_pid && !alive(pid)
    }

    // True only if `pid` is provably no longer a live process: `OpenProcess`
    // fails with `ERROR_INVALID_PARAMETER` (no such process), or it succeeds
    // but `GetExitCodeProcess` reports something other than `STILL_ACTIVE`
    // (the process object still exists but has already exited). Any other
    // `OpenProcess` failure (for example `ERROR_ACCESS_DENIED` against a
    // process owned by another user) answers "alive": the safe direction
    // when liveness cannot actually be determined. `Win32_System_Threading`
    // is already an enabled `windows-sys` feature (`Cargo.toml`, for
    // `GetProcessMemoryInfo`), so this needs no new dependency.
    fn process_is_alive(pid: u32) -> bool {
        // SAFETY: FFI call with no pointer arguments; `pid` is a plain value.
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            // SAFETY: called immediately after the failing call above, before
            // any other API that could change the last-error value.
            return unsafe { GetLastError() } != ERROR_INVALID_PARAMETER;
        }
        let mut exit_code = 0u32;
        // SAFETY: `handle` was just returned by `OpenProcess` above and is
        // valid; `exit_code` is a valid out-parameter for this one call.
        let still_active = unsafe {
            GetExitCodeProcess(handle, &mut exit_code) != 0 && exit_code == STILL_ACTIVE as u32
        };
        // SAFETY: `handle` is the same handle `OpenProcess` returned above,
        // not used again after this call, and closed exactly once.
        unsafe { CloseHandle(handle) };
        still_active
    }

    fn scratch_directory(name: &str) -> ScratchDirectory {
        let base = std::env::temp_dir();
        let prefix = format!("oneterm-vt-pty-{name}-");
        let current_pid = std::process::id();
        let current_name = format!("{prefix}{current_pid}");

        // Sweep siblings left by earlier test processes before creating this
        // one, skipping any whose PID is still alive. A live sibling's
        // directory is never swept regardless of how far into its own setup
        // it is -- including the window between `create_dir_all` and
        // `LoadLibraryW` -- because liveness alone decides, not whether the
        // DLL happens to be loaded yet (BUG-0082 F1/F1b: the earlier,
        // unconditional sweep deleted a live peer's just-created directory
        // in exactly that window).
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                if name
                    .to_str()
                    .is_some_and(|name| is_stale(name, &prefix, current_pid, process_is_alive))
                {
                    let _ = std::fs::remove_dir_all(entry.path());
                }
            }
        }

        let directory = base.join(&current_name);
        std::fs::create_dir_all(&directory).expect("scratch directory");
        ScratchDirectory(directory)
    }

    #[test]
    fn is_stale_keeps_a_sibling_whose_pid_is_still_alive() {
        assert!(!is_stale(
            "oneterm-vt-pty-bundled-4242",
            "oneterm-vt-pty-bundled-",
            9999,
            |_| true
        ));
    }

    #[test]
    fn is_stale_removes_a_sibling_whose_pid_has_exited() {
        assert!(is_stale(
            "oneterm-vt-pty-bundled-4242",
            "oneterm-vt-pty-bundled-",
            9999,
            |_| false
        ));
    }

    #[test]
    fn is_stale_keeps_its_own_pid_even_if_the_liveness_check_would_say_dead() {
        // A liveness function that always answers "dead" would still be
        // wrong about the current process (it is, by definition, running
        // this code); the own-PID check does not even ask.
        assert!(!is_stale(
            "oneterm-vt-pty-bundled-4242",
            "oneterm-vt-pty-bundled-",
            4242,
            |_| false
        ));
    }

    #[test]
    fn is_stale_keeps_a_name_that_does_not_parse_as_prefix_plus_pid() {
        assert!(!is_stale(
            "oneterm-vt-pty-bundled-not-a-pid",
            "oneterm-vt-pty-bundled-",
            9999,
            |_| true
        ));
        assert!(!is_stale(
            "some-unrelated-directory",
            "oneterm-vt-pty-bundled-",
            9999,
            |_| true
        ));
    }

    // BUG-0082 F1/F1a regression: a sibling directory named after a real,
    // live process's PID must survive `scratch_directory`'s sweep. Uses a
    // `cmd /c pause` child this test spawns and controls (rather than a
    // fake/simulated liveness answer) so the check exercises the real
    // `process_is_alive` FFI path end to end, not just `is_stale`'s pure
    // logic (already covered above).
    #[test]
    fn sweep_keeps_a_sibling_named_after_a_live_pid() {
        let mut child = std::process::Command::new("cmd")
            .args(["/C", "pause"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn a live child process");
        let live_pid = child.id();

        // The family name embeds `live_pid` too, so this test's sweep
        // (scoped to its own prefix) can never collide with another test's
        // `bundled`/`system` family or with another concurrent run of this
        // same test on the same machine.
        let family = format!("sweep-liveness-test-{live_pid}");
        let sibling = std::env::temp_dir().join(format!("oneterm-vt-pty-{family}-{live_pid}"));
        std::fs::create_dir_all(&sibling).expect("fake sibling directory");

        let own = scratch_directory(&family);

        assert!(
            sibling.exists(),
            "a sibling directory named after a live PID must not be swept"
        );

        drop(own);
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(&sibling);
    }

    // BUG-0082 F1b regression: two processes racing `scratch_directory` for
    // the same family must never sweep each other's live, just-created
    // directory. Spawns this test binary as two concurrent child processes
    // running only `conpty_api_prefers_the_bundled_host`, ten times, and
    // requires both to pass every time (before the liveness check, this
    // failed 13 of 15 real iterations -- see the packet's verify evidence).
    // `#[ignore]`d: it takes several seconds and forks child processes.
    // Run explicitly:
    // `cargo test -p oneterm-vt --lib -- --ignored concurrent_bundled_host_race`.
    #[test]
    #[ignore = "spawns two concurrent child test processes 10x (BUG-0082 F1b regression)"]
    fn concurrent_bundled_host_race_does_not_spuriously_fail() {
        let exe = std::env::current_exe().expect("test binary path");
        for iteration in 0..10 {
            let spawn = || {
                std::process::Command::new(&exe)
                    .args([
                        "conpty_api_prefers_the_bundled_host",
                        "--exact",
                        "--test-threads=1",
                    ])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .expect("spawn concurrent test process")
            };
            let mut a = spawn();
            let mut b = spawn();
            let status_a = a.wait().expect("wait for process A");
            let status_b = b.wait().expect("wait for process B");
            assert!(
                status_a.success() && status_b.success(),
                "iteration {iteration}: two concurrent bundled-host runs must both pass, \
                 got A={status_a:?} B={status_b:?}"
            );
        }
    }

    // `DEC-0013`: this is the test that stops a refactor from quietly turning
    // Sixel passthrough off by dropping the bundled-DLL path.
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

    fn parent(entries: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        entries
            .iter()
            .map(|(key, value)| (OsString::from(key), OsString::from(value)))
            .collect()
    }

    fn entries(block: &[u16]) -> Vec<String> {
        let text = String::from_utf16_lossy(block);
        assert!(
            text.ends_with("\0\0"),
            "the block must be double-terminated"
        );
        text.split('\0')
            .filter(|entry| !entry.is_empty())
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn a_block_with_no_entries_is_a_double_nul() {
        let block = environment_block(&HashMap::new(), parent(&[]));
        assert_eq!(block, [0, 0]);
    }

    #[test]
    fn an_empty_environment_still_copies_the_parent() {
        let block = environment_block(&HashMap::new(), parent(&[("PATH", "C:\\bin")]));
        assert_eq!(entries(&block), ["PATH=C:\\bin"]);
    }

    #[test]
    fn custom_environment_deduplicates_case_insensitively() {
        let mut custom = HashMap::new();
        custom.insert("ONETERM_PTY_TEST".to_owned(), "1".to_owned());
        let block = environment_block(
            &custom,
            parent(&[("Path", "C:\\bin"), ("oneterm_pty_test", "parent")]),
        );

        assert_eq!(entries(&block), ["ONETERM_PTY_TEST=1", "Path=C:\\bin"]);
    }

    /// The parent's own value must lose to the caller's, whatever its case.
    #[test]
    fn custom_entries_win_over_the_inherited_ones() {
        let mut custom = HashMap::new();
        custom.insert("oneterm_vt_pty_override".to_owned(), "child".to_owned());
        let block = environment_block(&custom, parent(&[("ONETERM_VT_PTY_OVERRIDE", "parent")]));

        assert_eq!(entries(&block), ["oneterm_vt_pty_override=child"]);
    }

    /// OneTerm launched from a Windows Terminal tab must not tell its shells
    /// they run in Windows Terminal.
    #[test]
    fn another_terminals_identity_is_not_inherited() {
        let mut custom = HashMap::new();
        custom.insert("TERM_PROGRAM".to_owned(), "Embedder".to_owned());
        let block = environment_block(
            &custom,
            parent(&[
                ("WT_SESSION", "0b5f6f2e"),
                ("wt_profile_id", "{61c54bbd}"),
                ("TERM_PROGRAM", "vscode"),
                ("ConEmuPID", "4242"),
                ("USERPROFILE", "C:\\Users\\me"),
            ]),
        );

        assert_eq!(
            entries(&block),
            ["TERM_PROGRAM=Embedder", "USERPROFILE=C:\\Users\\me"]
        );
    }
}
