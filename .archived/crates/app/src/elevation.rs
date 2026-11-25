//! The Win32 half of the elevated instance (`IN-0043`, `DEC-0019`): the process
//! token query that decides what this window is, the `runas` launch that starts
//! a second elevated OneTerm, and the message box the command-line errors use
//! before a window exists.
//!
//! The data half — the argument grammar, the trusted shell paths and the process
//! globals — is `oneterm_core::elevation`, which is pure `std` and carries the
//! unit tests.
//!
//! Off Windows nothing here has a meaning: [`process_elevation`] is a `const fn`
//! returning [`Elevation::NotElevated`], so every gate compiles away to today's
//! behaviour, and the launch is unreachable because the menu row that reaches it
//! is `cfg(windows)`.
//!
//! **Where the platform split is.** `cfg(windows)` covers the FFI calls and
//! nothing else — [`process_elevation`], [`console_owners`], [`free_console`],
//! [`request_elevation`] and [`fatal_message`]'s message box. Every decision
//! taken around them — [`console_action`], [`outcome_of`], [`notification_for`],
//! [`should_start_launch`] — is plain `std`, compiled and unit-tested on all
//! three CI runners. An `#[allow(dead_code)]` here would mean the split is in
//! the wrong place (`US-0130`, rework 2026-09-22).

use std::sync::atomic::{AtomicBool, Ordering};

use gpui::{App, Window};
use oneterm_core::elevation::Elevation;

/// What this process's token says about it.
///
/// The **only** source of the elevation marker and of every mode gate
/// (`DEC-0019` M5): a window elevated by any route — the "+" menu, a right-click
/// "Run as administrator", a policy auto-elevation — says so, and no window can
/// be made to claim an elevation it does not have.
///
/// A failed query is [`Elevation::Unknown`], not "not elevated": the
/// restrictions then apply (fail closed — a process that *is* elevated and fails
/// the query would otherwise run SSH, SFTP, the updater and `terminal.json`'s
/// program under an administrator token, unmarked) while the marker still
/// refuses to claim an elevation the token did not confirm (`DEC-0019` rule 2).
#[cfg(not(windows))]
pub(crate) const fn process_elevation() -> Elevation {
    Elevation::NotElevated
}

#[cfg(windows)]
pub(crate) fn process_elevation() -> Elevation {
    use windows_sys::Win32::Foundation::{CloseHandle, FALSE, HANDLE};
    use windows_sys::Win32::Security::{
        GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    // SAFETY: `OpenProcessToken` on the current process with `TOKEN_QUERY` is
    // always granted; the handle it writes is closed on every path below, and
    // `GetTokenInformation` is given a correctly sized `TOKEN_ELEVATION`.
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == FALSE {
            // Theoretical: querying one's own token cannot be denied.
            // Not logged here: `run()` calls this before `env_logger` exists.
            return Elevation::Unknown("OpenProcessToken(TOKEN_QUERY) failed");
        }
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut returned = 0u32;
        let queried = GetTokenInformation(
            token,
            TokenElevation,
            (&mut elevation as *mut TOKEN_ELEVATION).cast(),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        );
        CloseHandle(token);
        if queried == FALSE {
            return Elevation::Unknown("GetTokenInformation(TokenElevation) failed");
        }
        if elevation.TokenIsElevated != 0 {
            Elevation::Elevated
        } else {
            Elevation::NotElevated
        }
    }
}

/// Show `text` and wait, before the gpui application exists.
///
/// A release build is linked with `windows_subsystem = "windows"`
/// (`src/bin/oneterm.rs`), so it has no console and `eprintln!` would go
/// nowhere; the command line is parsed before any OneTerm window or theme
/// exists, so a native message box is the only surface there is.
pub(crate) fn fatal_message(text: &str) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            MB_ICONERROR, MB_OK, MB_SETFOREGROUND, MessageBoxW,
        };

        let caption = wide("OneTerm");
        let body = wide(text);
        // SAFETY: both strings are NUL-terminated and outlive the call, and a
        // null owner window is documented as allowed.
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                body.as_ptr(),
                caption.as_ptr(),
                MB_OK | MB_ICONERROR | MB_SETFOREGROUND,
            );
        }
    }
    eprintln!("{text}");
}

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt as _;
    std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Give up the console Windows allocated for this process, if it is ours alone.
