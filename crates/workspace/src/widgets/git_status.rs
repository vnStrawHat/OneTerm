//! Git status (branch, dirty marker, diffstat, ahead/behind) of the active
//! local terminal's cwd, shown in the StatusBar after the breadcrumb.
//!
//! The sampler runs on the UI thread every 500ms and only reads the cwd; the
//! git calls run on the background executor, at most one batch at a time and
//! at most once per [`POLL`] for an unchanged cwd — so a commit, checkout, or
//! edit made in the terminal shows up within about `POLL` + one tick. The
//! result is parked in a shared slot tagged with the cwd it was computed for,
//! so a slow answer for a directory the shell already left is never shown.
//!
//! Hidden (`None`) for SSH sessions, before the first OSC 7, outside a
//! repository, or when `git` is not on `PATH`.
//!
//! Label forms: `main`, `main* (+100 -41)` (work tree or index changed; lines
//! added in green and removed in red, against `HEAD`), `main ↑2 ↓1`
//! (ahead/behind the upstream), `a1b2c3d` (detached HEAD, short oid).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui::{App, Entity, WeakEntity, Window};
use gpui_component::{Icon, dock::DockArea};
use oneterm_theme::AppIcon;

use super::status_text::{Label, Segment, StatusText, Tone};

/// Minimum time between two git runs for the same cwd.
// ponytail: 2s polling; switch to a file-system watcher if it shows up in profiles.
const POLL: Duration = Duration::from_secs(2);

/// Last computed label, tagged with the cwd it belongs to.
type Slot = Arc<Mutex<Option<(PathBuf, Option<Label>)>>>;

