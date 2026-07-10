# 9. Design — Patch the `alacritty_terminal` Fork In Place

> **STATUS: R1 IMPLEMENTED · R2/R3 PROPOSED.** The lower-risk middle path first
> sketched in [`07-custom-engine-rfc.md`](07-custom-engine-rfc.md) §7.9.1: instead of
> replacing the engine (RFC 07) or adopting `libghostty-vt`
> ([`08`](08-libghostty-vt-evaluation.md)), **keep `alacritty_terminal` and add
> targeted patches to the fork** to attack the three costs (R1/R2/R3) from
> [`06-results-and-ceiling.md`](06-results-and-ceiling.md) §6.4.
>
> **R1 (the double-parse) is now implemented** — the fork is vendored under `vendor/`
> and OneTerm no longer runs a second `vte::Parser`. See §9.3.1 for what shipped.
> R2 (snapshot pooling) and R3 (grid mutation) remain proposals.
>
> **Precedent:** the dependency was *already* a patched fork —
> `zed-industries/alacritty @ fcf32fea…`, carrying `TerminalContent` / `display_iter`
> additions (see `docs/agents/dependencies.md` §3). Patching it further is a proven,
> in-tree-friendly technique, not a new capability.

---

## 9.1. Why patch instead of replace

| Property | Patch the fork (this doc) | Custom engine (07) | libghostty-vt (08) |
|---|---|---|---|
| The ~110 alacritty-typed sites | **unchanged** (types stay identical) | must migrate all | must migrate all |
| VT correctness | **inherited** (alacritty) | we own it | inherited (Ghostty) |
| Build complexity | pure Rust, no new toolchain | pure Rust | Zig + FFI + MSVC |
| Windows risk | **none** (works today) | none | unverified |
| Effort | **low–medium** (scoped patches) | very high | medium–high |
| Effect on delivered throughput | **none** — producer/ConPTY-bound (§6.7) | none (same reason) | none (same reason) |
| Effect on CPU/parse headroom | **large** (R1 shipped: ~+70% parse; R2/R3 free lock/CPU) | large | large |

The whole appeal: **capture the achievable wins with the least risk and zero consumer
migration.** These wins are **CPU/parse headroom, fewer allocations, and shorter lock
holds** — not delivered throughput: per §6.7 that is producer/ConPTY-bound and
size-dependent, so no fork patch moves it. It is the pragmatic first move and can be
shipped incrementally, one patch at a time, each gated on the existing instrumentation.

---

## 9.2. The three costs, mapped to concrete patches

Recap from 06 §6.4 (pump measured at **72% busy, 29 MiB/s** on DOOM-fire):

| Cost | What it is | Patchable in the fork? | Expected win |
|---|---|---|---|
| **R1** double-parse | every byte runs through `Processor::advance` **and** a 2nd `vte::Parser` (`OscSink`) for OSC 7/9/52/133 + `CSI 2J/3J` clear | **Yes — cleanest win.** Surface those events from alacritty's handler → delete OneTerm's 2nd parser | ~15–20% of pump |
| **R2** snapshot clone | `TerminalContent::from()` clones ~5.4k `IndexedCell` into a fresh `Vec` **under the `FairMutex`** every frame (~250–350 KB, ~200 µs lock hold) | **Partly, and mostly *without* a fork patch** (pool the buffer in `oneterm-core`); optional in-fork double-buffer for the lock hold | ~2–3% of pump |
| **R3** grid mutation | alacritty writing ~5.4k cells/frame (template copy + per-cell damage marking + wrap/wide-char checks) — the **dominant** cost | **Partly** (batch damage, leaner hot write path); full SoA storage = near-rewrite → out of scope | ~5–10% (shallow); large only via rewrite |

---

## 9.3. Patch R1 — surface OSC + clear from the fork (the key win)

**Today:** `crates/local/src/event_loop.rs` and `crates/ssh/src/task.rs` feed every byte
to a **second** `vte::Parser` driving `core/osc.rs::OscSink`, purely to catch what
alacritty's `EventListener` drops (OSC 7 cwd, OSC 9 notify/progress, OSC 52 clipboard,
OSC 133 shell-integration) and to detect `CSI 2J/3J`/RIS screen clears.

**Note this also removes a blocker:** the clear-detection coupling (06 §6.6.C) meant R1
could not be removed in pure-OneTerm code, because `OscSink` needs to see all CSI to
detect `CSI 2J/3J`, and DOOM-fire is entirely CSI. **Patching the fork dissolves that
blocker** — the clear signal comes from alacritty's own handler, which already parses the
CSI once.

**Patch:** extend the fork's ANSI handler / `EventListener` surface so a single parse
emits the events OneTerm needs:

- Route **OSC 7 / 9 / 52 / 133** (and, if not already, palette OSC 4/10/11/12, OSC 8
  hyperlink is already stored in the cell) to `Event`/listener callbacks.
- Emit a **`ScreenCleared`**-style signal on `CSI 2J` / `CSI 3J` / RIS (for the gutter
  timestamp reset that `OscSink::take_clear()` currently provides).

