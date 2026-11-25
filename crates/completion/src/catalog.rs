//! The catalogs: commands, their options and subcommands, read from JSON (specs/completion, "Catalogs").

use quadra_data::{Value, parse_json};

use crate::Family;

/// Where a suggestion comes from; for a catalog file, `Manual` or `External`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    /// Generated from the tools' help.
    External,
    /// Written by hand: ranked above `External`.
    Manual,
    /// The lines run this session: ranked above both.
    History,
}

/// The shells a catalog file is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Windows,
    Unix,
    Common,
}

/// An option of a command, with its hint (`-b` with `new-branch`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Flag {
    pub text: String,
    pub description: Option<String>,
}

/// A command or a subcommand.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Node {
    pub name: String,
    pub options: Vec<Flag>,
    pub subcommands: Vec<Node>,
}

impl Node {
    /// The subcommand of this name, by the family's case rule.
    pub(crate) fn child(&self, name: &str, family: Family) -> Option<&Node> {
        self.subcommands.iter().find(|node| family.same(&node.name, name))
    }
}

/// A command of a catalog, with where it is from.
pub(crate) struct Entry {
    pub source: Source,
    pub group: Group,
    pub node: Node,
}

/// The commands the engine knows.
#[derive(Default)]
pub struct Catalog {
    pub(crate) entries: Vec<Entry>,
}

/// The newest catalog schema this build reads.
const SCHEMA: u32 = 1;

macro_rules! files {
    ($source:literal, $group:literal: $($name:literal)*) => {
        [$(include_str!(concat!("../assets/", $source, "/", $group, "/", $name, ".json"))),*]
    };
}

/// The bundled files by source and group; `tests/catalog.rs` holds this list to `assets/`.
fn bundled_files() -> [(Source, Group, &'static [&'static str]); 5] {
    const EXTERNAL_WINDOWS: &[&str] = &files!("external", "windows":
        "assoc" "attrib" "cd" "chkdsk" "copy" "date" "del" "dir" "echo" "erase" "exit" "fc" "find"
        "findstr" "ftype" "ipconfig" "mklink" "more" "move" "netstat" "ping" "rd" "rmdir" "robocopy"
        "setx" "shutdown" "sort" "start" "systeminfo" "taskkill" "tasklist" "time" "timeout" "tracert"
        "tree" "type" "where" "whoami" "xcopy");
    const EXTERNAL_UNIX: &[&str] = &files!("external", "unix":
        "basename" "cat" "chmod" "chown" "cp" "cut" "date" "df" "dirname" "du" "echo" "env" "false"
        "grep" "head" "id" "ln" "ls" "mkdir" "mv" "nl" "printf" "pwd" "readlink" "realpath" "rm"
        "rmdir" "seq" "sleep" "sort" "split" "stat" "tac" "tail" "tee" "touch" "tr" "true" "uname"
        "uniq" "wc" "who" "whoami" "yes");
    const MANUAL_WINDOWS: &[&str] = &files!("manual", "windows": "ping");
    const MANUAL_UNIX: &[&str] = &files!("manual", "unix": "ifconfig");
    const MANUAL_COMMON: &[&str] = &files!("manual", "common": "cargo" "git");
    [
        (Source::External, Group::Windows, EXTERNAL_WINDOWS),
        (Source::External, Group::Unix, EXTERNAL_UNIX),
        (Source::Manual, Group::Windows, MANUAL_WINDOWS),
        (Source::Manual, Group::Unix, MANUAL_UNIX),
        (Source::Manual, Group::Common, MANUAL_COMMON),
    ]
}

impl Catalog {
    /// A catalog with no commands.
    pub fn new() -> Catalog {
        Catalog::default()
    }

    /// The files bundled with the crate.
    pub fn bundled() -> Catalog {
        let mut catalog = Catalog::new();
        for (source, group, files) in bundled_files() {
            for json in files {
                // A bundled file that does not read is a bug the tests catch: it is left out.
                let _ = catalog.add(json, source, group);
            }
        }
        catalog
    }

    /// Adds a command from its JSON; a malformed file or a newer schema is refused.
    pub fn add(&mut self, json: &str, source: Source, group: Group) -> Result<(), String> {
        let value = parse_json(json).map_err(|error| error.to_string())?;
        if let Some(Value::Number(schema)) = value.get("schema") {
            match schema.parse::<u32>() {
                Ok(version) if version <= SCHEMA => {}
                _ => return Err(format!("catalog schema {schema} is newer than {SCHEMA}")),
            }
        }
        self.entries.push(Entry { source, group, node: node(&value)? });
        Ok(())
    }

    /// How many commands the catalog has.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the catalog has no commands.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The command of this name in `groups` from a source `asks` lets: manual before external,
    /// then the earlier group.
    pub(crate) fn lookup(
        &self,
        name: &str,
        groups: &[Group],
        family: Family,
        asks: impl Fn(Source) -> bool,
    ) -> Option<&Entry> {
        let rank = |entry: &Entry| groups.iter().position(|group| *group == entry.group);
        self.entries
            .iter()
            .filter(|entry| asks(entry.source) && family.same(&entry.node.name, name))
            .filter_map(|entry| Some((entry, rank(entry)?)))
            .min_by_key(|(entry, rank)| (std::cmp::Reverse(entry.source), *rank))
            .map(|(entry, _)| entry)
    }
}

fn node(value: &Value) -> Result<Node, String> {
    let Some(Value::Text(name)) = value.get("name") else {
        return Err("a command has no name".to_string());
    };
    let options = match value.get("options") {
        Some(Value::List(items)) => items.iter().map(flag).collect::<Result<_, _>>()?,
        _ => Vec::new(),
    };
    let subcommands = match value.get("subcommands") {
        Some(Value::List(items)) => items.iter().map(node).collect::<Result<_, _>>()?,
        _ => Vec::new(),
    };
    Ok(Node { name: name.clone(), options, subcommands })
}

fn flag(value: &Value) -> Result<Flag, String> {
    match value {
        Value::Text(text) => Ok(Flag { text: text.clone(), description: None }),
        Value::Map(_) => {
            let Some(Value::Text(text)) = value.get("flag") else {
                return Err("an option has no flag".to_string());
            };
            let description = match value.get("description") {
                Some(Value::Text(hint)) if !hint.trim().is_empty() => Some(hint.clone()),
                _ => None,
            };
            Ok(Flag { text: text.clone(), description })
        }
        _ => Err("an option is neither a string nor an object".to_string()),
    }
}
