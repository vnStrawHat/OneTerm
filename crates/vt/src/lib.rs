//! `oneterm-vt` — OneTerm's own VT engine (IN-0029).
//!
//! Depends on no OneTerm crate and on no UI toolkit: the whole engine is driven
//! by bytes, so it is unit-testable and fuzzable without a runtime.

pub mod parser;
