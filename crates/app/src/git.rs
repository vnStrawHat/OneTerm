//! The status bar's git (specs/oneterm, "M4 in detail"): the branch of the repository the focused
//! space is in, read from its `.git`, and the commits ahead of its upstream, how many files
//! changed and the lines added and removed, from `git` run on the pool with no window. Read
//! again when the directory changes and every 10 s.

use std::path::{Path, PathBuf};
use std::time::Duration;

use quadra::{Scope, Signal, background, effect, interval, signal, spawn};

/// What the status bar says of a repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Repo {
    pub branch: String,
    /// None when `git` could not say.
    pub changes: Option<Changes>,
}

/// What differs from what is committed and pushed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    /// The commits ahead of the upstream.
    pub ahead: usize,
    /// The files changed, those git does not follow yet among them.
    pub files: usize,
    /// The lines added and removed against the last commit, in the files git follows.
    pub added: usize,
    pub removed: usize,
}

/// Keeps a signal the repository of the directory `cwd` gives.
pub(crate) fn follow(cwd: impl Fn() -> String + 'static) -> Signal<Option<Repo>> {
    let (repo, tick, owner) = (signal(None), signal(0u32), Scope::current());
    let _ = interval(Duration::from_secs(10), move || tick.update(|n| *n += 1));
    effect(move || {
        tick.get();
        let dir = PathBuf::from(cwd());
        // In the scope it was set up in, where the app's tasks run.
        owner.run(|| {
            spawn(async move {
                if let Ok(found) = background(move || read(&dir)).await {
                    repo.set(found);
                }
            })
            .detach()
        });
    });
    repo
}

/// The repository a directory is in, its branch and its changes.
fn read(dir: &Path) -> Option<Repo> {
    if dir.as_os_str().is_empty() {
        return None;
    }
    let git = dir.ancestors().map(|at| at.join(".git")).find(|git| git.exists())?;
    // A worktree's `.git` is a file that says where its folder is.
    let git = match std::fs::read_to_string(&git) {
        Ok(said) => PathBuf::from(said.strip_prefix("gitdir:")?.trim()),
        Err(_) => git,
    };
    let head = std::fs::read_to_string(git.join("HEAD")).ok()?;
    let head = head.trim();
    let branch = match head.strip_prefix("ref: refs/heads/") {
        Some(name) => name.to_string(),
        None => head.chars().take(7).collect(),
    };
    Some(Repo { branch, changes: changes(dir) })
}

/// What `git` prints for `args` in `dir`, run with no window; None when it is not there or
/// refuses.
fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let mut git = std::process::Command::new("git");
    git.args(args).current_dir(dir);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        git.creation_flags(CREATE_NO_WINDOW);
    }
    let out = git.output().ok().filter(|out| out.status.success())?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// What `git status --porcelain --branch` says, and the lines of `git diff --numstat HEAD`
/// (none where nothing is committed yet).
fn changes(dir: &Path) -> Option<Changes> {
    let (ahead, files) = counts(&git(dir, &["status", "--porcelain", "--branch"])?);
    let (added, removed) = git(dir, &["diff", "--numstat", "HEAD"]).map_or((0, 0), |said| lines(&said));
    Some(Changes { ahead, files, added, removed })
}

/// The lines added and removed, summed over the files (`12\t3\tsrc/a.rs`; a binary file says
/// `-` for both and counts none).
fn lines(said: &str) -> (usize, usize) {
    said.lines().fold((0, 0), |(added, removed), line| {
        let mut parts = line.split('\t').map(|it| it.parse::<usize>().unwrap_or(0));
        (added + parts.next().unwrap_or(0), removed + parts.next().unwrap_or(0))
    })
}

/// The commits ahead, from the first line (`## main...origin/main [ahead 2, behind 1]`), and
/// the lines after it, a file each.
fn counts(said: &str) -> (usize, usize) {
    let mut lines = said.lines().filter(|line| !line.is_empty()).peekable();
    let head = lines.next_if(|line| line.starts_with("## "));
    let ahead = head
        .and_then(|line| line.split("[ahead ").nth(1))
        .and_then(|rest| rest.split([',', ']']).next())
        .and_then(|n| n.trim().parse().ok())
        .unwrap_or(0);
    (ahead, lines.count())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_outside_a_repository_has_none() {
        assert_eq!(read(Path::new("")), None);
        assert_eq!(read(Path::new("Z:/no/such/place")), None);
    }

    #[test]
    fn ahead_and_changes_are_counted() {
        assert_eq!(counts("## main...origin/main [ahead 2, behind 1]\n M a.rs\n?? b\n"), (2, 2));
        assert_eq!(counts("## main...origin/main [ahead 12]\n"), (12, 0));
        assert_eq!(counts("## main\n M a.rs\n"), (0, 1));
        assert_eq!(counts(""), (0, 0));
    }

    #[test]
    fn lines_added_and_removed_are_summed_and_a_binary_file_counts_none() {
        assert_eq!(lines("12\t3\tsrc/a.rs\n-\t-\tlogo.png\n0\t40\told name.rs\n"), (12, 43));
        assert_eq!(lines(""), (0, 0));
    }

    #[test]
    fn the_branch_is_read_from_head() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.tmp/git-head");
        let git = root.join(".git");
        std::fs::create_dir_all(&git).unwrap();
        std::fs::write(git.join("HEAD"), "ref: refs/heads/feature/x\n").unwrap();
        assert_eq!(read(&root).map(|repo| repo.branch), Some("feature/x".to_string()));
        std::fs::write(git.join("HEAD"), "0123456789abcdef\n").unwrap();
        assert_eq!(read(&root).map(|repo| repo.branch), Some("0123456".to_string()));
    }
}