**OneTerm cleanup after the patch:**
- Delete the second `vte::Parser` + `OscSink` drive loop from `event_loop.rs` /
  `task.rs`; consume the new listener events in `local/listener.rs` / `ssh/listener.rs`
  instead (they already implement `EventListener` and handle Title/Clipboard/Colors).
- `core/osc.rs` shrinks to the payload types + `parse_cwd_url` / OSC52 codec (the
  `Perform` impl and its parser go away). Its extensive unit tests migrate to exercise
  the listener path.

**Risk:** low–medium. It changes fork behaviour (more events emitted) but not grid
semantics; covered by the existing `osc.rs` tests re-pointed at the listener.

### 9.3.1. What shipped (R1)

Implemented and green (`fmt` / `clippy -D warnings` / `build` / `test`):

- **Fork vendored** under `vendor/vte` (0.15.0) + `vendor/alacritty_terminal`
  (`fcf32fea…`), wired via `[patch]` in the root `Cargo.toml` and excluded from the
  workspace. This intentionally breaks the upstream alacritty rev-lock (see
  `docs/agents/dependencies.md`).
- **vte**: added `Handler::report_osc(params, bell_terminated)` (default no-op) and
  call it from `osc_dispatch`'s fallthrough, so every OSC vte doesn't handle (7/9/133/…)
  is forwarded.
- **alacritty_terminal**: added `Event::Osc { params, bell_terminated }` and
  `Event::ClearScreen`; `Term` forwards `report_osc` → `Event::Osc`, and emits
  `Event::ClearScreen` from `clear_screen` (`All`/`Saved` = `CSI 2J/3J`) and
  `reset_state` (RIS).
- **OneTerm**: `OscSink` (the second-parser `Perform` impl) is gone, replaced by a pure
  `parse_osc(&[&[u8]]) -> Option<OscPayload>` in `core/osc.rs`. `LocalListener` /
  `SshListener` handle `Event::Osc` (→ `parse_osc` → cwd/notify/progress/shell-integration)
  and `Event::ClearScreen` (→ `clear_epoch += 1`); OSC 52 query now routes
  `Event::ClipboardLoad` → `SessionEvent::ClipboardRead`. Both PTY pumps
  (`local/event_loop.rs`, `ssh/task.rs`) dropped the `vte::Parser` + `osc_sink` loop —
  now a **single** `Processor::advance` pass.
- **Measured on hardware (DOOM-fire, 45×~120):** the pump dropped from
  **72% busy / parse-bound** (06 §6.4) to **~44% busy / wait-bound**
  (`30 MiB/s | parse≈890ms wait≈1110ms over 2.0s`). Extrapolated parse capacity rose
  from ~40 MiB/s to **~69 MiB/s (~+70%)** — i.e. R1 removed ~40% of parse time, more
  than the ~15–20% predicted below. **The parser is no longer the limiter**; the pump
  is now wait-bound on ConPTY delivery. The remaining per-frame cost has shifted to the
  **render** (`paint_us≈15–21ms`, `quads≈13,280`, `bg_rects≈5,425`, `shapes=1 runs=1`
  — Tier-1 text batching intact), i.e. doc 03 territory, not the pump.

---

## 9.4. Patch R2 — kill the per-frame allocation (mostly no fork patch)

`TerminalContent::from()` (`core/content.rs`) allocates a new
`Vec<IndexedCell>` (~5.4k × ~48 B) **every frame while holding the lock**, then clones
each `Cell`.

**Step 1 — pool the buffer (no fork patch, do this first):**
- Give the session a reusable `Vec<IndexedCell>` (or an SoA scratch: `Box<[char]>` +
  `fg`/`bg`/`flags` arrays). Add `TerminalContent::fill_into(&mut self, term)` that
  `clear()`s and refills the pooled buffer instead of allocating. Eliminates ~250–350 KB
  of alloc/free churn per frame.
- Keep the lock hold minimal: copy raw fields fast; compute nothing derived under the
  lock.

**Step 2 — optional in-fork double buffer (bigger, medium risk):**
- Patch the fork so the render thread reads a back-buffer snapshot the pump swaps at
  frame boundaries, removing the ~200 µs lock hold entirely. Only worth it if step 1's
  measurement shows the lock hold still matters.

**Reality check:** R2 is ~2–3% of the pump (06 §6.2 measured ~200 µs/frame). Step 1 is
cheap and worth doing; step 2 has a poor effort/return ratio unless contention shows up.

---

## 9.5. Patch R3 — the dominant cost (only partly reachable)

alacritty's per-cell write path (template copy + `damage` marking + wrap/wide checks)
is the bulk of the 72%. Reachable, low-to-medium-risk fork patches:

- **Batch per-line damage.** For a full-row rewrite, mark the row dirty once instead of
  per cell. DOOM-fire rewrites whole rows → this removes repeated damage bookkeeping.
