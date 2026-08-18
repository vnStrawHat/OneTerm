# Third-party notices

OneTerm redistributes the following prebuilt binaries in its Windows x64
release packages. Rust crate dependencies are covered by `cargo deny` and
`docs/license-analysis.md`; this file lists only vendored binaries.

## Windows Terminal ConPTY (`conpty.dll`, `x64/OpenConsole.exe`)

| Field | Value |
|---|---|
| Source | <https://github.com/microsoft/terminal> (Windows Terminal), `src/host` / `src/winconpty` |
| Version | 1.23.2512.16003 (`FileVersion` of both binaries) |
| License | MIT (see below) |
| Location in repo | `crates/app/assets/conpty.dll`, `crates/app/assets/x64/OpenConsole.exe` |
| Shipped for | `x86_64-pc-windows-msvc` only (`crates/app/build.rs` skips other targets) |
| SHA-256 `conpty.dll` | `1f5ffd52ff118db975eeb25bac0051f4ceff3e051313fa03a5afffa9e75ee502` |
| SHA-256 `OpenConsole.exe` | `6b2915a9a91c0738346a6c6a7b3ee2b74e26582b0c92b1b16066e72570dddd68` |

Purpose: `alacritty_terminal` loads `conpty.dll` from the executable directory when
present; that DLL launches `x64/OpenConsole.exe` instead of the system
`conhost.exe`, so Ctrl+C reaches only the child process (see
`docs/terminal-backend.md`).

When refreshing the binaries, update the version and hashes above
(`Get-FileHash -Algorithm SHA256`).

```text
MIT License

Copyright (c) Microsoft Corporation. All rights reserved.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