///
/// A **release** build is a GUI-subsystem binary
/// (`windows_subsystem = "windows"`, `src/bin/oneterm.rs`) and has no console at
/// all, so `GetConsoleWindow()` is null and this does nothing. A **debug** or
/// `fast-dev` build is console-subsystem on purpose — it is how a developer
/// reads the log — and Windows gives such a process a console when it has none
/// to inherit. A process started through `runas` never inherits one, because the
/// launcher's console belongs to a different integrity level. So an elevated
/// OneTerm opened with a bare console window beside it, every time, in exactly
/// the build the owner runs (`IN-0043`, `US-0130` third rework).
///
/// Hiding it from the launching side does not work: `nShow` becomes the new
/// process's default show command and gpui's window inherits it, so
/// `SEE_MASK`/`SW_HIDE` hides the app window too (probed, `US-0130`).
///
/// **Only when the console is this process's alone.** `GetConsoleProcessList`
/// returning 1 means Windows allocated it for us — the `runas` case, and the
/// window nobody asked for. More than one means we inherited it from whoever
/// started us, most likely an administrator prompt running
/// `oneterm.exe --elevated-shell cmd` by hand: that window is not ours to close,
/// no unexpected window appeared, and the log belongs to the person reading it.
/// `FreeConsole()` unconditionally would throw away output somebody started the
/// process to see.
///
/// Called from `run()` **before logging is initialised**, so nothing is written
/// to a console that is about to go away. The elevated instance's `stderr` then
/// goes nowhere and is deliberately **not** redirected to a file: a log under the
/// configuration directory is a write, and M4's promise is that an elevated
/// window leaves nothing behind. Its diagnostics are the crash store, which M7
/// already keeps in `crashes/elevated/`.
///
/// The decision is [`console_action`] and is compiled and tested on every OS;
/// only the two Win32 queries and `FreeConsole` itself are `cfg(windows)`.
pub(crate) fn release_own_console() {
    let (has_console, owners) = console_owners();
    if console_action(has_console, owners) == ConsoleAction::Keep {
        return;
    }
    free_console();
}

/// Whether this process is attached to a console, and how many processes are
/// attached to it. Off Windows there is no such thing: a Unix process has a
/// controlling terminal it never allocated and must never disown.
#[cfg(not(windows))]
fn console_owners() -> (bool, u32) {
    (false, 0)
}

#[cfg(windows)]
fn console_owners() -> (bool, u32) {
    use windows_sys::Win32::System::Console::{GetConsoleProcessList, GetConsoleWindow};

    // SAFETY: both queries are parameterless or take a buffer we own, and
    // neither keeps a pointer past the call.
    unsafe {
        if GetConsoleWindow().is_null() {
            (false, 0)
        } else {
            let mut list = [0u32; 2];
            (
                true,
                GetConsoleProcessList(list.as_mut_ptr(), list.len() as u32),
            )
        }
    }
}

/// Unreachable off Windows: [`console_owners`] never reports one there.
#[cfg(not(windows))]
fn free_console() {}

#[cfg(windows)]
fn free_console() {
    // SAFETY: parameterless, and nothing has been written to this console yet —
    // `run()` calls this before the logger is initialised.
    if unsafe { windows_sys::Win32::System::Console::FreeConsole() }
        == windows_sys::Win32::Foundation::FALSE
    {
        // Nothing is broken by failing: the window stays and the log still
        // works. Worth a line, and this runs before the logger exists.
        eprintln!("OneTerm: could not release the console window");
    }
}

/// What to do about the console this process is attached to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConsoleAction {
    /// Leave it: there is none, or it is not ours alone.
    Keep,
    /// Ours alone, allocated for us. Disown it and the window goes with it.
    Free,
}

/// The decision, split out from the Win32 queries so it is testable on every
/// platform and so the "not ours to close" case is asserted rather than trusted.
///
/// `owners` is `GetConsoleProcessList`'s count, which is `0` when the call
/// fails — treated as "leave it alone", because a console we cannot count is
/// not one we should be closing.
fn console_action(has_console: bool, owners: u32) -> ConsoleAction {
    if has_console && owners == 1 {
        ConsoleAction::Free
    } else {
        ConsoleAction::Keep
    }
}

