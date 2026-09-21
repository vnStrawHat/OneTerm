//! OneTerm application core.
//!
//! Initializes the application, registers the UI, and opens the main window.
//! Shared logic behind the `oneterm` binary.
//!
//! The two binaries are thin shims that call [`run`]; this gives each binary its own
//! source file (avoiding the "file present in multiple build targets" warning).

use std::borrow::Cow;

use gpui::TaskExt as _;
use oneterm_workspace::OneTermWorkspace;

mod assets;
mod crash_report;
mod crash_report_dialog;
mod elevation;
mod init;
mod native_crash;
mod oom;
mod session_factory;
mod ssh_client_panel;
mod window;

use assets::CustomAssets;

/// Survive system-OOM spikes (see `oom.rs`): retry failed allocations after
/// releasing a ballast instead of aborting on the first NULL.
#[global_allocator]
static GLOBAL_ALLOC: oom::OomResilientAlloc = oom::OomResilientAlloc;

/// Exit code for a command line OneTerm did not understand.
const EXIT_UNRECOGNIZED_COMMAND_LINE: i32 = 2;
/// Exit code for a requested shell with no trusted path on this machine.
const EXIT_SHELL_NOT_INSTALLED: i32 = 3;

/// Record what this process is (the token) and what it was asked to open (the
/// command line), before any window, logger or allocator ballast exists.
///
/// **The argument selects the shell; the token decides everything else**
/// (`DEC-0019` M5). A process that is not elevated opens the requested shell as
/// an ordinary window with no marker and no restrictions — it cannot claim an
/// elevation it does not have — and says so once in the log.
///
/// Exits the process on a command line that is not understood (code 2) or names
/// a shell this machine has no trusted path for (code 3, elevated only): an
/// elevated window with no shell in it is worse than none.
fn read_process_identity() {
    use oneterm_core::elevation;

    let elevated = crate::elevation::process_is_elevated();
    elevation::set_elevated(elevated);

    let requested = match elevation::parse(std::env::args_os()) {
        Ok(requested) => requested,
        Err(error) => {
            crate::elevation::fatal_message(&error.to_string());
            std::process::exit(EXIT_UNRECOGNIZED_COMMAND_LINE);
        }
    };
    let Some(shell) = requested else {
        return;
    };
    if elevated {
        // Resolved here, in the elevated process, and never by the launching
        // one: a pre-flight check the elevated side then trusts is the hole with
        // extra steps (`DEC-0019` rule 4).
        if let Err(looked_for) = elevation::trusted_program_for(shell) {
            crate::elevation::fatal_message(&format!(
                "OneTerm cannot run {} as administrator: {} is not installed.",
                shell.shell_kind().display_name(),
                looked_for.display()
            ));
            std::process::exit(EXIT_SHELL_NOT_INSTALLED);
        }
    }
    elevation::set_initial_shell(shell);
}

/// Launch OneTerm: initialize logging, the app, the UI, then open the main window.
pub fn run() {
    // What this process is, and what it was asked to open, before anything else
    // (`IN-0043`). The token comes first because `crashes_dir()` needs it; the
    // parse comes before the ballast because a malformed command line must
    // produce a message and an exit, not a 64 MiB allocation first.
    read_process_identity();
    oom::init_ballast();
    let crash_capture_paths = match crash_report::prepare_capture_paths() {
        Ok(paths) => {
            crash_report::install_panic_hook(&paths);
            Some(paths)
        }
        Err(error) => {
            eprintln!("OneTerm failed to prepare crash capture storage: {error}");
            None
        }
    };

    // Initialize logging — reads the RUST_LOG env var, default: info for the app, warn for deps.
    // E.g. RUST_LOG=debug → show debug logs; RUST_LOG=ssh=trace → trace the SSH crate.
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,oneterm=debug"),
    )
    // Suppress framework noise: GPUI's a11y subsystem sends a debug log every frame
    // when accessibility is active — keep it at info even if RUST_LOG=debug.
    .filter_module("gpui", log::LevelFilter::Info)
    .format_timestamp_secs()
    .init();
    log::info!("OneTerm starting up");
    // The command line was read before the logger existed; report it now.
    if let Some(kind) = oneterm_core::elevation::initial_shell() {
        if oneterm_core::elevation::is_elevated() {
            log::info!(
                "Elevated window: opening {} as its one shell",
                kind.display_name()
            );
        } else {
            log::warn!(
                "{} was given to a process that is not elevated: opening {} with no elevation and no restrictions",
                oneterm_core::elevation::ELEVATED_SHELL_FLAG,
                kind.display_name()
            );
        }
    }
    let pending_crash_reports = match crash_report::load_pending_reports() {
        Ok(reports) => reports,
        Err(error) => {
            log::error!("Failed to load pending crash reports: {error}");
            Vec::new()
        }
    };
    let _native_crash_handler =
        crash_capture_paths
            .as_ref()
            .and_then(|paths| match native_crash::install(&paths.native) {
                Ok(handler) => Some(handler),
                Err(error) => {
                    log::error!("Failed to install native crash capture: {error}");
                    None
                }
            });

    // Windows: SetConsoleCtrlHandler safety net — ignore CTRL_C_EVENT.
    // With the bundled OpenConsole.exe, \x03 over the PTY is handled
    // correctly, so OneTerm never receives the signal. This handler is a backup
    // for the case where OpenConsole.exe is missing → fallback to system ConPTY.
    #[cfg(windows)]
    unsafe {
        extern "system" fn ignore_handler(ctrl_type: u32) -> windows_sys::Win32::Foundation::BOOL {
            match ctrl_type {
                windows_sys::Win32::System::Console::CTRL_C_EVENT
                | windows_sys::Win32::System::Console::CTRL_BREAK_EVENT => {
                    windows_sys::Win32::Foundation::TRUE
                }
                _ => windows_sys::Win32::Foundation::FALSE,
            }
        }
        windows_sys::Win32::System::Console::SetConsoleCtrlHandler(
            Some(ignore_handler),
            windows_sys::Win32::Foundation::TRUE,
        );
    }

    let app = gpui_platform::application().with_assets(CustomAssets);

    app.run(move |cx| {
        // Embed Lilex font (terminal default).
        cx.text_system()
            .add_fonts(vec![
                Cow::Borrowed(include_bytes!("../fonts/Lilex-Regular.ttf").as_slice()),
                Cow::Borrowed(include_bytes!("../fonts/Lilex-Bold.ttf").as_slice()),
                Cow::Borrowed(include_bytes!("../fonts/Lilex-Italic.ttf").as_slice()),
                Cow::Borrowed(include_bytes!("../fonts/Lilex-BoldItalic.ttf").as_slice()),
            ])
            .expect("Failed to load Lilex fonts");

        // Initialize gpui-component (theme, dock, root, ...).
        gpui_component::init(cx);

        // Set Lilex as the theme's default monospace font (after init registers Theme).
        cx.global_mut::<gpui_component::Theme>().mono_font_family = "Lilex".into();

        // Initialize the OneTerm UI (globals + feature panel registration + commands).
        crate::init::init(cx);
        // Bind key bindings for the workspace.
        OneTermWorkspace::bind_keys(cx);

        cx.activate(true);

        // Open the main window. A failure here leaves the app without a
        // window, so it must at least reach the log (CORR-63).
        crate::window::open_window(pending_crash_reports, cx).detach_and_log_err(cx);
    });
}