- **Leaner ASCII/BMP fast path** in `input()` for the common non-wide, non-combining
  case (skip checks that only matter for wide/zero-width).

**Out of scope for "patch":** converting alacritty's `Row<Cell>` to structure-of-arrays
storage. That rewrites the fork's core data structures and is **as risky as RFC 07** —
if you are going that deep, do RFC 07 or adopt libghostty-vt (08) instead. This doc
deliberately stops short of it.

So R3 via patching yields only the shallow wins (damage batching, fast path): useful
pump-CPU headroom, but per §6.7 it does **not** change delivered throughput — the pump is
wait-bound on ConPTY, which is the actual limiter.

---

## 9.6. Expected outcome (honest math)

Stacking the *low-risk* patches on the measured 72%-busy pump:

- R1 (delete double-parse): ~ −15–20%
- R2 step 1 (pool buffer): ~ −2–3%
- R3 shallow (damage batch + fast path): ~ −5–10%

→ pump ~**72% → ~48–55% busy**, parse throughput ~**1.3–1.4×**. Read these as **headroom**,
not delivery: §6.7 shows delivered throughput is producer/ConPTY-bound and size-dependent,
so freeing pump CPU does **not** raise it. What these numbers mean is less CPU per frame,
more parse budget, and room for more concurrent sessions.

This is consistent with 06 §6.7: the delivery floor is set outside OneTerm (producer +
ConPTY), which no fork patch changes; what the fork patches change is OneTerm's own
per-frame cost.

> **Measured update (R1 only).** The real drop was larger than predicted: pump
> **72% → ~44% busy**, and it flipped from **parse-bound to wait-bound** — parse
> capacity ~+70% (~40 → ~69 MiB/s). R1 alone therefore over-delivered vs the ~15–20%
> estimate, and it moved the limiter **off the parser entirely**. The remaining
> per-frame cost is now the **render** (`paint_us≈17ms` for ~13k quads / ~5.4k bg
> rects), so R2 (snapshot pooling) is unlikely to change delivered throughput — the next
> lever is the quad/bg-rect count on the render side (doc 03) for render CPU, and/or ConPTY
> delivery, which is now what the pump waits on. R3 (grid-mutation batching) still applies
> but is no longer the top pump cost.

---

## 9.7. Fork mechanics & rev-lock

- **Own a fork.** Fork `zed-industries/alacritty` → an OneTerm-owned repo; add the R1
  (and optional R2/R3) commits on top of `fcf32fea…`. Point the workspace `Cargo.toml`
  at the owned fork's rev (or use `[patch."https://github.com/zed-industries/alacritty"]`).
- **gpui rev-lock is untouched.** gpui does **not** depend on `alacritty_terminal`
  (only OneTerm does — dependencies.md §1 vs §3), so this change is orthogonal to the
  pinned gpui/gpui-component revs.
- **Rebase cost.** The only ongoing burden is rebasing the OneTerm patches onto future
  `zed-industries/alacritty` updates *if/when* we bump for `TerminalContent`/
  `display_iter` improvements. The patch set is small and localized (handler +
  damage), so rebases should be light.
- **Follow dependencies.md §3:** swapping to an owned fork is a dependency decision →
  open an issue and record the new rev in dependencies.md §1/§3.

---

## 9.8. Rollout & testing (incremental, each gated)

1. **R1 first** — highest clean win, self-contained, and it removes the clear-detection
   blocker. Land the fork handler patch + delete `OscSink`'s parser; re-point
   `osc.rs` tests at the listener; re-measure the `[PTY pump]` line.
2. **R2 step 1** — pool the snapshot buffer in `core/content.rs`; verify with
   `prepaint_us` and a lock-hold micro-measurement.
3. **R3 shallow** — damage batching + ASCII fast path in the fork; re-measure.
4. Stop when the curve flattens; anything beyond is RFC 07 / doc 08 territory.

**Testing:** keep the existing `osc.rs`, `search.rs`, `url.rs`, `content.rs`,
box-drawing tests green (they are the parity net); add a differential test that the
fork's new OSC/clear events match what `OscSink` produced for the same byte streams
(so R1 is behaviour-preserving); re-run the perf meter per 06 §6.6.

---

## 9.9. Recommendation

If any further work is funded, **do R1 first** — it is well-scoped, low-risk, keeps all
types, deletes real code (the whole `OscSink` parser), removes the double-parse
(~15–20%), and dissolves the clear-detection blocker. **R1 is now shipped** (§9.3.1) and
delivered ~+70% parse headroom. Then measure and decide whether R2 step 1 / R3 shallow
are worth it. Treat this doc as the **"cheaper CPU/parse headroom, not a rewrite"** path:
it frees pump CPU with minimal risk. It does **not** change delivered throughput (that is
producer/ConPTY-bound and size-dependent — §6.7), so do not fund it expecting higher
throughput. Only escalate to RFC 07 or libghostty-vt (08) if there is a
*throughput-independent* motivation (owning the engine, dropping the fork, a non-ConPTY
transport) that is a hard requirement.
