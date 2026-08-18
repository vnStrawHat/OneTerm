# Evolvability Review

**Score: 6.0 / 10**

## EVOLVE-01 — The intended layered refactor is a strong foundation but is not closed

- **Files:** `docs/refactor/ui-crate-restructure.md:155-208,505-536`; `crates/app/src/init.rs`; current crate manifests
- **Modules:** Workspace evolution plan
- **Severity:** **Medium**
- **Explanation:** The repository has a concrete migration plan and has already extracted core, terminal, settings, state, workspace, and feature crates. However, `oneterm-ui` remains an undocumented shared fork and “done” criteria still describe removing `crates/ui`.
- **Why it matters:** Half-completed migrations create two sources of truth and make future refactors riskier.
- **Recommended solution:** Close the current migration with a decision record: retain and govern `oneterm-ui`, or remove it. Update completion criteria and delete obsolete paths/docs.

## EVOLVE-02 — Monolithic terminal interface makes future capability additions expensive

- **Files:** `crates/terminal/src/session.rs:155-407`; `crates/terminal/src/contracts.rs:46-151`
- **Modules:** Terminal capability API
- **Severity:** **High**
- **Explanation:** New features must be added to a large trait implemented by local, SSH, and fake sessions. Optional SFTP/cwd/network methods are already embedded as defaults.
- **Why it matters:** Every future backend or test double inherits unrelated surface area, and API changes create broad churn.
- **Recommended solution:** Implement capability traits as actual boundaries with object-safe composition or a small session aggregate. Keep optional capabilities discoverable without forcing local sessions to implement SFTP.

## EVOLVE-03 — Hidden globals constrain future multi-window/plugin support

- **Files:** `crates/terminal/src/factory.rs:48-58`; `crates/state/src/commands.rs`; `crates/app/src/init.rs:20-61`
- **Modules:** Runtime services and commands
- **Severity:** **Medium**
- **Explanation:** A process-wide factory and GPUI globals make current startup simple but assume one application composition and one backend implementation.
- **Why it matters:** Supporting profiles, multiple app contexts, plugins, headless tests, or embedded use will require invasive global replacement.
- **Recommended solution:** Move service ownership to an app entity/context and keep global lookup as a compatibility layer. Define explicit lifecycle/ownership for services.

## EVOLVE-04 — No persistence schema/version/recovery strategy

- **Files:** `crates/settings/src/{terminal_config/mod.rs,ui_config.rs}`; `crates/session-ui/src/session_state.rs`; `crates/workspace/src/layout/workspace/persistence.rs`
- **Modules:** User data formats
- **Severity:** **Medium/High**
- **Explanation:** JSON structs use serde defaults, but there is no explicit schema version/migration pipeline for terminal, UI, sessions, or docks. Parse errors generally fall back to defaults/empty state.
- **Why it matters:** Future field changes can lose user settings or make older layouts unrecoverable.
- **Recommended solution:** Add versioned envelopes and migration functions. Preserve unknown fields where feasible, backup before migration, and test old fixtures through current versions.

## EVOLVE-05 — The local UI fork is a long-term maintenance commitment without a delta contract

- **Files:** `crates/ui/src/dock/*`; `crates/ui/Cargo.toml`; `docs/agents/dependencies.md`
- **Modules:** UI infrastructure
- **Severity:** **Medium**
- **Explanation:** A local dock fork enables necessary patches, but the repository does not identify the exact upstream delta or rebase cadence.
- **Why it matters:** Upstream component updates and security fixes become difficult to adopt safely.
- **Recommended solution:** Keep the fork minimal, document every patch and rationale, pin the source commit, and add a periodic upstream-diff review.

## EVOLVE-06 — Feature ownership and initialization are good extension points

- **Files:** `crates/app/src/init.rs:28-61`; feature `lib.rs` files; `crates/workspace/src/layout/workspace/*`
- **Modules:** Feature registration/composition
- **Severity:** **Strength**
- **Explanation:** Feature crates self-register panels/globals and the app composes them. The shell calls named panels and commands rather than importing feature internals.
- **Why it matters:** A new feature can be added without making the shell depend on its implementation, provided the actual graph/docs remain aligned.
- **Recommended solution:** Preserve this pattern; add compile-time/integration verification for registration order and panel names.

## Evolvability strengths

- Domain types are placed in lower crates and backend protocols are hidden behind traits.
- Design documents include explicit phased migration and risk sections.
- The app composition root localizes omniscient knowledge.
- The terminal engine is GPUI-free, enabling non-UI tests and future frontends.
