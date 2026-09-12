//! Developer diagnostics shared by the `oneterm-tools` binaries.
//!
//! The crate is never a dependency of the application and depends on no OneTerm
//! crate; it sits outside the L0-L4 layering
//! (`docs/agents/crate-dependency-rules.md`).
//!
//! The library half exists so the VT parity corpus can be both a command-line
//! tool and a `#[test]`: `vt-corpus` drives it by hand, and
//! `tests/corpus_check.rs` runs the same comparison inside
//! `cargo test --workspace`, which is what makes a drifted expectation fail the
//! build.

pub mod bench;
pub mod corpus;
pub mod corpus_grep;
pub mod corpus_replay;
pub mod corpus_replay_new;
pub mod corpus_upstream;
