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
//! behaviour, and the launch logs and returns.

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
            log::error!(
                "failed to open the process token; this window is restricted and claims no elevation"
            );
            return Elevation::Unknown;
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
            log::error!(
                "failed to read TokenElevation; this window is restricted and claims no elevation"
            );
            return Elevation::Unknown;
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

/// Launch a **new** elevated OneTerm on `kind` (`DEC-0019` rule 1).
///
/// The only channel from this medium-integrity process into the high-integrity
/// one: the verb, this executable, and one token from a closed three-value enum.
/// No resolved path, no configuration, no user text ever crosses — resolving the
/// program here and passing it as an argument would make the unelevated process
/// the thing that names what the elevated process runs (`DEC-0019` rule 4).
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
    match launch(shell) {
        Ok(LaunchOutcome::Started) => {
            log::info!("started an elevated OneTerm on {}", shell.token());
        }
        Ok(LaunchOutcome::Declined) => {
            // The user said no. Telling them so is noise.
            log::info!("the elevation prompt was declined; nothing was started");
        }
        Err(reason) => {
            log::warn!("failed to start an elevated OneTerm: {reason}");
            gpui_component::WindowExt::push_notification(
                window,
                oneterm_theme::notif_ext::notify(
                    gpui_component::notification::NotificationType::Error,
                    format!("Could not start an administrator window: {reason}"),
                    cx,
                ),
                cx,
            );
        }
    }
}

pub(crate) enum LaunchOutcome {
    Started,
    /// The consent prompt was declined (`ERROR_CANCELLED`). A silent no-op.
    Declined,
}

#[cfg(not(windows))]
fn launch(_shell: oneterm_core::elevation::ElevatedShell) -> Result<LaunchOutcome, String> {
    Err("running as administrator is a Windows feature".to_string())
}

#[cfg(windows)]
fn launch(shell: oneterm_core::elevation::ElevatedShell) -> Result<LaunchOutcome, String> {
    use windows_sys::Win32::Foundation::{ERROR_CANCELLED, FALSE, GetLastError};
    use windows_sys::Win32::UI::Shell::{SEE_MASK_FLAG_NO_UI, SHELLEXECUTEINFOW, ShellExecuteExW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    // Never a name, never `PATH`, never `argv[0]`: resolving the executable by
    // name is how the wrong binary gets elevated.
    let executable = std::env::current_exe()
        .map_err(|error| format!("OneTerm's own path is unknown: {error}"))?;
    // Load-bearing, and `runas` does not inherit the caller's: a debug build
    // resolves `config_dir()` relative to the working directory, so an unset
    // `lpDirectory` would silently put the elevated instance on a different
    // configuration root.
    let directory = std::env::current_dir()
        .ok()
        .or_else(|| executable.parent().map(std::path::Path::to_path_buf))
        .ok_or_else(|| "no working directory to start from".to_string())?;

    let verb = wide("runas");
    let file = wide(&executable.to_string_lossy());
    // No user-supplied text ever reaches this string, so Windows command-line
    // quoting is not a hazard on this path and there is no quoting code here to
    // get wrong.
    let parameters = wide(&format!(
        "{} {}",
        oneterm_core::elevation::ELEVATED_SHELL_FLAG,
        shell.token()
    ));
    let working_directory = wide(&directory.to_string_lossy());

    // SAFETY: the struct is zeroed and its `cbSize` set as documented; every
    // pointer is a NUL-terminated wide string that outlives the call.
    let (started, last_error) = unsafe {
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
        // Suppress the shell's own error dialog — not the consent prompt, which
        // the AppInfo service owns — so every failure is reported once, by
        // OneTerm, in OneTerm's own style. `SEE_MASK_NOCLOSEPROCESS` is
        // deliberately not set: nothing here consumes `hProcess`, and a handle
        // that is never closed is a leak.
        info.fMask = SEE_MASK_FLAG_NO_UI;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = parameters.as_ptr();
        info.lpDirectory = working_directory.as_ptr();
        info.nShow = SW_SHOWNORMAL;
        let started = ShellExecuteExW(&mut info);
        (started, GetLastError())
    };

    if started != FALSE {
        return Ok(LaunchOutcome::Started);
    }
    if last_error == ERROR_CANCELLED {
        return Ok(LaunchOutcome::Declined);
    }
    Err(format!("Windows error {last_error}"))
}
