# US-0111 — independent verification

Verifier: adversarial second party. Nothing here was written by the implementer.
Target: `feat/us-0111-secondary-text-contrast-floor` @ `48131d7e` (3 commits on `main` @ `74d842e7`).
Method: every ratio below was recomputed with a WCAG 2.1 relative-luminance function written
from the spec in a scratch file, never by calling `scripts/check-theme-contrast.py`. Its own
sanity anchors: `#ffffff`/`#000000` = 21.0000, `#767676`/`#ffffff` = 4.5422,
`#5c6370`/`#23272e` = 2.4793.

## Verdict

**FAIL** — narrow, fixable, and everything except the surface list holds up.

The arithmetic, the fallback chain (one entry excepted), the gate wiring, the docs
corrections and the screenshots are all correct and reproduce exactly. What does not hold is
the claim the whole packet rests on: that each token is measured *against every surface it is
drawn on*. `scripts/check-theme-contrast.py:43-56` omits three surfaces that OneTerm
provably draws `muted.foreground` on, and on those surfaces **20 measurements across 9
variants are still below 4.5:1 — including the default theme at 4.38:1, and the key-binding
chip that finding `F32` is about**. The packet's own Risks section names this exact failure
mode ("The wrong background … will pass text that is still unreadable"); it materialised.
`docs/gui-layout.md:110-121` now states the stronger claim as fact, and `AGENTS.md:71` points
the next theme author at a check that does not cover it.

Remedy is small: add three surfaces to `SURFACES` and re-solve ~9 values.

## Claims

### Claim 1 — the check measures the right things — PARTIAL

Verified good:

- Ratio maths. Eight+ values from the packet's table re-derived independently, all to
  within 0.01:
  | Case | Packet | Independent |
  | --- | --- | --- |
  | Zed One Dark `muted.foreground` `#8f96a3` on `list.even.background` `#262a32` | 4.84 | 4.8363 |
  | Zed One Dark `muted.foreground` before `#5c6370` on `#262a32` | 2.38 | 2.3799 |
  | Zed One Dark `tab.foreground` on `tab.background` (both surfaces: `tab_bar` 5.3752, `tab` 5.0383 — worst wins) | 5.04 | 5.0383 |
  | Zed One Dark `table.head.foreground` `#89919e` on `#1e2227` (explicitly declared, not inherited) | 5.03 | 5.0312 |
  | Adventure `muted.foreground` `#7f8489` on `title_bar.background` `#0f1112` | 5.02 | 5.0162 |
  | Adventure `tab.foreground` `#747f88` on a fully transparent `tab.background` `#04040400` | 5.01 | 5.0122 |
  | Adventure `table.head.foreground` **alpha** `#feffffb3` over `#040404`, surface inherited `table.head → list.head → list → background` | 9.89 | 9.8929 |
  | Aurora Light `muted.foreground` `#5c6a7f` on the **gradient** `status_bar.background` `linear-gradient(180deg,#F8FAFC,#F1F5F9)`, worst stop | 5.01 | 5.0140 (worst stop; first stop 5.2499) |
  | Aurora Light `table.head.foreground` `#64748B` on `table.head.background` `#F8FAFC` | 4.55 | 4.5484 |
  | Alduin `table.head.foreground` **fallback** → `muted.foreground` `#959595` | 5.69 | 5.69 |
  So alpha compositing (`:112-118`), gradient stops (`:120-126`) and the
  `table.head.foreground → muted.foreground` fallback all behave as claimed.
- The inventory. An independent implementation over `main`'s theme JSONs reproduces
  **39 variants, 117 measurements, 68 below 4.5:1** exactly; over the branch, 117 / 0 below.
- **No regression.** All 117 before/after pairs compared independently: 0 got worse. Also 0
  got worse on the three omitted surfaces (all 20 residual failures improved, e.g. Zed One
  Dark 2.16 → 4.38).
- Self-test runs on every invocation (`scripts/check-theme-contrast.py:216`), not only under
  `--self-test`.
- Stdlib only, offline: imports are `argparse`, `json`, `re`, `sys`, `pathlib`
  (`:22-27`). No network call anywhere.

**MAJOR — the surface list is incomplete.** `SURFACES` (`scripts/check-theme-contrast.py:43-56`)
lists ten surfaces for `muted.foreground`. Three more are drawn in the shipping UI:

1. `muted.background` — the key-binding chip. `Kbd` renders
   `.text_color(cx.theme().muted_foreground).bg(cx.theme().tokens.muted)`
   (`reference/gpui-kit/crates/component/src/kbd.rs:223-225`; `outline` defaults to `false`,
   `kbd.rs:35`), and OneTerm builds the chip with plain `Kbd::new(stroke)`
   (`crates/settings-ui/src/key_bindings/key_bindings_ui.rs:166`). Confirmed from the
   packet's own evidence PNG: the `Ctrl+W` chip fill is `#1e2227`, not the `#23272e` the
   packet's E2E table measured against.
2. / 3. `list.hover.background` and `table.hover.background` — the hovered **and selected**
   row. OneTerm sets the selected SFTP row's background to `table_hover`
   (`crates/sftp-ui/src/table_delegate.rs:284-285`) and draws that row's cells in
   `muted_foreground` (`table_delegate.rs:317`); `crates/theme/src/theme.rs:69-77`
   (`apply_list_style_override`) makes `list_active = list_hover` for the session tree, whose
   host address is `muted_foreground` (`crates/session-ui/src/tree_render.rs:136`).

Measured independently on those surfaces (kit fallbacks applied: `table.hover → list.hover`,
`list.hover → accent.opacity(0.6)` over `background`, `schema.rs:957`):

| Variant | Surface | After |
| --- | --- | --- |
| Catppuccin Mocha | `muted.background` `#302d41` | **4.06:1** |
| Tokyo Moon | `muted.background` `#2d3149` | **4.19:1** |
| Mellifluous Dark | `muted.background` `#44444455` | **4.19:1** |
| Adventure Time | `list`/`table.hover.background` | **4.21:1** |
| Tokyo Storm | `list`/`table.hover.background` | **4.25:1** |
| Jellybeans | `muted.background` `#242424` | **4.26:1** |
| Mellifluous Dark | `list`/`table.hover.background` | **4.27:1** |
| Tokyo Moon | `list`/`table.hover.background` | **4.33:1** |
| Fahrenheit | `table.hover.background` `#1E1E1E` | **4.34:1** |
| **Zed One Dark (default theme)** | `list`/`table.hover.background` `#2c313c` | **4.38:1** |
| Jellybeans | `list`/`table.hover.background` | **4.42:1** |
| Matrix | `list`/`table.hover.background` | **4.43:1** |
| Adventure Time | `muted.background` `#29274a` | **4.50:1** |

20 measurements, 9 variants. The affected text is exactly what `F6` named — session host
addresses and SFTP dates — in the state a user is in whenever a row is selected, in the theme
the app starts in.

**MINOR — one fallback entry does not mirror the kit.** `FALLBACKS["sidebar.background"] =
"background"` (`scripts/check-theme-contrast.py:65`). The kit applies
`sidebar, fallback = self.background.blend(self.border.opacity(0.15))`
(`reference/gpui-kit/crates/component/src/theme/schema.rs:977-980`). 36 of 39 variants omit
`sidebar.background`, so the script measures a surface the application never paints. Real
ratios run 0.1–0.35 lower (e.g. Mellifluous Dark 4.95 → 4.62, Matrix 5.05 → 4.82); none cross
the floor today, so no value is wrong because of it — but the "mirrors the kit's fallback
chain" claim is false at that entry.

**MINOR — a missing token crashes instead of failing.** `muted.foreground` has no `FALLBACKS`
entry, so a future theme that omits it (the kit falls back to
`self.muted.blend(self.foreground.opacity(0.7))`, `schema.rs:776-779`) makes `resolve` raise an
unhandled `ValueError` traceback rather than print a diagnosis. Reproduced:
`ValueError: muted.foreground: no value and no fallback (stopped at muted.foreground)`. Same
path reaches `table.head.foreground`.

**MINOR — compositing base.** `measure` (`:142-154`) composites every translucent surface over
`background`. A translucent `tab.background` is actually painted on `tab_bar.background`. For
Zed One Dark this makes the reported 5.04 pessimistic against the real 5.38, so it errs safe
here, but the model is not generally conservative.

### Claim 2 — hierarchy kept — CONFIRMED

Worst-case `foreground` vs the raised secondary tokens over the same surface set, computed
independently for all 39 variants. Exactly the two inversions the implementer disclosed:

