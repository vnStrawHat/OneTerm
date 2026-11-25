//! The engine: suggestions at the cursor, and the history (specs/completion).

use std::ops::Range;

use crate::catalog::{Catalog, Entry, Flag, Node, Source};
use crate::history::History;
use crate::line::{Parsed, floor};
use crate::rank::{Candidate, finish, matches};
use crate::{Config, Family, redact};

/// What a suggestion is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Command,
    Subcommand,
    Option,
    /// A whole line of the history.
    Line,
}

/// One entry of the list the overlay shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Suggestion {
    /// What is shown, and what the replaced bytes become.
    pub text: String,
    /// The bytes of the line it replaces: the token at the cursor, or for a `Line` the line from
    /// its first non-blank; it ends at the cursor.
    pub range: Range<usize>,
    pub kind: Kind,
    pub source: Source,
    /// The hint shown after an option (`new-branch`).
    pub detail: Option<String>,
    /// The bytes at the start of `text` that match what is typed; 0 for a fuzzy match.
    pub matched: usize,
    /// Whether it extends what is typed (by the family's case rule); a fuzzy match does not.
    pub prefix: bool,
}

impl Suggestion {
    /// What accepting it types at the cursor: how many characters to erase before it, then the
    /// text. A prefix match erases nothing, one that differs in case erases from the first
    /// difference.
    pub fn edit(&self, line: &str) -> (usize, &str) {
        let typed = line.get(self.range.clone()).unwrap_or("");
        let same: usize =
            typed.chars().zip(self.text.chars()).take_while(|(a, b)| a == b).map(|(a, _)| a.len_utf8()).sum();
        (typed[same..].chars().count(), &self.text[same..])
    }
}

/// The catalogs, the settings and the session's history. The app shares one between its tabs.
pub struct Engine {
    config: Config,
    catalog: Catalog,
    histories: [History; 3],
}

impl Engine {
    /// An engine over the bundled catalogs.
    pub fn new(config: Config) -> Engine {
        Engine::with_catalog(config, Catalog::bundled())
    }

    /// An engine over a catalog of its own.
    pub fn with_catalog(config: Config, catalog: Catalog) -> Engine {
        Engine { config, catalog, histories: Default::default() }
    }

    /// The settings.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Applies new settings at once; a smaller `max_history` drops the least recently used.
    pub fn set_config(&mut self, config: Config) {
        for history in &mut self.histories {
            history.trim(config.max_history);
        }
        self.config = config;
    }

    /// The suggestions at `cursor` (a byte offset) of `line`, best first.
    pub fn suggest(&self, line: &str, cursor: usize, family: Family) -> Vec<Suggestion> {
        self.ask(line, cursor, family, false)
    }

    /// As `suggest`, for the trigger key: a command is completed before `min_prefix_len`.
    pub fn suggest_now(&self, line: &str, cursor: usize, family: Family) -> Vec<Suggestion> {
        self.ask(line, cursor, family, true)
    }

    /// Keeps a line that was run, redacted first with `redact_sensitive`.
    pub fn record(&mut self, line: &str, family: Family) {
        if !self.config.enabled || !self.config.asks(Source::History) {
            return;
        }
        let line = if self.config.redact_sensitive { redact::redact(line) } else { line.trim().to_string() };
        if !line.is_empty() {
            self.histories[family.index()].record(&line, self.config.max_history);
        }
    }

    /// The lines kept for a family, in the order they were first run.
    pub fn history(&self, family: Family) -> impl Iterator<Item = &str> {
        self.histories[family.index()].records.iter().map(|record| record.line.as_str())
    }

    /// Forgets every line.
    pub fn clear_history(&mut self) {
        self.histories = Default::default();
    }

    fn ask(&self, line: &str, cursor: usize, family: Family, forced: bool) -> Vec<Suggestion> {
        if !self.config.enabled {
            return Vec::new();
        }
        let cursor = floor(line, cursor);
        let parsed = Parsed::new(line, cursor);
        let token = parsed.token.as_str();
        let mut out = Vec::new();
        if parsed.before.is_empty() {
            if !forced && token.chars().count() < self.config.min_prefix_len {
                return Vec::new();
            }
            self.commands(&parsed, family, &mut out);
        } else if family.is_option(token) {
            self.options(&parsed, family, &mut out);
        } else {
            if let Some((entry, node, _)) = self.resolve(&parsed, family) {
                for child in &node.subcommands {
                    let _ = self.push(&mut out, &parsed, family, &child.name, Kind::Subcommand, entry.source);
                }
            }
            self.lines(line, cursor, family, &mut out);
        }
        finish(out, line, cursor, family, &self.config)
    }

