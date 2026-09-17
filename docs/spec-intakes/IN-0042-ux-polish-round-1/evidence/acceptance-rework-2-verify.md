# Independent verification — `BUG-0070` and the second acceptance-rework round of `US-0117`, `US-0123`, `BUG-0069`

Date: 2026-09-17
Verifier: an independent agent, adversarial brief, own worktree
(`.claude/worktrees/agent-a400d2df984df8ebe`), no access to the implementer's session.

## Verdicts

| Item | Verdict |
|---|---|
| 1. `BUG-0070` — the pump-yield test measures the contract, not the load | **PASS** (2 minor findings) |
| 2. `US-0123` rework — About ships unbound | **PASS** (1 minor finding) |
| 3. `BUG-0069` rework — labels sit level with their indicator | **PASS with findings** (1 major, 2 minor) |
| 4. `US-0117` rework — the ring is the whole cue | **PASS** (1 nit) |
| 5. Focused tests + `pwsh scripts/ci-local.ps1` | **PASS** |
| **Overall** | **PASS with findings** — every behavioural claim held under attack; every finding is in the written record, not in the code. |

No finding asks for a code change. Three ask for a correction in a packet or a rustdoc.

## Correction to the brief: which sha carries the four commits

The brief named the branch tip as `c6f41b32`. It is not: `c6f41b32` is the **oldest** of the four
and carries `BUG-0070` alone. The chain is

```
fe241cfb (main)
  └─ c6f41b32  fix(terminal): the pump-yield test measures the contract, not the load   (BUG-0070)
      └─ a09bee8e  fix(key-bindings): About ships unbound so F1 reaches the terminal again  (US-0123)
          └─ 83a197b7  fix(session-ui): radio and checkbox labels sit level with their indicator  (BUG-0069)
              └─ be6b34e2  fix(terminal-view): the active-Space ring is the whole cue, the number chip is gone  (US-0117)
```

and `worktree-agent-a8dd065c2c935208a` is at `be6b34e2`. Items 2-4 are not present at
`c6f41b32`, so this verification was run on `be6b34e2` (which has `c6f41b32` as an ancestor);
everything below, including this report's own commit, sits on top of it.

## Method

- `git diff main...HEAD` read in full (30 files, +781/-176).
- Every cited line in `gpui`, `gpui-component` and the workspace re-opened and re-read.
- Three mutation probes against `BUG-0070`, built and run.
- The pump-yield test run 20x standalone and 2x under `cargo test --workspace`.
- A **new** GUI walk, driven from this verifier's own `cargo build -p oneterm-app` (debug), own
  pid, isolated `USERPROFILE`/`HOME`/working directory, `PrintWindow(…, 2)` + posted `WM_*`.
  Every pixel measurement below is taken from this walk's frames, not from the implementer's.
- Searches: `Grep` tool / `rtk proxy grep` only. Nothing outside the worktree was modified.

---

## 1. `BUG-0070` — PASS

Packet: `docs/spec-intakes/IN-0032-terminal-crate-tidy/BUG-0070-pump-yield-test-only-passes-under-load.md`.
Change: `crates/terminal/src/handle.rs`, inside `mod tests` only.

### Claims

