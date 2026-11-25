//! The local side of the expanded SFTP view (specs/oneterm, "M9 in detail"): a folder of this
//! machine, listed on the pool; and what both sides share: hidden names, the comparison, how
//! sizes, times and free names read.

use std::path::{Path, PathBuf};

use oneterm_ssh::Entry;
use quadra::{Scope, Signal, background, signal, spawn};

/// A folder of this machine and what is chosen in it.
#[derive(Clone, Copy)]
pub(crate) struct Local {
    pub dir: Signal<PathBuf>,
    pub entries: Signal<Vec<Entry>>,
    pub selected: Signal<Option<String>>,
    /// Why the list is empty, or what failed.
    pub said: Signal<String>,
    owner: Scope,
}

impl Local {
    pub fn new(start: PathBuf) -> Local {
        let local = Local {
            dir: signal(start.clone()),
            entries: signal(Vec::new()),
            selected: signal(None),
            said: signal(String::new()),
            owner: Scope::current(),
        };
        local.load(start);
        local
    }

    /// Shows a folder, listed on the pool.
    pub fn load(&self, dir: PathBuf) {
        let local = *self;
        self.owner.run(|| {
            spawn(async move {
                let listed = dir.clone();
                let got =
                    background(move || list(&listed)).await.unwrap_or_else(|_| Err("the work failed".into()));
                match got {
                    Ok(entries) => {
                        local.dir.set(dir);
                        local.entries.set(entries);
                        local.said.set(String::new());
                    }
                    Err(why) => local.said.set(why),
                }
                local.selected.set(None);
            })
            .detach()
        });
    }

    pub fn refresh(&self) {
        self.load(self.dir.get_untracked());
    }

    pub fn up(&self) {
        if let Some(parent) = self.dir.get_untracked().parent() {
            self.load(parent.to_path_buf());
        }
    }
}

/// A local folder's entries as the engine gives a remote one's: folders first, then names
/// whatever the case.
fn list(dir: &Path) -> Result<Vec<Entry>, String> {
    let mut entries: Vec<Entry> = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .filter_map(|it| {
            let meta = it.metadata().ok()?;
            let since = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok();
            let name = it.file_name().to_string_lossy().into_owned();
            Some(Entry {
                name,
                dir: meta.is_dir(),
                size: meta.len(),
                modified: since.map_or(0, |it| it.as_secs()),
                mode: 0,
            })
        })
        .collect();
    entries.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(entries)
}

/// A column a pane is sorted by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum By {
    #[default]
    Name,
    Size,
    Modified,
    /// The permission bits, as a number.
    Mode,
}

/// A pane's order: its column, and whether the other way.
pub(crate) type Sort = (By, bool);

/// Entries in a pane's order, folders first.
pub(crate) fn sorted(mut entries: Vec<Entry>, (by, back): Sort) -> Vec<Entry> {
    entries.sort_by(|a, b| {
        let named = || a.name.to_lowercase().cmp(&b.name.to_lowercase());
        let order = match by {
            By::Name => named(),
            By::Size => a.size.cmp(&b.size).then_with(named),
            By::Modified => a.modified.cmp(&b.modified).then_with(named),
            By::Mode => (a.mode & 0o7777).cmp(&(b.mode & 0o7777)).then_with(named),
        };
        b.dir.cmp(&a.dir).then(if back { order.reverse() } else { order })
    });
    entries
}

/// The entries shown: those whose names start with `.` only when hidden ones show.
pub(crate) fn visible(entries: &[Entry], hidden: bool) -> Vec<Entry> {
    entries.iter().filter(|it| hidden || !it.name.starts_with('.')).cloned().collect()
}

/// How a remote file stands to its local namesake: `older`, `newer`, or nothing when they
/// look the same or there is none.
pub(crate) fn compared(remote: &Entry, local: &[Entry]) -> Option<&'static str> {
    let twin = local.iter().find(|it| !it.dir && it.name == remote.name)?;
    match remote.modified.cmp(&twin.modified) {
        _ if remote.dir => None,
        std::cmp::Ordering::Less => Some("older"),
        std::cmp::Ordering::Greater => Some("newer"),
        std::cmp::Ordering::Equal => None,
    }
}