    /// The command and the node the tokens before the cursor walk to, with the ancestors' options.
    fn resolve(&self, parsed: &Parsed, family: Family) -> Option<(&Entry, &Node, Vec<&Flag>)> {
        let groups = family.groups(self.config.windows_allow_coreutils);
        let entry = self.catalog.lookup(parsed.head()?, groups, family, |source| self.config.asks(source))?;
        let (mut node, mut ancestors) = (&entry.node, Vec::new());
        for token in parsed.before.iter().skip(1).filter(|token| !family.is_option(token)) {
            let Some(child) = node.child(token, family) else { break };
            ancestors.extend(&node.options);
            node = child;
        }
        Some((entry, node, ancestors))
    }

    fn commands(&self, parsed: &Parsed, family: Family, out: &mut Vec<Candidate>) {
        for group in family.groups(self.config.windows_allow_coreutils) {
            for entry in self.catalog.entries.iter().filter(|entry| entry.group == *group) {
                if self.config.asks(entry.source) {
                    let _ = self.push(out, parsed, family, &entry.node.name, Kind::Command, entry.source);
                }
            }
        }
        if self.config.asks(Source::History) {
            let history = &self.histories[family.index()];
            for record in &history.records {
                let Some(first) = record.line.split_whitespace().next() else { continue };
                if let Some(candidate) = self.push(out, parsed, family, first, Kind::Command, Source::History)
                {
                    candidate.frecency = history.frecency(record);
                }
            }
        }
    }

    fn options(&self, parsed: &Parsed, family: Family, out: &mut Vec<Candidate>) {
        let token = parsed.token.as_str();
        if let Some((entry, node, ancestors)) = self.resolve(parsed, family) {
            let inherited = if self.config.inherit_ancestor_options { ancestors } else { Vec::new() };
            let own = node.options.iter().map(|flag| (flag, 0.0));
            for (flag, frecency) in own.chain(inherited.into_iter().map(|flag| (flag, -1.0))) {
                if let Some((prefix, matched)) = matches(&flag.text, token, family, self.config.fuzzy) {
                    out.push(Candidate {
                        prefix,
                        matched,
                        frecency,
                        detail: flag.description.clone(),
                        ..Candidate::new(&flag.text, Kind::Option, entry.source, parsed.token_start)
                    });
                }
            }
            return;
        }
        // A command no catalog knows: the options it was run with.
        let (Some(head), true) = (parsed.head(), self.config.asks(Source::History)) else { return };
        let history = &self.histories[family.index()];
        let mut seen: Vec<&str> = Vec::new();
        for record in &history.records {
            let mut words = record.line.split_whitespace();
            if !words.next().is_some_and(|first| family.same(first, head)) {
                continue;
            }
            for word in words.filter(|word| family.is_option(word)) {
                if seen.contains(&word) {
                    continue;
                }
                seen.push(word);
                if let Some(candidate) = self.push(out, parsed, family, word, Kind::Option, Source::History) {
                    candidate.frecency = history.frecency(record);
                }
            }
        }
    }

    /// Whole lines of the history that start with what is typed.
    fn lines(&self, line: &str, cursor: usize, family: Family, out: &mut Vec<Candidate>) {
        if !self.config.asks(Source::History) {
            return;
        }
        let typed = line[..cursor].trim_start();
        let start = cursor - typed.len();
        let history = &self.histories[family.index()];
        for record in history.records.iter().filter(|record| record.line.len() > typed.len()) {
            if family.starts(&record.line, typed) {
                out.push(Candidate {
                    prefix: true,
                    matched: typed.len(),
                    frecency: history.frecency(record),
                    ..Candidate::new(&record.line, Kind::Line, Source::History, start)
                });
            }
        }
    }

    /// A candidate for the token at the cursor, when `text` matches it.
    fn push<'a>(
        &self,
        out: &'a mut Vec<Candidate>,
        parsed: &Parsed,
        family: Family,
        text: &str,
        kind: Kind,
        source: Source,
    ) -> Option<&'a mut Candidate> {
        let (prefix, matched) = matches(text, &parsed.token, family, self.config.fuzzy)?;
        out.push(Candidate { prefix, matched, ..Candidate::new(text, kind, source, parsed.token_start) });
        out.last_mut()
    }
}
