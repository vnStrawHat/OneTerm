# High-Level Design: One home per check, one Rust setup

Intake: IN-0041
Lane: normal
Date: 2026-09-16

## Idea

A check belongs in the job whose platform or subject it actually tests, and in exactly one such
job. Today three checks are copied across the Linux and Windows quality jobs for no property
either platform has, one platform flavour of `cfg(unix)` is untested, and the Rust setup is
copy-pasted six times.

The rule this design applies:

| A check that exercises... | ...belongs in |
| --- | --- |
| platform-conditional code (`cfg(windows)`, `cfg(unix)`, the GPUI backends) | every platform job that has that code |
| a cargo feature whose `#[cfg(feature)]` sites are all platform-free | one job, the primary platform's, or the subject's own job |
| the `oneterm-vt` crate as an outward artifact (package, API surface, features, rustdoc) | `vt-package` |

Applied:

- **`terminal-diagnostics` clippy** is a feature gate over `crates/local-shell`,
  `crates/ssh` and `crates/terminal-view`, none of it platform-conditional. It stays on
  Windows, the primary platform, and leaves the Linux job.
- **`vt-paranoid`** is the whole-history grid integrity walk. Nothing in it is
  platform-conditional; `oneterm-vt`'s only `cfg(windows)` / `cfg(unix)` code is `src/pty/`,
  which `cargo test --workspace` already exercises on Linux and on Windows. It leaves both
  quality jobs for `vt-package`.
- **`regex`** is a `oneterm-vt` feature too, so it follows `vt-paranoid` into `vt-package`,
  the job that already builds the crate in three feature configurations.
- **macOS** gains `-p oneterm-vt`. `BUG-0063` is the evidence: `libc::sigset_t` is a `u32`
  alias on macOS and a `{ __val: [c_ulong; 16] }` struct on glibc, so "it compiles on one Unix"
  is not "it compiles on Unix". macOS is the flavour nothing currently builds.
- **The setup triple** — read the pinned channel, `dtolnay/rust-toolchain`,
  `Swatinem/rust-cache` — becomes `.github/actions/setup-rust`, a local composite action with
  one optional `components` input. The pinned SHAs are carried over character for character;
  the point is one place to change them, not a change to them.

Deliberately **not** done: `needs:` edges between jobs (owner declined — serialising the matrix
costs more wall-clock than the duplication did), and any change to the step set of
`scripts/ci-local.ps1` / `scripts/ci-local.sh`, which run on one machine and must stay a
superset of every job.

## Diagram

```text
before                                   after
------                                   -----
workspace-quality   (ubuntu)             workspace-quality   (ubuntu)
  setup x3 inline                          uses: ./.github/actions/setup-rust
  fmt                                      fmt
  clippy                                   clippy
  clippy +terminal-diagnostics  --\
  test --workspace                  \      test --workspace
  test vt +vt-paranoid  -------\     \
  test vt +regex  ----------\   \     \
                             \   \     \
vt-package          (ubuntu)  \   \     \  vt-package        (ubuntu)
  setup x3 inline              \   \     \   uses: ./.github/actions/setup-rust
  feature matrix build          \   \     \  feature matrix build
                                 >---+-------> test vt +vt-paranoid
                                /    |         test vt +regex
  six-leaf dep check           /     |       six-leaf dep check
  headless / doc / api / package     |       headless / doc / api / package
                                     |
windows-quality  (windows)           |     windows-quality   (windows)
  setup x3 inline                    |       uses: ./.github/actions/setup-rust
  clippy                             |       clippy
  clippy +terminal-diagnostics <-----+       clippy +terminal-diagnostics
  test --workspace                           test --workspace
  test vt +vt-paranoid  (dropped here)

macos-tests      (macos)                   macos-tests       (macos)
  setup x3 inline                            uses: ./.github/actions/setup-rust
  test -p core,terminal,local-shell,ssh      test ... ssh -p oneterm-vt

vt-bench (windows) / vt-esctest (ubuntu)   both: uses: ./.github/actions/setup-rust
  setup x3 inline                          steps otherwise unchanged
```

## UI Wireframe

N/A — no UI surface. The change is confined to CI configuration.

## Data Flow

1. A push or pull request matching the existing `paths:` filters starts all eight jobs in
   parallel; no `needs:` edge is added, so the matrix shape is unchanged.
2. Each of the six Rust jobs runs `./.github/actions/setup-rust`, which reads `channel` out of
   `rust-toolchain.toml`, installs exactly that toolchain plus the components the job asked for,
   and restores that job's cargo cache.
3. Each check then runs in its single owning job, per the table above.
4. The local twin, `scripts/ci-local.*`, still runs the union of every job's checks on one
   machine, so a contributor's gate is unchanged by any of this.

## Detail Design

- [ ] Detail design: not needed
- Reason: normal lane; one workflow file plus one composite action, both fully described by the
  table and diagram above. There is no code to shape.