/// `2026-10-28 10:52` where the user is; nothing for no time.
pub(crate) fn when(seconds: u64) -> String {
    if seconds == 0 {
        return String::new();
    }
    let at = quadra::time::DateTime::from_unix(seconds as i64, quadra::time::utc_offset());
    format!("{} {:02}:{:02}", at.date, at.time.hour, at.time.minute)
}

/// `Nov 16 10:52` where the user is, the month in the app's language; nothing for no time.
pub(crate) fn day(seconds: u64) -> String {
    if seconds == 0 {
        return String::new();
    }
    let at = quadra::time::DateTime::from_unix(seconds as i64, quadra::time::utc_offset());
    let month: String = quadra::language().locale().month(at.date.month).chars().take(3).collect();
    format!("{month} {:02} {:02}:{:02}", at.date.day, at.time.hour, at.time.minute)
}

/// `name`, or `name (1).ext` and on when that is taken.
pub(crate) fn free_name(folder: &Path, name: &str) -> PathBuf {
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (name, String::new()),
    };
    let mut path = folder.join(name);
    let mut n = 1;
    while path.exists() || path.with_file_name(format!("{}.part", file(&path))).exists() {
        path = folder.join(format!("{stem} ({n}){ext}"));
        n += 1;
    }
    path
}

fn file(path: &Path) -> String {
    path.file_name().map(|it| it.to_string_lossy().into_owned()).unwrap_or_default()
}

/// A size as people read it: `812 B`, `2.4 KB`, `41 MB`.
pub(crate) fn size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let (mut value, mut unit) = (bytes as f64 / 1024.0, 0);
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        (value, unit) = (value / 1024.0, unit + 1);
    }
    if value < 10.0 { format!("{value:.1} {}", UNITS[unit]) } else { format!("{value:.0} {}", UNITS[unit]) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_free_names() {
        assert_eq!(
            (size(812), size(2458), size(41 * 1024 * 1024)),
            ("812 B".into(), "2.4 KB".into(), "41 MB".into())
        );
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.tmp/free-name");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(free_name(&dir, "a.txt"), dir.join("a.txt"));
        std::fs::write(dir.join("a.txt"), b"").unwrap();
        assert_eq!(free_name(&dir, "a.txt"), dir.join("a (1).txt"));
        assert_eq!(free_name(&dir, ".bashrc"), dir.join(".bashrc"));
        let names: Vec<String> = list(&dir).unwrap().into_iter().map(|it| it.name).collect();
        assert_eq!(names, ["a.txt"]);
    }

    #[test]
    fn panes_sort_by_a_column_with_folders_first() {
        let e = |name: &str, dir, size, modified| Entry {
            name: name.into(),
            dir,
            size,
            modified,
            mode: 0o600 + size as u32,
        };
        let all = vec![e("b", false, 1, 30), e("A", false, 9, 10), e("z", true, 0, 20), e("c", false, 5, 20)];
        let names = |sort| sorted(all.clone(), sort).into_iter().map(|it| it.name).collect::<Vec<_>>();
        assert_eq!(names((By::Name, false)), ["z", "A", "b", "c"]);
        assert_eq!(names((By::Name, true)), ["z", "c", "b", "A"]);
        assert_eq!(names((By::Size, true)), ["z", "A", "c", "b"]);
        assert_eq!(names((By::Modified, false)), ["z", "A", "c", "b"]);
        assert_eq!(names((By::Mode, false)), ["z", "b", "c", "A"]);
    }

    #[test]
    fn hidden_names_and_comparisons() {
        let at = |name: &str, modified| Entry { name: name.into(), size: 1, modified, ..Entry::default() };
        let all = [at(".env", 1), at("a", 5)];
        assert_eq!(visible(&all, false), [at("a", 5)]);
        assert_eq!(visible(&all, true).len(), 2);
        let local = [at("a", 5)];
        assert_eq!(compared(&at("a", 3), &local), Some("older"));
        assert_eq!(compared(&at("a", 9), &local), Some("newer"));
        assert_eq!(compared(&at("a", 5), &local), None);
        assert_eq!(compared(&at("b", 1), &local), None);
        assert_eq!(when(0), "");
    }
}
