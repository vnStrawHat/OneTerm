# Dependency LICENSE Analysis & License Recommendation for OneTerm

> Date: 2025. Scope: **runtime dependencies only** (excluding dev-deps / build-deps).
> Tools: `cargo-license` + direct inspection of `Cargo.toml` + `LICENSE-*` files + `strings`/`nm` on built binary.
>
> **Status (2026-08):** the counts below are a point-in-time snapshot. The policy this
> analysis motivates is now enforced mechanically by `deny.toml` (`cargo deny check
> licenses bans advisories`, run by the `cargo-deny` job in `.github/workflows/ci.yml`
> and by `scripts/ci-local.sh` with `--full`): the allow-list mirrors §1, and the `zlog` /
> `ztracing` / `ztracing_macro` GPL exceptions of §2 are crate-scoped so any other GPL
> crate fails the check. The project itself is licensed **Apache-2.0** (root `LICENSE`,
> `[workspace.package] license`), not the dual grant discussed in §4.

---

## 1. Overview — License Distribution

The project pulls in **~840 crate entries** (runtime). Distribution by license group:

| License group | ~Count | Nature | Constraint on project |
|---|---|---|---|
| **Apache-2.0 OR MIT** | ~569 | Permissive (dual) | None — just retain NOTICE |
| **MIT** | ~181 | Permissive | None — just retain NOTICE |
| **Apache-2.0** | ~37 | Permissive | None — retain NOTICE + state changes (§4 Apache) |
| **MIT OR Unlicense** | ~12 | Permissive | None |
| **Apache-2.0 OR MIT OR Zlib** | ~28 | Permissive | None |
| **Unicode-3.0** | ~18 | Permissive | None |
| **BSD-3-Clause / BSD-2-Clause / ISC / CC0-1.0 / 0BSD** | ~28 | Permissive | None |
| **Apache-2.0 OR GPL-2.0** (`self_cell`) | 1 | Dual → choose Apache-2.0 | None |
| **MPL-2.0** (`dwrote`, `option-ext`) | 2 | **Weak copyleft (file-level)** | Only applies to MPL files, does not spread to project |
| **bzip2-1.0.6** (`libbz2-rs-sys`) | 1 | BSD-like | Negligible |
| **GPL-3.0-or-later** (see §2) | 3 | **Strong copyleft** (on paper) | **Conditional — see analysis §2** |

> **~99% of dependencies are permissive.** Only 3 Zed crates declaring `license = "GPL-3.0-or-later"` require detailed analysis.

---

## 2. Deep Analysis of the `zlog` → `ztracing` → `ztracing_macro` Chain

### 2.1. Zed's Licensing Model

The Zed repo (`zed-industries/zed`) has **two license files at the root**: `LICENSE-APACHE` + `LICENSE-GPL`.
Each sub-crate contains a **symlink** to one or both license files in its own directory — this is **how Zed "marks" the license** for each crate:

| Crate | `LICENSE-APACHE` | `LICENSE-GPL` | `Cargo.toml license` | Effective license |
|---|:---:|:---:|---|---|
| `gpui`, `gpui_platform`, `gpui_windows`, `gpui_macros` | ✅ | ❌ | `Apache-2.0` | **Apache-2.0 only** |
| `sum_tree`, `collections`, `util`, `util_macros`, `refineable`, `derive_refineable`, `scheduler`, `http_client`, `perf` | ✅ | ❌ | `Apache-2.0` or `none` | **Apache-2.0 only** |
| `gpui_util`, `gpui_shared_string` | ✅ | ❌ | `none` (not declared) | **Apache-2.0** — the LICENSE-APACHE file is the authoritative grant |
| **`ztracing`** | ✅ | ✅ | `GPL-3.0-or-later` | **Dual-licensed: Apache-2.0 OR GPL-3.0-or-later** |
| **`ztracing_macro`** | ✅ | ✅ | `GPL-3.0-or-later` | **Dual-licensed: Apache-2.0 OR GPL-3.0-or-later** |
| **`zlog`** | ❌ | ✅ | `GPL-3.0-or-later` | **GPL-3.0-or-later only** |

> **`ztracing` and `ztracing_macro` have BOTH LICENSE-APACHE and LICENSE-GPL** → **dual-licensed**.
> The `Cargo.toml` declaring only `GPL-3.0-or-later` is SPDX metadata (picking one option for cargo-about),
> but the presence of `LICENSE-APACHE` in the crate's directory constitutes a **legally effective Apache-2.0 grant**.
> → **We have the right to choose Apache-2.0 for `ztracing` and `ztracing_macro`.**

