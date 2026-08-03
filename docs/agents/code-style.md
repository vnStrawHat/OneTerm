# Code conventions — OneTerm

> File split from `AGENTS.md` (section 4). Every rule here is **mandatory** when writing Rust for the OneTerm project.

## 1. Style & format

- Run `cargo fmt` before committing. Config lives in `.rustfmt.toml`.
- `cargo clippy --workspace --all-targets -- -D warnings` **must pass** before merging.
- Naming: `snake_case` (function, variable, module), `PascalCase` (type, trait, enum variant), `SCREAMING_SNAKE_CASE` (const).
- **English-only** — all code comments, doc comments (`///`), and any written content in the codebase **must be in English**. Do not write Vietnamese (or any other non-English language) in code. This is a hard rule with zero exceptions (see AGENTS.md Core principle 6).
- Group imports, separated by a blank line:

```rust
use std::sync::Arc;

use gpui::*;
use gpui_component::{button::*, *};

use crate::state::AppState;
```

## Rust coding guidelines

### General principles

* Prioritize correctness, readability, and maintainability over cleverness or premature optimization.
* Write idiomatic Rust. Prefer standard library types and patterns whenever practical.
* Follow the existing architecture and coding style of the crate. Consistency is more valuable than personal preference.
* Prefer extending existing code over introducing new abstractions.
* Keep changes localized. Avoid unrelated refactoring while implementing a feature or fixing a bug.
* Optimize only after profiling identifies a real bottleneck.
* Do not preserve backward compatibility. Remove obsolete paths instead of adding compatibility layers, fallbacks, or migrations

### Crate organization

* A crate should represent a single domain or capability.

  Prefer:

      editor
      workspace
      language
      project

  Instead of:

      models
      services
      helpers

* Prefer extending an existing crate before creating a new one.
* Dependencies should flow in one direction. If two crates depend on each other, extract the shared functionality into a new crate.
* Keep crate boundaries clear. Avoid exposing implementation details across crates.
* Extract reusable functionality into a dedicated crate only after multiple crates need it.

### Module organization

* Organize modules around domain concepts rather than implementation details.

  Prefer:

      workspace.rs
      workspace_settings.rs
      workspace_tests.rs

  Instead of:

      settings.rs
      tests.rs
      helpers.rs

* A module should represent one primary concept.
* Prefer extending an existing module before creating a new one.
* Avoid creating folders that contain only a single source file.
* Do not use `mod.rs` unless the module naturally contains multiple related files.
* Keep related types, implementations, helper functions, and tests close together.

### File organization

* A file should have one primary responsibility.
* Split files by concept rather than by implementation type.
* Split large files because responsibilities diverge, not because they exceed an arbitrary number of lines.

  Prefer:

      project.rs
      project_search.rs
      project_settings.rs

  Instead of:

      project_part1.rs
      project_part2.rs

* Keep `lib.rs` and `mod.rs` focused on module declarations and public exports. Business logic belongs elsewhere.
* Prefer local helper functions over creating `helper.rs`, `common.rs`, or `utils.rs`.
* If helper code becomes reusable across multiple modules, extract a dedicated module with a descriptive name.

### Naming

* Name files, modules, types, and functions after business concepts.
* Prefer descriptive names over generic names.

  Prefer:

      Workspace
      Project
      Selection
      Diagnostics

  Instead of:

      Manager
      Processor
      Helper
      Common
      Util

* Avoid abbreviations unless they are widely understood.
* Name functions after what they do, not how they do it.
* Name boolean variables so they read naturally in conditions.

### Public APIs

* Keep public APIs intentionally small.
* Prefer private visibility by default.
* Prefer `pub(crate)` over `pub` when wider visibility is unnecessary.
* Design APIs that are difficult to misuse.
* Expose behavior instead of internal implementation details.

### Types and design

* Prefer domain-specific types over primitive values whenever practical.
* Prefer structs over tuples for structured data.
* Prefer enums over boolean parameters.

  Prefer:

      enum SaveMode {
          Normal,
          Force,
      }

  Instead of:

      save(force: bool)

