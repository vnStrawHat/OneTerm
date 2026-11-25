//! OneTerm's command completion (specs/completion): from the line at the prompt and its cursor, a short
//! ranked list of what may be typed next. The engine has no view; the terminal's overlay asks it.

mod catalog;
mod engine;
mod history;
mod line;
mod rank;
pub mod redact;

pub use catalog::{Catalog, Group, Source};
pub use engine::{Engine, Kind, Suggestion};

/// The kind of shell a line is typed into.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Family {
    Cmd,
    PowerShell,
    Unix,
}

impl Family {
    /// The family of a program's path (by its file name) or a shell's name, by its words; Unix
    /// when neither names PowerShell nor Cmd. A live title is no good to it: one that shows a
    /// folder named `cmd` reads as Cmd.
    pub fn detect(text: &str) -> Family {
        let text = text.rsplit(['/', '\\']).next().unwrap_or(text).to_ascii_lowercase();
        let mut words = text.split(|c: char| !c.is_ascii_alphanumeric()).filter(|word| !word.is_empty());
        if words.clone().any(|word| word == "pwsh" || word == "powershell") {
            Family::PowerShell
        } else if words.any(|word| word == "cmd") || text.contains("command prompt") {
            Family::Cmd
        } else {
            Family::Unix
        }
    }

    /// The family a setting names: `cmd`, `powershell` or `pwsh`, `unix`, `bash`, `zsh`, `sh`.
    pub fn parse(name: &str) -> Option<Family> {
        match name.to_ascii_lowercase().as_str() {
            "cmd" => Some(Family::Cmd),
            "powershell" | "pwsh" => Some(Family::PowerShell),
            "unix" | "bash" | "zsh" | "sh" => Some(Family::Unix),
            _ => None,
        }
    }

    /// Whether names and options match without case (Cmd and PowerShell).
    pub fn ignores_case(self) -> bool {
        self != Family::Unix
    }

    /// Whether a token is an option: `/` or `-` first for Cmd, `-` for the others.
    pub fn is_option(self, token: &str) -> bool {
        token.starts_with('-') || (self == Family::Cmd && token.starts_with('/'))
    }

    /// The catalog groups searched, the first winning a name both have.
    pub(crate) fn groups(self, allow_unix: bool) -> &'static [Group] {
        match (self, allow_unix) {
            (Family::Unix, _) => &[Group::Unix, Group::Common],
            (_, false) => &[Group::Windows, Group::Common],
            (_, true) => &[Group::Windows, Group::Common, Group::Unix],
        }
    }

    /// Two names are the same, by the family's case rule.
    pub(crate) fn same(self, a: &str, b: &str) -> bool {
        if self.ignores_case() { a.eq_ignore_ascii_case(b) } else { a == b }
    }

    /// `text` starts with `prefix`, by the family's case rule.
    pub(crate) fn starts(self, text: &str, prefix: &str) -> bool {
        text.get(..prefix.len()).is_some_and(|head| self.same(head, prefix))
    }

    pub(crate) fn index(self) -> usize {
        self as usize
    }
}

/// Which sources are asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sources {
    /// The lines run this session.
    pub memory: bool,
    /// The catalogs written by hand.
    pub manual: bool,
    /// The catalogs generated from the tools' help.
    pub external: bool,
}

impl Default for Sources {
    fn default() -> Sources {
        Sources { memory: true, manual: true, external: true }
    }
}

/// The user's settings of completion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// Completion at all.
    pub enabled: bool,
    /// Tab accepts what is chosen; else it goes to the shell (the view's).
    pub accept_tab: bool,
    /// The lines kept per family; 0 keeps none.
    pub max_history: usize,
    /// Characters of a command typed before it is completed.
    pub min_prefix_len: usize,
    /// Rows the overlay shows at once; the list is at most four times this.
    pub max_visible_items: usize,
    pub sources: Sources,
    /// Matches as a subsequence, after the prefix matches.
    pub fuzzy: bool,
    /// The options of a subcommand's ancestors too, ranked lower.
    pub inherit_ancestor_options: bool,
    /// Nothing on the alternate screen (the view's).
    pub disable_in_alt_screen: bool,
    /// Only where the shell marks a command being typed (the view's).
    pub require_prompt_region: bool,
    /// Cmd and PowerShell also know the Unix commands.
    pub windows_allow_coreutils: bool,
    /// The family of every session, over what is detected.
    pub force_family: Option<Family>,
    /// Secrets are taken out of a line before it is kept.
    pub redact_sensitive: bool,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            enabled: true,
            accept_tab: true,
            max_history: 500,
            min_prefix_len: 1,
            max_visible_items: 8,
            sources: Sources::default(),
            fuzzy: true,
            inherit_ancestor_options: true,
            disable_in_alt_screen: true,
            require_prompt_region: true,
            windows_allow_coreutils: false,
            force_family: None,
            redact_sensitive: true,
        }
    }
}

impl Config {
    /// The family of a session: `force_family`, else the one `Family::detect` reads.
    pub fn family(&self, program_or_title: &str) -> Family {
        self.force_family.unwrap_or_else(|| Family::detect(program_or_title))
    }

    /// Whether a source is asked; the memory only when it keeps lines.
    pub fn asks(&self, source: Source) -> bool {
        match source {
            Source::History => self.sources.memory && self.max_history > 0,
            Source::Manual => self.sources.manual,
            Source::External => self.sources.external,
        }
    }
}
