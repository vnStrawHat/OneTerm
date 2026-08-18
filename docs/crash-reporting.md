# Crash reporting and recovery

OneTerm captures diagnostics for unrecoverable **Rust panics** and supported platform-native fatal crashes, then stores completed reports locally so they can be reviewed after restart.

## Capture boundary

`oneterm-app` prepares one collision-resistant crash identity per process and installs two handlers before normal UI initialization:

- A Rust panic hook records the OneTerm version, Unix timestamp, OS/architecture, thread name, panic payload/location, and a forced Rust backtrace. It then invokes the previously installed panic hook.
- `crash-handler` 0.8.0 catches supported native exceptions/signals. Windows coverage includes structured exceptions and CRT invalid-parameter/purecall failures; Linux/Android coverage includes `SIGABRT`, `SIGBUS`, `SIGFPE`, `SIGILL`, `SIGSEGV`, and `SIGTRAP`; macOS coverage uses Mach exception ports plus `SIGABRT`.

Completed reports live under the platform configuration directory's `crashes/` child. On Unix the directory is created `0700` and every report file `0600`, because panic payloads and backtraces may carry host names, remote paths, or command text beyond the redacted home prefix. Their sortable names have this form:

```text
YYYYMMDDTHHMMSSmmmZ-p<PID>-<8 lowercase hex random>.crash.txt
```

The UTC timestamp makes ordering readable, the process ID separates simultaneous live instances, and the random suffix protects against PID reuse and same-time collisions. A process chooses this identity before either callback runs; native crash callbacks never allocate or generate randomness.

Crash reports are unique append-free text artifacts, not shared JSON documents. Panic capture therefore creates and durably syncs its unique destination directly; native promotion owns an atomically claimed staging file and durably overwrites only the matching identity. The crash store deliberately does not use `oneterm_core::atomic_write`, whose persistent advisory `.lock` and replacement `.bak` artifacts are appropriate for shared documents but unnecessary here. A second panic in the same process does not truncate the first completed report.

Native callbacks run in a compromised context. They write only bounded, stack-formatted metadata and platform context through a direct OS write to a pre-opened sibling `.native.tmp` file. They do not allocate, lock, log, unwind a Rust stack, or run normal persistence code. On restart, OneTerm promotes non-empty staging owned by processes that are no longer live into the matching `.crash.txt`. If the same crash already produced a Rust panic report, both diagnostics are retained in that completed report. Staging owned by another live OneTerm PID is skipped so concurrent instances cannot consume or unlink one another's destination. Returning `Handled(false)` preserves normal platform termination and downstream handler behavior.

Capture remains best effort. It cannot guarantee a report when process state or the stack/file handle is too damaged for the callback, when another handler terminates first, or for forced OS termination and power loss. Native reports contain bounded exception/signal context rather than a symbolized native stack or minidump.

## Path privacy

Before a Rust panic report is persisted, every occurrence of the current user's home-directory prefix is replaced with `<USER_HOME>`. Both native and alternate slash separators are recognized; Windows matching is ASCII case-insensitive. Loading every report applies the same redaction defensively and rewrites the file when it still holds an unredacted path.

This redaction protects the home-directory prefix only. Panic messages and application data may still contain other sensitive host names, remote paths, commands, or user-provided values, so users must review a GitHub draft before submitting it.

## Reconciliation and retention

Empty completed/staging artifacts are discarded at startup.

The process table needed to decide whether a staging file's owner is still alive is enumerated lazily, only when a foreign `.native.tmp` exists, so a normal startup does not pay for it.

After promotion and sanitization, completed reports are ordered newest-first by their sortable names. OneTerm retains the newest 20 completed reports and deletes older ones. A single report that cannot be read or pruned is logged and skipped; it never hides the remaining reports. A report created later by another running instance may temporarily exceed the limit until the next startup reconciliation.

## Hidden verification trigger

Clicking the OneTerm application icon in About ten times triggers an intentional Rust panic. The counter resets after the tenth click. This behavior exists only to verify the panic capture, restart, and recovery workflow; it is intentionally not presented as a normal action. It is always active in debug builds; release builds enable it only when the `ONETERM_ENABLE_CRASH_TRIGGER` environment variable is set, so a production build cannot be crashed by clicking the icon.

Native callback behavior is verified through `crash-handler`'s platform simulation API in focused tests where the target supports it. The About trigger remains a Rust panic and does not intentionally execute invalid native memory access in normal application builds.

## Recovery lifecycle

After the main window opens, OneTerm shows completed reports sequentially, newest first (`crates/app/src/crash_report_dialog.rs`, next to the crash store in the composition root):

- **Dismiss** closes and deletes only the report currently displayed, then opens the next report if one exists. A failed deletion is logged; the report remains on disk for a later launch, but the in-memory queue may continue.
- **Copy** copies only the current report to the system clipboard without deleting or advancing it.
- **Create Issue** opens the new-issue page of the GitHub repository this build was configured for (the updater's `UPDATE_REPOSITORY` constant) with only the issue title prefilled. The crash report is deliberately omitted from the URL to avoid browser/GitHub URL-length failures. The local report is retained; users can use Copy and paste it into the issue manually.

Closing through the dialog close button, Escape, overlay click, application window close, or any non-Dismiss route retains the current and remaining reports and does not advance the queue. They are shown again on a later launch.

## Privacy and support expectations

Crash reports remain local unless the user explicitly copies them. Create Issue only opens a title-prefilled draft and does not place report content in the URL; users can review and paste copied diagnostics before submission.