* Prefer composition over unnecessary abstraction.
* Avoid introducing traits until multiple implementations or generic behavior are required.
* Constructors should return fully initialized, valid objects.
* Avoid partially initialized state.
* Group related parameters into configuration structs when function signatures become difficult to understand.
* Small duplication is preferable to premature abstraction.

### Functions

* Keep functions focused on one responsibility.
* Extract helper functions when logical steps have meaningful names.
* Prefer early returns over deeply nested control flow.

  Prefer:

      if !condition {
          return;
      }

      do_work();

* Prefer pattern matching when working with enums or state machines.
* Avoid long parameter lists. Introduce a domain type when appropriate.
* Prefer explicit control flow over clever one-liners.

### Ownership and borrowing

* Prefer borrowing over cloning.
* Clone only when ownership requires it or when it significantly simplifies the implementation.
* Keep mutable borrows as short as possible.
* Prefer immutable data. Keep mutable state localized.
* Avoid unnecessary shared ownership with `Rc` or `Arc`.
* Pass dependencies explicitly rather than relying on global state.

### Collections

* Choose collection types intentionally based on access patterns.
* Prefer iterator adapters when they improve readability.
* Prefer explicit loops when iterator chains become difficult to understand.
* Avoid allocating intermediate collections unless necessary.
* Prefer `HashMap`, `BTreeMap`, `Vec`, and other standard collections unless a specialized collection provides clear value.

### Error handling

* Avoid `unwrap()`, `expect()`, and `panic!()` in production code.
* Prefer propagating errors with `?`.
* Add meaningful context when propagating errors.
* Return domain-specific errors whenever practical.
* Error messages should explain what failed and why.
* Handle errors where enough context exists to produce a useful message.

### Async and concurrency

* Keep async boundaries explicit.
* Spawn background tasks only when ownership and lifetime are well understood.
* Avoid holding locks across `.await`.
* Share immutable data whenever possible.
* Keep synchronization scopes as small as practical.

### State management

* Minimize mutable state.
* Keep state ownership obvious.
* Avoid global mutable state.
* Prefer explicit state transitions over implicit side effects.
* Make invalid states difficult to represent.

### Documentation and comments

* Comments should explain why, not what.

  Good:

      // The server requires monotonically increasing IDs.

  Bad:

      // Increment the counter.

* Document assumptions, invariants, and ownership expectations when they are not obvious.
* Every `unsafe` block must explain why it is safe and which invariants are maintained.
* Remove outdated comments when changing code.

### Testing

* Add tests for every new behavior.
* Add a regression test for every bug fix.
* Keep unit tests in the same module as the code they verify.

  Prefer:

      workspace.rs
      workspace_tests.rs

  or, for small modules:

      workspace.rs
          #[cfg(test)]
          mod tests { ... }

  Instead of:

      tests/
          workspace.rs

* Unit tests should remain part of the module they test so they can access private items (`use super::*`) without expanding the crate's public API.
* Do not change item visibility (`pub`, `pub(crate)`, etc.) solely to make code testable.
* Keep production files focused. When unit tests become substantial, move them into a sibling `*_tests.rs` file using:

      // workspace.rs
      #[cfg(test)]
      mod workspace_tests;

* Use the `tests/` directory only for integration tests that exercise the crate through its public API.
* Integration tests should verify interactions between modules/crates rather than internal implementation details.
* Organize tests by feature or module. Avoid large catch-all test files.
* Each test should verify one behavior.
* Prefer deterministic tests.
* Extract reusable fixtures only after they become repetitive.

### Performance

* Measure before optimizing.
* Prefer readable code over micro-optimizations.
* Optimize algorithms before optimizing syntax.
* Avoid unnecessary allocations and cloning in hot paths.

### Final checklist

* Before considering work complete, ensure:

  - `cargo fmt` passes.
  - `cargo clippy` passes without introducing unnecessary `#[allow]` attributes.
  - `cargo test` passes.
  - New behavior is covered by tests.
  - New code follows existing crate conventions.
  - Public APIs remain minimal.
  - No unnecessary files, modules, or abstractions were introduced.
