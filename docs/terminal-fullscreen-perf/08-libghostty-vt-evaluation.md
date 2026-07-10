# 8. Evaluation — `libghostty-vt` as the Terminal Engine

> **STATUS: EVALUATION / NOT SCHEDULED.** Assesses using **`libghostty-vt`** (the VT
> parser + terminal-state library extracted from [Ghostty](https://ghostty.org)) as an
> alternative to (a) the pinned `alacritty_terminal` fork and (b) the from-scratch
> custom engine in [`07-custom-engine-rfc.md`](07-custom-engine-rfc.md).
>
> ✅ **Verification: checked against the live Ghostty repo/docs, the published Rust
> bindings, the alacritty_terminal API, and first-hand benchmarking (Nov 2025 / Jul 2026).**
> This revision folds in four real-world data points that reshape the decision (see §8.9):
> 1. **`termwiz` was benchmarked → measurably lower per-frame engine+integration cost than
>    the alacritty fork** (pure-Rust baseline, measured).
> 2. **`libghostty-rs` does not currently build on Windows** in practice (Zig 0.15.2 +
> Ghostty `uucode_build_tables.exe` break; [zig#24944](https://github.com/ziglang/zig/issues/24944)).
> 3. **The from-scratch custom engine (RFC 07) is rejected as too risky.**
> 4. **A fourth option — patch the `alacritty_terminal` fork (R1 + R2 surgery) — is now
>    the leading low-risk path**, and the primitives it needs (`vte::Handler` dispatch +
>    `Term::damage()`) already exist in the library.

---

> **Framing note (read first).** This evaluation compares engines on **cost structure**,
> not delivered throughput. Per [`06`](06-results-and-ceiling.md) §6.7, the end-to-end
> delivered throughput of the DOOM-fire workload is bound by the **producer + ConPTY** and
> **scales with window size** (§6.4.1) — **no engine choice changes it**. So the
> comparisons below are about **per-frame CPU / parse overhead, correctness,
> Windows-buildability, and dependency/maintenance cost** — i.e. how much CPU headroom an
> engine leaves — not any delivery figure. Where one tool "measured faster" than another,
> read that as *lower per-frame engine+integration CPU on a benchmark harness*, a proxy for
> headroom; it is **not** a delivered-throughput target and cross-tool numbers were taken on
> different harnesses.

## 8.1. What `libghostty-vt` is (and isn't)

- **Is:** the VT/ANSI parser + terminal *state* engine from Ghostty, written in **Zig**,
  exposed via a **C ABI** ([`include/ghostty/vt.h`](https://github.com/ghostty-org/ghostty/blob/main/include/ghostty/vt.h)
> + module headers). Same architectural layer as `alacritty_terminal`'s `Term`/`Processor`/`Grid`.
  Ghostty's README marks the embeddable-`libghostty` roadmap step ✅; `-vt` is the first
  shipped sub-library, *"already available and usable today for Zig and C"*.
- **Is not:** a renderer, a font/shaper, or a PTY. OneTerm keeps its GPUI renderer + ConPTY.
- **Zero-dependency** (not even libc); **[VERIFIED] MIT** ([`LICENSE`](https://github.com/ghostty-org/ghostty/blob/main/LICENSE)).
- **API stability:** core logic production-proven; **C API explicitly alpha, not yet
  tagged** ("API signatures still in flux") → pin a commit; own a thin adapter.
- **Parser:** SIMD-oriented, cache-conscious grid. *Caveat:* Ghostty's README positions
  its grid as **on par with Alacritty** (*"within a few percentage points"*) — so the real
  engine-side wins are **R1** (no double parse) and **R2** (dirty-tracking read path),
  *not* a raw-grid-speed win.

---

## 8.2. How it maps onto the three costs (from RFC §7.2)

| Cost | With `alacritty` today | With `libghostty-vt` | Verdict |
|---|---|---|---|
| **R1 — double parse** (Processor + a 2nd `vte::Parser` for OSC 7/9/52/133 + `CSI 2J/3J`) | present | **[VERIFIED] removable in principle** — `-vt` ships an OSC parser surface (`osc.h`: 0/1/2/7/8/9/52/777/133 + color 4/10/11/12) + `modes.h`; safe API exposes `on_pty_write`. **[VERIFY]:** inline event emission vs parallel parse. | likely win |
| **R2 — snapshot clone** (~5.4k cells copied under `FairMutex` per render) | present | **[VERIFIED] a dirty-tracking render-state API exists** (`render.h`): global `FALSE/PARTIAL/FULL` + per-row dirty, row/cell iterators, batched `_get_multi`, `GhosttyCell` is a `uint64_t` value type. ⚠️ All `-vt` types are **`!Send + !Sync`** → terminal + render-state live on the **pump thread**, dirty-bounded copy → **channel** → GPUI thread. No raw pointer+stride bulk export. | workable; fully-dirty frames still pay per-cell field extraction |
| **R3 — opaque grid mutation** | fixed (rev-pinned) | **[VERIFIED] peer-class with Alacritty** (per Ghostty README) — not categorically faster. Subsumed into R2's dirty-tracking benefit. | marginal |

The original make-or-break (R2) is de-risked *in principle*; the integration blocker is
**Windows** (§8.3), and the *same* R1/R2 surgery that `-vt` would solve can instead be
applied **in-place to the alacritty fork we already have** (§8.5) — without FFI, Zig, or a
new dependency.

---

## 8.3. The Rust binding + Windows build status (revised — **broken in practice**)

A safe Rust binding exists: [`libghostty-vt`](https://crates.io/crates/libghostty-vt) v0.2.0
+ [`libghostty-vt-sys`](https://crates.io/crates/libghostty-vt-sys), repo
[`uzaaft/libghostty-rs`](https://github.com/uzaaft/libghostty-rs) (Uzaaft + pluiedev;
`MIT OR Apache-2.0`; MSRV 1.90; ~13k downloads; active July 2026). The `-sys` `build.rs`
fetches the pinned Ghostty source and runs `zig build` → `libghostty-vt.a` (static,
default). Bindings are checked-in (no libclang for consumers). `links = "ghostty-vt"`.

**Windows — the CI exists but the build is currently broken (first-hand + issue tracker).**
The crate's `windows-ci.yml` targets `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc`
with Zig 0.15.2 — so the *intent* and matrix are correct — **but**:
- issue [#9](https://github.com/Uzaaft/libghostty-rs/issues/9) + [zig#24944](https://github.com/ziglang/zig/issues/24944):
  *"On Windows, Zig 0.15.2 fails when Ghostty's `uucode_build_tables.exe` …"* — a concrete
  build break in the Zig-on-MSVC path;
- **first-hand:** the build does not currently succeed on Windows.

So the previous revision's "MSVC CI-tested → low-risk" conclusion is **downgraded**: the
matrix is wired but **not green**, and OneTerm's primary platform is Windows. `-vt` is
therefore **currently blocked** for OneTerm until that break is fixed upstream (Ghostty
uucode tables / Zig 0.15.2 on MSVC). It stays a *contingent* candidate, not a ready one.

The remaining `-vt` integration costs (independent of the Windows break): **Zig 0.15.x on
PATH** for every build (dev + CI + release; vendor the pinned Ghostty source via
`GHOSTTY_SOURCE_DIR` for offline CI), and a **third-party pre-1.0 binding** (small, 5k
SLoC, fork-able) — both acceptable *if* Windows builds, which it currently does not.

---

## 8.4. Option landscape (with the four new data points)

Delivered throughput is identical across all options (producer/ConPTY-bound, §6.7); they
differ in **per-frame engine+integration CPU cost** (headroom), correctness,
Windows-buildability, and maintenance.

| Dimension | Alacritty fork (today) | **Patch alacritty fork (§8.5)** | `termwiz` | `libghostty-vt` (Rust crate) | Custom engine (RFC 07) |
|---|---|---|---|---|---|
| Per-frame engine+integration cost | baseline | **lower** (R1+R2 removed; bounded by alacritty's R3 grid) | **lower [VERIFIED, measured]** | comparable-or-lower (unmeasured; peer-class grid, gated by R2 FFI cost) | *lowest (unproven)* |
| Effect on delivered throughput | — | **none** (§6.7) | none | none | none |
| Windows | ✅ works | ✅ works (our code) | ✅ works (pure Rust; docs.rs `x86_64-pc-windows-msvc`) | ❌ **currently broken** (Zig 0.15.2 + uucode on MSVC) | ✅ (our code) |
| New deps / toolchain | none | none | one crate (large, WezTerm's) | Zig + third-party binding | none |
| VT correctness | ✅ battle-tested | ✅ battle-tested (unchanged engine) | ✅ (WezTerm-proven) | ✅ (Ghostty, xterm-audited) | ❌ we own all edge cases |
| Effort to parity | 0 | **medium** (R1 hooks + R2 dirty copy on existing engine) | low–medium (swap engine + ~110-site migration) | medium (FFI done by crate; migration + Windows-fix wait) | **very high** — ❌ **rejected as too risky** |
| Risk | low | **lowest** | low–medium | **high (Windows blocked)** | **too high (rejected)** |
| Maintenance | track rev-pinned fork | track fork + small patch set | track a large external lib | track binding (pre-1.0) + Ghostty commit + Zig | own a VT engine forever |

The two viable, Windows-working, correctness-inheriting paths are now **patch-alacritty**
and **`termwiz`**. `libghostty-vt` is the most attractive *on paper* but is blocked on our
primary platform. RFC 07 is off the table.

---

## 8.5. Option D — patch the `alacritty_terminal` fork (R1 + R2 surgery)

**[VERIFIED feasibility]** The key insight from re-reading the alacritty_terminal API:
**both R1 and R2 are removable using primitives that already exist in the library** — this
is not an engine rewrite, it is wiring up existing internal hooks. We keep alacritty's
battle-tested parser/grid (R3 unchanged, which is fine — it's peer-fast) and surgically
remove the two costs that are *OneTerm's own integration overhead*, not alacritty's.

### R1 patch — kill the second `vte::Parser`  ✅ shipped (doc 09)

Today OneTerm runs alacritty's `Processor` (which drives `Term` as a `vte::Handler`) **and**
a *second* `vte::Parser` over the same bytes to catch OSC 7/9/52/133 + `CSI 2J/3J`. But
**alacritty's `Term` already implements `vte::Handler` for all of these**:
- `set_title` (OSC 0/1/2), `set_hyperlink` (OSC 8), `clipboard_store`/`clipboard_load`
  (OSC 52), `set_color`/`dynamic_color_sequence`/`reset_color` (OSC 4/10/11/12),
  `set_mode`/`set_private_mode`/`report_private_mode` (mode changes),
  `clear_screen`/`clear_line` (`CSI 2J/2K/3J`), `set_scrolling_region`, …

**Patch:** add an event/callback sink to our `Term` wrapper (or a thin `Handler`
decorator) that emits the OSC/mode/clear events we care about *during* the single parse,
and delete the second `vte::Parser`. alacritty's parsing is **unchanged** — we just
*observe* dispatches it already makes. → **R1 gone**, one parse pass instead of two. This
is exactly what R1 in [`09`](09-patch-alacritty-fork.md) shipped (parse capacity ~+70%,
pump parse-bound → wait-bound). Effort: medium. Risk: low (no parser logic touched).

### R2 patch — replace the full snapshot clone with dirty-row copy

Today OneTerm clones ~5.4k cells under a `FairMutex` every frame. But **alacritty already
exposes row-level damage**: `Term::damage() -> TermDamage` (*"Collect the information about
the changes in the lines, which could be used to minimize the amount of drawing
operations"*) + `reset_damage()`.

**Patch:** maintain a OneTerm-owned backbuffer; each frame, read `Term::damage()`, copy
**only dirty rows** into the backbuffer under a short lock, then let GPUI render from the
backbuffer **lock-free**. This is the RFC 07 double-buffer idea, but built *on top of*
alacritty's existing grid + damage API — no from-scratch grid.
- **Normal workloads (small dirty region):** big win — copy tens of cells, not 5.4k, and
  the `FairMutex` is held for microseconds, not a full clone.
- **DOOM-fire (full-screen dirty every frame):** degrades to the full copy (no R2 win),
  but R1 removal still helps, and lock hold time drops (single pass, no second parser).

Effort: medium. Risk: low–medium (threading care around the backbuffer; no engine logic).

### R3 — intentionally untouched

alacritty's grid mutation stays. R3 was always peer-class/marginal (Ghostty README
confirms parity), and rewriting it is exactly the risk RFC 07 carries and we are rejecting.

### Estimated headroom & why it's attractive

- **Headroom:** R1 removal + R2 dirty-copy remove *OneTerm's own* per-frame overhead (the
  double-parse and the full clone); the residual per-frame cost is alacritty's grid
  mutation (R3, unchanged, peer-fast). This frees pump/render CPU. **Per §6.7 it does not
  change delivered throughput** — that is producer/ConPTY-bound. For **normal workloads**
  the relative CPU win is larger, because R2's full-clone is pure waste that mostly vanishes.
- **Notably:** `termwiz` measured a lower per-frame cost via a *cleaner* integration (no R1
  double-parse, no R2 full-clone). Patching alacritty to remove R1+R2 targets the **same
  overhead** that separates our fork from termwiz — so patched-alacritty could land in the
  **same low-overhead class as termwiz** while keeping alacritty's grid (peer-fast) and its
  correctness, at **lower risk and zero new dependencies**.
- **Tradeoffs:** we carry a small patch set on a rev-pinned fork forever (we already pin a
  fork; this adds a contained diff). The residual R3 grid cost is the price we accept for
  not owning a VT engine.

---

## 8.6. `termwiz` — the measured pure-Rust baseline

**[VERIFIED] lower per-frame engine+integration cost than the alacritty fork** (benchmarked).
A **pure-Rust** terminal model + parser (WezTerm's;
[`docs.rs`](https://docs.rs/termwiz/latest/termwiz/)), *"implemented according to Paul
Williams' ANSI parser state machine"* (the same reference Ghostty's parser cites). It
**builds and runs on Windows** (docs.rs publishes
[`x86_64-pc-windows-msvc`](https://docs.rs/termwiz/latest/x86_64-pc-windows-msvc/termwiz/)),
needs **no FFI, no Zig, no third-party wrapper**. It measured with **less per-frame cost
than our alacritty fork** — disproving the earlier "may not beat alacritty" guess; the gap
is plausibly exactly the R1+R2 integration overhead §8.5 targets. (Per §6.7 this is a CPU
headroom advantage, not a delivered-throughput one.)

**Decision role:** `termwiz` is the **proven, Windows-working, pure-Rust** option. It is
both (a) the cost-structure baseline to match with patch-alacritty, and (b) the **fallback
/ alternative** if patch-alacritty underperforms — at the cost of adopting a large
WezTerm-maintained library and the ~110-site migration to owned types.

---

## 8.7. `libghostty-vt` open questions (status after this revision)

1. **Availability/API** — [VERIFIED] real, alpha, no tag; pin a commit.
2. **License** — [VERIFIED] MIT (binding `MIT OR Apache-2.0`).
3. **Windows/MSVC** — **[VERIFIED] currently broken** (Zig 0.15.2 + `uucode_build_tables.exe`
   on MSVC; zig#24944; first-hand). Matrix is wired but not green. **Blocked** until fixed
   upstream — this is why `-vt` is *contingent*, not recommended, for OneTerm.
4. **Cell read path (R2)** — [VERIFIED] render-state API with dirty tracking; `!Send+!Sync`
   → pump-thread + channel model. Residual: per-cell field-extraction cost (measured later).
5. **Event surface (R1)** — [VERIFIED] exposed; residual: inline emission vs parallel parse.
6. **Feature parity** — [VERIFIED] broad (scrollback, reflow, wide-char, OSC 8/133, Kitty
   graphics, selection, paste, formatter).
7. **Threading** — [VERIFIED] `!Send + !Sync`; docs recommend terminal-on-own-thread +
   channels. Forces pump-thread-owns + channel (cleaner than FairMutex clone).
8. **Binary size / Zig-on-PATH** — [VERIFY]; vendor source via `GHOSTTY_SOURCE_DIR`.

---

## 8.8. Recommended spike (re-prioritized for the new reality)

The custom engine is out; `libghostty-vt` is Windows-blocked; `termwiz` is measured and
works. So the spike is now **two cheap, low-risk experiments in the existing harness**,
ranked by friction:

1. **Patch-alacritty (cheapest, first):** on the existing fork, implement the R1
   event-sink (delete the second `vte::Parser` — **already shipped**, doc 09) and the R2
   `Term::damage()` dirty-row backbuffer + channel. Re-run the instrumentation (06 §6.6).
   **Goal:** does it shed the R1+R2 overhead (matching termwiz's cleaner cost structure) at
   zero new deps? This is a contained, reversible patch on code we already ship.
2. **`termwiz` baseline (already measured; confirm in our harness):** wire `termwiz` behind
   the same `TerminalEngine` trait + owned `Cell` types in a throwaway, build on
   `x86_64-pc-windows-msvc`, run DOOM-fire. Confirms the cost-structure delta in our exact
   setup and sizes the ~110-site migration cost.
3. **`libghostty-vt` (contingent, only if the Windows break is fixed upstream):** `cargo
   add libghostty-vt`, build on `x86_64-pc-windows-msvc`, and only proceed if it links.
   Measure the render-state read path. **Do not block the decision on this** — it is a
   future candidate, not a current option.

**Gate logic:** there is **no throughput target** in scope — §6.7 shows delivered
throughput is producer/ConPTY-bound and size-dependent, so neither the rejected custom
engine nor any engine swap changes it. The realistic bar is **cost-structure**: "does
patch-alacritty shed the R1 double-parse and R2 full-clone overhead (freeing pump CPU +
parse headroom) on Windows, at acceptable risk/deps?" If patch-alacritty removes R1+R2
cleanly → **adopt it** (lowest risk, no deps, correctness inherited). If it proves awkward
→ `termwiz` (proven pure-Rust, Windows-works) is the fallback. Revisit `libghostty-vt`
only when its Windows build is green.

---

## 8.9. Recommendation

The four data points reorder the decision:

- **`libghostty-vt`** remains the most attractive engine *on paper* (inherited Ghostty
  correctness, a dirty-tracking render-state API, SIMD parser), but it is **currently
  unbuildable on Windows** — OneTerm's primary platform — and adds a Zig toolchain + a
  pre-1.0 third-party binding. It is a **future candidate, contingent on the upstream
  Zig-0.15.2/MSVC + Ghostty uucode break being fixed.** Do not block on it.

- **The from-scratch custom engine (RFC 07)** is **rejected as too risky** (we would own
  every VT edge case forever). Its only theoretical edge is a bit more CPU headroom, which
  per §6.7 does **not** translate into delivered throughput — so it is not worth the risk.

- **`termwiz`** is **proven** (measured lower per-frame overhead), **pure-Rust**,
  **Windows-working**, and a strong candidate — but a large new dependency requiring the
  ~110-site migration.

- **Patching the `alacritty_terminal` fork (R1 + R2 surgery, §8.5)** is the **recommended
  first path**: it removes the *same* R1 double-parse and R2 full-snapshot-clone overhead
  that separates our fork from `termwiz`, using **primitives already in the library**
  (`vte::Handler` dispatch + `Term::damage()`), at **the lowest risk, zero new
  dependencies, and unchanged VT correctness**, on an engine that already works on Windows.
  R1 is already shipped (doc 09).

**Therefore:**
1. **Run the patch-alacritty spike first (§8.8 step 1).** It is the cheapest, most
   reversible, lowest-risk experiment and targets exactly the two costs (R1, R2) that are
   OneTerm's own overhead. (R1 done; R2 remains.)
2. **If it sheds the R1+R2 overhead → adopt it.** Accept that the residual per-frame cost
   is alacritty's (peer-fast) grid, and that delivered throughput is unchanged either way
   (§6.7) — that trade is the price of not owning a VT engine, and it is the right trade.
3. **If it falls short → adopt `termwiz`** (proven lower overhead, Windows-works, pure Rust)
   as the fallback, accepting the larger dependency + migration.
4. **Keep `libghostty-vt` on the radar** and re-spike when its Windows MSVC build is green;
   its render-state API and inherited correctness would make it the strongest option *if*
   it ever builds on Windows.

> Reminder (06 §6.3): none of R1/R2/R3 affects the **render** side — the renderer is
> decoupled from the PTY pump / delivered throughput; this evaluation is purely about the
> pump/engine CPU cost. And per 06 §6.6: even the standing delivered-throughput ceiling is
> fine for *normal* terminal use; the spike is about headroom and the cost structure, not a
> hard requirement.

---

### Sources (checked Nov 2025 / Jul 2026)

- Ghostty: [repo](https://github.com/ghostty-org/ghostty) · [`LICENSE`](https://github.com/ghostty-org/ghostty/blob/main/LICENSE) (MIT) · [`README`](https://github.com/ghostty-org/ghostty/blob/main/README.md) · C API [`vt.h`](https://github.com/ghostty-org/ghostty/blob/main/include/ghostty/vt.h), [`render.h`](https://github.com/ghostty-org/ghostty/blob/main/include/ghostty/vt/render.h), [`screen.h`](https://github.com/ghostty-org/ghostty/blob/main/include/ghostty/vt/screen.h), [`osc.h`](https://github.com/ghostty-org/ghostty/blob/main/include/ghostty/vt/osc.h), [`modes.h`](https://github.com/ghostty-org/ghostty/blob/main/include/ghostty/vt/modes.h) · [announcement](https://mitchellh.com/writing/libghostty-is-coming) · PRs [#11506](https://github.com/ghostty-org/ghostty/pull/11506)/[#8840](https://github.com/ghostty-org/ghostty/pull/8840) · xterm audit [#632](https://github.com/ghostty-org/ghostty/issues/632)
- Rust bindings: [`libghostty-vt`](https://crates.io/crates/libghostty-vt) · [`libghostty-vt-sys`](https://crates.io/crates/libghostty-vt-sys) · repo [`uzaaft/libghostty-rs`](https://github.com/uzaaft/libghostty-rs) · Windows break: issue [#9](https://github.com/Uzaaft/libghostty-rs/issues/9) + [zig#24944](https://github.com/ziglang/zig/issues/24944) · [`windows-ci.yml`](https://github.com/uzaaft/libghostty-rs/blob/master/.github/workflows/windows-ci.yml)
- alacritty patch primitives: [`alacritty_terminal::Term`](https://docs.rs/alacritty_terminal/latest/alacritty_terminal/term/struct.Term.html) — `damage()`/`reset_damage()`, `vte::Handler` impl (`set_title`, `set_hyperlink`, `clipboard_store`/`load`, `set_color`, `set_mode`/`set_private_mode`, `clear_screen`/`clear_line`)
- `termwiz`: [docs.rs](https://docs.rs/termwiz/latest/termwiz/) · [`x86_64-pc-windows-msvc`](https://docs.rs/termwiz/latest/x86_64-pc-windows-msvc/termwiz/) · [wezterm/termwiz](https://github.com/wezterm/wezterm/tree/main/termwiz) — **measured lower per-frame cost than the alacritty fork**