- `Solarized Light`: `foreground` **4.39:1**, `muted.foreground` 5.02:1, `tab.foreground` 5.05:1.
- `Everforest Light`: `foreground` **4.71:1**, `muted.foreground` 5.05:1, `tab.foreground` 5.00:1.

No other variant has `muted.foreground` at or above `foreground`. Eleven variants show
`tab.foreground == foreground` to the digit (Ayu Light/Dark, Matrix, Mellifluous Dark, Molokai
Light/Dark, Tokyo Night/Storm/Moon) — those are ties by inheritance (`tab.foreground` absent →
`foreground`, `schema.rs:1000`), i.e. the packet's "unchanged (inherited)" rows, not
inversions. Nearest genuine approaches: `Ayu Light` 5.01 vs 4.73, `Alduin` 5.65 vs 5.05.
The Gaps entry naming `Ayu Light` 5.01 and `Alduin` 5.65 is also correct.

### Claim 3 — only lightness moved — CONFIRMED in substance

All 47 changed values extracted from `git diff main...HEAD -- crates/theme/themes` and
converted to HSL independently:

- **Saturation: max drift 0.0045 across all 47** (i.e. < 0.5 %). Effectively constant.
- Hue: 35 of 47 under 1.5°; the remaining 12 up to 5.71°
  (`Flexoki Light tab.foreground #8d8986 → #696563`). Every one of those is a near-neutral
  colour (S < 0.07), where hue is numerically ill-conditioned — a 5.7° rotation at S = 0.026
  is not a visible recolour. The claim stands.
- Direction: every dark-mode value moved lighter, every light-mode value darker, as stated.
- 47 values / 23 files / 33 of 39 variants, `fahrenheit.json` untouched — all confirmed
  against the diff.

### Claim 4 — gate wiring — CONFIRMED

- `scripts/ci-local.ps1:148`, `scripts/ci-local.sh:115` — both beside the other Python
  data-file checks.
- `.github/workflows/ci.yml:65-69`, inside the `dependency-graph` job: `runs-on:
  ubuntu-latest`, `actions/setup-python` 3.11, **no Rust toolchain in that job** (`ci.yml:53-60`).
- `AGENTS.md:135` lists it in §4's command list; `AGENTS.md:71` in the §3.4 theme bullet.
- Offline, stdlib-only: confirmed above.
- **Negative test run by me**, not taken on trust: `zed-one-dark.json`'s `muted.foreground`
  put back to `#5c6370` →
  `zed-one-dark.json: Zed One Dark (dark): muted.foreground on list.even.background is 2.38:1`,
  `EXIT=1`. Restored → `117 measurements, all >= 4.5:1`, `EXIT=0`, `git status --porcelain`
  empty. Matches the packet's quoted output verbatim.

### Claim 5 — docs — MOSTLY CONFIRMED

- `EMBEDDED_THEME_FILES` is real: `crates/theme/src/theme.rs:28`. `BUILTIN_THEMES` does not
  exist anywhere in the tree except as the packet's own account of the correction. Both
  `AGENTS.md:71` and `docs/agents/structure.md:232` now name it correctly. **Correct.**