> **`zlog` has only `LICENSE-GPL`** → GPL-3.0-or-later only. No Apache option.

### 2.2. Is `zlog` (GPL-only) actually in the binary?

**Answer: NO.** Verified via `strings` + `nm` on both debug and release binaries.

#### Dependency chain & actual code usage

```
gpui (Apache-2.0)
  └── sum_tree (Apache-2.0)
        ├── use ztracing::instrument;     ← 9 #[instrument] attributes across 2 files
        └── zlog::init_test();            ← only inside #[cfg(test)]  → NOT compiled into binary
```

```
ztracing (dual Apache OR GPL)
  ├── #[cfg(ztracing)]      → zlog::info!(...)    ← ONLY compiled when ZTRACING env var is set
  ├── #[cfg(not(ztracing))] → pub fn init() {}    ← default build: EMPTY, does NOT call zlog
  ├── #[cfg(not(ztracing))] → pub use ztracing_macro::instrument;
  └── zlog (GPL-only)       ← dependency in Cargo.toml, BUT...
```

#### Why `zlog` is not in the binary

| Condition | Default (normal build) | Profiling build (`ZTRACING=1`) |
|---|---|---|
| `zlog::info!(...)` in `ztracing::init()` | `#[cfg(ztracing)]` → **NOT compiled** | ✅ compiled → zlog enters binary |
| `zlog::init_test()` in `sum_tree` | `#[cfg(test)]` → **NOT compiled** | Not compiled (test only) |
| **Which symbols from `zlog` are referenced?** | **NONE** — zero symbols | Yes (`zlog::info!`) |
| **Is zlog in the binary?** | **NO** — dead-stripped | ✅ Yes |

#### `ztracing_macro::instrument` — a no-op identity macro

```rust
#[proc_macro_attribute]
pub fn instrument(
    _attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    item  // ← Returns the item UNCHANGED — generates no additional code!
}
```

- `ztracing_macro` is a **proc-macro** → runs at **compile-time only** → its code is NEVER in the binary.
- In the default build, `#[instrument]` is merely an **identity passthrough** — it adds no tracing span, calls nothing.
- A proc-macro's license **does not contaminate the binary** (similar to `syn`, `quote` — they run only at compile time).

#### Verification on the built binary

```bash
$ strings target/release/oneterm.exe | grep -ic 'zlog'      # → 0
$ strings target/release/oneterm.exe | grep -ic 'ztracing'  # → 0
$ strings target/debug/oneterm-debug.exe | grep -ic 'zlog|ztracing'  # → 0
$ nm target/release/oneterm.exe | grep -i 'zlog'             # → (empty)
```

→ **Zero machine code from `zlog` and `ztracing` in the binary**, in both debug and release builds.

> `zlog` is compiled into an `.rlib` (the file `target/release/deps/libzlog-*.rlib` exists),
> but because **no symbols are referenced**, the linker **dead-strips** all of its code.

### 2.3. Conclusion on GPL-3.0 contamination

| Crate | Effective license | In binary? | Contamination? |
|---|---|---|---|
| `ztracing` | **Apache-2.0** (dual, we choose Apache) | ❌ (dead-stripped) | **NO** |
| `ztracing_macro` | **Apache-2.0** (dual, we choose Apache) | ❌ (proc-macro, compile-time only) | **NO** |
| `zlog` | **GPL-3.0-or-later** (GPL only) | ❌ (dead-stripped, 0 symbols) | **NO** (no machine code in binary) |

**→ There is NO GPL-3.0 code in the binary in the default build. The project can use a permissive license.**

### 2.4. Risks / conditions to maintain

| Condition | Consequence |
|---|---|
| Setting env var `ZTRACING=1` at build time | `zlog::info!` gets compiled → zlog (GPL) enters binary → **must use GPL-3.0** |
| Building with `cargo test` | `zlog::init_test()` gets compiled → but test binaries are not distributed → no issue |
| A future gpui version uses `zlog` directly (not behind cfg) | zlog enters binary → **must use GPL-3.0** |
| Conservative legal interpretation: compiling GPL code in the build graph even if dead-stripped | A conservative lawyer might argue it's a derivative work → would need GPL. **However**, if no machine code is in the binary, the argument "not a combined work" has merit |

> **Recommendation**: never set `ZTRACING=1` when building a release binary for distribution.

---

## 3. License Violation Assessment: Using Techniques from Zed Terminal & Windows Terminal

### 3.1. Windows Terminal — **MIT** ✅ completely safe

