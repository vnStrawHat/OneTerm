# Work: Bump the bundled ConPTY pair with a script and a manifest

ID: US-0070
Intake: IN-0030
Created: 2026-09-12

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: maintenance
- Risk lane: normal
- Spec Intake, when required: `IN-0030`

## Outcome

`crates/app/assets/conpty.dll` and `crates/app/assets/x64/OpenConsole.exe` can be moved to a
different upstream version with one command, the result is described by a manifest that the
licence notice is generated from, and the existing CI gate fails when the binaries and that
manifest disagree. The mechanism is used once to move from 1.23.2512.16003 to the newest
published stable version.

## Scope

- [x] In scope: `scripts/bump-conpty.ps1` (bump + `-Check`), `crates/app/assets/conpty-manifest.json`,
  generating `THIRD-PARTY-NOTICES.md` §1 from the manifest inside `scripts/third-party-notices.py`,
  the actual version bump, and the docs that describe the bundle.
- [x] Out of scope: `crates/app/build.rs` (the copy + half-written-file guard from `BUG-0053`
  stays as is), the release scripts, any Rust code, arm64/x86 variants of the pair.

## Acceptance

- [x] `pwsh scripts/bump-conpty.ps1 -Version <x.y.z>` / `-Latest` installs a matched pair from an
  authoritative, versioned source and refuses anything not validly signed by Microsoft.
- [x] `crates/app/assets/conpty-manifest.json` records version, source URL, download date, and
  per-file SHA-256 + file version.
- [x] `pwsh scripts/bump-conpty.ps1 -Check` exits 0 when the assets match the manifest and
  non-zero when they do not.
- [x] `THIRD-PARTY-NOTICES.md` §1 (version, digests, source) is generated from the manifest; a
  bump plus `python scripts/third-party-notices.py` is all it takes to update the notice.
- [x] The quality gate catches asset/manifest drift on every platform without a new CI step.
- [x] The tracked pair is bumped to the newest published stable version, and the previous pair
  stays recoverable from git.
- [x] `pwsh scripts/ci-local.ps1` passes.
- [x] A GUI walk on the bumped build shows the bundled host serving a local shell, with echo,
  interrupt, resize reflow and Sixel behaving at least as well as before the bump.

## Documentation

### Owning Docs Reviewed

- `docs/decisions/DEC-0005-terminate-only-oneterm-s-own.md` — the updater must terminate only
  install-directory `OpenConsole.exe` processes. Still in force; re-affirmed by `DEC-0013`.
- `docs/spec-intakes/IN-0020-build-rewrites-conpty-assets/` (`BUG-0053`) — the copy in
  `build.rs` must stay atomic (temp file + rename) because a running OneTerm has the files
  open. Unchanged by this work.
- `docs/spec-intakes/IN-0028-sixel-graphics/high-level-design.md` — records the OpenConsole
  1.23.2512 32 KiB DCS byte-loss in its risk table; this is the defect a bump might fix.
- `docs/license-analysis.md` §7 — compliance checklist for the redistributed MIT binaries.
- `THIRD-PARTY-NOTICES.md` §1 + `scripts/third-party-notices.py` — where the bundle is declared.
- `docs/agents/structure.md` — lists `crates/app/assets/` contents and what `build.rs` does.
- `docs/PROJECT.md` — stack section describes the Windows local PTY.
- `scripts/README.md` — script inventory and the quality-gate table.

### Documentation Action

Update required:

- `THIRD-PARTY-NOTICES.md` (regenerated), `scripts/third-party-notices.py` (header text),
- `scripts/README.md` (new script + what the notices check now covers),
- `docs/license-analysis.md` §7 (point at the manifest as the record of the bundled version),
- `docs/agents/structure.md` (`assets/` now contains the manifest),
- `docs/decisions/DEC-0013-bundled-conpty-host-and-bump-script.md` (new),
- `docs/spec-intakes/IN-0028-sixel-graphics/high-level-design.md` (risk row: the host version it
  refers to has moved).

`docs/PROJECT.md` needs no change: it already describes Windows local PTY as ConPTY without
naming a host version, and the decision it points to is unaffected.

### Reconciliation

Docs changed: `THIRD-PARTY-NOTICES.md`, `scripts/third-party-notices.py`, `scripts/README.md`,
`docs/license-analysis.md`, `docs/agents/structure.md`, `docs/decisions/DEC-0013-…md` (new),
`docs/spec-intakes/IN-0028-sixel-graphics/high-level-design.md`, plus this intake's
`IN-0030.md` / `high-level-design.md` / this packet. `docs/PROJECT.md` unchanged for the reason
above. `docs/decisions/DEC-0005-terminate-only-oneterm-s-own.md` re-read and left accepted.

## Context