/// Indicator showing the git status of the active local terminal's cwd.
pub fn git_status(
    dock_area: WeakEntity<DockArea>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<StatusText> {
    let slot: Slot = Arc::default();
    let in_flight = Arc::new(AtomicBool::new(false));
    let mut last_cwd: Option<PathBuf> = None;
    let mut last_run: Option<Instant> = None;

    StatusText::new_entity(
        "git-status-indicator",
        Duration::from_millis(500),
        false,
        Some(Icon::new(AppIcon::GitBranch)),
        Box::new(move |cx| {
            let dock_area = dock_area.upgrade()?;
            let cwd = oneterm_state::active_terminal::local_cwd(&dock_area, cx)?;
            if last_cwd.as_ref() != Some(&cwd) {
                last_cwd = Some(cwd.clone());
                last_run = None;
            }
            let due = last_run.is_none_or(|t| t.elapsed() >= POLL);
            if due && !in_flight.swap(true, Ordering::AcqRel) {
                last_run = Some(Instant::now());
                let slot = slot.clone();
                let in_flight = in_flight.clone();
                cx.background_executor()
                    .spawn(async move {
                        let label = query(&cwd);
                        *lock(&slot) = Some((cwd, label));
                        in_flight.store(false, Ordering::Release);
                    })
                    .detach();
            }
            match &*lock(&slot) {
                Some((for_cwd, label)) if Some(for_cwd) == last_cwd.as_ref() => label.clone(),
                _ => None,
            }
        }),
        window,
        cx,
    )
}

/// A poisoned slot only means a query thread panicked; the value is still a
/// plain `Option`, so keep reading it.
fn lock(slot: &Slot) -> std::sync::MutexGuard<'_, Option<(PathBuf, Option<Label>)>> {
    slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Run a git subcommand in `cwd`; `None` on spawn failure or non-zero exit.
fn git(cwd: &Path, args: &[&str]) -> Option<String> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(cwd)
        .args(args)
        // Never write the index from the poller — the user's own git commands
        // in the same terminal must not hit a lock we hold.
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command.output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Build the label for `cwd`; `None` outside a repository or when git is
/// unavailable.
fn query(cwd: &Path) -> Option<Label> {
    let status = git(cwd, &["status", "--porcelain=v2", "--branch"])?;
    // Staged + unstaged lines against HEAD; fails (and is skipped) on an unborn branch.
    let diffstat = git(cwd, &["diff", "--numstat", "HEAD"]).map_or((0, 0), |s| parse_numstat(&s));
    build_label(&status, diffstat)
}

/// Sum the added/removed line counts of `git diff --numstat` output (binary
/// entries show `-` and count as zero).
fn parse_numstat(output: &str) -> (u64, u64) {
    output.lines().fold((0, 0), |(added, removed), line| {
        let mut cols = line.split('\t');
        let num = |col: Option<&str>| col.and_then(|c| c.parse::<u64>().ok()).unwrap_or(0);
        let a = num(cols.next());
        let r = num(cols.next());
        (added + a, removed + r)
    })
}

/// Turn `git status --porcelain=v2 --branch` output plus the diffstat into
/// the label segments.
fn build_label(status: &str, (added, removed): (u64, u64)) -> Option<Label> {
    let mut head = None;
    let mut oid = None;
    let mut ahead_behind = None;
    let mut dirty = false;
    for line in status.lines() {
        if let Some(header) = line.strip_prefix("# ") {
            if let Some(value) = header.strip_prefix("branch.head ") {
                head = Some(value);
            } else if let Some(value) = header.strip_prefix("branch.oid ") {
                oid = Some(value);
            } else if let Some(value) = header.strip_prefix("branch.ab ") {
                ahead_behind = Some(value);
            }
        } else if !line.is_empty() {
            // `1 ` / `2 ` / `u ` / `? ` entries: changed, renamed, unmerged, untracked.
            dirty = true;
        }
    }
    let head = head?;
    let mut name = if head == "(detached)" {
        oid.filter(|oid| *oid != "(initial)")
            .map(|oid| oid.chars().take(7).collect())
            .unwrap_or_else(|| "HEAD".to_string())
    } else {
        head.to_string()
    };
    if dirty {
        name.push('*');
    }
    let mut segments = vec![Segment::new(name, Tone::Foreground)];
    if added > 0 || removed > 0 {
        segments.push(Segment::new(" (", Tone::Foreground));
        segments.push(Segment::new(format!("+{added}"), Tone::Success));
        segments.push(Segment::new(" ", Tone::Foreground));
        segments.push(Segment::new(format!("-{removed}"), Tone::Danger));
        segments.push(Segment::new(")", Tone::Foreground));
    }
    if let Some(ahead_behind) = ahead_behind {
        // "+<ahead> -<behind>"
        let mut parts = ahead_behind.split(' ');
        let count = |part: Option<&str>| {
            part.and_then(|p| p.get(1..))
                .and_then(|n| n.parse::<u64>().ok())
                .unwrap_or(0)
        };
        let ahead = count(parts.next());
        let behind = count(parts.next());
        let mut tail = String::new();
        if ahead > 0 {
            tail.push_str(&format!(" ↑{ahead}"));
        }
        if behind > 0 {
            tail.push_str(&format!(" ↓{behind}"));
        }
        if !tail.is_empty() {
            segments.push(Segment::new(tail, Tone::Foreground));
        }
    }
    Some(Label(segments))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(label: &Option<Label>) -> Option<String> {
        label
            .as_ref()
            .map(|l| l.0.iter().map(|s| s.text.as_str()).collect())
    }

    #[test]
    fn build_label_formats_branch_dirty_diffstat_and_ahead_behind() {
        let clean = "# branch.oid 0123456789abcdef\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +0 -0\n";
        assert_eq!(text(&build_label(clean, (0, 0))).as_deref(), Some("main"));

        let dirty = "# branch.oid 0123456789abcdef\n# branch.head main\n1 .M N... 100644 100644 100644 abc def src/lib.rs\n? new.txt\n";
        let label = build_label(dirty, (100, 41)).unwrap();
        assert_eq!(
            text(&Some(label.clone())).as_deref(),
            Some("main* (+100 -41)")
        );
        assert_eq!(label.0[0].tone, Tone::Foreground);
        assert_eq!(label.0[2], Segment::new("+100", Tone::Success));
        assert_eq!(label.0[4], Segment::new("-41", Tone::Danger));

        // Untracked only: dirty marker without a diffstat.
        assert_eq!(text(&build_label(dirty, (0, 0))).as_deref(), Some("main*"));

        let diverged = "# branch.oid 0123456789abcdef\n# branch.head feat/x\n# branch.upstream origin/feat/x\n# branch.ab +2 -1\n";
        assert_eq!(
            text(&build_label(diverged, (0, 0))).as_deref(),
            Some("feat/x ↑2 ↓1")
        );

        let detached = "# branch.oid 0123456789abcdef\n# branch.head (detached)\n";
        assert_eq!(
            text(&build_label(detached, (0, 0))).as_deref(),
            Some("0123456")
        );

        let unborn = "# branch.oid (initial)\n# branch.head main\n";
        assert_eq!(text(&build_label(unborn, (0, 0))).as_deref(), Some("main"));

        // No branch header (not a repository / garbage) hides the indicator.
        assert_eq!(build_label("", (0, 0)), None);
        assert_eq!(build_label("fatal: not a git repository\n", (0, 0)), None);
    }

    #[test]
    fn parse_numstat_sums_lines_and_skips_binary() {
        let out = "10\t2\tsrc/a.rs\n90\t39\tsrc/b.rs\n-\t-\timg.png\n";
        assert_eq!(parse_numstat(out), (100, 41));
        assert_eq!(parse_numstat(""), (0, 0));
    }
}