/// Whether a `runas` request is outstanding.
///
/// The launch became asynchronous when it moved off the gpui thread, and with
/// it went the accidental debounce a modal call gave for free: five clicks on
/// `Run as administrator › PowerShell` used to be impossible, and became five
/// threads, five consent prompts and five administrator windows (`IN-0043`
/// NEW-8). Not a privilege problem — each window still costs its own consent —
/// but not what anybody meant by clicking twice.
static LAUNCH_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// Whether this click should start a launch, given one already outstanding.
///
/// Pure, so the rule is tested without a consent prompt: the whole point is the
/// case that needs two clicks and a UAC dialog to reach by hand.
fn should_start_launch(in_flight: bool) -> bool {
    !in_flight
}

/// Remember the thread that owns the gpui `App`.
///
/// Called once from `run()`, before the application starts. It exists for one
/// debug assertion — see [`ElevationRequest::execute`] — which is the cheapest
/// way to keep the fix below from being undone by a refactor that "simplifies"
/// the launch back onto the calling thread.
pub(crate) fn remember_ui_thread() {
    let _ = UI_THREAD.set(std::thread::current().id());
}

static UI_THREAD: std::sync::OnceLock<std::thread::ThreadId> = std::sync::OnceLock::new();

/// `ERROR_CANCELLED` — the user declined the consent prompt.
///
/// Spelled out rather than imported so the outcome mapping below is one
/// function on every platform; `error_cancelled_matches_windows` checks it
/// against `windows_sys`' own constant on Windows.
const ERROR_CANCELLED_CODE: u32 = 1223;

/// What the `runas` launch did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LaunchOutcome {
    Started,
    /// The consent prompt was declined (`ERROR_CANCELLED`). A silent no-op.
    Declined,
    /// Anything else, with the reason already phrased for a human.
    Failed(String),
}

/// The `ShellExecuteExW` result as an outcome. Pure, so it is testable without
/// launching anything.
fn outcome_of(started: bool, last_error: u32) -> LaunchOutcome {
    if started {
        return LaunchOutcome::Started;
    }
    if last_error == ERROR_CANCELLED_CODE {
        return LaunchOutcome::Declined;
    }
    LaunchOutcome::Failed(format!("Windows error {last_error}"))
}

/// What the user is told. `None` means "say nothing".
///
/// A declined prompt is silent on purpose: the user said no, and telling them
/// so is noise (`DEC-0019`). Pure, and paired with [`outcome_of`] so the two
/// halves of "what happened" and "what is said about it" are both testable.
fn notification_for(outcome: &LaunchOutcome) -> Option<String> {
    match outcome {
        LaunchOutcome::Started | LaunchOutcome::Declined => None,
        LaunchOutcome::Failed(reason) => {
            Some(format!("Could not start an administrator window: {reason}"))
        }
    }
}

/// Everything the `runas` call needs, owned, so the thread that makes it needs
/// nothing from gpui and nothing from the caller's stack.
///
/// Built on the UI thread — `current_exe`, `current_dir` and a `format!` block
/// on nothing — and executed on a thread of its own.
pub(crate) struct ElevationRequest {
    /// `std::env::current_exe()`. Never a name, never `PATH`, never `argv[0]`:
    /// resolving the executable by name is how the wrong binary gets elevated.
    executable: std::path::PathBuf,
    /// `--elevated-shell <token>`, built from a closed three-variant enum. No
    /// user-supplied text can reach it, so Windows command-line quoting is not a
    /// hazard on this path and there is no quoting code here to get wrong.
    parameters: String,
    /// Load-bearing, and `runas` does not inherit the caller's: a debug build
    /// resolves `config_dir()` relative to the working directory, so an unset
    /// `lpDirectory` would silently put the elevated instance on a different
    /// configuration root.
    directory: std::path::PathBuf,
}

impl ElevationRequest {
    pub(crate) fn build(shell: oneterm_core::elevation::ElevatedShell) -> Result<Self, String> {
        let executable = std::env::current_exe()
            .map_err(|error| format!("OneTerm's own path is unknown: {error}"))?;
        let directory = std::env::current_dir()
            .ok()
            .or_else(|| executable.parent().map(std::path::Path::to_path_buf))
            .ok_or_else(|| "no working directory to start from".to_string())?;
        Ok(Self {
            executable,
            parameters: format!(
                "{} {}",
                oneterm_core::elevation::ELEVATED_SHELL_FLAG,
                shell.token()
            ),
            directory,
        })
    }