- `docs/gui-layout.md:110-121` — new section. Accurate on the tokens, the floor, the script
  and the gate. Two overclaims:
  - **MAJOR (same root cause as claim 1):** "clears 4.5:1 … against *every* surface it is
    composited over", and it names "key-binding chips" among the secondary text covered. Not
    true for the 20 measurements above.
  - **MINOR:** ":120 secondary text sits between 4.5:1 and about 6:1" is false for several
    shipped themes — `Molokai Light` `tab.foreground` 19.10:1, `Molokai Dark` 15.87:1,
    `Matrix` 15.75:1, `Catppuccin Macchiato` `muted.foreground` 8.17:1. Those are untouched
    values, but the sentence states the band as fact, not as guidance (AGENTS.md's "Aim for
    4.5–6:1" is fine).
- **TRIVIAL:** the packet cites `crates/theme/src/theme.rs:27-61`; the constant begins at
  line 28 (27 is the last doc-comment line).
- **NOTE:** `IN-0042.md:210` planned the focused proof as a "pure-data unit test next to the
  changed logic"; it shipped as a Python script. The packet's Context argues the ladder
  (`completion-catalog.py` precedent) and the argument is sound — recorded as a deviation from
  the intake's verification design, not a defect. `cargo test -p oneterm-theme` adds no
  contrast coverage; its two tests (`crates/theme/src/theme.rs:193`, `:207`) prove only that
  every edited JSON still parses and still registers its variants -- which they do.

### Claim 6 — evidence PNGs — CONFIRMED

All three present and readable. Pixels re-measured by me with PIL, sampling the solid core of
each glyph run and the modal background beside it:

| Scene | Sampled | Background | Glyph core | My ratio | Packet |
| --- | --- | --- | --- | --- | --- |
| 01 | session host `root@192.168.13.128:22` | `#23272e` | `#8f96a3` | **5.04:1** | 5.04 |
| 01 | `No SFTP connection.` empty state | `#23272e` | `#8f96a3` | **5.04:1** | 5.04 |
| 01 | `Search sessions...` placeholder | `#23272e` | `#8f96a3` | **5.04:1** | 5.04 |
| 34 | inactive tab label `Terminal` | `#f0f0f1` | `#5d677a` | **5.00:1** | 5.00 |
| 34 | session host text | `#fafafa` | `#5d677a` | **5.46:1** | 5.46 |
| 27 | `Default: ctrl-w` hint | `#23272e` | `#8f96a3` | **5.04:1** | 5.04 |
| 27 | `Edit` (primary, control) | `#23272e` | `#efefef` | 13.03:1 | — |

The after state is real: the scenes show the raised values, the clock in scene 01 reads
`2026-09-17 08:53:55`, and primary text still measures 13:1, so the hierarchy survived.

**MINOR — one evidence row measures the wrong surface.** The packet's row
"27 | key chip `Ctrl+W` | 2.48 | 5.04" reports the ratio against the window body. Sampling
the chip in `US-0111-27-settings-keybindings.png` gives fill `#1e2227`, glyph `#8f96a3`,
**5.38:1**. The number is better than claimed, but it was taken against a surface the chip is
not on — the same modelling gap as the major finding.

### Claim 7 — gate — see Commands

## Findings, ranked

### Major

1. **`SURFACES` omits `muted.background`, `list.hover.background` and
   `table.hover.background`, and 20 measurements across 9 variants are still below 4.5:1 on
   them** — including `Zed One Dark` (the startup theme) at 4.38:1 on the selected session /
   SFTP row, and the `F32` key-binding chip at 4.06:1 in `Catppuccin Mocha`. Proof of the
   draw sites: `reference/gpui-kit/crates/component/src/kbd.rs:223-225`,
   `crates/sftp-ui/src/table_delegate.rs:284-285` + `:317`,
   `crates/theme/src/theme.rs:69-77`, `crates/session-ui/src/tree_render.rs:136`. This
   falsifies the packet Outcome ("against the surface they are drawn on"), acceptance item 1,
   and the Evidence "Surfaces measured" list, and it is the risk the packet itself flagged.
2. **`docs/gui-layout.md:110-121` ships the unproven claim** — "clears 4.5:1 … against *every*
   surface it is composited over", naming key-binding chips. `AGENTS.md:71` sends the next
   theme author to a check that will not catch it.

### Minor

3. `FALLBACKS["sidebar.background"]` mirrors `background` instead of
   `background.blend(border × 0.15)` (`schema.rs:977-980`); 36 variants affected, real ratios
   0.1–0.35 lower, none below the floor today (min 4.62, `Mellifluous Dark`).
4. No `FALLBACKS` entry for `muted.foreground`; a theme omitting it produces an unhandled
   `ValueError` traceback instead of a diagnosis. Reproduced.
5. `measure` always composites translucent surfaces over `background`, not over the parent
   surface; safe for today's `tab.background` values, not safe by construction.
6. Evidence row "27 key chip" reports 5.04:1 against the body; the chip's real fill gives
   5.38:1.
7. `docs/gui-layout.md:120` states secondary text "sits between 4.5:1 and about 6:1" — false
   for `Molokai Light` (19.10), `Molokai Dark` (15.87), `Matrix` (15.75),
   `Catppuccin Macchiato` (8.17).
8. Packet cites `crates/theme/src/theme.rs:27-61`; the constant starts at line 28.
9. Deviation from `IN-0042.md:210` (planned pure-data unit test → Python script). Justified in
   the packet, recorded here for the intake's benefit.

### Not findings (checked, clean)

- No measurement regressed: 0 of 117 on the measured surfaces, 0 of 117 on the omitted ones.
- 117 / 68-below inventory reproduced exactly from `main`.
- No stale `BUILTIN_THEMES` reference anywhere in the tree.
- `muted.foreground` on `tab.active.background`: 0 below floor.
- Self-test runs on every invocation; the `#767676`/`#777777` AA-boundary anchors are correct
  (4.5422 / 4.4781 independently).
- The `F6` and `F32` quotations in the packet match
  `research/ux-walkthrough-2026-09-16.md:64` and `:90` verbatim.
- `32-theme-dropdown.png` not re-captured — already disclosed by the implementer in Gaps.

## Commands

```text
python scripts/check-theme-contrast.py
  -> check-theme-contrast: 117 measurements, all >= 4.5:1                       (exit 0)

python scripts/check-theme-contrast.py --self-test
  -> check-theme-contrast: self-test passed                                     (exit 0)

# negative test, run by the verifier: zed-one-dark muted.foreground -> #5c6370
python scripts/check-theme-contrast.py
  -> zed-one-dark.json: Zed One Dark (dark): muted.foreground on list.even.background is 2.38:1
  -> check-theme-contrast: 1 secondary token(s) below the 4.5:1 floor           (exit 1)
# restored; git status --porcelain empty; check back to exit 0

# independent re-derivation (scratch scripts, not the repo's)
main themes   -> 39 variants, 117 measurements, 68 below 4.5:1
branch themes -> 117 measurements compared, 0 regressed, 0 still below floor
omitted surfaces -> 117 measurements; before 103 below floor, after 20 below floor; regressions 0
changed values   -> 47 values, saturation drift <= 0.0045, hue drift <= 5.71deg (all S < 0.07)

pwsh scripts/ci-local.ps1
  -> ci-local: all checks passed.

cargo test -p oneterm-theme
  -> test theme::tests::every_embedded_theme_file_parses ... ok
  -> test theme::tests::zed_default_themes_are_present_under_their_registry_names ... ok
  -> test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Note on the gate: the first attempt aborted in `cargo clippy --workspace --all-targets` with
`error: could not compile 'windows' (lib) … STATUS_STACK_BUFFER_OVERRUN` after a
`handle_alloc_error` — an out-of-memory abort on a machine running several builds at once, not
a defect in this branch. Re-run with `CARGO_BUILD_JOBS=2` and it passed.

## Gaps in this verification

- No GUI walk of my own. The evidence PNGs were re-measured pixel-wise but not re-captured, so
  the 20 sub-floor measurements above are proven from the theme JSON and the draw sites in
  source, not from a screenshot of a selected row or a chip in an affected theme.
- `reference/gpui-kit/` is not checked out in this worktree; the kit's schema, `kbd.rs` and
  `tooltip.rs` were read from the main checkout at
  `D:\TrungKFC-Research\Rust\myTerm2\reference\gpui-kit\` (read-only).
- Surface coverage was audited for `muted.foreground` (21 call sites), tooltips
  (`tooltip.rs:114`, popover — covered), notifications (`notification.rs:424`, popover —
  covered), the settings window (no explicit background; inherits `background` — covered) and
  `tab.active.background` (clean). `table.foot.foreground` and
  `description_list.label.foreground` also inherit `muted.foreground` in the kit but appear
  unused by OneTerm; not measured.
- `cfg(unix)`/macOS rendering not exercised.


---

# Re-verification of 57fd345d — 2026-09-17

Second pass, scoped to the rework. Target: `feat/us-0111-secondary-text-contrast-floor`
@ `57fd345d`, one commit on top of this report's own `a5f289b8`. Same method: the surface
model was re-implemented from the kit's `schema.rs` semantics in a scratch file and the
numbers below come from that implementation, not from running the packet's script.

## Verdict: **PASS**

Both majors are fixed, all six minors are fixed, and the fixes hold under an independent
re-derivation of the whole 702-pairing model. Three trivia remain (two wrong line numbers and
one missing citation), none of which changes a measured value. Nothing regressed: of the 702
pairings, **0** are worse than on `main`.

## Per-finding status

| # | Round-1 finding | Status | Evidence |
| --- | --- | --- | --- |
| 1 | **MAJOR** `SURFACES` omits `muted.background`, `list.hover.background`, `table.hover.background`; 20 pairings below the floor | **FIXED** | `muted.foreground` now lists **15** surfaces (`scripts/check-theme-contrast.py:43-106`) including all three, plus `accent.background` and `tab.active.background` that I had not found. Independent re-derivation: **702 pairings, 0 below 4.5:1** on the branch; **492 below** on `main`. Both numbers match the packet exactly. |
| 2 | **MAJOR** `docs/gui-layout.md` ships the "every surface it is composited over" claim | **FIXED** | `docs/gui-layout.md:110-127` now says "every surface the `SURFACES` table in `scripts/check-theme-contrast.py` lists for it", enumerates them, and adds "that table is the contract, and it is only as complete as its last review". `AGENTS.md:71` carries the same scoping and tells a theme author to add a surface rather than assume coverage. |
| 3 | **MINOR** `sidebar.background` fallback does not mirror the kit | **FIXED** | `FALLBACKS` (`:134`) is now `("blend", "background", "border", 0.15)`. Verified numerically on a hand-made theme in my scratch dir: `background #202020`, `border #c0c0c0` → script `#383838`, my own `0xc0/255*0.15 + 0x20/255*0.85` → `#383838`. |
| 4 | **MINOR** a missing `muted.foreground` raises an unhandled traceback | **FIXED** | `FALLBACKS["muted.foreground"] = ("blend", "muted.background", "foreground", 0.7)` (`:150`), matching `schema.rs:777-779`; verified `#202020`/`#ffffff` → `#bcbcbc` against my own arithmetic. A token with no value *and* no kit-expressible fallback now reports by name: feeding `{"background": "#000000", "foreground": "#ffffff"}` gives `ValueError: muted.background: the theme does not define it and the kit has no fallback` instead of a bare traceback. |
| 5 | **MINOR** translucent surfaces composited over `background`, not their parent | **FIXED** | New `PARENTS` table (`:110-126`) and `surface()` (`:223-231`). Verified on a hand-made theme: `tab.background #ffffff80` on a white `tab_bar` over a **black** body resolves to `#ffffff` (the old model would have given `#808080`), and `#767676` on it measures 4.5422 — my independent value for that grey on pure white. |
| 6 | **MINOR** the chip evidence row measured against the window body | **FIXED** | Re-measured `US-0111-27-settings-keybindings.png` myself: chip fill `#1e2227`, glyph `#9aa1ac`, **6.14:1** — which is what the packet's table now reports for `Zed One Dark` `muted.foreground` on `muted.background`. |
| 7 | **MINOR** "sits between 4.5:1 and about 6:1" is false for shipped themes | **FIXED** | Sentence gone. `docs/gui-layout.md:124-127` now says raised values land near 5:1 while some untouched tokens sit far higher, citing `Molokai Light`'s inherited `tab.foreground` at 19:1 — I measure 19.10:1. `AGENTS.md:71` says "Aim for about 5:1". |
| 8 | **TRIVIAL** `theme.rs:27-61` | **FIXED** | Both citations now read `28-61` (packet `:60`, `:137`); the constant is at `crates/theme/src/theme.rs:28`. |
| 9 | **NOTE** focused proof is a Python script, not the unit test `IN-0042.md:210` planned | Unchanged, accepted | Still the right call; `cargo test -p oneterm-theme` continues to prove only that the edited JSON parses and registers. |

## Checks asked for this round

### 1. Surface list and inventory — CONFIRMED

- 15 surfaces for `muted.foreground` (`scripts/check-theme-contrast.py:82-98`), containing
  `muted.background`, `list.hover.background`, `table.hover.background`, `accent.background`
  and `tab.active.background`. 15 + 2 (`tab.foreground`) + 1 (`table.head.foreground`) = 18
  per variant × 39 variants = **702**, which is what `pairings()` reports.
- **Independent inventory:** branch → `39 variants, 702 pairings, 0 below 4.5:1`; `main` →
  `39 variants, 702 pairings, 492 below 4.5:1`. Exactly the claimed figures.
- **New draw sites verified in source, not taken on trust:**
  - `accent.background`: the hovered menu row is `this.bg(cx.theme().tokens.accent)`
    (`reference/gpui-kit/crates/component/src/menu/menu_item.rs:115`) and the shortcut `Kbd`
    on it is forced transparent — `.bg(gpui::transparent_white())`
    (`menu/popup_menu.rs:1114`) — so its `muted_foreground` text lands straight on the accent
    fill. Both citations are exact.
  - `tab.active.background`: `let muted = cx.theme().muted_foreground;`
    (`crates/terminal-view/src/panel/tab_title.rs:110`) in `render_tab_strip`. Exact.
  - `muted.background`: `kbd.rs:223-225` with `outline: false` at `kbd.rs:35`. Exact.
  - `list.hover` / `table.hover`: `crates/sftp-ui/src/table_delegate.rs:284-285` and `:317`;
    `crates/theme/src/theme.rs:69-77`. Exact.
  - `background` for the Settings pages: `GroupBoxVariant::Outline` resolves to
    `(None, Some(border), true)` — no fill (`reference/gpui-kit/crates/component/src/group_box.rs:133-135`). Correct.
- **The two exclusions are true.** `apply_list_style_override` (`crates/theme/src/theme.rs:69-77`)
  sets `list_active = list_hover` and `table_active = transparent`, so the JSON
  `list.active.background` / `table.active.background` are never painted. Every
  `apply_config` / `Theme::change` call site in the workspace is followed by that override
  (`crates/theme/src/theme.rs:101-102`, `:113-114`, `:149-150`, `:176`;
  `crates/settings-ui/src/appearance.rs:56-57`, `:92-93`), so there is no path that leaves the
  JSON values live.
- **Value count:** 51 changed token values across **37 of 39** variants, independently
  recounted from the theme JSON — matching the commit message.
- **Spot checks** (independent, `main` → branch):

  | Variant | Surface | main | now | why it matters |
  | --- | --- | --- | --- | --- |
  | Zed One Dark | `muted.background` `#1e2227` | 2.65 | **6.14** | the chip fill |
  | Zed One Dark | `accent.background` `#2c313c` | 2.16 | **5.01** | hovered menu row |
  | Zed One Dark | `table.hover.background` `#2c313c` | 2.16 | **5.01** | selected SFTP row |
  | Zed One Dark | `tab.active.background` `#23272e` | 2.48 | **5.76** | active tab subtitle |
  | Catppuccin Mocha | `muted.background` `#302d41` | 2.73 | **5.01** | the 4.06:1 chip from round 1 |
  | Tokyo Storm | `list.hover.background` (derived) | 1.77 | **6.15** | derived `accent × 0.6` surface |
  | Ayu Light | `table.even.background` (derived) | 2.12 | **5.59** | derived alternating row |
  | Matrix | `accent.background` `#002d00` | 2.65 | **5.02** | a saturated theme |

- **No regression:** all 702 pairings compared `main` → branch, **0 got worse**.
- **Hue/saturation still constant** across the 51 values: max saturation drift **0.0063**, max
  hue drift **3.09°** on any colour with S > 0.07.

### 2. Model mechanics — CONFIRMED

`python scripts/check-theme-contrast.py --self-test` → `check-theme-contrast: self-test passed`
(exit 0), and the self-test now covers the parent-compositing case, the three derived
fallbacks and the missing-token diagnosis (`:296-317`). I did not rely on it: the four
mechanics above were re-derived on hand-made themes held **only** in my scratch directory
(`scratchpad/badtheme.py`, `scratchpad/scratch-bad-theme.json`); nothing was written into
`crates/theme/themes/` and `git status --porcelain` stayed empty throughout. A hand-made theme
carrying the original `#5c6370` fails **18 of 18** pairings, worst `muted.foreground` on
`accent.background` at 2.16:1 — i.e. the enlarged list does bite.

### 3. Docs — CONFIRMED, with one trivium

`docs/gui-layout.md:110-127` and `AGENTS.md:71` no longer overclaim; the 4.5–6:1 sentence is
gone; the `EMBEDDED_THEME_FILES` line reference is corrected. See the table above.

### 4. New evidence PNG — CONFIRMED

`evidence/US-0111-hover-menu-row.png` shows the `+` menu with `New SSH Session` hovered.
Sampled by me:

| Sample | Background | Glyph core | Ratio |
| --- | --- | --- | --- |
| `Ctrl+S` hint on the hovered row | `#2c313c` (`accent.background`) | `#9aa1ac` | **5.01:1** |
| `New SSH Session` label, same row | `#2c313c` | `#efefef` (`accent.foreground`) | 11.33:1 |
| `PowerShell`, unhovered row | `#1e2227` (`popover.background`) | `#efefef` | 13.91:1 |

The hint's claimed 5.01:1 is exact, it agrees with the value the script derives from the JSON,
and the row shows the hint keeping `muted_foreground` while the label takes
`accent_foreground` — which is the behaviour the `popup_menu.rs:1114` citation predicts. The
three re-captured scenes also carry the new values (host text 5.76:1, chip 6.14:1, light-theme
inactive tab 5.00:1).

### 5. Gate — GREEN

## New findings (all trivial, none affects a measured value)

1. **Wrong draw site for `table.head.foreground`.** `scripts/check-theme-contrast.py:103-105`
   cites `crates/sftp-ui/src/table_delegate.rs:440`, which is the `"Empty directory."`
   empty-state label in `muted_foreground` — not a column header and not
   `table.head.foreground`. The pair is really drawn at
   `reference/gpui-kit/crates/component/src/table/state.rs:1753-1754`
   (`.bg(cx.theme().tokens.table_head).text_color(cx.theme().table_head_foreground)`), with
   OneTerm's header content coming from `render_th` (`table_delegate.rs:251`). The pairing is
   correct; only the pointer is.
2. **Off-by-one on `panel.rs`.** Both `scripts/check-theme-contrast.py:49` and the packet
   (`:274`) cite `crates/settings-ui/src/panel.rs:29`; `SETTINGS_GROUP_VARIANT` is at line 28.
3. **One surface has no citation.** The script states "Every entry below therefore cites the
   source line that draws that pair" (`:13`), but `sidebar.background` (`:64`) carries only a
   prose note, and `list.even.background` / `table.even.background` lean on their neighbour's
   line. The surface itself is legitimate — the Settings window wraps the kit's sidebar+page
   `Settings` widget (`crates/settings-ui/src/panel.rs:3`) and the kit's sidebar paints
   `tokens.sidebar` — so this is a documentation gap, not a measurement one.

## Commands

```text
python scripts/check-theme-contrast.py
  -> check-theme-contrast: 702 foreground/surface pairings across 117 token/variant rows, all >= 4.5:1   (exit 0)

python scripts/check-theme-contrast.py --self-test
  -> check-theme-contrast: self-test passed                                     (exit 0)

# independent re-implementation of the reworked model (scratch, not the repo's script)
branch -> 39 variants, 702 pairings, 0 below 4.5:1
main   -> 39 variants, 702 pairings, 492 below 4.5:1
702 pairings compared; 0 regressed; 0 below floor after
51 changed values; max saturation drift 0.0063; max hue drift on non-neutral colours 3.09 deg

# hand-made themes, scratch dir only
PARENTS: tab.background #ffffff80 over white tab_bar over black body -> #ffffff (4.5422 for #767676)
sidebar fallback  -> #383838 (independent #383838)
muted.foreground  -> #bcbcbc (independent #bcbcbc)
list.hover        -> #333333 (independent #333333)
missing token     -> ValueError: muted.background: the theme does not define it and the kit has no fallback
a theme carrying the original #5c6370 -> 18 of 18 pairings below the floor

pwsh scripts/ci-local.ps1        # CARGO_BUILD_JOBS=3
  -> ==> python scripts/check-theme-contrast.py
  -> check-theme-contrast: 702 foreground/surface pairings across 117 token/variant rows, all >= 4.5:1
  -> ci-local: all checks passed.
```

Gate note: the first run of `ci-local.ps1` aborted in `cargo test --workspace` with
`rustc-LLVM ERROR: IO failure on output stream: no space on device` — the shared `D:` volume
hit 0 bytes free while several agents were building. `cargo fmt` and both `clippy` steps had
already passed. Clearing `target/debug/incremental` and re-running with `CARGO_INCREMENTAL=0`
completed green. Not a property of this branch.

## Gaps in this re-verification

- The hovered-row, chip and active-tab surfaces were photographed on `Zed One Dark` only; the
  other 38 variants rest on the theme JSON plus the source citations above, as the packet's
  own Gaps section says.
- I did not re-audit for a *sixteenth* missing surface beyond the sweep in round 1 plus the
  kit's `Kbd`, `PopupMenu`, `tooltip`, `notification`, `GroupBox` and `Table` head paths. The
  script's own framing — the table is the contract and may be incomplete — is now the honest
  statement of that residual risk.
- `cfg(unix)`/macOS rendering still not exercised.
