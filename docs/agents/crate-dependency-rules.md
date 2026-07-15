# Crate & dependency rules — OneTerm

> Split out from [`structure.md`](structure.md) § 3.1. This file is the **authoritative**
> list of the hard rules governing OneTerm's crates and the dependencies between them.
> For the per-crate responsibility table and the directory tree, see
> [`structure.md`](structure.md) § 3 and § 1.

These are **hard rules**. A change that violates any of them must not be merged.
They keep the graph acyclic, keep protocol code out of the UI, and keep the shell
independent of the features.

## Layers (dependencies point **down only**)

```
L4  app                      ← the only omniscient crate (wires everything)
    │
L3  workspace (shell) │ terminal-view · sftp-ui · session-ui · settings-ui (features) │ ssh · local-shell (backends)
    │
L2  state · theme            (shared runtime state / theming)
    │
L1  actions · settings       (gpui action structs / config load-save)
    │
L0  core · terminal · highlight   (domain + engine + leaf; core = pure domain)
```

An edge `A → B` (A depends on B) is allowed **only** when B sits in a strictly
lower layer — with the single explicit same-layer exception noted in R5
(`session-ui → terminal-view`). Same-layer and upward edges are otherwise forbidden.

## Invariants

| # | Rule | Rationale | How to verify |
|---|---|---|---|
| **R1** | **No dependency cycle.** The crate graph is a DAG. | Cyclic crates cannot compile/scale. | `cargo build --workspace` (cargo errors on cycles); `cargo tree -e normal` shows a tree, not a back-edge. |
| **R2** | **Dependencies point down only** (higher layer → lower layer). No same-layer or upward edges except R5. | Predictable, testable layering. | Inspect `[dependencies]`; cross-check against the table in [`structure.md`](structure.md) § 3. |
| **R3** | **No UI→backend edge.** No UI crate (shell **or** any feature) may depend on `ssh` / `local-shell`. Only `app` depends on the backends. UI creates sessions through `oneterm_terminal::SessionFactory` (installed by `app`). | Keeps protocol code out of the UI; lets features stay backend-agnostic and testable. | `cargo tree -i oneterm-ssh -e normal` and `cargo tree -i oneterm-local-shell -e normal` must each list **only `oneterm-app`**. |
| **R4** | **The shell is feature-agnostic.** `workspace` MUST NOT depend on any `*-ui` feature crate or backend. It builds panels **by name** (gpui-component `PanelRegistry`) and drives features via the `oneterm_state::commands::WorkspaceCommands` fn-pointer registry. | The shell must not know which features exist. | `cargo tree -p oneterm-workspace -e normal` shows no `*-ui`, `oneterm-ssh`, or `oneterm-local-shell`. |
| **R5** | **Features do not cross-depend.** A feature crate MUST NOT depend on another feature's internals — with the **single allowed edge** `session-ui → terminal-view` (opening an SSH session spawns a `TerminalPanel`). Shared cross-feature logic goes in `state`. | Prevents a feature tangle; keeps the one legitimate edge explicit. | `cargo tree -p <feature> -e normal`: the only `*-ui` dep permitted is `session-ui → terminal-view`. |
| **R6** | **`core` is pure domain.** No `gpui`, no `gpui-component`, no `alacritty_terminal`. Types + traits (`AppError`, `SftpBackend`, `SshConfig`, `LocalShellConfig`) only. | The domain must not pull in UI or a specific terminal engine. | `cargo tree -p oneterm-core -e normal` shows no `gpui*` and no `alacritty_terminal`. |
| **R7** | **The engine is gpui-free.** `terminal` is alacritty-coupled but MUST NOT depend on `gpui` / `gpui-component`. | The engine is reusable and unit-testable without a UI. | `cargo tree -p oneterm-terminal -e normal` shows no `gpui*`. |
| **R8** | **Backends implement traits only.** `ssh` / `local-shell` depend on **only** `core` + `terminal` (+ their protocol crates); they implement `TerminalSession` / `SftpBackend` and depend on **no** UI crate. | Backends are swappable behind trait objects. | `cargo tree -p oneterm-ssh -e normal` / `-p oneterm-local-shell` show no UI crate. |
| **R9** | **`app` is the only omniscient crate.** Only `app` may depend on backends + features + shell together. It installs `AppSessionFactory`, runs each feature's `init()`, and assembles `WorkspaceCommands`. | Single wiring point; everyone else stays layered. | Only `crates/app/Cargo.toml` lists a backend **and** a feature crate. |
| **R10** | **New shared types go in the lowest crate that needs them.** A type used by two features/shell belongs in `core` / `terminal` / `settings` / `state` (whichever is lowest and fits), never duplicated. | Avoids duplicate/divergent types and up-edges. | Review: is the new type reachable from the lowest common layer? |
| **R11** | **Naming.** Package name = `oneterm-<dir>`; inside a backend the `core` re-export is aliased `use oneterm_core as core`. New path crates are workspace members and listed in root `members`. | Consistency + discoverability. | Check `crates/<x>/Cargo.toml` `name` and root `Cargo.toml` `members`. |
| **R12** | **Feature self-registration.** Each feature owns its `init(cx)` that registers its dock panels + feature globals; `app::init` calls them. Neither the shell nor another feature registers a feature's panels. | Feature encapsulation; saved layouts deserialize by panel name. | Panels are registered in the owning feature's `lib.rs init()`. |

## Full-graph verification (one shot)

```bash
cargo build --workspace                                   # R1 (no cycle)
cargo tree -i oneterm-ssh -e normal                       # R3: only oneterm-app
cargo tree -i oneterm-local-shell -e normal               # R3: only oneterm-app
cargo tree -p oneterm-core   -e normal                    # R6: no gpui*, no alacritty_terminal
cargo tree -p oneterm-terminal -e normal                  # R7: no gpui*
cargo tree -p oneterm-workspace -e normal                 # R4: no *-ui / backend
```

> When adding or moving a crate, re-run the commands above and update the
> responsibility table in [`structure.md`](structure.md) § 3 and the tree in § 1.
