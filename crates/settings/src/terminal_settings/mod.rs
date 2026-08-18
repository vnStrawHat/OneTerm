//! `TerminalSettings` — global shell config + rendering options.
//!
//! The live model + globals live in [`settings`]; the config↔settings mapping
//! is split across [`apply`] (config → settings) and [`persist`] (settings →
//! config + save). Color/font parsing helpers live in [`color`] and [`font`],
//! and runtime mutators in [`mutators`].

mod apply;
mod color;
mod font;
mod mutators;
mod persist;
mod settings;

pub use color::{hsla_to_hex, parse_hex_color};
pub use font::parse_weight;
pub use settings::{
    ColorOverrides, TerminalBlink, TerminalCursorShape, TerminalPadding, TerminalSettings,
    TerminalSettingsGlobal,
};
