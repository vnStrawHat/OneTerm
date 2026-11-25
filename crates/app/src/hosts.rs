//! Saved hosts and the recent ones (specs/oneterm, "M8 in detail"): rows of words in the
//! settings, so the file reads as the page does.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use oneterm_ssh::{Auth, Forward, Hop, Target};

use crate::model::Model;
use crate::shells::{Launch, Remote};

pub(crate) mod tag;

const AUTHS: [&str; 4] = ["Auto", "Agent", "Key", "Password"];
/// How many recent targets are kept.
const RECENT: usize = 5;
/// The most jump hosts on a way, as the engine takes.
const HOPS: usize = 4;
/// A host the user saved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Host {
    pub name: String,
    pub group: String,
    pub remote: Remote,
    /// The name of the saved host this one is reached through, or nothing.
    pub jump: String,
}

impl Host {
    fn row(&self) -> Vec<String> {
        let (auth, key) = match &self.remote.auth {
            Auth::Agent => (1, String::new()),
            Auth::Key(path) => (2, path.display().to_string()),
            Auth::Password => (3, String::new()),
            Auth::Auto => (0, String::new()),
        };
        let forwards: Vec<String> = self.remote.forwards.iter().map(Forward::label).collect();
        let (name, group, target) = (self.name.clone(), self.group.clone(), self.remote.target.label());
        vec![name, group, target, AUTHS[auth].into(), key, self.jump.clone(), forwards.join(" ")]
    }

    /// A row of five words (before M10) or seven: a jump host and the forwards.
    fn of(row: &[String]) -> Option<Host> {
        let [name, group, target, auth, key, rest @ ..] = row else { return None };
        let auth = match auth.as_str() {
            "Agent" => Auth::Agent,
            "Key" => Auth::Key(PathBuf::from(key)),
            "Password" => Auth::Password,
            _ => Auth::Auto,
        };
        let mut remote = Remote::new(Target::parse(target)?, auth);
        let forwards = rest.get(1).map_or("", String::as_str).split_whitespace();
        remote.forwards = forwards.map(Forward::parse).collect::<Option<_>>()?;
        let jump = rest.first().cloned().unwrap_or_default();
        Some(Host { name: name.clone(), group: group.clone(), remote, jump })
    }

    /// The host's address as a row shows it: `user@host`, its port when not 22.
    pub fn address(&self) -> String {
        self.remote.target.label()
    }
}

/// The way to a host through the saved host `name` and the ones it is reached through; a
/// name that is gone ends it.
fn jumps_of(hosts: &[Host], mut name: String) -> Vec<Hop> {
    let mut hops = Vec::new();
    while let Some(host) = hosts.iter().find(|it| it.name == name).filter(|_| hops.len() < HOPS) {
        hops.insert(0, Hop { target: host.remote.target.clone(), auth: host.remote.auth.clone() });
        name = host.jump.clone();
    }
    hops
}

/// The index of an auth among the dialogs' choices, and back.
pub(crate) fn auth_of(index: usize, key: String) -> Auth {
    match index {
        1 => Auth::Agent,
        2 => Auth::Key(PathBuf::from(key)),
        3 => Auth::Password,
        _ => Auth::Auto,
    }
}

impl Model {
    /// The saved hosts, in the order saved.
    pub fn hosts(&self) -> Vec<Host> {
        let mut all: Vec<Host> =
            self.prefs().hosts.with(|rows| rows.iter().filter_map(|row| Host::of(row)).collect());
        let found = all.clone();
        for host in &mut all {
            host.remote.jumps = jumps_of(&found, host.jump.clone());
        }
        all
    }

    /// The choices of a jump host select: none, then the saved hosts.
    pub(crate) fn jump_choices(&self) -> Vec<String> {
        std::iter::once("None".to_string()).chain(self.hosts().into_iter().map(|it| it.name)).collect()
    }

    /// The host chosen in a jump host select.
    pub(crate) fn jump_host(&self, chosen: usize) -> Option<Host> {
        chosen.checked_sub(1).and_then(|at| self.hosts().into_iter().nth(at))
    }

    /// Saves a host, in place of one of the same name, whose colour tag it keeps.
    pub fn save_host(&self, host: Host) {
        let mut row = host.row();
        self.prefs().hosts.update(|rows| match rows.iter().position(|it| it.first() == Some(&host.name)) {
            Some(at) => {
                row.extend(rows[at].get(7).cloned());
                rows[at] = row;
            }
            None => rows.push(row),
        });
    }

    pub fn remove_host(&self, name: &str) {
        self.prefs().hosts.update(|rows| rows.retain(|it| it.first().map(String::as_str) != Some(name)));
        self.set_sealed(name, None);
        self.set_startup(name, None);
    }

    /// What is written once a host's shell is open: the command it chose, as the list has it
    /// now.
    pub(crate) fn startup(&self, name: &str) -> Option<String> {
        self.startup_command(&self.startup_name(name)?)
    }

    /// The name of the startup command a host chose.
    pub(crate) fn startup_name(&self, name: &str) -> Option<String> {
        let rows = self.prefs().startup;
        rows.with_untracked(|rows| {
            rows.iter().find(|it| it.first().map(String::as_str) == Some(name))?.get(1).cloned()
        })
    }

