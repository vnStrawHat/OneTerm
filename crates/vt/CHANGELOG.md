# Changelog

All notable changes to `oneterm-vt` are recorded here, in the shape
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) describes. An entry belongs here when it
changes what an embedder compiles against or what bytes the terminal replies with.

## The promise

The crate is not on crates.io: other projects depend on it by git, pinned to a tag. The promise
below applies to those tags exactly as it would to published releases.

The crate is `0.x`, and Cargo treats a minor bump as breaking. So does this crate:

1. A removal, a signature change, or a new variant on an exhaustive enum is a **minor** bump, with
   an entry below naming the item.
2. A new method, a new module, a new default-off feature, or a new variant on a
   `#[non_exhaustive]` enum is a **patch** bump.
3. Raising the minimum supported Rust version is a **minor** bump. Never a patch.
4. `parser::Dispatch` is the engine's internal semantic contract; its method set changes at patch
   level.
5. Anything reachable only with a non-default feature carries the same promise as the default
   surface. A feature is never a stability escape hatch.
6. Behaviour is not the API, with one exception: **the reply bytes for `DA1`, `DA2`, `DSR`,
   `DECRQM`, `XTVERSION` and the OSC colour queries are a contract**, because programs parse them.
   Changing one is a minor bump and an entry below, even though no Rust signature moved.
   `Config::product_name` exists so an embedder can change the identity half of those replies
   without the engine changing the shape half.
7. `FeedStats` counter semantics are documented per field and are part of the contract. A counter
   that starts counting a different thing is a minor bump.

Not promised: grid internals reachable through `grid::Screen`, the exact `SeqNo` values,
allocation behaviour, and performance. The row-identity guarantee -- a `RowId` names the same
content for as long as that content is live -- **is** promised, because embedder caches are keyed
by it.

The version number is shared with the application this engine was written for, so a release can
carry no API change at all. Such a release says so below rather than being omitted.

## [Unreleased]

### Added

- `Config::product_name`: what the terminal calls itself in `XTVERSION` (`CSI > 0 q`) and `DA2`
  (`CSI > c`). `None` answers `oneterm-vt(<version>)`. The value is sanitised before it is used:
  C0, `DEL` and C1 controls are dropped so a name cannot end the DCS string early, and the result
  is cut to 64 bytes. `DA2` packs the trailing `(<major>.<minor>.<patch>)` into one number with
  each component saturating at 99.
- `README.md`, `CHANGELOG.md`, `LICENSE`, `NOTICE` and `examples/headless.rs`: the crate is
  packaged so another project can depend on it by git, and documented so somebody who has never
  seen the OneTerm repository can use it.
- `search`: scrollback search, in two phases. `GridText::from_terminal` copies one `char` per
  cell under the embedder's lock; `search_grid_text` matches against that copy without it, and
  returns `SearchMatch`es keyed by `RowId`. `SearchOptions` carries `case_sensitive` and
  `whole_word`. Neither form of match crosses a row boundary.
- The `regex` feature, **off by default**: it adds `SearchPattern::Regex(&regex::Regex)` and makes
  `regex` an optional dependency. With the feature off the crate still has no dependency beyond the
  six it always had. `SearchOptions` is ignored for a regex — the pattern owns its own `(?i)` and
  `\b` — in every build; a debug assertion fires when either field is non-default, but a
  release build carries no such check and silently ignores them. `SearchPattern` is
  `#[non_exhaustive]` so the same `match` compiles either way. A regular expression that can
  match the empty string reports one zero-width `SearchMatch` per position, the position past
  the last cell included, so `start_col` can be one past the last column.
- `SearchOptions` is `#[non_exhaustive]`: a future option must not be a breaking change.
  `SearchOptions::default()` plus field assignment is the construction form.

### Changed

- With no `product_name` set, `XTVERSION` now answers `oneterm-vt(<version>)` instead of naming
  the application this engine was extracted from.
- Every public item is documented; `#![warn(missing_docs)]` keeps it that way.