    /// Ask the AppInfo service to start an elevated OneTerm, and block until it
    /// answers.
    ///
    /// **This must never run on the thread that owns the gpui `App`.**
    /// `ShellExecuteExW` is modal while the consent request is outstanding and
    /// pumps the calling thread's message queue to stay responsive. Called from
    /// a gpui callback, that re-enters OneTerm's window procedure while the
    /// `App` `RefCell` is still mutably borrowed by the callback, which logs
    /// `RefCell already borrowed` for every message dispatched and then aborts
    /// the process the moment a queued async task reaches
    /// `AsyncApp::update_entity` (`gpui-pre/src/app/async_context.rs:65`). That
    /// is not a theoretical hazard: it is what the owner hit on the first click
    /// of a `Run as administrator ›` row (`IN-0043`, `US-0130` rework).
    ///
    /// So: a thread of its own, its own COM apartment, and not one gpui type in
    /// scope.
    ///
    /// The platform split is [`request_elevation`] and nothing else: the fields
    /// are read here, and what a launch result *means* is [`outcome_of`], on
    /// every OS.
    pub(crate) fn execute(self) -> LaunchOutcome {
        debug_assert_ne!(
            Some(&std::thread::current().id()),
            UI_THREAD.get(),
            "ShellExecuteExW pumps messages; running it on the gpui thread re-enters \
             the window proc while the App RefCell is borrowed (IN-0043)"
        );

        let (started, last_error) =
            request_elevation(&self.executable, &self.parameters, &self.directory);
        outcome_of(started, last_error)
    }
}

/// Ask the AppInfo service to start `executable` elevated, and block until it
/// answers: `(started, GetLastError())`, which [`outcome_of`] turns into a
/// [`LaunchOutcome`].
///
/// Off Windows this is unreachable — the `Run as administrator` submenu is
/// `cfg(windows)` (`crates/terminal-view/src/panel/terminal_panel.rs`) — and the
/// seam is here, at the FFI call itself, rather than one level up so that the
/// request, the outcome mapping and the notification are one code path compiled
/// and tested on all three CI runners.
#[cfg(not(windows))]
fn request_elevation(
    _executable: &std::path::Path,
    _parameters: &str,
    _directory: &std::path::Path,
) -> (bool, u32) {
    // `ERROR_NOT_SUPPORTED`: there is no `runas` verb to invoke.
    (false, 50)
}

#[cfg(windows)]
fn request_elevation(
    executable: &std::path::Path,
    parameters: &str,
    directory: &std::path::Path,
) -> (bool, u32) {
    use windows_sys::Win32::System::Com::{
        COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize,
    };
    use windows_sys::Win32::UI::Shell::{
        SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let verb = wide("runas");
    let file = wide(&executable.to_string_lossy());
    let parameters = wide(parameters);
    let working_directory = wide(&directory.to_string_lossy());

    // SAFETY: COM is initialized for this thread and uninitialized before it
    // ends; the struct is zeroed and its `cbSize` set as documented; every
    // pointer is a NUL-terminated wide string that outlives the call.
    let (started, last_error) = unsafe {
        // `ShellExecuteEx` delegates to COM shell extensions, so the thread
        // must have an apartment. Ours, not gpui's: nothing else runs here.
        let com = CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);

        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
        // `SEE_MASK_NOASYNC`: this thread has no message loop and exits as
        // soon as the call returns, so the operation must complete before it
        // does. `SEE_MASK_FLAG_NO_UI` suppresses the shell's own error
        // dialog — not the consent prompt, which the AppInfo service owns —
        // so every failure is reported once, by OneTerm, in OneTerm's style.
        // `SEE_MASK_NOCLOSEPROCESS` is deliberately not set: nothing here
        // consumes `hProcess`, and a handle never closed is a leak.
        info.fMask = SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = parameters.as_ptr();
        info.lpDirectory = working_directory.as_ptr();
        info.nShow = SW_SHOWNORMAL;
        let started = ShellExecuteExW(&mut info);
        let last_error = windows_sys::Win32::Foundation::GetLastError();

        if com >= 0 {
            CoUninitialize();
        }
        (started, last_error)
    };

    (started != windows_sys::Win32::Foundation::FALSE, last_error)
}