    /// Saves the name of a host's startup command, or forgets it (`None` or empty).
    pub(crate) fn set_startup(&self, name: &str, command: Option<String>) {
        let command = command.filter(|it| !it.trim().is_empty());
        self.prefs().startup.update(|rows| {
            rows.retain(|it| it.first().map(String::as_str) != Some(name));
            rows.extend(command.map(|it| vec![name.to_string(), it]));
        });
    }

    /// A space connected to `remote`: with the saved host's startup command, and its saved
    /// secret when the data key is in memory.
    pub(crate) fn launch_to(&self, remote: Remote) -> Launch {
        let host = self.hosts().into_iter().find(|it| it.remote.target == remote.target);
        let startup = host.and_then(|it| self.startup(&it.name));
        let secret = self.saved_secret(&remote);
        Launch { ssh: Some(remote), secret, startup, ..self.launch(None) }
    }

    /// Opens a tab connected to a host, or a space beside the focused one; it is recent.
    pub fn connect_to(&self, remote: Remote, here: bool) {
        self.connect_all(vec![remote], here);
    }

    /// As `connect_to`, the engine answered with a secret the first time it asks for it.
    pub(crate) fn connect_with(&self, remote: Remote, here: bool, secret: Option<crate::vault::Secret>) {
        let label = remote.target.label();
        let launch = Launch { secret, ..self.launch_to(remote) };
        if here {
            self.split_with(launch)
        } else {
            self.new_tab_with(launch)
        }
        self.remember(label);
    }

    /// A target connected to is the most recent.
    pub(crate) fn remember(&self, label: String) {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |it| it.as_secs());
        self.prefs().recent.update(|rows| {
            rows.retain(|it| it.first() != Some(&label));
            rows.insert(0, vec![label, now.to_string()]);
            rows.truncate(RECENT);
        });
    }

    /// The recent targets, newest first, with when (seconds since 1970).
    pub fn recent(&self) -> Vec<(Remote, u64)> {
        self.prefs().recent.with(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let [label, at] = &row[..] else { return None };
                    let host = self.hosts().into_iter().find(|it| it.address() == *label);
                    let remote = match host {
                        Some(host) => host.remote,
                        None => Remote::new(Target::parse(label)?, Auth::Auto),
                    };
                    Some((remote, at.parse().unwrap_or(0)))
                })
                .collect()
        })
    }

    /// What a remote space is called: its host's name, else `user@host`.
    pub fn host_name(&self, remote: &Remote) -> String {
        let host = self.hosts().into_iter().find(|it| it.remote.target == remote.target);
        host.map_or_else(|| remote.target.label(), |it| it.name)
    }

    /// Whether a space is connected to a target.
    pub fn connected_to(&self, target: &Target) -> bool {
        self.tabs().with(|tabs| {
            tabs.iter().any(|tab| {
                tab.spaces.with(|spaces| {
                    spaces.iter().any(|space| {
                        space.remote.as_ref().is_some_and(|it| it.target == *target)
                            && space.session.exited().get().is_none()
                    })
                })
            })
        })
    }
}

/// How long ago, in a word or two: `now`, `2 min ago`, `3 h ago`, `yesterday`, `4 d ago`.
pub(crate) fn ago(then: u64) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |it| it.as_secs());
    match now.saturating_sub(then) {
        0..60 => "now".into(),
        s @ 60..3600 => format!("{} min ago", s / 60),
        s @ 3600..86_400 => format!("{} h ago", s / 3600),
        86_400..172_800 => "yesterday".into(),
        s => format!("{} d ago", s / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_is_a_row_of_words_and_back() {
        let mut remote = Remote::new(Target::parse("ops@10.20.0.2:2222").unwrap(), Auth::Key("k".into()));
        remote.forwards = vec![
            Forward::parse("8080:localhost:80").unwrap(),
            Forward::parse("R5432:db:5432").unwrap(),
            Forward::parse("D1080").unwrap(),
        ];
        let host =
            Host { name: "prod-lb".into(), group: "Production".into(), remote, jump: "bastion".into() };
        let row = [
            "prod-lb",
            "Production",
            "ops@10.20.0.2:2222",
            "Key",
            "k",
            "bastion",
            "8080:localhost:80 R5432:db:5432 D1080",
        ];
        assert_eq!(host.row(), row);
        assert_eq!(Host::of(&host.row()), Some(host));
        let old: Vec<String> = row[..5].iter().map(|it| it.to_string()).collect();
        assert_eq!(Host::of(&old).unwrap().jump, "");
        assert_eq!(Host::of(&["a".into(), "b".into(), "-x".into(), "Auto".into(), String::new()]), None);
        assert!(ago(0).ends_with("d ago"));
    }

    #[test]
    fn jump_hosts_chain_outermost_first_and_stop() {
        let host = |name: &str, jump: &str| {
            let remote = Remote::new(Target::parse(&format!("u@{name}")).unwrap(), Auth::Auto);
            Host { name: name.into(), group: String::new(), remote, jump: jump.into() }
        };
        let all = [host("app", "mid"), host("mid", "edge"), host("edge", ""), host("loop", "loop")];
        let names = |hops: Vec<Hop>| hops.into_iter().map(|it| it.target.host).collect::<Vec<_>>();
        assert_eq!(names(jumps_of(&all, "mid".into())), ["edge", "mid"]);
        assert_eq!(names(jumps_of(&all, "gone".into())), Vec::<String>::new());
        assert_eq!(jumps_of(&all, "loop".into()).len(), HOPS);
    }
}