- Repo: `microsoft/terminal` → `LICENSE` = MIT.
- **Copying code**: MIT allows free use — just retain the copyright notice.
- **Using techniques/algorithms**: ideas are not copyrightable → no violation.
- Project already uses: `windows`, `windows-sys`, `winapi`, `uds_windows` (MIT/Apache).
- **Conclusion**: No risk. You can study and re-implement any technique (ConPTY, rendering pipeline, buffer ring, text shaping…).

### 3.2. Zed Terminal — distinguish 3 levels clearly

| Activity | Source crate license | Violation? |
|---|---|---|
| **Copying source** from `crates/terminal/` (GPL-3.0) or `crates/terminal_view/` (GPL-3.0) | GPL-only (only has `LICENSE-GPL`) | **Violation** if project is not GPL-3.0. **No violation** if project is GPL-3.0+ |
| **Studying & re-implementing ideas** (damage tracking, cell diff, scrollback, IME, OSC 52, link detection…) | Ideas are not copyrightable | **No violation** — independent re-implementation under any license |
| **Using `alacritty_terminal`** (Zed fork, Apache-2.0) | Apache-2.0 | **No violation** — already used, compatible with any license |

#### Practical recommendations for Zed Terminal

1. **Do NOT copy source** from `crates/terminal/` or `crates/terminal_view/` unless the project accepts GPL-3.0.
2. **You may** read the source to understand the algorithm → write your own (clean-room). Note the reference source.
3. **You may** use `alacritty_terminal` (Apache-2.0) — the project already does this correctly.
4. When referencing `terminal_view` for rendering a cell grid with gpui, only study the **pattern/API shape** — do not paste code.

---

## 4. License Recommendation for the Project

### Goal: "as open as possible" + no violations

Based on the analysis in §2: **no GPL-3.0 machine code is in the binary** in the default build.
→ The project **can** use a **permissive license**.

#### ✅ RECOMMENDED: `Apache-2.0 OR MIT` (dual permissive)

```
LICENSE = Apache-2.0 OR MIT
```

| Criterion | Assessment |
|---|---|
| "Most open" (permissive) | ✅✅ — allows closed-source derivatives, commercial use, modification |
| Compatible with GPL-3.0 deps? | ✅ (Apache-2.0 is compatible with GPL-3.0; MIT is too) |
| Compatible with all deps? | ✅ — all deps are permissive or dual-permissive |
| GPL-3.0 contamination? | **NO** — zlog is dead-stripped, ztracing/ztracing_macro choose Apache-2.0 |
| Fits Rust ecosystem? | ✅ — this is the most common dual-license in Rust (95% of crates) |
| Commercial friendly? | ✅ |

**This is the most permissive license that does not violate any dependency license.**

#### Comparison of options

| Option | License | Permissive? | No violations? | Commercial? | Recommended? |
|---|---|---|---|---|---|
| **Apache-2.0 OR MIT** | Dual permissive | ✅✅ | ✅ | ✅ | **✅ RECOMMENDED** |
| Apache-2.0 | Permissive | ✅ | ✅ | ✅ | Good (but single license) |
| MIT | Permissive | ✅ | ✅ | ✅ | Good (but lacks patent grant) |
| GPL-3.0-or-later | Copyleft | ❌ (restrictive) | ✅ | ⚠️ | Overly conservative — unnecessary |
| AGPL-3.0-or-later | Strong copyleft | ❌ | ✅ | ❌ | Too restrictive |

### Why GPL-3.0 is NOT needed (correcting the initial analysis)

The initial analysis (in the previous version of this file) concluded GPL-3.0 contamination via `gpui → sum_tree → ztracing → zlog`.
**However, closer inspection reveals:**

1. `ztracing` + `ztracing_macro` have **both LICENSE-APACHE and LICENSE-GPL** → **dual-licensed**, we choose Apache-2.0.
2. `zlog` (GPL-only) has **no symbols referenced** in the default build → **dead-stripped** → no machine code in the binary.
3. `ztracing_macro::instrument` is a **no-op identity macro** + is a **proc-macro** (compile-time only) → not in the binary.
4. `strings`/`nm` confirmed: **0** occurrences of `zlog`/`ztracing` in both debug and release binaries.

→ **No GPL-3.0 code in the binary** → a permissive license is legally sound.

---

## 5. Summary Table of All Zed Crates in the Dependency Tree

