# DEC-0013 OneTerm bundles the Windows Terminal ConPTY pair and bumps it only through the script

Date: 2026-09-12

## Status

Accepted

## Context

On Windows, local shells run through ConPTY. Two implementations are available:

- the OS one — `kernel32!CreatePseudoConsole`, served by the inbox `conhost.exe`; and
- Windows Terminal's — `conpty.dll` next to `oneterm.exe`, which launches
  `x64\OpenConsole.exe` from the same directory.

OneTerm has bundled the second since the beginning (`crates/app/assets/`, copied by
`crates/app/build.rs`). Dropping the bundle was tried on `fix/conpty-inbox` (`4c8d510`,
unmerged) and rejected: the inbox `conhost.exe` on Windows 11 10.0.26100.1 swallows the Sixel
DCS, so images stopped rendering in local shells. The inbox host also ties VT fidelity to
whatever Windows build the user happens to run, and it lags OpenConsole by months.

Keeping the bundle has its own cost: two redistributed Microsoft binaries (~1.1 MB) whose
provenance, version and licence must be recorded, and which nobody knew how to update — the
current pair was copied by hand from Zed's `bundle-windows.ps1`, and a known OpenConsole defect
(1.23.2512 drops one byte per 32 KiB `WriteFile` inside a DCS —
`docs/spec-intakes/IN-0028-sixel-graphics/high-level-design.md`) had no upgrade path.

## Decision

1. OneTerm keeps bundling `conpty.dll` + `x64/OpenConsole.exe` as a **matched pair** from the
   same upstream package version. They are shipped for `x86_64` Windows targets only.
2. The `kernel32` ConPTY stays the fallback: the vendored `alacritty_terminal`
   `tty/windows/conpty.rs` loads `conpty.dll` if it is next to the executable and otherwise uses
   the OS one, so a build without the assets (aarch64, a stripped install) still works — with
   the inbox host's fidelity.
3. The pair is changed **only** by `pwsh scripts/bump-conpty.ps1`, which takes the version from
   the `Microsoft.Windows.Console.ConPTY` NuGet package (MIT, published by Microsoft from
   microsoft/terminal), refuses binaries that are not validly Authenticode-signed by Microsoft,
   and records package, version, source URL, date, per-file SHA-256 and file version in
   `crates/app/assets/conpty-manifest.json`.
4. That manifest is the single source for `THIRD-PARTY-NOTICES.md` §1. Generating the notice
   re-hashes the tracked binaries, so `python scripts/third-party-notices.py --check` — already
   part of CI and `scripts/ci-local.*` — fails when binaries and manifest drift apart. No new
   CI step is introduced.
5. Rollback is `git checkout <rev> -- crates/app/assets`, not the script: old package versions
   are not guaranteed to stay on the NuGet flat container (1.23.251216003 is not there; it was
   only ever a GitHub release asset).

`DEC-0005 Terminate only OneTerm's own OpenConsole.exe before a Windows update` stays
**accepted** and in force: because the install directory keeps hosting an `OpenConsole.exe`, the
updater must still terminate only the console hosts whose image path lies inside OneTerm's own
install directory.

## Alternatives

- [x] Selected: bundle the pair, script the bump, record it in a manifest that CI verifies.
- [ ] Ship nothing and use the OS ConPTY (`fix/conpty-inbox`) — rejected by the owner: the inbox
  conhost drops Sixel, and VT support becomes a function of the user's Windows build.
- [ ] Keep bundling but update by hand — rejected: that is the status quo that left the tree on a
  version with a known DCS byte-loss bug and no record of where the files came from.
- [ ] Download the pair at build time instead of tracking it — rejected: makes the build
  network-dependent and non-reproducible, and CI could no longer verify the exact shipped bytes.
- [ ] Track the `win-x86` / `win-arm64` copies too — rejected as unused: `build.rs` copies the
  pair for `x86_64` only, and an arm64 build deliberately falls back to the OS ConPTY.

## Consequences

- [x] Benefit to confirm: a bump is one command plus one regeneration, and the licence notice,
  digests and version follow automatically (confirmed by the 1.23.2512.16003 →
  1.24.2607.10001 bump in `US-0070`).
- [ ] Tradeoff or follow-up to address: OneTerm keeps redistributing ~1.1 MB of Microsoft
  binaries and stays responsible for their MIT notice; the pair must be re-bumped when upstream
  fixes matter. The 32 KiB DCS byte loss is **not** fixed by 1.24.2607.10001 (re-checked during
  the `US-0070` walk), so that defect still waits for a later host. Only x86_64 gets the bundled
  host, so arm64 fidelity still follows the user's Windows build.