- `crates/app/build.rs` copies the pair into `target/<profile>/` for `x86_64` targets only, via a
  temp-file-plus-rename that never rewrites an identical file (`BUG-0053`); an aarch64 build
  ships nothing and falls back to the OS ConPTY.
- The vendored `alacritty_terminal` `tty/windows/conpty.rs` `LoadLibrary`s `conpty.dll` from the
  executable's directory and falls back to `kernel32`.
- `conpty.dll` starts `<dll dir>\x64\OpenConsole.exe`, so the two files must always come from the
  same package version.
- The previously bundled 1.23.251216003 came from Zed's `reference/zed/script/bundle-windows.ps1`,
  which downloads the same NuGet package from a microsoft/terminal GitHub release asset.

## Plan

- [x] Confirm the source: NuGet `Microsoft.Windows.Console.ConPTY` ships both files per
  architecture with a version list and a stable URL.
- [x] Write `scripts/bump-conpty.ps1` (`-Version` / `-Latest` / `-Check`).
- [x] Generate `THIRD-PARTY-NOTICES.md` §1 from the manifest inside the existing notices script,
  re-hashing the assets while doing so (no new CI entry).
- [x] Run the bump to the newest stable version and regenerate the notice.
- [x] Update the docs listed above.
- [x] `pwsh scripts/ci-local.ps1` and a GUI walk on a `fast-dev` build.

## Decisions

- `DEC-0013` — the pair stays bundled; bumps go through the script and the manifest.
- `DEC-0005` (terminate only OneTerm's own `OpenConsole.exe`) remains accepted and applies.

## Verification Plan

- `pwsh scripts/bump-conpty.ps1 -Check` on a good tree, and with a corrupted manifest.
- `python scripts/third-party-notices.py --check` on a good tree, and with a corrupted manifest.
- `pwsh scripts/ci-local.ps1`.
- GUI walk (fast-dev build in a scratch target dir): the copied assets carry the new file
  version; the shell's console host is the bundled `OpenConsole.exe`; `echo hi`; Ctrl+C on
  `ping -t 127.0.0.1`; window resize reflow; `type snake.six` renders the image; `cat snake.six`
  (Git `cat.exe`, 32 KiB writes) to see whether the DCS byte loss is still present.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [x] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Bump: `Microsoft.Windows.Console.ConPTY` **1.23.2512.16003 → 1.24.2607.10001** (package version
`1.24.260710001`, the newest non-preview on nuget.org on 2026-09-12), installed with
`pwsh scripts/bump-conpty.ps1 -Latest`. Both binaries Authenticode-verified
(`Valid`, `CN=Microsoft Corporation`). The previous pair stays in git history on `main`
(`c936ac0`).

| Check | Result |
| --- | --- |
| `pwsh scripts/bump-conpty.ps1 -Check` | `OK` for both files; exit 0 |
| same, with one manifest digest altered | `MISMATCH conpty.dll`, exit 1 |
| `python scripts/third-party-notices.py --check` | up to date; with the altered digest it prints the manifest/disk digests and exits 1 |
| `pwsh scripts/ci-local.ps1` | **all checks passed** — 45 test binaries, 1131 passed, 0 failed, 5 ignored (raw `test result` lines) |
| fast-dev build | `target/fast-dev/{conpty.dll, x64/OpenConsole.exe}` both `1.24.2607.10001` |
| GUI walk | `evidence/gui-walk.md` — bundled `OpenConsole.exe` hosts the shell; `echo`/`ver`; ping interrupted, shell and app alive; `type snake.six` renders; resize reflows |

Gaps:

- **The 32 KiB DCS byte loss is not fixed by 1.24.** `cat snake.six` (Git `cat.exe`) still shows
  the same speckled bands as on 1.23 (`evidence/US-0070-sixel-cat-byte-loss.png` vs
  `../IN-0028-sixel-graphics/evidence/US-0067-rework-conpty-cat-byte-loss.png`). The comparison
  is visual; the raw-capture byte measurement from `IN-0028` was not repeated. The bump is kept
  anyway — nothing regressed, and the mechanism is the deliverable.
- **Ctrl+C was sent as a console `CTRL_C_EVENT`, not as a keystroke**: the workstation was locked
  during the walk, so real input could not be injected (posted key messages do not carry modifier
  state). The app's own Ctrl+C path is unchanged by this work but is unverified on this build.
- `-Version <old>` cannot restore 1.23.251216003 (not on the NuGet flat container); rollback is
  `git checkout <rev> -- crates/app/assets`. Not exercised.
- Not walked: a release build, the Windows self-update path with the new pair, arm64 (ships no
  bundled host), SSH sessions (no ConPTY in that path).
- The script was only run against nuget.org; the GitHub-release fallback source is documented in
  the high-level design but not implemented.

## Handoff

Branch `feat/conpty-bump`, not merged and not pushed.
