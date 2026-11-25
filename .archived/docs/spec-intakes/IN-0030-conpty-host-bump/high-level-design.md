# High-Level Design: ConPTY host bump mechanism

Intake: IN-0030
Lane: normal
Date: 2026-09-12

## Idea

Keep bundling Windows Terminal's `conpty.dll` + `x64/OpenConsole.exe`, but stop treating them as
mystery blobs. One PowerShell script owns the pair: it downloads a named version of the
`Microsoft.Windows.Console.ConPTY` NuGet package, verifies it, installs the x64 pair into
`crates/app/assets/`, and writes `crates/app/assets/conpty-manifest.json` describing exactly what
landed. `scripts/third-party-notices.py` generates `THIRD-PARTY-NOTICES.md` §1 from that manifest
and re-hashes the tracked binaries while doing so, so the CI check that already runs
(`--check`) fails if anyone bumps the binaries without the script, or the manifest without the
binaries.

### Why the NuGet package

| Source | Ships both files as a matched pair | Versioned, stable URL | Notes |
| --- | --- | --- | --- |
| `Microsoft.Windows.Console.ConPTY` on nuget.org | yes — `runtimes/win-{x86,x64,arm64}/native/conpty.dll` + `build/native/runtimes/{x86,x64,arm64}/OpenConsole.exe` | yes — `https://api.nuget.org/v3-flatcontainer/microsoft.windows.console.conpty/<version>/…nupkg`, plus an `index.json` version list | **selected.** MIT, published by Microsoft from microsoft/terminal; the package node-pty / VS Code consume. Authenticode-signed binaries. |
| microsoft/terminal GitHub releases | only as an attached `.nupkg` asset on *some* releases (this is where the currently bundled 1.23.251216003 came from — see `reference/zed/script/bundle-windows.ps1`) | per-release asset names change, and the version list needs the GitHub API | fallback only: the same payload, less predictable addressing |
| Windows Terminal MSIX / winget install | yes, but inside a signed app package | no direct file URL | rejected: extraction depends on the installed package layout |

The pair must be matched: `conpty.dll` launches `<dll dir>\x64\OpenConsole.exe`, and the two
speak a private protocol. Any source that gives one without the other is unusable.

## Diagram

```text
 developer                       nuget.org (api.nuget.org/v3-flatcontainer)
    │  pwsh scripts/bump-conpty.ps1 -Latest | -Version x.y.z
    ├──────── GET index.json ───────────────▶ version list (previews filtered out)
    ├──────── GET ….nupkg ──────────────────▶ zip
    │            │ open zip (integrity), extract 2 entries
    │            │ Get-AuthenticodeSignature == Valid && O=Microsoft Corporation
    │            ▼
    │   crates/app/assets/conpty.dll
    │   crates/app/assets/x64/OpenConsole.exe
    │   crates/app/assets/conpty-manifest.json   {version, source, date, per-file sha256 + file version}
    │
    └─ python scripts/third-party-notices.py  ──▶ THIRD-PARTY-NOTICES.md §1

 CI / ci-local (any OS, offline)
    python scripts/third-party-notices.py --check
       ├─ re-hash the two tracked assets, compare with the manifest   → exit 1 on drift
       └─ compare the regenerated notice with the committed one       → exit 1 on drift

 build (unchanged)
    crates/app/build.rs (x86_64 only) ──▶ target/<profile>/{conpty.dll, x64/OpenConsole.exe}
    alacritty_terminal tty/windows/conpty.rs: LoadLibrary("conpty.dll") next to the exe,
                                              else kernel32 (inbox conhost)
```

## UI Wireframe

`N/A — no UI surface`. The change is build-time packaging and developer tooling.

## Data Flow

1. The developer runs `pwsh scripts/bump-conpty.ps1 -Latest` (or `-Version <x.y.z>`).
2. `-Latest` reads the flat-container `index.json` and takes the last version with no `-` suffix
   (pre-releases such as `1.25.…-preview` are skipped).
3. The `.nupkg` is downloaded to the temp directory. Opening it as a zip and extracting the two
   entries is the download's integrity check; a truncated or corrupt file fails here, before
   anything is written into the repository.
4. Each extracted binary must have a `Valid` Authenticode signature whose signer subject contains
   `O=Microsoft Corporation`; otherwise the script throws and nothing is installed.
5. The pair is copied into `crates/app/assets/`, and the manifest is rewritten with the package
   id, version, download URL, project URL, licence, date, and per-file `entry`, `file_version`
   and `sha256`.
6. The script prints `old -> new` per file and reminds the developer to regenerate the notice.
7. `python scripts/third-party-notices.py` renders §1 from the manifest; the same code path under
   `--check` is what CI runs.
8. Nothing else changes: `build.rs` copies whatever is in `assets/`, so the next build picks the
   new host up.

## Detail Design

- [x] Detail design: not needed
- Reason: one script, one JSON file, and ~40 lines of generator change; the behaviour is fully
  described above and proved by `-Check` plus the CI notice check.
