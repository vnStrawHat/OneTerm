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
