# Third-party notices

OneTerm is licensed under the Apache License 2.0 (see [`LICENSE`](LICENSE) and
[`NOTICE`](NOTICE)). The distributed binaries also contain or redistribute the
third-party components below. This file is **generated** by
`python scripts/third-party-notices.py` (CI verifies it with `--check`); edit the
script's header text or the manifests, not this file.

## 1. Bundled non-Rust components (Windows releases)

| Component | Version | Source | Licence |
|---|---|---|---|
| `conpty.dll` | 1.23.2512.16003 (Windows Terminal) | <https://github.com/microsoft/terminal> | MIT — Copyright (c) Microsoft Corporation |
| `x64/OpenConsole.exe` | 1.23.2512.16003 (Windows Terminal) | <https://github.com/microsoft/terminal> | MIT — Copyright (c) Microsoft Corporation |

`crates/app/build.rs` copies both files from `crates/app/assets/` next to `oneterm.exe`
so ConPTY uses Windows Terminal's console host instead of the system `conhost.exe`
(correct Ctrl+C delivery). They are unmodified upstream binaries; SHA-256 of the tracked
copies: `conpty.dll` `1f5ffd52ff118db975eeb25bac0051f4ceff3e051313fa03a5afffa9e75ee502`,
`OpenConsole.exe` `6b2915a9a91c0738346a6c6a7b3ee2b74e26582b0c92b1b16066e72570dddd68`.

MIT licence text (Windows Terminal):

> Copyright (c) Microsoft Corporation.
>
> Permission is hereby granted, free of charge, to any person obtaining a copy of this
> software and associated documentation files (the "Software"), to deal in the Software
> without restriction, including without limitation the rights to use, copy, modify,
> merge, publish, distribute, sublicense, and/or sell copies of the Software, and to
> permit persons to whom the Software is furnished to do so, subject to the following
> conditions: The above copyright notice and this permission notice shall be included
> in all copies or substantial portions of the Software. THE SOFTWARE IS PROVIDED
> "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
> TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
> NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
> CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
> OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
> DEALINGS IN THE SOFTWARE.

## 2. Vendored and patched forks (`vendor/`)

| Crate | Upstream | Base revision | Licence | OneTerm delta |
|---|---|---|---|---|
| `vte` 0.15.0 | <https://crates.io/crates/vte> | 0.15.0 | Apache-2.0 OR MIT | `vendor/patches/vte/` |
| `alacritty_terminal` 0.26.1-dev | <https://github.com/zed-industries/alacritty> | `fcf32feacb367b75ec84dd40f041e4fd411d3cc1` | Apache-2.0 | `vendor/patches/alacritty_terminal/` |
| `gpui-component` 0.5.2 | <https://github.com/longbridge/gpui-component> | `ea6b194db04cc7c0474851f07c7d5b7a9df6a98b` | Apache-2.0 | `vendor/patches/gpui-component/` |

Each fork is pristine upstream plus the listed patch set (`vendor/README.md`); the
upstream `LICENSE-*` files are kept inside each vendored tree.

## 3. Copyleft analysis

Three Zed crates (`zlog`, `ztracing`, `ztracing_macro`) declare `GPL-3.0-or-later`.
`ztracing`/`ztracing_macro` are dual-licensed (Apache-2.0 chosen); `zlog` is GPL-only
but is compiled only behind `cfg(ztracing)` / `cfg(test)` and is not linked into the
release binary. Details and verification method: `docs/license-analysis.md`;
enforcement: `deny.toml` (`cargo deny check licenses`, CI job `cargo-deny`).

`crates/tools/src/bin/doom-fire.rs` (developer diagnostic, **not** part of any release)
is a Rust port of DOOM-fire-zig and keeps that project's GPL-3.0 licence; the
`oneterm-tools` package therefore declares `Apache-2.0 AND GPL-3.0-only`.

## 4. Rust crates linked into the release binary

Every crate reachable from `oneterm-app` (normal + build dependencies) for the release
targets in `deny.toml`. Licence = the SPDX expression declared by the crate; source =
crates.io unless a git URL is shown. Full licence texts ship inside each crate's
package in the Cargo registry / git checkout.


