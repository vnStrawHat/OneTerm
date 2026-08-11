//! Capture, store, and recover diagnostics for unrecoverable crashes.

use std::{
    backtrace::Backtrace,
    fs,
    io::{self, Write as _},
    panic::{self, PanicHookInfo},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use sysinfo::{Pid, System};

const CRASHES_DIR: &str = "crashes";
const COMPLETED_SUFFIX: &str = ".crash.txt";
const NATIVE_SUFFIX: &str = ".native.tmp";
const LEGACY_CRASH_REPORT_FILE: &str = "pending-crash-report.txt";
const LEGACY_NATIVE_REPORT_FILE: &str = "native-crash-report.txt";
const MAX_REPORTS: usize = 20;
const REDACTED_USER_HOME: &str = "<USER_HOME>";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PendingCrashReport {
    pub(crate) path: PathBuf,
    pub(crate) contents: String,
}

pub(crate) struct CrashCapturePaths {
    pub(crate) native: PathBuf,
    panic: PathBuf,
}

pub(crate) fn prepare_capture_paths() -> io::Result<CrashCapturePaths> {
    let directory = crashes_dir();
    fs::create_dir_all(&directory)?;
    let identity = unique_identity(&directory)?;
    Ok(CrashCapturePaths {
        panic: directory.join(format!("{identity}{COMPLETED_SUFFIX}")),
        native: directory.join(format!("{identity}{NATIVE_SUFFIX}")),
    })
}

pub(crate) fn install_panic_hook(paths: &CrashCapturePaths) {
    let report_path = paths.panic.clone();
    let previous_hook = panic::take_hook();

    panic::set_hook(Box::new(move |panic_info| {
        let report = format_report(panic_info);
        if let Err(error) = write_new_report(&report_path, report.as_bytes())
            && error.kind() != io::ErrorKind::AlreadyExists
        {
            eprintln!(
                "OneTerm failed to persist crash report at {}: {error}",
                report_path.display()
            );
        }
        previous_hook(panic_info);
    }));
}

pub(crate) fn load_pending_reports() -> io::Result<Vec<PendingCrashReport>> {
    let directory = crashes_dir();
    fs::create_dir_all(&directory)?;
    import_legacy_reports(&directory)?;
    promote_inactive_native_reports(&directory)?;
    cleanup_legacy_crash_artifacts(&directory)?;
    load_completed_reports(&directory)
}

pub(crate) fn delete_pending_report(path: PathBuf) -> io::Result<()> {
    if path.parent() != Some(crashes_dir().as_path())
        || !path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(COMPLETED_SUFFIX))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "crash report path is outside the managed crash store",
        ));
    }
    delete_report(&path)
}

fn crashes_dir() -> PathBuf {
    oneterm_core::config_dir().join(CRASHES_DIR)
}

fn unique_identity(directory: &Path) -> io::Result<String> {
    let now = chrono::Utc::now();
    let timestamp = now.format("%Y%m%dT%H%M%S%3fZ");
    let pid = std::process::id();

    for _ in 0..32 {
        let random = rand::random::<u32>();
        let identity = format!("{timestamp}-p{pid}-{random:08x}");
        let completed = directory.join(format!("{identity}{COMPLETED_SUFFIX}"));
        let native = directory.join(format!("{identity}{NATIVE_SUFFIX}"));
        if !completed.exists() && !native.exists() {
            return Ok(identity);
        }
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "failed to allocate a unique crash report identity",
    ))
}

fn import_legacy_reports(directory: &Path) -> io::Result<()> {
    let config_dir = oneterm_core::config_dir();
    import_legacy_report(&config_dir.join(LEGACY_CRASH_REPORT_FILE), directory)?;
    import_legacy_report(&config_dir.join(LEGACY_NATIVE_REPORT_FILE), directory)
}