/// Launch a **new** elevated OneTerm on `kind` (`DEC-0019` rule 1).
///
/// The only channel from this medium-integrity process into the high-integrity
/// one: the verb, this executable, and one token from a closed three-value enum.
/// No resolved path, no configuration, no user text ever crosses — resolving the
/// program here and passing it as an argument would make the unelevated process
/// the thing that names what the elevated process runs (`DEC-0019` rule 4).
///
/// **Returns immediately.** The `ShellExecuteExW` call happens on a thread of
/// its own and the outcome comes back through a channel, because the call is
/// modal and pumps messages while the consent prompt is up; making it from this
/// gpui callback re-enters the window procedure with the `App` `RefCell` still
/// borrowed and takes the process down (see [`ElevationRequest::execute`]).
pub(crate) fn launch_elevated_shell(
    kind: oneterm_core::ShellKind,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(shell) = oneterm_core::elevation::ElevatedShell::from_shell_kind(kind) else {
        // Unreachable from the menu, which only offers the three Windows kinds.
        log::warn!("{} cannot be run as administrator", kind.display_name());
        return;
    };
    // Cheap and non-blocking, so it stays on this thread and a failure is
    // reported without a round trip.
    let request = match ElevationRequest::build(shell) {
        Ok(request) => request,
        Err(reason) => {
            report(LaunchOutcome::Failed(reason), window, cx);
            return;
        }
    };

    // One request at a time. `swap` rather than load-then-store: two clicks
    // dispatched in the same frame must not both see `false`.
    if !should_start_launch(LAUNCH_IN_FLIGHT.swap(true, Ordering::SeqCst)) {
        log::info!("an elevation request is already awaiting consent; ignoring this one");
        return;
    }

    let (sender, receiver) = async_channel::bounded(1);
    let token = shell.token();
    if let Err(error) = std::thread::Builder::new()
        .name("oneterm-elevate".into())
        .spawn(move || {
            let outcome = request.execute();
            // The receiver is dropped only when the window is gone, in which
            // case there is nobody left to tell.
            oneterm_core::report_best_effort(
                "report the elevation outcome",
                sender.send_blocking(outcome),
            );
        })
    {
        LAUNCH_IN_FLIGHT.store(false, Ordering::SeqCst);
        report(
            LaunchOutcome::Failed(format!("could not start the elevation helper: {error}")),
            window,
            cx,
        );
        return;
    }

    window
        .spawn(cx, async move |cx| {
            let outcome = receiver.recv().await;
            // Released here and on every other exit path, so a failed launch
            // does not wedge the menu row for the rest of the session.
            LAUNCH_IN_FLIGHT.store(false, Ordering::SeqCst);
            let Ok(outcome) = outcome else {
                // The helper thread died without answering; it has already
                // logged whatever it knew.
                return;
            };
            match &outcome {
                LaunchOutcome::Started => {
                    log::info!("started an elevated OneTerm on {token}");
                }
                LaunchOutcome::Declined => {
                    log::info!("the elevation prompt was declined; nothing was started");
                }
                LaunchOutcome::Failed(reason) => {
                    log::warn!("failed to start an elevated OneTerm: {reason}");
                }
            }
            oneterm_core::report_best_effort(
                "notify the elevation outcome",
                cx.update(|window, cx| notify_outcome(&outcome, window, cx)),
            );
        })
        .detach();
}

/// Say the outcome now, on this thread, for the failures that happen before the
/// helper thread is even started.
fn report(outcome: LaunchOutcome, window: &mut Window, cx: &mut App) {
    if let LaunchOutcome::Failed(reason) = &outcome {
        log::warn!("failed to start an elevated OneTerm: {reason}");
    }
    notify_outcome(&outcome, window, cx);
}