| Claim | Verdict | Evidence |
|---|---|---|
| No production line changed | **true** | `git diff main...HEAD -- crates/terminal/src/handle.rs` touches only `mod tests`. `Demand` (`handle.rs:53-84`), `lock_for_render` (`:133-137`), `render_demand_raised` (`:147-149`), `raise_render_demand` (`:155-157`) are byte-identical to `main`. |
| The old shape counted from the **raise**, while the pump was unthrottled | **true** | Old line: `let chunks_waited = chunks.load(…) - at_raise;` with `at_raise` taken before `demand.raise()`. The pump's loop body (`:358-364`) is an uncontended `std::sync::Mutex` lock/unlock plus two relaxed atomics. |
| The pump now publishes `asked_at`, and the frame waits for it | **true** | `handle.rs:370-375` (`compare_exchange` from the `u64::MAX` sentinel) and `:390-400` (the wait, 5 s deadline). |
| A 20 ms standing window with the demand raised and the lock untaken bounds the pump at `window/park + 1` = 81 | **true** | `handle.rs:407-410`; `parks_in_window = 20000/250 + 1 = 81`. |
| The frame reads the count **under** its own guard | **true** | `handle.rs:415-425` — `chunks.load` (`:422`) is inside the `engine.lock()` scope, before `demand.release()`. |
| `docs/terminal-backend.md` § 5.1 needs no change | **true** | § 5.1 (`terminal-backend.md:165-218`) specifies *when* a pump asks (a chunk boundary, after the batch's replies) and *what* it does with a `true` (drops the guard, ends the batch). It states no chunk count. Its one latency figure ("about 400 us under a flood") is the ssh path's, unaffected by a test-only change. |

### Does the product really yield? — mutation probes, run here

| Probe | Result | Margin |
|---|---|---|
| **A.** The test pump never parks (`std::thread::sleep(PUMP_PARK)` removed) | **FAIL 3/3** — `while_standing` = 513 834 / 519 447 / 552 355 against 81 | ~6 400x |
| **B.** `Demand::is_raised` back to the one-shot `swap(0, …)` the `US-0082` rework replaced | **FAIL 3/3** — 608 820 / 609 916 / 607 031 against 81 | ~7 500x |
| **C.** The pump never asks (`if false && demand.is_raised()`) — *the packet declared this one by construction and did not run it; run here* | **FAIL** at the 5 s deadline, `handle.rs:393`, "the pump never asked while a frame was waiting". `test result: FAILED … finished in 5.00s`; process wall clock 6.74 s. No hang. | — |

All three mutations were reverted; `git status` is clean against `be6b34e2`.

**Probe A also settles which assertion is load-bearing.** Under A the *old* assertion
(`chunks_waited <= 8`) read 10, 8 and 45 — it would have caught the broken pump only 2 runs in 3.
The standing-window assertion is what makes the test discriminate, exactly as the packet's
"The measurement that actually discriminates" section says.

### Is 81 an upper bound a sleep can only overshoot?

Yes, in the direction that matters. Chunks in the window ≤ `window_actual / park_actual + 1`,
and both sleeps only overshoot. The bound uses the *nominal* ratio, so it is unsafe only if the
20 ms window overshoots **relatively more** than the 250 µs park — the opposite of how any
fixed-granularity timer behaves, because relative overshoot falls as the requested interval
grows. Measured on this host (`thread::sleep` on Windows 11 uses a high-resolution waitable
timer): the effective park is ≈ 555 µs, giving 35-36 chunks against 81, a 2.25x margin.

Instrumented baseline, 5 runs (temporary `eprintln!`, reverted):

```
asked_at=82 while_standing=36 parks_in_window=81 chunks_waited=0 at_contend=119 chunks_in=119 waited=13.6µs
asked_at=10 while_standing=35 parks_in_window=81 chunks_waited=0 at_contend=45  chunks_in=45  waited=14µs
asked_at=44 while_standing=36 parks_in_window=81 chunks_waited=0 at_contend=81  chunks_in=81  waited=7.6µs
asked_at=11 while_standing=35 parks_in_window=81 chunks_waited=0 at_contend=47  chunks_in=47  waited=25.6µs
asked_at=9  while_standing=36 parks_in_window=81 chunks_waited=0 at_contend=45  chunks_in=45  waited=8.2µs
```

This independently reproduces the packet's recorded "while_standing: min 33, max 39" and
"chunks_waited: 0 x 20". **Load can only help**: contention lowers `while_standing` and
`chunks_waited`, both of which are upper-bounded — so unlike the version it replaces, this test
cannot pass because the machine is busy, and cannot fail because it is idle.

### Findings

- **B70-m1 (minor, failure path only): the pump thread leaks when the 5 s deadline fires.**
  The deadline `assert!` at `handle.rs:392-396` panics **before** `stop.store(true)` and
  `pump.join()` (`:430-431`), so the pump thread is detached and spins `while !stop` at full
  speed until the test binary exits. Observed under probe C: 5.00 s of test, 6.74 s of process.
  It cannot hang the run and it only happens on a failing run, but a `scopeguard`-style stop
  before the assert, or an `Option`-checked `stop`/`join` on the error path, would cost one line.
- **B70-m2 (minor): the first wait loop has no deadline.**
  `while chunks.load(Relaxed) < 4 { yield_now() }` (`handle.rs:383-385`) spins forever if the
  pump never reaches 4 chunks. Unreachable in practice (the pump's only panic source is a
  poisoned lock it alone could poison), and it pre-dates this packet — but the packet's claim
  that a broken pump "fails on the deadline instead of hanging" holds for the *second* loop only.
- **Not a finding, recorded for the reader:** § 5.1's opening still calls the demand "a one-bit
  flag" (`terminal-backend.md:182`), which the same paragraph and `handle.rs:44-51` then correct
  ("a count of waiters, not a one-shot flag"). It dates from `US-0082` (`abcceba8`), predates
  this packet, and the packet's scope line ("§ 5.1, if the contract text needs changing") makes
  it a fair thing to have tidied in passing. It changes no claim.

---

## 2. `US-0123` rework — PASS

| Claim | Verdict | Evidence |
|---|---|---|
| `about`'s default is `None` | **true** | `crates/settings-ui/src/key_bindings/key_bindings_actions.rs:120`, with the two-move comment at `:113-119`. |
| The rule test updated | **true** | `key_bindings_actions.rs:440-442` — `assert_eq!(default_for("about"), None);`. |
| `state.rs` effective map updated | **true** | `crates/settings-ui/src/key_bindings/state.rs:403` — `assert_eq!(effective["about"], "");`. |
| `ACCEPTED_SINGLE_CTRL_DEFAULTS` untouched | **true** | `key_bindings_actions.rs:460-465`, still the four rows (`close_panel`, `new_terminal_tab`, `find`, `open_settings`); `f1` was never a bare-`Ctrl` default, so the set has nothing to lose. |
| Nothing else references `f1` as a binding | **true** | The only live `f1` in `crates/` is the terminal view's own key map, `crates/terminal-view/src/input/keys.rs:350` (plus its `US-0108` test) — i.e. exactly where the key now goes. |
| `DEC-0018` amended in Status, the Decision row and the Consequence | **true** | `docs/decisions/DEC-0018-…md:10-15` (Status note), `:83` (Decision row: "unbound *(amended 2026-09-17; shipped as `f1` first)*"), `:194-208` (the `f1` Consequence rewritten from an open tradeoff to `[x] Resolved`, plus a new open tradeoff for the amendment). |
| The app menu shows no hint | **true, reproduced** | `evidence/US-0123-verify2-app-menu.png` — this verifier's own walk. About is the first item and carries no keystroke; Settings shows `Ctrl+,` and Quit `Ctrl+Shift+Q`, so hints are being rendered. |
| The Key Bindings page shows `—` | **true, reproduced byte for byte** | This verifier's own capture of the page is **sha256-identical** to the committed `evidence/US-0123-rw2-key-bindings.png`. Not re-committed for that reason. About OneTerm reads `—`, next to Toggle Gutter's. |
| A user who had overridden About keeps it | **true, reproduced live** | Bound About to `F2` from the page: the row reads `F2` with a `Default: (unbound)` sub-line (`evidence/US-0123-verify2-about-bound-f2.png`) and `ui_config.json` gains `{"about": "f2"}`. `is_at_default("f2", None)` is false (`state.rs:215`), so every non-empty override now persists. |
| Reset on About restores "unbound" | **true, reproduced live** | `on_reset` (`key_bindings_ui.rs:307-313`) reads `.and_then(\|a\| a.default).unwrap_or_default()` → `""`. Clicking Reset removed `about` from `ui_config.json` entirely and returned the page to a capture **sha256-identical** to the pre-override one. |

### Finding

- **US123-m1 (minor): one Gaps sentence is false for exactly the user it names.**
  `US-0123-…md:521-523` says "**A user who bound About to `f1` by hand keeps it.** … anyone who
  typed it deliberately keeps their override". They do not. `overrides_from_effective`
  (`state.rs:222-233`) drops any entry for which `is_at_default` holds, and while the default was
  `f1`, `is_at_default("f1", Some("f1"))` is true — so a deliberately typed `f1` was never
  written to `ui_config.json` and that user comes back unbound. The preceding half of the same
  sentence states the rule that makes it false. A user who bound About to anything *else* does
  keep it, which is what the live probe above shows. Impact is a rebind; the record is what needs
  the correction. (The same mechanic was already noted by an earlier verification at
  `evidence/settings-ui-wave1-verify.md:323`.)

---

## 3. `BUG-0069` rework — PASS with findings

### The mechanism

| Claim | Verdict | Evidence |
|---|---|---|
| gpui clips every text line to its own line box | **true** | `paint_line` builds `line_bounds` of height `line_height * (wraps + 1)` and opens `window.paint_layer(line_bounds, …)`; `paint_layer` intersects with the content mask and **pushes a scene layer** over it. |
| `padding_top = (line_height − ascent − descent) / 2`, negative on a ~1.2 em font at `relative(1.)`, so the descender leaves the layer | **true** | The formula is one line below the `paint_layer` call. Bottom of glyph = `padding_top + ascent + descent = (lh + a + d)/2 > lh` whenever `a + d > lh`. |
| The clip is the line box and not `Checkbox`'s `overflow_hidden` | **true, and this correction matters** | `gpui-component-0.6.0/src/checkbox.rs:317` has `overflow_hidden`; `radio.rs` has **none anywhere** — yet the `Radio` clipped too. The first rework's mechanism ("it is the clip [of the slot] that does the damage") was wrong; this one is right. |
| `Radio`/`Checkbox` apply `refine_style(&self.style)` **after** their own `items_start()`, so `.items_center()` at the call site wins | **true** | `radio.rs` — `impl Styled for Radio` at `:136-140`, `.items_start()` at `:198`, `.refine_style(&self.style)` at `:211`, same `h_flex()` chain. `checkbox.rs` — `impl Styled` at `:119-123`, `.items_start()` at `:257`, `.refine_style(&self.style)` at `:271`. |
| Five call sites, all given `.items_center()` | **true, and they are all of them in `session-ui`** | `auth_form.rs:123`, `session_dialog.rs:291`, `session_dialog.rs:585`, `connect_dialog.rs:199`, `quick_connect_dialog.rs:146`. A grep for `Radio::new` / `Checkbox::new` / `RadioGroup::` across `session-ui`, `settings-ui`, `sftp-ui`, `terminal-view` and `workspace` returns these five plus `sftp-ui/src/edit.rs:600` and `sftp-ui/src/render.rs:242`, which still use `.label(…)` — the carried gap, disclosed at `BUG-0069-…md:477-479`. |

### Measurements — re-taken on this verifier's own build

`evidence/BUG-0069-verify2-session-dialog.png`, 1600x1000, default dark theme, 16 px UI font.
Ink is any pixel more than 26/255 from the dialog ground on any channel; the result is stable at
thresholds 10, 20 and 40.

The bands are **pixel-identical to the implementer's committed frame**: indicator rows 520-535
and 690-705, label ink 523-534 and 693-708, on both captures. The frames are authentic.

| Row | Indicator rows (centre) | Cap top (all-caps run) | Baseline | Cap centre | Indicator low by | Descender rows |
|---|---|---|---|---|---|---|
| Authentication → Private Key (`Radio`) | 520-535 (527.5) | 524 (`P`, and `SSH` of "SSH Agent") | 535 | 529.0 | **1.5 px** | `y` 535-538 = **4** |
| Logging → Use global (`Radio`) | 690-705 (697.5) | 694 (`U`) | 705 | 699.0 | **1.5 px** | `g` 705-708 = **4** |
| Forward the SSH agent… (`Checkbox`, committed frame) | 663-678 (670.5) | 667 (`SSH`) | 678 | 672.0 | **1.5 px** | `g` of "agent" 678-681 = **4** |
| **Before**, `evidence/after/13-new-ssh-session-dialog.png` | 463-478 (470.5) | 471 (`SSH`) | 482 | 476.0 | **5.5 px** | — |

**The descender is whole — 4 rows below the baseline — on every control.** That half of the
acceptance passes outright, and the `y` of "Key" is visible in this verifier's own 4x crop
(`evidence/BUG-0069-verify2-auth-row-4x.png`).

**The improvement is exactly 4.0 px** (5.5 → 1.5), which is 0.25 em at a 16 px font — precisely
the leading the packet's own explanation predicts `items_center` would take back.

### Findings

- **B69-MAJOR-1: the rustdoc that carries the root cause cites a crate the project does not
  build.** `crates/state/src/form_dialog.rs:100` and `BUG-0069-…md:348` both name
  `gpui-0.2.2/src/text_system/line.rs`. The workspace's `gpui` is **`gpui-pre`**
  (`Cargo.toml:40` — `gpui = { package = "gpui-pre", version = "0.3" }`), resolved to
  `gpui-pre 0.3.3`; there is no `gpui` package in `Cargo.lock` at all. The mechanism is
  nonetheless correct: the same `paint_line` / `paint_layer` / `padding_top` code is at
  `gpui-pre-0.3.3/src/text_system/line.rs:344-364`, and `paint_layer` at
  `gpui-pre-0.3.3/src/window.rs:4134-4143`. So this is a wrong pointer under a right conclusion —
  but it is *the* pointer, in the rustdoc a future reader will follow, and the repository's own
  convention everywhere else is `gpui-pre-0.3.x/…` (five other docs, including
  `evidence/session-ui-wave1-verify.md:8,151,427` in this same intake). Ranked major because
  this packet's previous round was reopened for, among other things, **two wrong crate names**.
  Fix: substitute the `gpui-pre-0.3.3` paths and line numbers above.
- **B69-m2 (minor): "within 1 px" is 1.5 px; the 0.5 px comes from counting an ascender as the
  cap top.** `measure.py`'s `report` returns the topmost ink row of whatever x-range it is
  given, and the ranges used were whole labels: "Private" tops out at 523 on its `t`/`i`-dot, not
  at the `P`'s 524; "Use global" at 693 on its `l`/`b`, not at the `U`'s 694. Measured against an
  unambiguous all-caps run instead, every control is 1.5 px, and the before-frame is 5.5 px
  rather than 5.0. The *delta* the packet claims is exact; both absolute numbers are 0.5 px
  optimistic, and the acceptance line "within 1 px on every control" does not literally hold.
  The residual is inherent — centring a 1.5 em line box puts `(ascent − descent) / 2` above the
  baseline on the indicator's centre — and the packet's own Gaps already explain it; the number
  is what needs correcting, not the fix.
- **B69-m3 (minor): the line ranges cited for the "after" argument do not contain it.**
  For "both apply `.refine_style(&self.style)` after their own `.items_start()`",
  `form_dialog.rs:88-90` cites `radio.rs:136-140,195-199` and `checkbox.rs:119-123,255-257`, and
  `BUG-0069-…md:362,368-370` cites the same spans. Those cover `impl Styled` and the
  `.items_start()` call only; the `.refine_style(&self.style)` the argument turns on is
  at `radio.rs:211` and `checkbox.rs:271`. The ordering is real — that is why the fix works — but
  a reader checking the citation cannot see it.

---

## 4. `US-0117` rework — PASS

| Claim | Verdict | Evidence |
|---|---|---|
| `space_number_chip` / `space_chip_tooltip` and their test deleted | **true** | Neither name appears anywhere under `crates/` any more. `space/render.rs`'s `mod tests` keeps only `space_border_color`'s two tests. |
| The corner slot is back to badge-only | **true, and provably byte-for-byte** | `git diff 03763c40^ be6b34e2 -- crates/terminal-view/src/space/render.rs` — the diff against the **pre-`US-0117`** file contains no line of the badge/corner code at all. The only residue is `active_cue_ring`, its rustdoc, the `cue` binding, the `Div` import, and the new explanatory comment. |
| The ring is untouched | **true** | `active_cue_ring` (`space/render.rs:210-212`) and `space_border_color` unchanged; the render site `:190` unchanged. |
| No dead code | **true** | `display_number` still used (`space/render.rs:250`, `input/menu.rs:138`, `terminal_view/agent_status.rs:51`). `trim_path_title` still used by its own module (`panel/tab_title.rs:73`) — dropping the `pub(crate) use` from `panel/mod.rs` (the line after `:22`) leaves it reachable and used. `cargo clippy --workspace --all-targets -- -D warnings` clean (gate, below). |
| Frames 09/09b show the ring and no chip | **true, reproduced** | `evidence/US-0117-verify2-09-split-two-terminals.png` (ring on the right Space) and `…-09b-split-focus-moved.png` (ring on the left after a click). Neither Space carries a `#N`, and the corner of both is empty because neither is a channel member. Captured on this verifier's own build and walk. |
| `docs/terminal-split.md` decision 8 and `docs/gui-layout.md` amended | **true** | `terminal-split.md:105-115`, `gui-layout.md:141-143`; both read as a record of what was tried and removed, and neither now describes a chip as shipping. |

### Nit

- **US117-n1: the "What changed" table over-reports one import.** `US-0117-…md:365` says the
  deleted functions took "the `Tooltip`/`Stateful`/`SharedString` imports they alone needed".
  `SharedString` was correctly **kept** (`space/render.rs:7`) — it is still used at `:35`. Only
  `Tooltip`, `Stateful` and `prelude::FluentBuilder` went.

---

## 5. Commands

All run from this worktree at `be6b34e2`, `CARGO_BUILD_JOBS=6`.

**The 20-run tally** —
`cargo test -p oneterm-terminal --lib -- --exact handle::tests::a_pump_yields_to_the_demand_within_a_bounded_number_of_chunks`,
20 consecutive standalone runs on an otherwise idle machine:

```
passed=20 failed=0
```

(`chunks_waited` 0 on every instrumented run, bound 8; `while_standing` 35-36, bound 81.)

**Workspace** — `cargo test --workspace`, 2 runs: `exit=0`, `exit=0`, no `test result: FAILED`
line in either.

**Focused** — `cargo test -p oneterm-terminal -p oneterm-settings-ui -p oneterm-state -p oneterm-session-ui -p oneterm-terminal-view`:

```
test result: ok. 218 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out   (oneterm-terminal)
test result: ok. 351 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out   (oneterm-terminal-view)
test result: ok.  72 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok.  52 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok.  41 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

734 tests, 0 failures, exit 0.

**Gate** — `pwsh scripts/ci-local.ps1`:

```
==> python scripts/check-english.py
English contributor-text check passed for 962 files.

==> python scripts/completion-catalog.py validate
[completion-catalog] all catalogs valid

==> python scripts/check-theme-contrast.py
check-theme-contrast: 702 foreground/surface pairings across 117 token/variant rows, all >= 4.5:1

==> python scripts/third-party-notices.py --check
THIRD-PARTY-NOTICES.md is up to date.

ci-local: all checks passed.
```

Every step ran, in the order `AGENTS.md` § 4 lists, and the run includes
`cargo clippy --workspace --all-targets -- -D warnings` (twice, the second with
`oneterm-app/terminal-diagnostics`) — which is what retires the dead-code question in item 4.
`check-english.py` also covered this report.

## Frames taken by this verification

All from one walk on this verifier's own debug build, 1600x1000, default dark theme, isolated
profile. Named `<ID>-verify2-<scene>.png` beside this file.

- `BUG-0069-verify2-session-dialog.png` — the New SSH Session dialog; the source of every
  measurement in item 3.
- `BUG-0069-verify2-auth-row-4x.png` — the Authentication row at 4x (Password / Private Key /
  SSH Agent). Indicators level with the labels, the `y` of "Key" whole.
- `BUG-0069-verify2-logging-row-4x.png` — the Logging row at 4x (Use global / On / Off). The `g`
  of "global" whole.
- `US-0123-verify2-app-menu.png` — the application menu; About first, no shortcut hint.
- `US-0123-verify2-about-bound-f2.png` — the Key Bindings page with About deliberately bound to
  `F2`: the chip reads `F2` and the row gains a `Default: (unbound)` line.
- `US-0117-verify2-09-split-two-terminals.png` / `-09b-split-focus-moved.png` — scenes 09 and
  09b; the ring moves with the active Space and no Space carries a `#N`.

Two more captures were taken and **not committed, because they came out sha256-identical to
`US-0123-rw2-key-bindings.png`**: this verifier's own Key Bindings page, and the same page after
clicking Reset on About. That identity is the evidence, and a duplicate file would not add to it.

## Gaps in this verification

- **Windows only.** The mutation probes, the 20 runs and every frame are from one Windows 11
  host. `BUG-0070`'s bound rests on `std::sync::Mutex` being unfair and on `thread::sleep`
  parking; both hold everywhere OneTerm builds, but CI is where the other two platforms run.
- **The walk posts `WM_*` messages**, which set no real modifier state. Nothing here demonstrates
  that a terminal program now *receives* `F1` — that remains the code claim `US-0123` records.
  What is demonstrated is that About is unbound, shows no hint and resets to unbound.
- **The app was built in `debug`, not `fast-dev`.** The scenes are static dialogs and splits, so
  the profile changes no pixel; a flood-dependent scene would need `fast-dev`.
- **`harness.db` was not read or written**, per the brief. Whether the three `Reopened` status
  boxes and `BUG-0070`'s `Implemented` box match the database is unverified here.
- **The `sftp-ui` `.label(…)` sites were not re-measured.** `edit.rs:600` ("Always upload this
  file while editing" — three descenders) and `render.rs:242` still clip by the mechanism this
  packet proved. They are a disclosed carried gap, not a regression, and are out of `BUG-0069`'s
  scope; they belong to whoever sweeps them next.
- **No test pins any of it.** `CONTROL_LABEL_LINE_HEIGHT = 1.5`, the five `items_center()` call
  sites, and the absence of a sixth control are all unpinned, exactly as `BUG-0069`'s Gaps say.
