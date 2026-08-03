//! Wire-format (JSON) schema + parsing for catalog files (docs 02 §5).
//!
//! Deserializes the on-disk `{ name, options, subcommands? }` shape into the
//! runtime [`CommandNode`](super::CommandNode) model, and enforces the supported
//! schema version. Kept separate from the runtime catalog store so the wire
//! format can evolve without touching lookup/precedence logic.

use serde::Deserialize;

use super::{CommandNode, Flag};

/// The highest catalog schema major version this build understands. Files with a
/// higher `schema` are logged and ignored (docs 02 §5).
pub const SUPPORTED_SCHEMA: u32 = 1;

#[derive(Deserialize)]
#[serde(untagged)]
enum RawFlag {
    Str(String),
    Obj {
        flag: String,
        #[serde(default)]
        description: Option<String>,
    },
}

impl RawFlag {
    fn into_flag(self) -> Flag {
        match self {
            RawFlag::Str(text) => Flag {
                text,
                description: None,
            },
            RawFlag::Obj { flag, description } => Flag {
                text: flag,
                description: description.filter(|d| !d.trim().is_empty()),
            },
        }
    }
}

#[derive(Deserialize)]
struct RawNode {
    #[serde(default)]
    schema: Option<u32>,
    name: String,
    #[serde(default)]
    options: Vec<RawFlag>,
    #[serde(default)]
    subcommands: Vec<RawNode>,
}

impl RawNode {
    fn into_node(self) -> CommandNode {
        CommandNode {
            name: self.name,
            options: self.options.into_iter().map(RawFlag::into_flag).collect(),
            subcommands: self
                .subcommands
                .into_iter()
                .map(RawNode::into_node)
                .collect(),
        }
    }
}

/// Parse a single command node from its JSON string. Rejects unknown major
/// schema versions (returns `Err`, logged by the caller). Public for tests.
pub fn parse_node(json: &str) -> Result<CommandNode, String> {
    let raw: RawNode = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if let Some(v) = raw.schema {
        if v > SUPPORTED_SCHEMA {
            return Err(format!(
                "unsupported catalog schema version {v} (max {SUPPORTED_SCHEMA})"
            ));
        }
    }
    Ok(raw.into_node())
}