/// One notification, never a dialog (`docs/agents/error-policy.md`: dialogs are
/// for decisions), and nothing at all for a declined prompt.
fn notify_outcome(outcome: &LaunchOutcome, window: &mut Window, cx: &mut App) {
    let Some(message) = notification_for(outcome) else {
        return;
    };
    gpui_component::WindowExt::push_notification(
        window,
        oneterm_theme::notif_ext::notify(
            gpui_component::notification::NotificationType::Error,
            message,
            cx,
        ),
        cx,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `NEW-8`: one consent prompt per click, not one per click **plus** every
    /// click that lands while the first is still up.
    #[test]
    fn a_second_click_is_ignored_while_a_prompt_is_outstanding() {
        assert!(
            should_start_launch(false),
            "the first click must start a launch"
        );
        assert!(
            !should_start_launch(true),
            "a click while a consent prompt is outstanding must be ignored"
        );
        // ...and the flag is not sticky: the next click after the outcome
        // arrives starts a launch again.
        assert!(should_start_launch(false));
    }

    #[test]
    fn a_declined_prompt_says_nothing_and_a_failure_says_why() {
        assert_eq!(outcome_of(true, 0), LaunchOutcome::Started);
        assert_eq!(notification_for(&LaunchOutcome::Started), None);

        assert_eq!(
            outcome_of(false, ERROR_CANCELLED_CODE),
            LaunchOutcome::Declined,
            "declining the consent prompt is the normal case, not an error"
        );
        assert_eq!(
            notification_for(&LaunchOutcome::Declined),
            None,
            "the user said no; telling them so is noise"
        );

        // Anything else is one notification naming the OS error.
        let denied = outcome_of(false, 1260);
        assert_eq!(denied, LaunchOutcome::Failed("Windows error 1260".into()));
        let message = notification_for(&denied).expect("a failure must be reported once");
        assert!(message.contains("1260"), "{message}");
        assert!(message.contains("administrator window"), "{message}");
    }

    /// The whole of what crosses into the elevated process: this executable and
    /// one token from a closed three-value enum. No user text reaches the
    /// command line, which is why there is no Windows quoting code here to get
    /// wrong — asserted on every OS, because the shape is not Win32.
    #[test]
    fn the_request_carries_the_flag_and_one_token() {
        let request = ElevationRequest::build(oneterm_core::elevation::ElevatedShell::Pwsh)
            .expect("this process has a path and a working directory");
        assert_eq!(request.parameters, "--elevated-shell pwsh");
        assert!(request.executable.is_absolute(), "never a name, never PATH");
        assert!(request.directory.is_absolute());
    }

    /// Only a console this process owns alone is disowned. The `runas` case is
    /// exactly `owners == 1`: Windows allocated it for us and nobody asked for
    /// the window. Anything else is somebody else's console.
    #[test]
    fn only_a_console_of_our_own_is_released() {
        // A release build: GUI subsystem, no console ever existed.
        assert_eq!(console_action(false, 0), ConsoleAction::Keep);
        // A debug build started by `runas`: a console allocated for us alone.
        assert_eq!(console_action(true, 1), ConsoleAction::Free);
        // Started from somebody's administrator prompt: their window, their log.
        assert_eq!(console_action(true, 2), ConsoleAction::Keep);
        assert_eq!(console_action(true, 8), ConsoleAction::Keep);
        // `GetConsoleProcessList` failed. A console we cannot count is not one
        // to close.
        assert_eq!(console_action(true, 0), ConsoleAction::Keep);
    }

    /// The queries behave as the decision above assumes: a process with a
    /// console reports at least one owner. Never calls `FreeConsole` — that
    /// would detach the test harness from the console running it.
    #[cfg(windows)]
    #[test]
    fn the_console_queries_agree_with_each_other() {
        use windows_sys::Win32::System::Console::{GetConsoleProcessList, GetConsoleWindow};

        // SAFETY: parameterless / a buffer we own; neither mutates process state.
        let (has_console, owners) = unsafe {
            let mut list = [0u32; 8];
            (
                !GetConsoleWindow().is_null(),
                GetConsoleProcessList(list.as_mut_ptr(), list.len() as u32),
            )
        };
        if has_console {
            assert!(
                owners >= 1,
                "a process attached to a console must be counted among its owners"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn error_cancelled_matches_windows() {
        assert_eq!(
            ERROR_CANCELLED_CODE,
            windows_sys::Win32::Foundation::ERROR_CANCELLED
        );
    }
}