| Crate | Version | Licence | Source |
|---|---|---|---|
| `accesskit` | 0.24.1 | MIT OR Apache-2.0 | crates.io |
| `accesskit_atspi_common` | 0.18.1 | MIT OR Apache-2.0 | crates.io |
| `accesskit_consumer` | 0.35.0 | MIT OR Apache-2.0 | crates.io |
| `accesskit_consumer` | 0.36.0 | MIT OR Apache-2.0 | crates.io |
| `accesskit_consumer` | 0.37.0 | MIT OR Apache-2.0 | crates.io |
| `accesskit_macos` | 0.26.2 | MIT OR Apache-2.0 | crates.io |
| `accesskit_unix` | 0.21.1 | MIT OR Apache-2.0 | crates.io |
| `accesskit_windows` | 0.32.1 | MIT OR Apache-2.0 | crates.io |
| `addr2line` | 0.25.1 | Apache-2.0 OR MIT | crates.io |
| `adler2` | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | crates.io |
| `aead` | 0.6.1 | MIT OR Apache-2.0 | crates.io |
| `aes` | 0.8.4 | MIT OR Apache-2.0 | crates.io |
| `aes` | 0.9.1 | MIT OR Apache-2.0 | crates.io |
| `aes-gcm` | 0.11.0-rc.4 | Apache-2.0 OR MIT | crates.io |
| `ahash` | 0.8.12 | MIT OR Apache-2.0 | crates.io |
| `aho-corasick` | 1.1.4 | Unlicense OR MIT | crates.io |
| `alacritty_terminal` | 0.26.1-dev | Apache-2.0 | vendored fork (vendor/, see section 2) |
| `aligned` | 0.4.3 | MIT OR Apache-2.0 | crates.io |
| `aligned-vec` | 0.6.4 | MIT | crates.io |
| `allocator-api2` | 0.2.21 | MIT OR Apache-2.0 | crates.io |
| `anstream` | 1.0.0 | MIT OR Apache-2.0 | crates.io |
| `anstyle` | 1.0.14 | MIT OR Apache-2.0 | crates.io |
| `anstyle-parse` | 1.0.0 | MIT OR Apache-2.0 | crates.io |
| `anstyle-query` | 1.1.5 | MIT OR Apache-2.0 | crates.io |
| `anstyle-wincon` | 3.0.11 | MIT OR Apache-2.0 | crates.io |
| `anyhow` | 1.0.104 | MIT OR Apache-2.0 | crates.io |
| `ar_archive_writer` | 0.5.2 | Apache-2.0 WITH LLVM-exception | crates.io |
| `arc-swap` | 1.9.1 | MIT OR Apache-2.0 | crates.io |
| `arg_enum_proc_macro` | 0.3.4 | MIT | crates.io |
| `argon2` | 0.6.0-rc.8 | MIT OR Apache-2.0 | crates.io |
| `arrayref` | 0.3.9 | BSD-2-Clause | crates.io |
| `arrayvec` | 0.7.6 | MIT OR Apache-2.0 | crates.io |
| `as-raw-xcb-connection` | 1.0.1 | MIT OR Apache-2.0 | crates.io |
| `as-slice` | 0.2.1 | MIT OR Apache-2.0 | crates.io |
| `ash` | 0.38.0+1.3.281 | MIT OR Apache-2.0 | crates.io |
| `ashpd` | 0.13.11 | MIT | crates.io |
| `async-broadcast` | 0.7.2 | MIT OR Apache-2.0 | crates.io |
| `async-channel` | 1.9.0 | Apache-2.0 OR MIT | crates.io |
| `async-channel` | 2.5.0 | Apache-2.0 OR MIT | crates.io |
| `async-compression` | 0.4.42 | MIT OR Apache-2.0 | crates.io |
| `async-executor` | 1.14.0 | Apache-2.0 OR MIT | crates.io |
| `async-fs` | 2.2.0 | Apache-2.0 OR MIT | crates.io |
| `async-global-executor` | 2.4.1 | Apache-2.0 OR MIT | crates.io |
| `async-io` | 2.6.0 | Apache-2.0 OR MIT | crates.io |
| `async-lock` | 3.4.2 | Apache-2.0 OR MIT | crates.io |
| `async-net` | 2.0.0 | Apache-2.0 OR MIT | crates.io |
| `async-process` | 2.5.0 | Apache-2.0 OR MIT | crates.io |
| `async-recursion` | 1.1.1 | MIT OR Apache-2.0 | crates.io |
| `async-signal` | 0.2.14 | Apache-2.0 OR MIT | crates.io |
| `async-std` | 1.13.2 | Apache-2.0 OR MIT | crates.io |
| `async-tar` | 0.5.1 | MIT/Apache-2.0 | crates.io |
| `async-task` | 4.7.1 | Apache-2.0 OR MIT | crates.io |
| `async-trait` | 0.1.89 | MIT OR Apache-2.0 | crates.io |
| `async_zip` | 0.0.18 | MIT | crates.io |
| `atomic` | 0.5.3 | Apache-2.0/MIT | crates.io |
| `atomic-waker` | 1.1.2 | Apache-2.0 OR MIT | crates.io |
| `atspi` | 0.29.0 | Apache-2.0 OR MIT | crates.io |
| `atspi-common` | 0.13.0 | Apache-2.0 OR MIT | crates.io |
| `atspi-proxies` | 0.13.0 | Apache-2.0 OR MIT | crates.io |
| `autocfg` | 1.5.1 | Apache-2.0 OR MIT | crates.io |
| `av-scenechange` | 0.14.1 | MIT | crates.io |
| `av1-grain` | 0.2.5 | BSD-2-Clause | crates.io |
| `avif-serialize` | 0.8.9 | BSD-3-Clause | crates.io |
| `backtrace` | 0.3.76 | MIT OR Apache-2.0 | crates.io |
| `base16ct` | 1.0.0 | Apache-2.0 OR MIT | crates.io |
| `base62` | 2.2.4 | MIT | crates.io |
| `base64` | 0.22.1 | MIT OR Apache-2.0 | crates.io |
| `base64ct` | 1.8.3 | Apache-2.0 OR MIT | crates.io |
| `bcrypt-pbkdf` | 0.11.0 | MIT OR Apache-2.0 | crates.io |
| `bindgen` | 0.71.1 | BSD-3-Clause | crates.io |
| `bit-set` | 0.8.0 | Apache-2.0 OR MIT | crates.io |
| `bit-set` | 0.9.1 | Apache-2.0 OR MIT | crates.io |
| `bit-vec` | 0.8.0 | Apache-2.0 OR MIT | crates.io |
| `bit-vec` | 0.9.1 | Apache-2.0 OR MIT | crates.io |
| `bit_field` | 0.10.3 | Apache-2.0/MIT | crates.io |
| `bitflags` | 1.3.2 | MIT/Apache-2.0 | crates.io |
| `bitflags` | 2.13.0 | MIT OR Apache-2.0 | crates.io |
| `bitstream-io` | 4.10.0 | MIT/Apache-2.0 | crates.io |
| `blake2` | 0.11.0-rc.6 | MIT OR Apache-2.0 | crates.io |
| `block` | 0.1.6 | MIT | crates.io |
| `block-buffer` | 0.10.4 | MIT OR Apache-2.0 | crates.io |
| `block-buffer` | 0.12.1 | MIT OR Apache-2.0 | crates.io |
| `block-padding` | 0.3.3 | MIT OR Apache-2.0 | crates.io |
| `block-padding` | 0.4.2 | MIT OR Apache-2.0 | crates.io |
| `block2` | 0.5.1 | MIT | crates.io |
| `block2` | 0.6.2 | MIT | crates.io |
| `blocking` | 1.6.2 | Apache-2.0 OR MIT | crates.io |
| `blowfish` | 0.10.0 | MIT OR Apache-2.0 | crates.io |
| `borsh` | 1.7.0 | MIT OR Apache-2.0 | crates.io |
| `bstr` | 1.12.1 | MIT OR Apache-2.0 | crates.io |
| `built` | 0.8.1 | MIT | crates.io |
| `bumpalo` | 3.20.3 | MIT OR Apache-2.0 | crates.io |
| `bytemuck` | 1.25.0 | Zlib OR Apache-2.0 OR MIT | crates.io |
| `bytemuck_derive` | 1.10.2 | Zlib OR Apache-2.0 OR MIT | crates.io |
| `byteorder` | 1.5.0 | Unlicense OR MIT | crates.io |
| `byteorder-lite` | 0.1.0 | Unlicense OR MIT | crates.io |
| `bytes` | 1.12.0 | MIT | crates.io |
| `bzip2` | 0.6.1 | MIT OR Apache-2.0 | crates.io |
| `calloop` | 0.14.4 | MIT | crates.io |
| `calloop-wayland-source` | 0.4.1 | MIT | crates.io |
| `cbc` | 0.1.2 | MIT OR Apache-2.0 | crates.io |
| `cbc` | 0.2.1 | MIT OR Apache-2.0 | crates.io |
| `cbindgen` | 0.28.0 | MPL-2.0 | crates.io |
| `cc` | 1.2.64 | MIT OR Apache-2.0 | crates.io |
| `cexpr` | 0.6.0 | Apache-2.0/MIT | crates.io |
| `cfg-if` | 1.0.4 | MIT OR Apache-2.0 | crates.io |
| `cfg_aliases` | 0.2.1 | MIT | crates.io |
| `cgl` | 0.3.2 | MIT / Apache-2.0 | crates.io |
| `chacha20` | 0.10.1 | MIT OR Apache-2.0 | crates.io |
| `chrono` | 0.4.45 | MIT OR Apache-2.0 | crates.io |
| `cipher` | 0.4.4 | MIT OR Apache-2.0 | crates.io |
| `cipher` | 0.5.2 | MIT OR Apache-2.0 | crates.io |
| `clang-sys` | 1.8.1 | Apache-2.0 | crates.io |
| `cmov` | 0.5.4 | Apache-2.0 OR MIT | crates.io |
| `cocoa` | 0.25.0 | MIT OR Apache-2.0 | crates.io |
| `cocoa` | 0.26.0 | MIT OR Apache-2.0 | crates.io |
| `cocoa-foundation` | 0.1.2 | MIT OR Apache-2.0 | crates.io |
| `cocoa-foundation` | 0.2.0 | MIT OR Apache-2.0 | crates.io |
| `codespan-reporting` | 0.13.1 | Apache-2.0 | crates.io |
| `collections` | 0.1.0 | Apache-2.0 | https://github.com/zed-industries/zed |
| `color_quant` | 1.1.0 | MIT | crates.io |
| `colorchoice` | 1.0.5 | MIT OR Apache-2.0 | crates.io |
| `command-fds` | 0.3.3 | Apache-2.0 | crates.io |
| `compression-codecs` | 0.4.38 | MIT OR Apache-2.0 | crates.io |
| `compression-core` | 0.4.32 | MIT OR Apache-2.0 | crates.io |
| `concurrent-queue` | 2.5.0 | Apache-2.0 OR MIT | crates.io |
| `const-oid` | 0.10.2 | Apache-2.0 OR MIT | crates.io |
| `const-random` | 0.1.18 | MIT OR Apache-2.0 | crates.io |
| `const-random-macro` | 0.1.16 | MIT OR Apache-2.0 | crates.io |
| `convert_case` | 0.10.0 | MIT | crates.io |
| `convert_case` | 0.11.0 | MIT | crates.io |
| `core-foundation` | 0.10.0 | MIT OR Apache-2.0 | crates.io |
| `core-foundation` | 0.9.4 | MIT OR Apache-2.0 | crates.io |
| `core-foundation-sys` | 0.8.7 | MIT OR Apache-2.0 | crates.io |
| `core-graphics` | 0.23.2 | MIT OR Apache-2.0 | crates.io |
| `core-graphics` | 0.24.0 | MIT OR Apache-2.0 | crates.io |
| `core-graphics-helmer-fork` | 0.24.0 | MIT OR Apache-2.0 | crates.io |
| `core-graphics-types` | 0.1.3 | MIT OR Apache-2.0 | crates.io |
| `core-graphics-types` | 0.2.0 | MIT OR Apache-2.0 | crates.io |
| `core-graphics2` | 0.5.2 | MIT OR Apache-2.0 | crates.io |
| `core-text` | 21.0.0 | MIT OR Apache-2.0 | crates.io |
| `core-video` | 0.5.2 | MIT OR Apache-2.0 | crates.io |
| `core_maths` | 0.1.1 | MIT | crates.io |
| `cosmic-text` | 0.19.0 | MIT OR Apache-2.0 | crates.io |
| `cpubits` | 0.1.1 | MIT OR Apache-2.0 | crates.io |
| `cpufeatures` | 0.2.17 | MIT OR Apache-2.0 | crates.io |
| `cpufeatures` | 0.3.0 | MIT OR Apache-2.0 | crates.io |
| `crash-context` | 0.8.0 | MIT | crates.io |
| `crash-handler` | 0.8.0 | MIT OR Apache-2.0 | crates.io |
| `crc32fast` | 1.5.0 | MIT OR Apache-2.0 | crates.io |
| `crossbeam-deque` | 0.8.6 | MIT OR Apache-2.0 | crates.io |
| `crossbeam-epoch` | 0.9.20 | MIT OR Apache-2.0 | crates.io |
| `crossbeam-queue` | 0.3.12 | MIT OR Apache-2.0 | crates.io |
| `crossbeam-utils` | 0.8.21 | MIT OR Apache-2.0 | crates.io |
| `crunchy` | 0.2.4 | MIT | crates.io |
| `crypto-bigint` | 0.7.5 | Apache-2.0 OR MIT | crates.io |
| `crypto-common` | 0.1.7 | MIT OR Apache-2.0 | crates.io |
| `crypto-common` | 0.2.2 | MIT OR Apache-2.0 | crates.io |
| `crypto-primes` | 0.7.2 | Apache-2.0 OR MIT | crates.io |
| `ctor` | 1.0.7 | Apache-2.0 OR MIT | crates.io |
| `ctr` | 0.10.1 | MIT OR Apache-2.0 | crates.io |
| `ctutils` | 0.4.2 | Apache-2.0 OR MIT | crates.io |
| `cursor-icon` | 1.2.0 | MIT OR Apache-2.0 OR Zlib | crates.io |
| `curve25519-dalek` | 5.0.0-rc.0 | BSD-3-Clause | crates.io |
| `curve25519-dalek-derive` | 0.1.1 | MIT/Apache-2.0 | crates.io |
| `dashmap` | 6.2.1 | MIT | crates.io |
| `data-encoding` | 2.11.0 | MIT | crates.io |
| `data-url` | 0.3.2 | MIT OR Apache-2.0 | crates.io |
| `deflate64` | 0.1.12 | MIT | crates.io |
| `defmt` | 1.1.0 | MIT OR Apache-2.0 | crates.io |
| `defmt-macros` | 1.1.0 | MIT OR Apache-2.0 | crates.io |
| `defmt-parser` | 1.0.0 | MIT OR Apache-2.0 | crates.io |
| `delegate` | 0.13.5 | MIT OR Apache-2.0 | crates.io |
| `der` | 0.8.0 | Apache-2.0 OR MIT | crates.io |
| `derive_more` | 2.1.1 | MIT | crates.io |
| `derive_more-impl` | 2.1.1 | MIT | crates.io |
| `derive_refineable` | 0.1.0 | Apache-2.0 | https://github.com/zed-industries/zed |
| `des` | 0.9.0 | MIT OR Apache-2.0 | crates.io |
| `digest` | 0.10.7 | MIT OR Apache-2.0 | crates.io |
| `digest` | 0.11.3 | MIT OR Apache-2.0 | crates.io |
| `dirs` | 6.0.0 | MIT OR Apache-2.0 | crates.io |
| `dirs-sys` | 0.5.0 | MIT OR Apache-2.0 | crates.io |
| `dispatch` | 0.2.0 | MIT | crates.io |
| `dispatch2` | 0.3.1 | Zlib OR Apache-2.0 OR MIT | crates.io |
| `displaydoc` | 0.2.6 | MIT OR Apache-2.0 | crates.io |
| `dlib` | 0.5.3 | MIT | crates.io |
| `document-features` | 0.2.12 | MIT OR Apache-2.0 | crates.io |
| `downcast-rs` | 1.2.1 | MIT/Apache-2.0 | crates.io |
| `dunce` | 1.0.5 | CC0-1.0 OR MIT-0 OR Apache-2.0 | crates.io |
| `dwrote` | 0.11.5 | MPL-2.0 | crates.io |
| `dyn-clone` | 1.0.20 | MIT OR Apache-2.0 | crates.io |
| `ecdsa` | 0.17.0-rc.18 | Apache-2.0 OR MIT | crates.io |
| `ed25519` | 3.0.0 | Apache-2.0 OR MIT | crates.io |
| `ed25519-dalek` | 3.0.0-rc.0 | BSD-3-Clause | crates.io |
| `either` | 1.16.0 | MIT OR Apache-2.0 | crates.io |
| `elliptic-curve` | 0.14.0-rc.33 | Apache-2.0 OR MIT | crates.io |
| `embed-resource` | 3.0.9 | MIT | crates.io |
| `encoding_rs` | 0.8.35 | (Apache-2.0 OR MIT) AND BSD-3-Clause | crates.io |
| `endi` | 1.1.1 | MIT | crates.io |
| `enum-iterator` | 2.3.0 | 0BSD | crates.io |
| `enum-iterator-derive` | 1.5.0 | 0BSD | crates.io |
| `enum_dispatch` | 0.3.13 | MIT OR Apache-2.0 | crates.io |
| `enumflags2` | 0.7.12 | MIT OR Apache-2.0 | crates.io |
| `enumflags2_derive` | 0.7.12 | MIT OR Apache-2.0 | crates.io |
| `env_filter` | 2.0.0 | MIT OR Apache-2.0 | crates.io |
| `env_logger` | 0.11.11 | MIT OR Apache-2.0 | crates.io |
| `equator` | 0.4.2 | MIT | crates.io |
| `equator-macro` | 0.4.2 | MIT | crates.io |
| `equivalent` | 1.0.2 | Apache-2.0 OR MIT | crates.io |
| `erased-serde` | 0.4.10 | MIT OR Apache-2.0 | crates.io |
| `errno` | 0.3.14 | MIT OR Apache-2.0 | crates.io |
| `etagere` | 0.2.15 | MIT/Apache-2.0 | crates.io |
| `euclid` | 0.22.14 | MIT OR Apache-2.0 | crates.io |
| `event-listener` | 2.5.3 | Apache-2.0 OR MIT | crates.io |
| `event-listener` | 5.4.1 | Apache-2.0 OR MIT | crates.io |
| `event-listener-strategy` | 0.5.4 | Apache-2.0 OR MIT | crates.io |
| `exr` | 1.74.0 | BSD-3-Clause | crates.io |
| `fastrand` | 1.9.0 | Apache-2.0 OR MIT | crates.io |
| `fastrand` | 2.4.1 | Apache-2.0 OR MIT | crates.io |
| `fax` | 0.2.7 | MIT | crates.io |
| `fdeflate` | 0.3.7 | MIT OR Apache-2.0 | crates.io |
| `ff` | 0.14.0 | MIT/Apache-2.0 | crates.io |
| `fiat-crypto` | 0.3.0 | MIT OR Apache-2.0 OR BSD-1-Clause | crates.io |
| `filedescriptor` | 0.8.3 | MIT | crates.io |
| `filetime` | 0.2.29 | MIT/Apache-2.0 | crates.io |
| `find-msvc-tools` | 0.1.9 | MIT OR Apache-2.0 | crates.io |
| `fixedbitset` | 0.5.7 | MIT OR Apache-2.0 | crates.io |
| `flate2` | 1.1.9 | MIT OR Apache-2.0 | crates.io |
| `float-cmp` | 0.9.0 | MIT | crates.io |
| `float-ord` | 0.3.2 | MIT / Apache-2.0 | crates.io |
| `float_next_after` | 1.0.0 | MIT | crates.io |
| `fluent-uri` | 0.1.4 | MIT | crates.io |
| `flume` | 0.11.1 | Apache-2.0/MIT | crates.io |
| `fnv` | 1.0.7 | Apache-2.0 / MIT | crates.io |
| `foldhash` | 0.1.5 | Zlib | crates.io |
| `foldhash` | 0.2.0 | Zlib | crates.io |
| `font-types` | 0.11.3 | MIT OR Apache-2.0 | crates.io |
| `fontconfig-parser` | 0.5.8 | MIT | crates.io |
| `fontdb` | 0.23.0 | MIT | crates.io |
| `foreign-types` | 0.5.0 | MIT/Apache-2.0 | crates.io |
| `foreign-types-macros` | 0.2.3 | MIT/Apache-2.0 | crates.io |
| `foreign-types-shared` | 0.3.1 | MIT/Apache-2.0 | crates.io |
| `form_urlencoded` | 1.2.2 | MIT OR Apache-2.0 | crates.io |
| `freetype-sys` | 0.20.1 | MIT | crates.io |
| `fsevent-sys` | 4.1.0 | MIT | crates.io |
| `futf` | 0.1.5 | MIT / Apache-2.0 | crates.io |
| `futures` | 0.3.32 | MIT OR Apache-2.0 | crates.io |
| `futures-channel` | 0.3.32 | MIT OR Apache-2.0 | crates.io |
| `futures-concurrency` | 7.7.1 | MIT OR Apache-2.0 | crates.io |
| `futures-core` | 0.3.32 | MIT OR Apache-2.0 | crates.io |
| `futures-executor` | 0.3.32 | MIT OR Apache-2.0 | crates.io |
| `futures-io` | 0.3.32 | MIT OR Apache-2.0 | crates.io |
| `futures-lite` | 1.13.0 | Apache-2.0 OR MIT | crates.io |
| `futures-lite` | 2.6.1 | Apache-2.0 OR MIT | crates.io |
| `futures-macro` | 0.3.32 | MIT OR Apache-2.0 | crates.io |
| `futures-sink` | 0.3.32 | MIT OR Apache-2.0 | crates.io |
| `futures-task` | 0.3.32 | MIT OR Apache-2.0 | crates.io |
| `futures-util` | 0.3.32 | MIT OR Apache-2.0 | crates.io |
| `generic-array` | 0.14.7 | MIT | crates.io |
| `generic-array` | 1.4.3 | MIT | crates.io |
| `gethostname` | 1.1.0 | Apache-2.0 | crates.io |
| `getrandom` | 0.2.17 | MIT OR Apache-2.0 | crates.io |
| `getrandom` | 0.3.4 | MIT OR Apache-2.0 | crates.io |
| `getrandom` | 0.4.3 | MIT OR Apache-2.0 | crates.io |
| `ghash` | 0.6.0 | Apache-2.0 OR MIT | crates.io |
| `gif` | 0.13.3 | MIT OR Apache-2.0 | crates.io |
| `gif` | 0.14.2 | MIT OR Apache-2.0 | crates.io |
| `gimli` | 0.32.3 | MIT OR Apache-2.0 | crates.io |
| `gl_generator` | 0.14.0 | Apache-2.0 | crates.io |
| `glob` | 0.3.3 | MIT OR Apache-2.0 | crates.io |
| `globset` | 0.4.18 | Unlicense OR MIT | crates.io |
| `globwalk` | 0.8.1 | MIT | crates.io |
| `glow` | 0.17.0 | MIT OR Apache-2.0 OR Zlib | crates.io |
| `glutin_wgl_sys` | 0.6.1 | Apache-2.0 | crates.io |
| `gpu-allocator` | 0.28.0 | MIT OR Apache-2.0 | crates.io |
| `gpu-descriptor` | 0.3.2 | MIT OR Apache-2.0 | crates.io |
| `gpu-descriptor-types` | 0.2.0 | MIT OR Apache-2.0 | crates.io |
| `gpui` | 0.2.2 | Apache-2.0 | https://github.com/zed-industries/zed |
| `gpui-component` | 0.5.2 | Apache-2.0 | vendored fork (vendor/, see section 2) |
| `gpui-component-assets` | 0.5.1 | Apache-2.0 | https://github.com/longbridge/gpui-component |
| `gpui-component-macros` | 0.5.1 | Apache-2.0 | https://github.com/longbridge/gpui-component |
| `gpui_linux` | 0.1.0 | Apache-2.0 | https://github.com/zed-industries/zed |
| `gpui_macos` | 0.1.0 | Apache-2.0 | https://github.com/zed-industries/zed |
| `gpui_macros` | 0.1.0 | Apache-2.0 | https://github.com/zed-industries/zed |
| `gpui_platform` | 0.1.0 | Apache-2.0 | https://github.com/zed-industries/zed |
| `gpui_shared_string` | 0.1.0 | Apache-2.0 (no manifest field; LICENSE-APACHE in the Zed monorepo) | https://github.com/zed-industries/zed |
| `gpui_util` | 0.1.0 | Apache-2.0 (no manifest field; LICENSE-APACHE in the Zed monorepo) | https://github.com/zed-industries/zed |
| `gpui_wgpu` | 0.1.0 | Apache-2.0 | https://github.com/zed-industries/zed |
| `gpui_windows` | 0.1.0 | Apache-2.0 | https://github.com/zed-industries/zed |
| `grid` | 1.0.1 | MIT | crates.io |
| `group` | 0.14.0 | MIT/Apache-2.0 | crates.io |
| `h2` | 0.4.15 | MIT | crates.io |
| `half` | 2.7.1 | MIT OR Apache-2.0 | crates.io |
| `harfrust` | 0.5.2 | MIT | crates.io |
| `hash32` | 0.3.1 | MIT OR Apache-2.0 | crates.io |
| `hashbrown` | 0.14.5 | MIT OR Apache-2.0 | crates.io |
| `hashbrown` | 0.15.5 | MIT OR Apache-2.0 | crates.io |
| `hashbrown` | 0.16.1 | MIT OR Apache-2.0 | crates.io |
| `hashbrown` | 0.17.1 | MIT OR Apache-2.0 | crates.io |
| `heapless` | 0.9.3 | MIT OR Apache-2.0 | crates.io |
| `heck` | 0.4.1 | MIT OR Apache-2.0 | crates.io |
| `heck` | 0.5.0 | MIT OR Apache-2.0 | crates.io |
| `hex` | 0.4.3 | MIT OR Apache-2.0 | crates.io |
| `hex-literal` | 1.1.0 | MIT OR Apache-2.0 | crates.io |
| `hexf-parse` | 0.2.1 | CC0-1.0 | crates.io |
| `hkdf` | 0.12.4 | MIT OR Apache-2.0 | crates.io |
| `hkdf` | 0.13.0 | MIT OR Apache-2.0 | crates.io |
| `hmac` | 0.12.1 | MIT OR Apache-2.0 | crates.io |
| `hmac` | 0.13.0 | MIT OR Apache-2.0 | crates.io |
| `home` | 0.5.12 | MIT OR Apache-2.0 | crates.io |
| `html5ever` | 0.27.0 | MIT OR Apache-2.0 | crates.io |
| `http` | 1.4.2 | MIT OR Apache-2.0 | crates.io |
| `http-body` | 1.0.1 | MIT | crates.io |
| `http-body-util` | 0.1.3 | MIT | crates.io |
| `http_client` | 0.1.0 | Apache-2.0 | https://github.com/zed-industries/zed |
| `httparse` | 1.10.1 | MIT OR Apache-2.0 | crates.io |
| `hybrid-array` | 0.4.12 | MIT OR Apache-2.0 | crates.io |
| `hyper` | 1.10.1 | MIT | crates.io |
| `hyper-rustls` | 0.27.9 | Apache-2.0 OR ISC OR MIT | crates.io |
| `hyper-util` | 0.1.20 | MIT | crates.io |
| `iana-time-zone` | 0.1.65 | MIT OR Apache-2.0 | crates.io |
| `icu_collections` | 2.2.0 | Unicode-3.0 | crates.io |
| `icu_locale_core` | 2.2.0 | Unicode-3.0 | crates.io |
| `icu_normalizer` | 2.2.0 | Unicode-3.0 | crates.io |
| `icu_normalizer_data` | 2.2.0 | Unicode-3.0 | crates.io |
| `icu_properties` | 2.2.0 | Unicode-3.0 | crates.io |
| `icu_properties_data` | 2.2.0 | Unicode-3.0 | crates.io |
| `icu_provider` | 2.2.0 | Unicode-3.0 | crates.io |
| `idna` | 1.1.0 | MIT OR Apache-2.0 | crates.io |
| `idna_adapter` | 1.2.2 | Apache-2.0 OR MIT | crates.io |
| `ignore` | 0.4.26 | Unlicense OR MIT | crates.io |
| `image` | 0.25.10 | MIT OR Apache-2.0 | crates.io |
| `image-webp` | 0.2.4 | MIT OR Apache-2.0 | crates.io |
| `imagesize` | 0.13.0 | MIT | crates.io |
| `imgref` | 1.12.2 | CC0-1.0 OR Apache-2.0 | crates.io |
| `indexmap` | 2.14.0 | Apache-2.0 OR MIT | crates.io |
| `inotify` | 0.10.2 | ISC | crates.io |
| `inotify-sys` | 0.1.5 | ISC | crates.io |
| `inout` | 0.1.4 | MIT OR Apache-2.0 | crates.io |
| `inout` | 0.2.2 | MIT OR Apache-2.0 | crates.io |
| `instant` | 0.1.13 | BSD-3-Clause | crates.io |
| `internal-russh-num-bigint` | 0.5.0 | MIT OR Apache-2.0 | crates.io |
| `inventory` | 0.3.24 | MIT OR Apache-2.0 | crates.io |
| `io-surface` | 0.16.1 | MIT OR Apache-2.0 | crates.io |
| `ipnet` | 2.12.0 | MIT OR Apache-2.0 | crates.io |
| `is-docker` | 0.2.0 | MIT | crates.io |
| `is-wsl` | 0.4.0 | MIT | crates.io |
| `is_terminal_polyfill` | 1.70.2 | MIT OR Apache-2.0 | crates.io |
| `itertools` | 0.11.0 | MIT OR Apache-2.0 | crates.io |
| `itertools` | 0.13.0 | MIT OR Apache-2.0 | crates.io |
| `itertools` | 0.14.0 | MIT OR Apache-2.0 | crates.io |
| `itoa` | 1.0.18 | MIT OR Apache-2.0 | crates.io |
| `jiff` | 0.2.29 | Unlicense OR MIT | crates.io |
| `jobserver` | 0.1.34 | MIT OR Apache-2.0 | crates.io |
| `keccak` | 0.2.0 | Apache-2.0 OR MIT | crates.io |
| `kem` | 0.3.0 | Apache-2.0 OR MIT | crates.io |
| `khronos-egl` | 6.0.0 | MIT/Apache-2.0 | crates.io |
| `khronos_api` | 3.1.0 | Apache-2.0 | crates.io |
| `kurbo` | 0.11.3 | Apache-2.0 OR MIT | crates.io |
| `kv-log-macro` | 1.0.7 | MIT OR Apache-2.0 | crates.io |
| `lazy_static` | 1.5.0 | MIT OR Apache-2.0 | crates.io |
| `leak` | 0.1.2 | Apache-2.0 OR MIT | crates.io |
| `leaky-cow` | 0.1.1 | MIT / Apache-2.0 | crates.io |
| `lebe` | 0.5.3 | BSD-3-Clause | crates.io |
| `libbz2-rs-sys` | 0.2.5 | bzip2-1.0.6 | crates.io |
| `libc` | 0.2.186 | MIT OR Apache-2.0 | crates.io |
| `libloading` | 0.8.9 | ISC | crates.io |
| `libm` | 0.2.16 | MIT | crates.io |
| `linebender_resource_handle` | 0.1.1 | Apache-2.0 OR MIT | crates.io |
| `link-section` | 0.18.2 | Apache-2.0 OR MIT | crates.io |
| `linktime-proc-macro` | 0.2.0 | Apache-2.0 OR MIT | crates.io |
| `linux-raw-sys` | 0.12.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | crates.io |
| `linux-raw-sys` | 0.4.15 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | crates.io |
| `litemap` | 0.8.2 | Unicode-3.0 | crates.io |
| `litrs` | 1.0.0 | MIT OR Apache-2.0 | crates.io |
| `lock_api` | 0.4.14 | MIT OR Apache-2.0 | crates.io |
| `log` | 0.4.32 | MIT OR Apache-2.0 | crates.io |
| `loop9` | 0.1.5 | MIT | crates.io |
| `lru-slab` | 0.1.2 | MIT OR Apache-2.0 OR Zlib | crates.io |
| `lsp-types` | 0.97.0 | MIT | crates.io |
| `lyon` | 1.0.19 | MIT OR Apache-2.0 | crates.io |
| `lyon_algorithms` | 1.0.20 | MIT OR Apache-2.0 | crates.io |
| `lyon_geom` | 1.0.19 | MIT OR Apache-2.0 | crates.io |
| `lyon_path` | 1.0.19 | MIT OR Apache-2.0 | crates.io |
| `lyon_tessellation` | 1.0.20 | MIT OR Apache-2.0 | crates.io |
| `mac` | 0.1.1 | MIT/Apache-2.0 | crates.io |
| `mach2` | 0.5.0 | BSD-2-Clause OR MIT OR Apache-2.0 | crates.io |
| `mach2` | 0.6.0 | BSD-2-Clause OR MIT OR Apache-2.0 | crates.io |
| `malloc_buf` | 0.0.6 | MIT | crates.io |
| `markdown` | 1.0.0 | MIT | crates.io |
| `markup5ever` | 0.12.1 | MIT OR Apache-2.0 | crates.io |
| `markup5ever_rcdom` | 0.3.0 | MIT OR Apache-2.0 | crates.io |
| `maybe-rayon` | 0.1.1 | MIT | crates.io |
| `md-5` | 0.10.6 | MIT OR Apache-2.0 | crates.io |
| `md5` | 0.8.0 | Apache-2.0/MIT | crates.io |
| `media` | 0.1.0 | Apache-2.0 | https://github.com/zed-industries/zed |
| `memchr` | 2.8.2 | Unlicense OR MIT | crates.io |
| `memmap2` | 0.9.10 | MIT OR Apache-2.0 | crates.io |
| `memoffset` | 0.9.1 | MIT | crates.io |
| `metal` | 0.33.0 | MIT OR Apache-2.0 | crates.io |
| `minimal-lexical` | 0.2.1 | MIT/Apache-2.0 | crates.io |
| `miniz_oxide` | 0.8.9 | MIT OR Zlib OR Apache-2.0 | crates.io |
| `mio` | 1.2.1 | MIT | crates.io |
| `miow` | 0.6.1 | MIT OR Apache-2.0 | crates.io |
| `ml-kem` | 0.3.2 | Apache-2.0 OR MIT | crates.io |
| `module-lattice` | 0.2.3 | Apache-2.0 OR MIT | crates.io |
| `moxcms` | 0.8.1 | BSD-3-Clause OR Apache-2.0 | crates.io |
| `naga` | 29.0.3 | MIT OR Apache-2.0 | https://github.com/zed-industries/wgpu.git |
| `nanorand` | 0.7.0 | Zlib | crates.io |
| `new_debug_unreachable` | 1.0.6 | MIT | crates.io |
| `nix` | 0.29.0 | MIT | crates.io |
| `nix` | 0.31.3 | MIT | crates.io |
| `no_std_io2` | 0.9.4 | Apache-2.0 OR MIT | crates.io |
| `nom` | 7.1.3 | MIT | crates.io |
| `nom` | 8.0.0 | MIT | crates.io |
| `noop_proc_macro` | 0.3.0 | MIT | crates.io |
| `normpath` | 1.5.1 | MIT OR Apache-2.0 | crates.io |
| `notify` | 7.0.0 | CC0-1.0 | crates.io |
| `notify-types` | 1.0.1 | MIT OR Apache-2.0 | crates.io |
| `ntapi` | 0.4.3 | Apache-2.0 OR MIT | crates.io |
| `nu-ansi-term` | 0.50.3 | MIT | crates.io |
| `num` | 0.4.3 | MIT OR Apache-2.0 | crates.io |
| `num-bigint` | 0.4.6 | MIT OR Apache-2.0 | crates.io |
| `num-bigint-dig` | 0.9.1 | MIT/Apache-2.0 | crates.io |
| `num-complex` | 0.4.6 | MIT OR Apache-2.0 | crates.io |
| `num-derive` | 0.4.2 | MIT OR Apache-2.0 | crates.io |
| `num-integer` | 0.1.46 | MIT OR Apache-2.0 | crates.io |
| `num-iter` | 0.1.45 | MIT OR Apache-2.0 | crates.io |
| `num-rational` | 0.4.2 | MIT OR Apache-2.0 | crates.io |
| `num-traits` | 0.2.19 | MIT OR Apache-2.0 | crates.io |
| `num_cpus` | 1.17.0 | MIT OR Apache-2.0 | crates.io |
| `objc` | 0.2.7 | MIT | crates.io |
| `objc-foundation` | 0.1.1 | MIT | crates.io |
| `objc-sys` | 0.3.5 | MIT | crates.io |
| `objc2` | 0.5.2 | MIT | crates.io |
| `objc2` | 0.6.4 | MIT | crates.io |
| `objc2-app-kit` | 0.2.2 | MIT | crates.io |
| `objc2-app-kit` | 0.3.2 | Zlib OR Apache-2.0 OR MIT | crates.io |
| `objc2-cloud-kit` | 0.3.2 | Zlib OR Apache-2.0 OR MIT | crates.io |
| `objc2-core-data` | 0.2.2 | MIT | crates.io |
| `objc2-core-data` | 0.3.2 | Zlib OR Apache-2.0 OR MIT | crates.io |
| `objc2-core-foundation` | 0.3.2 | Zlib OR Apache-2.0 OR MIT | crates.io |
| `objc2-core-graphics` | 0.3.2 | Zlib OR Apache-2.0 OR MIT | crates.io |
| `objc2-core-image` | 0.2.2 | MIT | crates.io |
| `objc2-core-image` | 0.3.2 | Zlib OR Apache-2.0 OR MIT | crates.io |
| `objc2-core-text` | 0.3.2 | Zlib OR Apache-2.0 OR MIT | crates.io |
| `objc2-core-video` | 0.3.2 | Zlib OR Apache-2.0 OR MIT | crates.io |
| `objc2-encode` | 4.1.0 | MIT | crates.io |
| `objc2-foundation` | 0.2.2 | MIT | crates.io |
| `objc2-foundation` | 0.3.2 | MIT | crates.io |
| `objc2-io-kit` | 0.3.2 | Zlib OR Apache-2.0 OR MIT | crates.io |
| `objc2-io-surface` | 0.3.2 | Zlib OR Apache-2.0 OR MIT | crates.io |
| `objc2-metal` | 0.2.2 | MIT | crates.io |
| `objc2-metal` | 0.3.2 | Zlib OR Apache-2.0 OR MIT | crates.io |
| `objc2-quartz-core` | 0.2.2 | MIT | crates.io |
| `objc2-quartz-core` | 0.3.2 | Zlib OR Apache-2.0 OR MIT | crates.io |
| `objc_exception` | 0.1.2 | MIT | crates.io |
| `objc_id` | 0.1.1 | MIT | crates.io |
| `object` | 0.37.3 | Apache-2.0 OR MIT | crates.io |
| `once_cell` | 1.21.4 | MIT OR Apache-2.0 | crates.io |
| `once_cell_polyfill` | 1.70.2 | MIT OR Apache-2.0 | crates.io |
| `oo7` | 0.6.0 | MIT | crates.io |
| `open` | 5.3.5 | MIT | crates.io |
| `openssl-probe` | 0.2.1 | MIT OR Apache-2.0 | crates.io |
| `option-ext` | 0.2.0 | MPL-2.0 | crates.io |
| `ordered-float` | 5.3.0 | MIT | crates.io |
| `ordered-stream` | 0.2.0 | MIT OR Apache-2.0 | crates.io |
| `p256` | 0.14.0-rc.10 | Apache-2.0 OR MIT | crates.io |
| `p384` | 0.14.0-rc.10 | Apache-2.0 OR MIT | crates.io |
| `p521` | 0.14.0-rc.10 | Apache-2.0 OR MIT | crates.io |
| `pageant` | 0.2.1 | Apache-2.0 | crates.io |
| `parking` | 2.2.1 | Apache-2.0 OR MIT | crates.io |
| `parking_lot` | 0.12.5 | MIT OR Apache-2.0 | crates.io |
| `parking_lot_core` | 0.9.12 | MIT OR Apache-2.0 | crates.io |
| `password-hash` | 0.6.1 | MIT OR Apache-2.0 | crates.io |
| `paste` | 1.0.15 | MIT OR Apache-2.0 | crates.io |
| `pastey` | 0.1.1 | MIT OR Apache-2.0 | crates.io |
| `pathdiff` | 0.2.3 | MIT/Apache-2.0 | crates.io |
| `pathfinder_geometry` | 0.5.1 | MIT/Apache-2.0 | crates.io |
| `pathfinder_simd` | 0.5.6 | MIT OR Apache-2.0 | crates.io |
| `pbkdf2` | 0.12.2 | MIT OR Apache-2.0 | crates.io |
| `pbkdf2` | 0.13.0 | MIT OR Apache-2.0 | crates.io |
| `pem-rfc7468` | 1.0.0 | Apache-2.0 OR MIT | crates.io |
| `percent-encoding` | 2.3.2 | MIT OR Apache-2.0 | crates.io |
| `perf` | 0.1.0 | Apache-2.0 | https://github.com/zed-industries/zed |
| `phc` | 0.6.1 | Apache-2.0 OR MIT | crates.io |
| `phf` | 0.11.3 | MIT | crates.io |
| `phf` | 0.13.1 | MIT | crates.io |
| `phf_codegen` | 0.11.3 | MIT | crates.io |
| `phf_generator` | 0.11.3 | MIT | crates.io |
| `phf_generator` | 0.13.1 | MIT | crates.io |
| `phf_macros` | 0.13.1 | MIT | crates.io |
| `phf_shared` | 0.11.3 | MIT | crates.io |
| `phf_shared` | 0.13.1 | MIT | crates.io |
| `pico-args` | 0.5.0 | MIT | crates.io |
| `pin-project` | 1.1.13 | Apache-2.0 OR MIT | crates.io |
| `pin-project-internal` | 1.1.13 | Apache-2.0 OR MIT | crates.io |
| `pin-project-lite` | 0.2.17 | Apache-2.0 OR MIT | crates.io |
| `pin-utils` | 0.1.0 | MIT OR Apache-2.0 | crates.io |
| `piper` | 0.2.5 | MIT OR Apache-2.0 | crates.io |
| `pkcs1` | 0.8.0-rc.4 | Apache-2.0 OR MIT | crates.io |
| `pkcs5` | 0.8.0 | Apache-2.0 OR MIT | crates.io |
| `pkcs8` | 0.11.0 | Apache-2.0 OR MIT | crates.io |
| `pkg-config` | 0.3.33 | MIT OR Apache-2.0 | crates.io |
| `png` | 0.17.16 | MIT OR Apache-2.0 | crates.io |
| `png` | 0.18.1 | MIT OR Apache-2.0 | crates.io |
| `polling` | 3.11.0 | Apache-2.0 OR MIT | crates.io |
| `pollster` | 0.2.5 | Apache-2.0/MIT | crates.io |
| `pollster` | 0.4.0 | Apache-2.0/MIT | crates.io |
| `poly1305` | 0.9.0 | Apache-2.0 OR MIT | crates.io |
| `polyval` | 0.7.1 | Apache-2.0 OR MIT | crates.io |
| `postage` | 0.5.0 | MIT | crates.io |
| `potential_utf` | 0.1.5 | Unicode-3.0 | crates.io |
| `ppv-lite86` | 0.2.21 | MIT OR Apache-2.0 | crates.io |
| `precomputed-hash` | 0.1.1 | MIT | crates.io |
| `presser` | 0.3.1 | MIT OR Apache-2.0 | crates.io |
| `prettyplease` | 0.2.37 | MIT OR Apache-2.0 | crates.io |
| `primefield` | 0.14.0 | Apache-2.0 OR MIT | crates.io |
| `primeorder` | 0.14.0-rc.10 | Apache-2.0 OR MIT | crates.io |
| `proc-macro-crate` | 3.5.0 | MIT OR Apache-2.0 | crates.io |
| `proc-macro-error-attr2` | 2.0.0 | MIT OR Apache-2.0 | crates.io |
| `proc-macro-error2` | 2.0.1 | MIT OR Apache-2.0 | crates.io |
| `proc-macro2` | 1.0.106 | MIT OR Apache-2.0 | crates.io |
| `profiling` | 1.0.18 | MIT OR Apache-2.0 | crates.io |
| `profiling-procmacros` | 1.0.18 | MIT OR Apache-2.0 | crates.io |
| `proptest` | 1.10.0 | MIT OR Apache-2.0 | https://github.com/proptest-rs/proptest |
| `proptest-macro` | 0.5.0 | MIT OR Apache-2.0 | https://github.com/proptest-rs/proptest |
| `psm` | 0.1.31 | MIT OR Apache-2.0 | crates.io |
| `pxfm` | 0.1.29 | BSD-3-Clause OR Apache-2.0 | crates.io |
| `qoi` | 0.4.1 | MIT/Apache-2.0 | crates.io |
| `quick-error` | 1.2.3 | MIT/Apache-2.0 | crates.io |
| `quick-error` | 2.0.1 | MIT/Apache-2.0 | crates.io |
| `quick-xml` | 0.30.0 | MIT | crates.io |
| `quick-xml` | 0.39.4 | MIT | crates.io |
| `quinn` | 0.11.9 | MIT OR Apache-2.0 | crates.io |
| `quinn-proto` | 0.11.14 | MIT OR Apache-2.0 | crates.io |
| `quinn-udp` | 0.5.14 | MIT OR Apache-2.0 | crates.io |
| `quote` | 1.0.45 | MIT OR Apache-2.0 | crates.io |
| `rand` | 0.10.1 | MIT OR Apache-2.0 | crates.io |
| `rand` | 0.8.6 | MIT OR Apache-2.0 | crates.io |
| `rand` | 0.9.4 | MIT OR Apache-2.0 | crates.io |
| `rand_chacha` | 0.3.1 | MIT OR Apache-2.0 | crates.io |
| `rand_chacha` | 0.9.0 | MIT OR Apache-2.0 | crates.io |
| `rand_core` | 0.10.1 | MIT OR Apache-2.0 | crates.io |
| `rand_core` | 0.6.4 | MIT OR Apache-2.0 | crates.io |
| `rand_core` | 0.9.5 | MIT OR Apache-2.0 | crates.io |
| `rand_xorshift` | 0.4.0 | MIT OR Apache-2.0 | crates.io |
| `range-alloc` | 0.1.5 | MIT OR Apache-2.0 | crates.io |
| `rangemap` | 1.7.1 | MIT/Apache-2.0 | crates.io |
| `rav1e` | 0.8.1 | BSD-2-Clause | crates.io |
| `ravif` | 0.13.0 | BSD-3-Clause | crates.io |
| `raw-window-handle` | 0.6.2 | MIT OR Apache-2.0 OR Zlib | crates.io |
| `raw-window-metal` | 1.1.0 | MIT OR Apache-2.0 | crates.io |
| `rayon` | 1.12.0 | MIT OR Apache-2.0 | crates.io |
| `rayon-core` | 1.13.0 | MIT OR Apache-2.0 | crates.io |
| `read-fonts` | 0.37.0 | MIT OR Apache-2.0 | crates.io |
| `read-fonts` | 0.39.2 | MIT OR Apache-2.0 | crates.io |
| `ref-cast` | 1.0.25 | MIT OR Apache-2.0 | crates.io |
| `ref-cast-impl` | 1.0.25 | MIT OR Apache-2.0 | crates.io |
| `refineable` | 0.1.0 | Apache-2.0 | https://github.com/zed-industries/zed |
| `regex` | 1.12.4 | MIT OR Apache-2.0 | crates.io |
| `regex-automata` | 0.4.14 | MIT OR Apache-2.0 | crates.io |
| `regex-syntax` | 0.8.11 | MIT OR Apache-2.0 | crates.io |
| `renderdoc-sys` | 1.1.0 | MIT OR Apache-2.0 | crates.io |
| `reqwest` | 0.12.28 | MIT OR Apache-2.0 | crates.io |
| `resvg` | 0.45.1 | Apache-2.0 OR MIT | crates.io |
| `rfc6979` | 0.5.0 | Apache-2.0 OR MIT | crates.io |
| `rgb` | 0.8.53 | MIT | crates.io |
| `ring` | 0.17.14 | Apache-2.0 AND ISC | crates.io |
| `ropey` | 2.0.0-beta.1 | MIT OR Apache-2.0 | crates.io |
| `roxmltree` | 0.20.0 | MIT OR Apache-2.0 | crates.io |
| `rsa` | 0.10.0-rc.18 | MIT OR Apache-2.0 | crates.io |
| `russh` | 0.61.2 | Apache-2.0 | crates.io |
| `russh-cryptovec` | 0.61.0 | Apache-2.0 | crates.io |
| `russh-sftp` | 2.3.0 | Apache-2.0 | crates.io |
| `russh-util` | 0.52.0 | Apache-2.0 | crates.io |
| `rust-embed` | 8.11.0 | MIT | crates.io |
| `rust-embed-impl` | 8.11.0 | MIT | crates.io |
| `rust-embed-utils` | 8.11.0 | MIT | crates.io |
| `rust-i18n` | 4.1.0 | MIT | crates.io |
| `rust-i18n-macro` | 4.1.0 | MIT | crates.io |
| `rust-i18n-support` | 4.1.0 | MIT | crates.io |
| `rustc-demangle` | 0.1.27 | MIT/Apache-2.0 | crates.io |
| `rustc-hash` | 1.1.0 | Apache-2.0/MIT | crates.io |
| `rustc-hash` | 2.1.2 | Apache-2.0 OR MIT | crates.io |
| `rustc_version` | 0.4.1 | MIT OR Apache-2.0 | crates.io |
| `rustix` | 0.38.44 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | crates.io |
| `rustix` | 1.1.4 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | crates.io |
| `rustix-openpty` | 0.2.0 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | crates.io |
| `rustls` | 0.23.40 | Apache-2.0 OR ISC OR MIT | crates.io |
| `rustls-native-certs` | 0.8.4 | Apache-2.0 OR ISC OR MIT | crates.io |
| `rustls-pki-types` | 1.14.1 | MIT OR Apache-2.0 | crates.io |
| `rustls-webpki` | 0.103.13 | ISC | crates.io |
| `rustversion` | 1.0.22 | MIT OR Apache-2.0 | crates.io |
| `rusty-fork` | 0.3.1 | MIT/Apache-2.0 | crates.io |
| `rustybuzz` | 0.20.1 | MIT | crates.io |
| `ryu` | 1.0.23 | Apache-2.0 OR BSL-1.0 | crates.io |
| `salsa20` | 0.11.0 | MIT OR Apache-2.0 | crates.io |
| `same-file` | 1.0.6 | Unlicense/MIT | crates.io |
| `schannel` | 0.1.29 | MIT | crates.io |
| `scheduler` | 0.1.0 | Apache-2.0 | https://github.com/zed-industries/zed |
| `schemars` | 1.2.1 | MIT | crates.io |
| `schemars_derive` | 1.2.1 | MIT | crates.io |
| `scoped-tls` | 1.0.1 | MIT/Apache-2.0 | crates.io |
| `scopeguard` | 1.2.0 | MIT OR Apache-2.0 | crates.io |
| `screencapturekit` | 0.2.8 | MIT OR Apache-2.0 | crates.io |
| `screencapturekit-sys` | 0.2.8 | MIT OR Apache-2.0 | crates.io |
| `scrypt` | 0.12.0 | MIT OR Apache-2.0 | crates.io |
| `seahash` | 4.1.0 | MIT | crates.io |
| `sec1` | 0.8.1 | Apache-2.0 OR MIT | crates.io |
| `security-framework` | 3.7.0 | MIT OR Apache-2.0 | crates.io |
| `security-framework-sys` | 2.17.0 | MIT OR Apache-2.0 | crates.io |
| `self_cell` | 1.2.2 | Apache-2.0 OR GPL-2.0-only | crates.io |
| `semver` | 1.0.28 | MIT OR Apache-2.0 | crates.io |
| `serde` | 1.0.228 | MIT OR Apache-2.0 | crates.io |
| `serde_bytes` | 0.11.19 | MIT OR Apache-2.0 | crates.io |
| `serde_core` | 1.0.228 | MIT OR Apache-2.0 | crates.io |
| `serde_derive` | 1.0.228 | MIT OR Apache-2.0 | crates.io |
| `serde_derive_internals` | 0.29.1 | MIT OR Apache-2.0 | crates.io |
| `serde_fmt` | 1.1.0 | Apache-2.0 OR MIT | crates.io |
| `serde_json` | 1.0.150 | MIT OR Apache-2.0 | crates.io |
| `serde_json_lenient` | 0.2.4 | MIT/Apache-2.0 | crates.io |
| `serde_repr` | 0.1.20 | MIT OR Apache-2.0 | crates.io |
| `serde_spanned` | 0.6.9 | MIT OR Apache-2.0 | crates.io |
| `serde_spanned` | 1.1.1 | MIT OR Apache-2.0 | crates.io |
| `serde_urlencoded` | 0.7.1 | MIT/Apache-2.0 | crates.io |
| `serde_yaml` | 0.9.34+deprecated | MIT OR Apache-2.0 | crates.io |
| `serdect` | 0.4.3 | Apache-2.0 OR MIT | crates.io |
| `sha1` | 0.11.0 | MIT OR Apache-2.0 | crates.io |
| `sha1_smol` | 1.0.1 | BSD-3-Clause | crates.io |
| `sha2` | 0.10.9 | MIT OR Apache-2.0 | crates.io |
| `sha2` | 0.11.0 | MIT OR Apache-2.0 | crates.io |
| `sha3` | 0.11.0 | MIT OR Apache-2.0 | crates.io |
| `sharded-slab` | 0.1.7 | MIT | crates.io |
| `shellexpand` | 3.1.2 | MIT/Apache-2.0 | crates.io |
| `shlex` | 1.3.0 | MIT OR Apache-2.0 | crates.io |
| `shlex` | 2.0.1 | MIT OR Apache-2.0 | crates.io |
| `signal-hook` | 0.4.4 | MIT OR Apache-2.0 | crates.io |
| `signal-hook-registry` | 1.4.8 | MIT OR Apache-2.0 | crates.io |
| `signature` | 3.0.0 | Apache-2.0 OR MIT | crates.io |
| `simd-adler32` | 0.3.9 | MIT | crates.io |
| `simd_helpers` | 0.1.0 | MIT | crates.io |
| `simplecss` | 0.2.2 | Apache-2.0 OR MIT | crates.io |
| `siphasher` | 1.0.3 | MIT/Apache-2.0 | crates.io |
| `skrifa` | 0.40.0 | MIT OR Apache-2.0 | crates.io |
| `skrifa` | 0.42.1 | MIT OR Apache-2.0 | crates.io |
| `slab` | 0.4.12 | MIT | crates.io |
| `slotmap` | 1.1.1 | Zlib | crates.io |
| `smallvec` | 1.15.2 | MIT OR Apache-2.0 | crates.io |
| `smol` | 2.0.2 | Apache-2.0 OR MIT | crates.io |
| `smol_str` | 0.3.6 | MIT OR Apache-2.0 | crates.io |
| `socket2` | 0.6.4 | MIT OR Apache-2.0 | crates.io |
| `spin` | 0.10.1 | MIT | crates.io |
| `spin` | 0.9.9 | MIT | crates.io |
| `spirv` | 0.4.0+sdk-1.4.341.0 | Apache-2.0 | crates.io |
| `spki` | 0.8.0 | Apache-2.0 OR MIT | crates.io |
| `ssh-cipher` | 0.3.0-rc.9 | Apache-2.0 OR MIT | crates.io |
| `ssh-encoding` | 0.3.0-rc.9 | Apache-2.0 OR MIT | crates.io |
| `ssh-key` | 0.7.0-rc.10 | Apache-2.0 OR MIT | crates.io |
| `stable_deref_trait` | 1.2.1 | MIT OR Apache-2.0 | crates.io |
| `stacker` | 0.1.24 | MIT OR Apache-2.0 | crates.io |
| `stacksafe` | 0.1.4 | Apache-2.0 | crates.io |
| `stacksafe-macro` | 0.1.4 | Apache-2.0 | crates.io |
| `static_assertions` | 1.1.0 | MIT OR Apache-2.0 | crates.io |
| `str_indices` | 0.4.4 | MIT OR Apache-2.0 | crates.io |
| `streaming-iterator` | 0.1.9 | MIT OR Apache-2.0 | crates.io |
| `strict-num` | 0.1.1 | MIT | crates.io |
| `string_cache` | 0.8.9 | MIT OR Apache-2.0 | crates.io |
| `string_cache_codegen` | 0.5.4 | MIT OR Apache-2.0 | crates.io |
| `strum` | 0.27.2 | MIT | crates.io |
| `strum_macros` | 0.27.2 | MIT | crates.io |
| `subtle` | 2.6.1 | BSD-3-Clause | crates.io |
| `sum_tree` | 0.1.0 | Apache-2.0 | https://github.com/zed-industries/zed |
| `sval` | 2.20.0 | Apache-2.0 OR MIT | crates.io |
| `sval_buffer` | 2.20.0 | Apache-2.0 OR MIT | crates.io |
| `sval_dynamic` | 2.20.0 | Apache-2.0 OR MIT | crates.io |
| `sval_fmt` | 2.20.0 | Apache-2.0 OR MIT | crates.io |
| `sval_json` | 2.20.0 | Apache-2.0 OR MIT | crates.io |
| `sval_nested` | 2.20.0 | Apache-2.0 OR MIT | crates.io |
| `sval_ref` | 2.20.0 | Apache-2.0 OR MIT | crates.io |
| `sval_serde` | 2.20.0 | Apache-2.0 OR MIT | crates.io |
| `svg_fmt` | 0.4.5 | MIT/Apache-2.0 | crates.io |
| `svgtypes` | 0.15.3 | Apache-2.0 OR MIT | crates.io |
| `swash` | 0.2.9 | Apache-2.0 OR MIT | crates.io |
| `syn` | 2.0.118 | MIT OR Apache-2.0 | crates.io |
| `sync_wrapper` | 1.0.2 | Apache-2.0 | crates.io |
| `synstructure` | 0.13.2 | MIT | crates.io |
| `sys-locale` | 0.3.2 | MIT OR Apache-2.0 | crates.io |
| `sysinfo` | 0.31.4 | MIT | crates.io |
| `sysinfo` | 0.37.2 | MIT | crates.io |
| `system-configuration` | 0.7.0 | MIT OR Apache-2.0 | crates.io |
| `system-configuration-sys` | 0.6.0 | MIT OR Apache-2.0 | crates.io |
| `taffy` | 0.10.1 | MIT | crates.io |
| `take-until` | 0.2.0 | MIT | crates.io |
| `tao-core-video-sys` | 0.2.0 | MIT | crates.io |
| `tar` | 0.4.46 | MIT OR Apache-2.0 | crates.io |
| `tempfile` | 3.27.0 | MIT OR Apache-2.0 | crates.io |
| `tendril` | 0.4.3 | MIT/Apache-2.0 | crates.io |
| `termcolor` | 1.4.1 | Unlicense OR MIT | crates.io |
| `thiserror` | 1.0.69 | MIT OR Apache-2.0 | crates.io |
| `thiserror` | 2.0.18 | MIT OR Apache-2.0 | crates.io |
| `thiserror-impl` | 1.0.69 | MIT OR Apache-2.0 | crates.io |
| `thiserror-impl` | 2.0.18 | MIT OR Apache-2.0 | crates.io |
| `thread_local` | 1.1.9 | MIT OR Apache-2.0 | crates.io |
| `tiff` | 0.11.3 | MIT | crates.io |
| `tiny-keccak` | 2.0.2 | CC0-1.0 | crates.io |
| `tiny-skia` | 0.11.4 | BSD-3-Clause | crates.io |
| `tiny-skia-path` | 0.11.4 | BSD-3-Clause | crates.io |
| `tinystr` | 0.8.3 | Unicode-3.0 | crates.io |
| `tinyvec` | 1.11.0 | Zlib OR Apache-2.0 OR MIT | crates.io |
| `tinyvec_macros` | 0.1.1 | MIT OR Apache-2.0 OR Zlib | crates.io |
| `tokio` | 1.52.3 | MIT | crates.io |
| `tokio-macros` | 2.7.0 | MIT | crates.io |
| `tokio-rustls` | 0.26.4 | MIT OR Apache-2.0 | crates.io |
| `tokio-util` | 0.7.18 | MIT | crates.io |
| `toml` | 0.8.23 | MIT OR Apache-2.0 | crates.io |
| `toml` | 1.1.2+spec-1.1.0 | MIT OR Apache-2.0 | crates.io |
| `toml_datetime` | 0.6.11 | MIT OR Apache-2.0 | crates.io |
| `toml_datetime` | 1.1.1+spec-1.1.0 | MIT OR Apache-2.0 | crates.io |
| `toml_edit` | 0.22.27 | MIT OR Apache-2.0 | crates.io |
| `toml_edit` | 0.25.12+spec-1.1.0 | MIT OR Apache-2.0 | crates.io |
| `toml_parser` | 1.1.2+spec-1.1.0 | MIT OR Apache-2.0 | crates.io |
| `toml_write` | 0.1.2 | MIT OR Apache-2.0 | crates.io |
| `toml_writer` | 1.1.1+spec-1.1.0 | MIT OR Apache-2.0 | crates.io |
| `tower` | 0.5.3 | MIT | crates.io |
| `tower-http` | 0.6.11 | MIT | crates.io |
| `tower-layer` | 0.3.3 | MIT | crates.io |
| `tower-service` | 0.3.3 | MIT | crates.io |
| `tracing` | 0.1.44 | MIT | crates.io |
| `tracing-attributes` | 0.1.31 | MIT | crates.io |
| `tracing-core` | 0.1.36 | MIT | crates.io |
| `tracing-log` | 0.2.0 | MIT | crates.io |
| `tracing-subscriber` | 0.3.23 | MIT | crates.io |
| `tree-sitter` | 0.26.9 | MIT | crates.io |
| `tree-sitter-json` | 0.24.8 | MIT | crates.io |
| `tree-sitter-language` | 0.1.7 | MIT | crates.io |
| `triomphe` | 0.1.15 | MIT OR Apache-2.0 | crates.io |
| `try-lock` | 0.2.5 | MIT | crates.io |
| `ttf-parser` | 0.25.1 | MIT OR Apache-2.0 | crates.io |
| `typeid` | 1.0.3 | MIT OR Apache-2.0 | crates.io |
| `typenum` | 1.20.1 | MIT OR Apache-2.0 | crates.io |
| `uds_windows` | 1.2.1 | MIT | crates.io |
| `unarray` | 0.1.4 | MIT OR Apache-2.0 | crates.io |
| `unicase` | 2.9.0 | MIT OR Apache-2.0 | crates.io |
| `unicode-bidi` | 0.3.18 | MIT OR Apache-2.0 | crates.io |
| `unicode-bidi-mirroring` | 0.4.0 | MIT/Apache-2.0 | crates.io |
| `unicode-ccc` | 0.4.0 | MIT/Apache-2.0 | crates.io |
| `unicode-id` | 0.3.6 | MIT OR Apache-2.0 | crates.io |
| `unicode-ident` | 1.0.24 | (MIT OR Apache-2.0) AND Unicode-3.0 | crates.io |
| `unicode-linebreak` | 0.1.5 | Apache-2.0 | crates.io |
| `unicode-properties` | 0.1.4 | MIT/Apache-2.0 | crates.io |
| `unicode-script` | 0.5.8 | MIT OR Apache-2.0 | crates.io |
| `unicode-segmentation` | 1.13.3 | MIT OR Apache-2.0 | crates.io |
| `unicode-vo` | 0.1.0 | MIT/Apache-2.0 | crates.io |
| `unicode-width` | 0.2.2 | MIT OR Apache-2.0 | crates.io |
| `unicode-xid` | 0.2.6 | MIT OR Apache-2.0 | crates.io |
| `universal-hash` | 0.6.1 | MIT OR Apache-2.0 | crates.io |
| `unsafe-libyaml` | 0.2.11 | MIT | crates.io |
| `untrusted` | 0.9.0 | ISC | crates.io |
| `url` | 2.5.8 | MIT OR Apache-2.0 | crates.io |
| `usvg` | 0.45.1 | Apache-2.0 OR MIT | crates.io |
| `utf-8` | 0.7.6 | MIT OR Apache-2.0 | crates.io |
| `utf8_iter` | 1.0.4 | Apache-2.0 OR MIT | crates.io |
| `utf8parse` | 0.2.2 | Apache-2.0 OR MIT | crates.io |
| `util` | 0.1.0 | Apache-2.0 | https://github.com/zed-industries/zed |
| `util_macros` | 0.1.0 | Apache-2.0 | https://github.com/zed-industries/zed |
| `uuid` | 1.23.3 | Apache-2.0 OR MIT | crates.io |
| `v_frame` | 0.3.9 | BSD-2-Clause | crates.io |
| `value-bag` | 1.12.0 | Apache-2.0 OR MIT | crates.io |
| `value-bag-serde1` | 1.12.0 | Apache-2.0 OR MIT | crates.io |
| `value-bag-sval2` | 1.12.0 | Apache-2.0 OR MIT | crates.io |
| `version_check` | 0.9.5 | MIT/Apache-2.0 | crates.io |
| `vswhom` | 0.1.0 | MIT | crates.io |
| `vswhom-sys` | 0.1.3 | MIT | crates.io |
| `vte` | 0.15.0 | Apache-2.0 OR MIT | vendored fork (vendor/, see section 2) |
| `wait-timeout` | 0.2.1 | MIT/Apache-2.0 | crates.io |
| `waker-fn` | 1.2.0 | Apache-2.0 OR MIT | crates.io |
| `walkdir` | 2.5.0 | Unlicense/MIT | crates.io |
| `want` | 0.3.1 | MIT | crates.io |
| `wasm-bindgen` | 0.2.125 | MIT OR Apache-2.0 | crates.io |
| `wasm-bindgen-macro` | 0.2.125 | MIT OR Apache-2.0 | crates.io |
| `wasm-bindgen-macro-support` | 0.2.125 | MIT OR Apache-2.0 | crates.io |
| `wasm-bindgen-shared` | 0.2.125 | MIT OR Apache-2.0 | crates.io |
| `wayland-backend` | 0.3.15 | MIT | crates.io |
| `wayland-client` | 0.31.14 | MIT | crates.io |
| `wayland-cursor` | 0.31.14 | MIT | crates.io |
| `wayland-protocols` | 0.32.12 | MIT | crates.io |
| `wayland-protocols-plasma` | 0.3.12 | MIT | crates.io |
| `wayland-protocols-wlr` | 0.3.12 | MIT | crates.io |
| `wayland-scanner` | 0.31.10 | MIT | crates.io |
| `wayland-sys` | 0.31.11 | MIT | crates.io |
| `web-time` | 1.1.0 | MIT OR Apache-2.0 | crates.io |
| `weezl` | 0.1.12 | MIT OR Apache-2.0 | crates.io |
| `wgpu` | 29.0.3 | MIT OR Apache-2.0 | https://github.com/zed-industries/wgpu.git |
| `wgpu-core` | 29.0.3 | MIT OR Apache-2.0 | https://github.com/zed-industries/wgpu.git |
| `wgpu-core-deps-apple` | 29.0.3 | MIT OR Apache-2.0 | https://github.com/zed-industries/wgpu.git |
| `wgpu-core-deps-windows-linux-android` | 29.0.3 | MIT OR Apache-2.0 | https://github.com/zed-industries/wgpu.git |
| `wgpu-hal` | 29.0.3 | MIT OR Apache-2.0 | https://github.com/zed-industries/wgpu.git |
| `wgpu-naga-bridge` | 29.0.3 | MIT OR Apache-2.0 | https://github.com/zed-industries/wgpu.git |
| `wgpu-types` | 29.0.3 | MIT OR Apache-2.0 | https://github.com/zed-industries/wgpu.git |
| `which` | 6.0.3 | MIT | crates.io |
| `winapi` | 0.3.9 | MIT/Apache-2.0 | crates.io |
| `winapi-util` | 0.1.11 | Unlicense OR MIT | crates.io |
| `windows` | 0.57.0 | MIT OR Apache-2.0 | crates.io |
| `windows` | 0.58.0 | MIT OR Apache-2.0 | crates.io |
| `windows` | 0.61.3 | MIT OR Apache-2.0 | crates.io |
| `windows` | 0.62.2 | MIT OR Apache-2.0 | crates.io |
| `windows-capture` | 1.5.0 | MIT | crates.io |
| `windows-collections` | 0.2.0 | MIT OR Apache-2.0 | crates.io |
| `windows-collections` | 0.3.2 | MIT OR Apache-2.0 | crates.io |
| `windows-core` | 0.57.0 | MIT OR Apache-2.0 | crates.io |
| `windows-core` | 0.58.0 | MIT OR Apache-2.0 | crates.io |
| `windows-core` | 0.61.2 | MIT OR Apache-2.0 | crates.io |
| `windows-core` | 0.62.2 | MIT OR Apache-2.0 | crates.io |
| `windows-future` | 0.2.1 | MIT OR Apache-2.0 | crates.io |
| `windows-future` | 0.3.2 | MIT OR Apache-2.0 | crates.io |
| `windows-implement` | 0.57.0 | MIT OR Apache-2.0 | crates.io |
| `windows-implement` | 0.58.0 | MIT OR Apache-2.0 | crates.io |
| `windows-implement` | 0.60.2 | MIT OR Apache-2.0 | crates.io |
| `windows-interface` | 0.57.0 | MIT OR Apache-2.0 | crates.io |
| `windows-interface` | 0.58.0 | MIT OR Apache-2.0 | crates.io |
| `windows-interface` | 0.59.3 | MIT OR Apache-2.0 | crates.io |
| `windows-link` | 0.1.3 | MIT OR Apache-2.0 | crates.io |
| `windows-link` | 0.2.1 | MIT OR Apache-2.0 | crates.io |
| `windows-numerics` | 0.2.0 | MIT OR Apache-2.0 | crates.io |
| `windows-numerics` | 0.3.1 | MIT OR Apache-2.0 | crates.io |
| `windows-registry` | 0.5.3 | MIT OR Apache-2.0 | crates.io |
| `windows-result` | 0.1.2 | MIT OR Apache-2.0 | crates.io |
| `windows-result` | 0.2.0 | MIT OR Apache-2.0 | crates.io |
| `windows-result` | 0.3.4 | MIT OR Apache-2.0 | crates.io |
| `windows-result` | 0.4.1 | MIT OR Apache-2.0 | crates.io |
| `windows-strings` | 0.1.0 | MIT OR Apache-2.0 | crates.io |
| `windows-strings` | 0.4.2 | MIT OR Apache-2.0 | crates.io |
| `windows-strings` | 0.5.1 | MIT OR Apache-2.0 | crates.io |
| `windows-sys` | 0.52.0 | MIT OR Apache-2.0 | crates.io |
| `windows-sys` | 0.59.0 | MIT OR Apache-2.0 | crates.io |
| `windows-sys` | 0.60.2 | MIT OR Apache-2.0 | crates.io |
| `windows-sys` | 0.61.2 | MIT OR Apache-2.0 | crates.io |
| `windows-targets` | 0.52.6 | MIT OR Apache-2.0 | crates.io |
| `windows-targets` | 0.53.5 | MIT OR Apache-2.0 | crates.io |
| `windows-threading` | 0.1.0 | MIT OR Apache-2.0 | crates.io |
| `windows-threading` | 0.2.1 | MIT OR Apache-2.0 | crates.io |
| `windows_aarch64_msvc` | 0.52.6 | MIT OR Apache-2.0 | crates.io |
| `windows_aarch64_msvc` | 0.53.1 | MIT OR Apache-2.0 | crates.io |
| `windows_x86_64_gnu` | 0.52.6 | MIT OR Apache-2.0 | crates.io |
| `windows_x86_64_gnu` | 0.53.1 | MIT OR Apache-2.0 | crates.io |
| `windows_x86_64_msvc` | 0.52.6 | MIT OR Apache-2.0 | crates.io |
| `windows_x86_64_msvc` | 0.53.1 | MIT OR Apache-2.0 | crates.io |
| `winnow` | 0.7.15 | MIT | crates.io |
| `winnow` | 1.0.3 | MIT | crates.io |
| `winreg` | 0.55.0 | MIT | crates.io |
| `winsafe` | 0.0.19 | MIT | crates.io |
| `wio` | 0.2.2 | MIT/Apache-2.0 | crates.io |
| `workspace-hack` | 0.1.0 | CC0-1.0 | crates.io |
| `writeable` | 0.6.3 | Unicode-3.0 | crates.io |
| `x11` | 2.21.0 | MIT | crates.io |
| `x11-clipboard` | 0.9.3 | MIT | crates.io |
| `x11rb` | 0.13.2 | MIT OR Apache-2.0 | crates.io |
| `x11rb-protocol` | 0.13.2 | MIT OR Apache-2.0 | crates.io |
| `xattr` | 0.2.3 | MIT/Apache-2.0 | crates.io |
| `xattr` | 1.6.1 | MIT OR Apache-2.0 | crates.io |
| `xcb` | 1.7.0 | MIT | crates.io |
| `xcursor` | 0.3.10 | MIT | crates.io |
| `xim-ctext` | 0.3.0 | MIT | https://github.com/zed-industries/xim-rs.git |
| `xim-parser` | 0.2.1 | MIT | https://github.com/zed-industries/xim-rs.git |
| `xkbcommon` | 0.8.0 | MIT | crates.io |
| `xkeysym` | 0.2.1 | MIT OR Apache-2.0 OR Zlib | crates.io |
| `xml-rs` | 0.8.28 | MIT | crates.io |
| `xml5ever` | 0.18.1 | MIT OR Apache-2.0 | crates.io |
| `xmlwriter` | 0.1.0 | MIT | crates.io |
| `y4m` | 0.8.0 | MIT | crates.io |
| `yazi` | 0.2.1 | Apache-2.0 OR MIT | crates.io |
| `yeslogic-fontconfig-sys` | 6.0.1 | MIT | crates.io |
| `yoke` | 0.8.3 | Unicode-3.0 | crates.io |
| `yoke-derive` | 0.8.2 | Unicode-3.0 | crates.io |
| `zbus` | 5.16.0 | MIT | crates.io |
| `zbus-lockstep` | 0.5.2 | MIT | crates.io |
| `zbus-lockstep-macros` | 0.5.2 | MIT | crates.io |
| `zbus_macros` | 5.16.0 | MIT | crates.io |
| `zbus_names` | 4.3.2 | MIT | crates.io |
| `zbus_xml` | 5.1.1 | MIT | crates.io |
| `zed-font-kit` | 0.14.1-zed | MIT OR Apache-2.0 | https://github.com/zed-industries/font-kit |
| `zed-scap` | 0.0.8-zed | MIT | https://github.com/zed-industries/scap |
| `zed-sum-tree` | 0.2.0 | Apache-2.0 | crates.io |
| `zed-xim` | 0.4.0-zed | MIT | https://github.com/zed-industries/xim-rs.git |
| `zeno` | 0.3.3 | Apache-2.0 OR MIT | crates.io |
| `zerocopy` | 0.8.52 | BSD-2-Clause OR Apache-2.0 OR MIT | crates.io |
| `zerocopy-derive` | 0.8.52 | BSD-2-Clause OR Apache-2.0 OR MIT | crates.io |
| `zerofrom` | 0.1.8 | Unicode-3.0 | crates.io |
| `zerofrom-derive` | 0.1.7 | Unicode-3.0 | crates.io |
| `zeroize` | 1.9.0 | Apache-2.0 OR MIT | crates.io |
| `zeroize_derive` | 1.5.0 | Apache-2.0 OR MIT | crates.io |
| `zerotrie` | 0.2.4 | Unicode-3.0 | crates.io |
| `zerovec` | 0.11.6 | Unicode-3.0 | crates.io |
| `zerovec-derive` | 0.11.3 | Unicode-3.0 | crates.io |
| `zip` | 2.4.2 | MIT | crates.io |
| `zlog` | 0.1.0 | GPL-3.0-or-later | https://github.com/zed-industries/zed |
| `zmij` | 1.0.21 | MIT | crates.io |
| `zopfli` | 0.8.3 | Apache-2.0 | crates.io |
| `ztracing` | 0.1.0 | GPL-3.0-or-later | https://github.com/zed-industries/zed |
| `ztracing_macro` | 0.1.0 | GPL-3.0-or-later | https://github.com/zed-industries/zed |
| `zune-core` | 0.4.12 | MIT OR Apache-2.0 OR Zlib | crates.io |
| `zune-core` | 0.5.1 | MIT OR Apache-2.0 OR Zlib | crates.io |
| `zune-inflate` | 0.2.54 | MIT OR Apache-2.0 OR Zlib | crates.io |
| `zune-jpeg` | 0.4.21 | MIT OR Apache-2.0 OR Zlib | crates.io |
| `zune-jpeg` | 0.5.15 | MIT OR Apache-2.0 OR Zlib | crates.io |
| `zvariant` | 5.12.0 | MIT | crates.io |
| `zvariant_derive` | 5.12.0 | MIT | crates.io |
| `zvariant_utils` | 3.4.0 | MIT | crates.io |

_909 third-party packages._