fn import_legacy_report(legacy_path: &Path, directory: &Path) -> io::Result<()> {
    let bytes = match fs::read(legacy_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };

    if !bytes.is_empty() {
        let identity = unique_identity(directory)?;
        let destination = directory.join(format!("{identity}{COMPLETED_SUFFIX}"));
        let text = String::from_utf8_lossy(&bytes);
        let sanitized = redact_user_home(&text, oneterm_core::home_dir().as_deref());
        write_new_report(&destination, sanitized.as_bytes())?;
    }

    remove_if_present(legacy_path)?;
    remove_if_present(&legacy_path.with_extension("bak"))?;
    remove_if_present(&report_lock_path(legacy_path))
}

fn promote_inactive_native_reports(directory: &Path) -> io::Result<()> {
    let system = System::new_all();
    let current_pid = std::process::id();
    let native_paths = report_paths_with_suffix(directory, NATIVE_SUFFIX)?;

    for native_path in native_paths {
        let Some(identity) = identity_from_path(&native_path, NATIVE_SUFFIX) else {
            continue;
        };
        let Some(owner_pid) = pid_from_identity(identity) else {
            continue;
        };
        if owner_pid != current_pid && system.process(Pid::from_u32(owner_pid)).is_some() {
            continue;
        }

        let claimed = directory.join(format!(
            "{identity}.native.claimed-p{current_pid}-{:08x}.tmp",
            rand::random::<u32>()
        ));
        match fs::rename(&native_path, &claimed) {
            Ok(()) => promote_claimed_native_report(identity, &claimed, directory)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn promote_claimed_native_report(
    identity: &str,
    claimed_path: &Path,
    directory: &Path,
) -> io::Result<()> {
    let native = fs::read(claimed_path)?;
    if native.is_empty() {
        return remove_if_present(claimed_path);
    }

    let completed_path = directory.join(format!("{identity}{COMPLETED_SUFFIX}"));
    let combined = match fs::read(&completed_path) {
        Ok(existing) if !existing.is_empty() => {
            let mut combined = existing;
            combined.extend_from_slice(b"\n\n--- Native crash context ---\n");
            combined.extend_from_slice(&native);
            combined
        }
        Ok(_) => native,
        Err(error) if error.kind() == io::ErrorKind::NotFound => native,
        Err(error) => return Err(error),
    };
    overwrite_report(&completed_path, &combined)?;
    remove_if_present(claimed_path)
}

fn load_completed_reports(directory: &Path) -> io::Result<Vec<PendingCrashReport>> {
    let mut paths = report_paths_with_suffix(directory, COMPLETED_SUFFIX)?;
    paths.sort_by(|left, right| right.file_name().cmp(&left.file_name()));

    for old_path in paths.iter().skip(MAX_REPORTS) {
        delete_report(old_path)?;
    }
    paths.truncate(MAX_REPORTS);

    let mut reports = Vec::with_capacity(paths.len());
    for path in paths {
        if let Some(contents) = load_and_sanitize_report(&path)? {
            reports.push(PendingCrashReport { path, contents });
        }
    }
    Ok(reports)
}

fn report_paths_with_suffix(directory: &Path, suffix: &str) -> io::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.ends_with(suffix))
        {
            paths.push(entry.path());
        }
    }
    Ok(paths)
}

fn cleanup_legacy_crash_artifacts(directory: &Path) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let should_remove = entry.file_name().to_str().is_some_and(|name| {
            name.ends_with(".crash.bak")
                || (name.starts_with('.') && name.ends_with(".crash.txt.lock"))
        });
        if should_remove {
            remove_if_present(&entry.path())?;
        }
    }
    Ok(())
}

fn identity_from_path<'a>(path: &'a Path, suffix: &str) -> Option<&'a str> {
    path.file_name()?.to_str()?.strip_suffix(suffix)
}

fn pid_from_identity(identity: &str) -> Option<u32> {
    let (_, pid_and_random) = identity.rsplit_once("-p")?;
    let (pid, _) = pid_and_random.split_once('-')?;
    pid.parse().ok()
}