| Crate | Directory | LICENSE-APACHE | LICENSE-GPL | Cargo.toml | Effective license |
|---|---|:---:|:---:|---|---|
| `gpui` | `crates/gpui/` | ✅ | ❌ | `Apache-2.0` | Apache-2.0 |
| `gpui_platform` | `crates/gpui_platform/` | ✅ | ❌ | `Apache-2.0` | Apache-2.0 |
| `gpui_windows` | `crates/gpui_windows/` | ✅ | ❌ | `Apache-2.0` | Apache-2.0 |
| `gpui_macros` | `crates/gpui_macros/` | ✅ | ❌ | `Apache-2.0` | Apache-2.0 |
| `gpui_util` | `crates/gpui_util/` | ✅ | ❌ | `none` | Apache-2.0 (file grant) |
| `gpui_shared_string` | `crates/gpui_shared_string/` | ✅ | ❌ | `none` | Apache-2.0 (file grant) |
| `sum_tree` | `crates/sum_tree/` | ✅ | ❌ | `Apache-2.0` | Apache-2.0 |
| `collections` | `crates/collections/` | ✅ | ❌ | `Apache-2.0` | Apache-2.0 |
| `util` | `crates/util/` | ✅ | ❌ | `Apache-2.0` | Apache-2.0 |
| `util_macros` | `crates/util_macros/` | ✅ | ❌ | `Apache-2.0` | Apache-2.0 |
| `refineable` | `crates/refineable/` | ✅ | ❌ | `Apache-2.0` | Apache-2.0 |
| `derive_refineable` | `crates/refineable/derive_refineable/` | ✅ | ❌ | `Apache-2.0` | Apache-2.0 |
| `scheduler` | `crates/scheduler/` | ✅ | ❌ | `Apache-2.0` | Apache-2.0 |
| `http_client` | `crates/http_client/` | ✅ | ❌ | `Apache-2.0` | Apache-2.0 |
| `perf` | `tooling/perf/` | ✅ | ❌ | `none` | Apache-2.0 (file grant) |
| `ztracing` | `crates/ztracing/` | ✅ | ✅ | `GPL-3.0-or-later` | **Apache-2.0 OR GPL** (dual) |
| `ztracing_macro` | `crates/ztracing_macro/` | ✅ | ✅ | `GPL-3.0-or-later` | **Apache-2.0 OR GPL** (dual, proc-macro) |
| `zlog` | `crates/zlog/` | ❌ | ✅ | `GPL-3.0-or-later` | **GPL-3.0-only** (but dead-stripped) |
| `alacritty_terminal` | (fork `zed-industries/alacritty`) | ✅ | ❌ | `Apache-2.0` | Apache-2.0 |

> **All Zed crates in the dependency tree are effectively Apache-2.0** (either directly, or dual-licensed with the option to choose Apache, or GPL-only but dead-stripped).

---

## 6. Crates Requiring Special Attention (non-permissive or gray-area)

| Crate | License | In binary? | Notes |
|---|---|---|---|
| `zlog` | GPL-3.0-or-later (GPL only) | **❌ dead-stripped** | 0 symbols referenced in default build. Only enters binary if `ZTRACING=1` |
| `ztracing` | Apache-2.0 OR GPL (dual) | ❌ (dead-stripped) | Choose Apache-2.0. Default build has only no-op code |
| `ztracing_macro` | Apache-2.0 OR GPL (dual) | ❌ (proc-macro) | Choose Apache-2.0. Compile-time only, identity macro |
| `dwrote` | MPL-2.0 | ✅ | Weak copyleft file-level, does not spread to project |
| `option-ext` | MPL-2.0 | ✅ | Same as above |
| `libbz2-rs-sys` | bzip2-1.0.6 | ✅ | BSD-like |
| `self_cell` | Apache-2.0 OR GPL-2.0 | ✅ | Choose Apache-2.0 |

---

## 7. Compliance Checklist for Distribution

1. **LICENSE file** at project root: Apache-2.0 (the shipped choice; see the status note at the top).
2. **NOTICE / third-party credit file** listing all dependencies + their licenses (generated from the Cargo metadata; the bundled Windows Terminal ConPTY binaries are MIT and need their own entry).
3. Retain copyright notices from: Zed Industries (gpui, sum_tree, ztracing…), Longbridge (gpui-component), Alacritty contributors, Microsoft (windows crates), Rust ecosystem contributors.
4. **Do NOT set `ZTRACING=1`** when building a release binary — prevents pulling `zlog` (GPL) into the binary.
5. If copying code from Windows Terminal (MIT): retain the Microsoft copyright notice in the NOTICE file.
6. If referencing algorithms from Zed `terminal_view` (GPL): re-implement independently, do NOT copy source.
7. When upgrading gpui to a new version: **re-run** `strings target/release/*.exe | grep -i 'zlog'` to verify zlog is still dead-stripped.