fn load_and_sanitize_report(path: &Path) -> io::Result<Option<String>> {
    let bytes = match fs::read(path) {
        Ok(bytes) if bytes.is_empty() => {
            delete_report(path)?;
            return Ok(None);
        }
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let report = String::from_utf8_lossy(&bytes).into_owned();
    let sanitized = redact_user_home(&report, oneterm_core::home_dir().as_deref());

    if sanitized != report {
        overwrite_report(path, sanitized.as_bytes())?;
    }
    remove_report_artifacts(path)?;

    Ok(Some(sanitized))
}

fn write_new_report(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    write_and_sync(file, bytes)
}

fn overwrite_report(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    write_and_sync(file, bytes)
}

fn write_and_sync(mut file: fs::File, bytes: &[u8]) -> io::Result<()> {
    file.write_all(bytes)?;
    file.sync_all()
}

fn delete_report(path: &Path) -> io::Result<()> {
    remove_if_present(path)?;
    remove_report_artifacts(path)
}

fn remove_report_artifacts(path: &Path) -> io::Result<()> {
    remove_if_present(&path.with_extension("bak"))?;
    remove_if_present(&report_lock_path(path))
}

fn report_lock_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("oneterm-crash-report");
    path.with_file_name(format!(".{name}.lock"))
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn format_report(panic_info: &PanicHookInfo<'_>) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    let thread = std::thread::current();
    let thread_name = thread.name().unwrap_or("unnamed");
    let message = panic_message(panic_info);
    let location = panic_info
        .location()
        .map(|location| {
            format!(
                "{}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            )
        })
        .unwrap_or_else(|| "unknown".to_owned());

    let report = format!(
        "OneTerm Crash Report\n\
         ====================\n\
         Version: {}\n\
         Timestamp (Unix): {timestamp}\n\
         OS: {}\n\
         Architecture: {}\n\
         Thread: {thread_name}\n\
         Panic: {message}\n\
         Location: {location}\n\n\
         Backtrace:\n{}\n",
        env!("ONETERM_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        Backtrace::force_capture(),
    );

    redact_user_home(&report, oneterm_core::home_dir().as_deref())
}

fn redact_user_home(report: &str, home: Option<&Path>) -> String {
    let Some(home) = home else {
        return report.to_owned();
    };
    let home = home.to_string_lossy();
    if home.len() <= 3 || matches!(home.as_ref(), "/" | "\\") {
        return report.to_owned();
    }

    let mut sanitized = report.to_owned();
    let native = home.to_string();
    let forward = native.replace('\\', "/");
    let backward = native.replace('/', "\\");

    for variant in [native, forward, backward] {
        if variant.is_empty() {
            continue;
        }
        #[cfg(windows)]
        {
            sanitized = replace_ascii_case_insensitive(&sanitized, &variant, REDACTED_USER_HOME);
        }
        #[cfg(not(windows))]
        {
            sanitized = sanitized.replace(&variant, REDACTED_USER_HOME);
        }
    }
    sanitized
}

#[cfg(windows)]
fn replace_ascii_case_insensitive(value: &str, pattern: &str, replacement: &str) -> String {
    let lowercase_value = value.to_ascii_lowercase();
    let lowercase_pattern = pattern.to_ascii_lowercase();
    let mut output = String::with_capacity(value.len());
    let mut start = 0;

    while let Some(relative) = lowercase_value[start..].find(&lowercase_pattern) {
        let matched = start + relative;
        output.push_str(&value[start..matched]);
        output.push_str(replacement);
        start = matched + pattern.len();
    }
    output.push_str(&value[start..]);
    output
}

fn panic_message(panic_info: &PanicHookInfo<'_>) -> String {
    if let Some(message) = panic_info.payload().downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = panic_info.payload().downcast_ref::<String>() {
        message.clone()
    } else {
        "Non-string panic payload".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn temporary_directory(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "oneterm-crash-report-test-{}-{name}",
            std::process::id()
        ));
        drop(fs::remove_dir_all(&path));
        fs::create_dir_all(&path).expect("fixture directory should be created");
        path
    }

    fn completed_path(directory: &Path, identity: &str) -> PathBuf {
        directory.join(format!("{identity}{COMPLETED_SUFFIX}"))
    }

    #[test]
    fn generated_identity_contains_time_pid_and_random_suffix() {
        let directory = temporary_directory("identity");
        let identity = unique_identity(&directory).expect("identity should be generated");
        let (_, pid_and_random) = identity.rsplit_once("-p").expect("PID separator");
        let (pid, random) = pid_and_random.split_once('-').expect("random separator");

        assert_eq!(pid, std::process::id().to_string());
        assert_eq!(random.len(), 8);
        assert!(random.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert!(identity.ends_with(random));
        let second = unique_identity(&directory).expect("second identity should be generated");
        assert_ne!(identity, second);
        fs::remove_dir_all(directory).expect("fixture should be deleted");
    }

    #[test]
    fn unique_report_write_preserves_first_crash_without_lock_or_backup() {
        let directory = temporary_directory("direct-write");
        let path = completed_path(&directory, "20260811T023500000Z-p1-a7f3c912");

        write_new_report(&path, b"first crash").expect("first report should be written");
        let error = write_new_report(&path, b"second crash")
            .expect_err("the completed identity must not be replaced");

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(&path).unwrap(), "first crash");
        assert!(!path.with_extension("bak").exists());
        assert!(!report_lock_path(&path).exists());
        fs::remove_dir_all(directory).expect("fixture should be deleted");
    }

    #[test]
    fn legacy_report_is_imported_then_removed() {
        let directory = temporary_directory("legacy-import");
        let legacy = directory.join("legacy-pending.txt");
        fs::write(&legacy, "legacy crash").expect("legacy fixture should be written");

        import_legacy_report(&legacy, &directory).expect("legacy report should import");

        assert!(!legacy.exists());
        let reports = load_completed_reports(&directory).expect("imported report should load");
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].contents, "legacy crash");
        fs::remove_dir_all(directory).expect("fixture should be deleted");
    }

    #[test]
    fn startup_cleanup_removes_orphan_crash_lock_and_backup() {
        let directory = temporary_directory("artifact-cleanup");
        let report = completed_path(&directory, "20260811T023500000Z-p1-a7f3c912");
        let lock = report_lock_path(&report);
        let backup = report.with_extension("bak");
        fs::write(&lock, []).expect("lock fixture should be written");
        fs::write(&backup, "backup").expect("backup fixture should be written");

        cleanup_legacy_crash_artifacts(&directory).expect("artifacts should be cleaned");

        assert!(!lock.exists());
        assert!(!backup.exists());
        fs::remove_dir_all(directory).expect("fixture should be deleted");
    }

    #[test]
    fn reports_load_newest_first_and_prune_after_twenty() {
        let directory = temporary_directory("retention");
        for index in 0..22 {
            let identity = format!("20260811T0235{index:02}000Z-p1-{index:08x}");
            fs::write(
                completed_path(&directory, &identity),
                format!("report {index}"),
            )
            .expect("fixture should be written");
        }

        let reports = load_completed_reports(&directory).expect("reports should load");

        assert_eq!(reports.len(), MAX_REPORTS);
        assert_eq!(reports[0].contents, "report 21");
        assert_eq!(reports[19].contents, "report 2");
        assert_eq!(
            report_paths_with_suffix(&directory, COMPLETED_SUFFIX)
                .unwrap()
                .len(),
            20
        );
        fs::remove_dir_all(directory).expect("fixture should be deleted");
    }

    #[test]
    fn inactive_native_staging_is_promoted_to_a_completed_report() {
        let directory = temporary_directory("inactive-native");
        let identity = format!("20260811T023500000Z-p{}-a7f3c912", std::process::id());
        let native = directory.join(format!("{identity}{NATIVE_SUFFIX}"));
        fs::write(&native, "native report").expect("native fixture should be written");

        promote_inactive_native_reports(&directory).expect("staging should promote");

        assert!(!native.exists());
        assert_eq!(
            fs::read_to_string(completed_path(&directory, &identity))
                .expect("completed report should be readable"),
            "native report"
        );
        fs::remove_dir_all(directory).expect("fixture should be deleted");
    }

    #[test]
    fn native_staging_combines_with_matching_panic_report() {
        let directory = temporary_directory("native-promotion");
        let identity = "20260811T023500000Z-p1-a7f3c912";
        let completed = completed_path(&directory, identity);
        let claimed = directory.join("claimed.tmp");
        fs::write(&completed, "panic report").expect("panic fixture should be written");
        fs::write(&claimed, "native report").expect("native fixture should be written");

        promote_claimed_native_report(identity, &claimed, &directory)
            .expect("promotion should succeed");

        let combined = fs::read_to_string(&completed).expect("combined report should be readable");
        assert!(combined.contains("panic report"));
        assert!(combined.contains("native report"));
        assert!(!claimed.exists());
        assert!(!completed.with_extension("bak").exists());
        assert!(!report_lock_path(&completed).exists());
        fs::remove_dir_all(directory).expect("fixture should be deleted");
    }

    #[test]
    fn empty_completed_report_is_removed() {
        let directory = temporary_directory("empty");
        let path = completed_path(&directory, "20260811T023500000Z-p1-a7f3c912");
        fs::write(&path, []).expect("fixture should be written");

        assert_eq!(
            load_and_sanitize_report(&path).expect("load should succeed"),
            None
        );
        assert!(!path.exists());
        fs::remove_dir_all(directory).expect("fixture should be deleted");
    }

    #[test]
    fn redacts_native_and_alternate_home_separators() {
        let report = "C:\\Users\\alice\\project\\main.rs\nC:/Users/alice/project/main.rs";
        let sanitized = redact_user_home(report, Some(Path::new("C:\\Users\\alice")));

        assert_eq!(
            sanitized,
            "<USER_HOME>\\project\\main.rs\n<USER_HOME>/project/main.rs"
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_home_redaction_is_ascii_case_insensitive() {
        let report = "c:\\users\\ALICE\\project\\main.rs";
        let sanitized = redact_user_home(report, Some(Path::new("C:\\Users\\Alice")));

        assert_eq!(sanitized, "<USER_HOME>\\project\\main.rs");
    }

    #[test]
    fn loading_legacy_report_rewrites_redacted_content_and_removes_backup() {
        let directory = temporary_directory("legacy-redaction");
        let path = completed_path(&directory, "20260811T023500000Z-p1-a7f3c912");
        let backup = path.with_extension("bak");
        let lock = report_lock_path(&path);
        let home = oneterm_core::home_dir().expect("test requires a home directory");
        let report = format!("panic at {}/project/main.rs", home.display());
        fs::write(&path, report).expect("report fixture should be written");
        fs::write(&backup, "legacy backup").expect("backup fixture should be written");
        fs::write(&lock, []).expect("lock fixture should be written");

        let loaded = load_and_sanitize_report(&path)
            .expect("load should succeed")
            .expect("report should exist");

        assert!(loaded.contains(REDACTED_USER_HOME));
        assert!(!loaded.contains(home.to_string_lossy().as_ref()));
        assert_eq!(
            fs::read_to_string(&path).expect("report should be readable"),
            loaded
        );
        assert!(!backup.exists());
        assert!(!lock.exists());
        fs::remove_dir_all(directory).expect("fixture should be deleted");
    }

    #[test]
    fn managed_cleanup_rejects_an_external_path() {
        let external = std::env::temp_dir().join("not-a-managed-crash.crash.txt");
        let error = delete_pending_report(external).expect_err("external path must be rejected");

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
}
