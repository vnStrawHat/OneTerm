# US-0127 — independent verification

Verifier: adversarial second party. Nothing here was written by the implementer.
Target: `fix/primary-text-contrast-floor` @ `357d273c` (4 commits on `main` @ `b64b70bc`).
Method: every ratio below was recomputed with a WCAG 2.1 relative-luminance function written
from the spec in a scratch file, never by calling `scripts/check-theme-contrast.py`. Its own
anchors: `#ffffff`/`#000000` = 21.0000, `#767676`/`#ffffff` = 4.5422,
`#5c6370`/`#23272e` = 2.4793. The committed frames were re-measured from the PNG pixels with
PIL; no GUI was driven.

## Verdict

**FAIL** — narrow, fixable without touching a single theme value, and everything except one
surface holds up.

The maths, the counts, the rules, the self-test, the negatives, the lightness-only moves and
the four frames all reproduce exactly. What does not hold is the one judgement the packet
makes on its own initiative: that `foreground` on `tab_bar.background` is "a number about
nothing" (`scripts/check-theme-contrast.py:85-89`, `docs/gui-layout.md:184-185`). **OneTerm
draws primary text on `tab_bar.background`** — the dock panel headers "Session" and "SFTP
Browser" — and the proof is in this packet's own before frames: the glyphs are the exact
`foreground` token on the exact `tab_bar.background` fill. So §4.2's original 4.39:1 and
4.71:1 were real measurements of a real pair, Solarized Light's primary text **was** below the
floor, and `evidence/before-after-report.md:236-239` now records the opposite as fact.

Nothing ships broken: with that surface added the committed state measures 0 below the floor
and 0 inverted across all 39 variants. The remedy is one tuple entry plus three sentences —
no re-colouring.

## Claims

### Claim 1 — primary text is not one token; every primary row cites a draw site — PARTIAL

**Confirmed.** The four-token model is right and every citation the packet adds is right.
Line numbers re-read in `reference/gpui-kit` (the reference tree is not in the worktree; read
read-only from the main checkout):

| Cited | Reads |
| --- | --- |
| kit `root.rs:593` | `.text_color(cx.theme().foreground)` on the window root — correct |
| kit `theme/schema.rs:970` | `apply_color!(popover_foreground, fallback = self.foreground)` — correct |
| kit `theme/schema.rs:984` | `apply_color!(sidebar_foreground, fallback = self.foreground)` — correct |
| kit `theme/schema.rs:997` | `apply_color!(tab_active_foreground, fallback = self.foreground)` — correct |

All twelve OneTerm draw sites and all nine kit draw sites in the four new `SURFACES` comment
blocks were opened and read. Correct to the line: `tab_title.rs:543`, `space/render.rs:46`,
`status_text.rs:42`, `status_text.rs:343`, `tree_render.rs:170`, `theme.rs:69-77`,
`table_delegate.rs:54`, `local_pane.rs:276`, kit `dialog/dialog.rs:574`,
`menu/menu_item.rs:105`, `menu/menu_item.rs:115-120` (the accent swap, exactly those six
lines), `list/list_item.rs:192`, `menu/popup_menu.rs:1441`, `tooltip.rs:115`,
`command/state.rs:828`, `sidebar/mod.rs:414`, `sidebar/group.rs:69` (the 70 % opacity),
`tab/tab.rs:175`. Two are off by a few lines and cosmetic — see MINOR 6.

The named suspects were each chased and each is covered: dialog bodies paint
`tokens.background` (kit `dialog/dialog.rs:574`) so they are the `background` row; tooltips
and the command palette route `popover.foreground`; notification toasts paint
`.bg(cx.theme().tokens.popover)` and set **no** text colour (kit `notification.rs:424`, the
only colour line in the file besides the four status icons), so a toast body is `foreground`
on `popover.background` — already measured, though the comment does not mention it; the
active tab label is `tab.active.foreground` on `tab.active.background` (confirmed in the
frames: Solarized's unchanged `#073642` label sits beside Everforest's changed one); SFTP
table cells and the local pane's headers are covered; the Settings window is the sidebar row
plus `background` (its GroupBox is `Outline`, no fill). `input` is a border-only token
(schema.rs:775) so text fields add no surface. `panel.background` appears in three theme
JSONs but no `panel` token exists in the kit schema — dead key, no surface.

**MAJOR 1 — `tab_bar.background` is a primary-text surface, and it is excluded on a false
premise.** `scripts/check-theme-contrast.py:85-89` lists `tab_bar.background` among the
surfaces "deliberately absent, because primary text is not what is drawn there … for the tab
strip, the secondary `tab.foreground`". That is not what the application draws. A dock group
holding one panel renders its header through the kit's `render_title`
(`reference/gpui-kit/crates/component/src/dock/tab_panel.rs:356-424`), which sets a text
colour only `when_some(title_style, …)` (`:380-382`), and `title_style` defaults to `None`
(kit `dock/panel.rs:87-89`) — neither `SessionPanel` nor `SftpPanel` overrides it
(`crates/session-ui/src/panel.rs:286-288`, `crates/sftp-ui/src/panel.rs:757-759`). The header
therefore inherits the root's `foreground`, on the tab-bar fill.

Measured from this packet's own committed frames, not from the source:

| Frame | Run | Glyph px | Surface px | Surface is |
| --- | --- | --- | --- | --- |
| `US-0127-before-solarized-light.png` | "Session" header, y48-54 x922-963 | `#586e75` (`foreground`) | `#eee8d5` | `tab_bar.background` |
| same | "SFTP Browser" header, y462-465 x924-1001 | `#586e75` | `#eee8d5` | `tab_bar.background` |
| `US-0127-before-everforest-light.png` | both headers | `#5f6d75` (`foreground`) | `#f4f1e2` | `tab_bar.background` (Everforest's `muted.background` is `#f1f0e2`, so the fill is unambiguous) |
| the two after frames | both headers | `#4b5e64` / `#546067` | unchanged | — |

Ratios on that surface, independently computed:

| Variant | `foreground` before | `foreground` after | `muted.foreground` | `tab.foreground` |
| --- | --- | --- | --- | --- |
| Solarized Light | **4.39** — below the floor | 5.56 | 5.02 | 5.05 |
| Everforest Light | 4.71 | 5.71 | 5.05 | 5.00 |
| Ayu Light | 5.69 | 7.17 | 6.35 | (inherits) |

Re-running the whole check with `"tab_bar.background"` appended to `SURFACES["foreground"]`:
1287 pairings, 546 comparisons; **before: 1 row below 4.5:1 (Solarized Light, 4.39) and 40
inverted comparisons** (up from 37); **after: 0 below, 0 inverted**. So the committed colours
already satisfy the missing row — the defect is in the contract, not in the shipped palette,
and the fix is additive.

This is the same finding `evidence/US-0111-verify.md:29-34` raised as its major: a token
paired with the wrong surface set proves nothing, in either direction. Here the omission runs
the other way — a surface the application really paints is declared not to exist.

### Claim 2 — the two rules, the self-test and the inventory — CONFIRMED, with one mislabel

Re-derived independently (own WCAG function, own alpha/gradient/parent/fallback resolution;
the `SURFACES`/`PARENTS`/`FALLBACKS` tables taken from the script only as its model of the
kit, each entry checked against `schema.rs` separately):

| | packet | independent |
| --- | --- | --- |
| variants | 39 | 39 |
| foreground/surface pairings | 1248 | 1248 (39 × 32) |
| token/variant rows | 273 | 273 (39 × 7) |
| primary/secondary comparisons | 507 | 507 (39 × 13) |
| before: rows below 4.5:1 | 0 | 0 |
| before: inverted | 10 | 10 rows — **37 comparisons** |
| after: below / inverted | 0 / 0 | 0 / 0 |

The ten inverted rows reproduce verbatim, variant for variant and ratio for ratio, including
`Ayu Light popover.foreground 4.88 vs 5.91` and `Solarized Light sidebar.foreground 4.77 vs
5.45`. Rule shape is right: `inversions` fails on `worst[1] <= worst[2]`
(`scripts/check-theme-contrast.py:390`), so a tie fails, and it picks the smallest-margin
surface, which is sound because a row passes only if every surface passes.

Self-test: all five cases the acceptance names are present and real — below-floor fixture
(`:434-437`), inverted fixture (`:442-449`), upright fixture (`:450-452`), tie (`:453-455`),
and override-only-`popover.foreground` (`:456-460`) — plus a guard that every `HIERARCHY`
pair actually shares a surface (`:440-441`). It runs on every invocation, not only under
`--self-test` (`:482`). **Mutation test:** weakening `:390` to `worst[1] < worst[2]` (ties
allowed) makes `--self-test` fail with `AssertionError: a tie is a failure`, exit 1;
restored, it passes. The test is load-bearing.

§4.2's three corrections, each checked:

- **"Alduin was not flagged"** — correct. Alduin does not appear in the before-state inversion
  list at any surface.
- **"Ayu Light is a third inverted variant"** — correct, and inverted on all four of its pairs.
- **"the 4.39/4.71 figures were `foreground` scored against the tab strip"** — correct as to
  provenance (`#586e75` on `#eee8d5` = 4.39, `#5f6d75` on `#f4f1e2` = 4.71, to the digit), but
  the conclusion drawn from it is false. See MAJOR 1 and MAJOR 2. The companion figure is
  right inside the packet's own surface list and wrong outside it: Solarized Light's before
  minimum is 4.53 on `list.hover.background` as stated, but 4.39 on `tab_bar.background` once
  that surface is counted. (Everforest Light's before minimum is 4.59 on a hovered row, Ayu
  Light's 5.01 on an alternating row — both as the packet's per-surface tables say.)

**MINOR 4 — "comparisons inverted: 10" counts rows, not comparisons.** The Evidence table
(`US-0127-…md:234`) puts 10 on a row labelled "comparisons inverted", and the `--report`
footer prints `{comparisons()} primary/muted.foreground comparisons, {len(inverted)} inverted`
(`:499`) — 507 of one unit against 10 of another. `inversions()` emits one row per variant and
pair by design (`:376-379`); the real before-state count is **37 inverted comparisons of 507**.
Pass/fail is unaffected.

### Claim 3 — six keys, four colours, lightness only — CONFIRMED

HSL recomputed independently from the six changed JSON values:

| token(s) | before → after | Δh | Δs | Δl |
| --- | --- | --- | --- | --- |
| Solarized Light `foreground` | `#586E75` → `#4B5E64` | −0.08° | +0.14 pp | −5.88 pp |
| Everforest Light `foreground`, `popover.foreground`, `tab.active.foreground` | `#5F6D75` → `#546067` | +0.29° | −0.22 pp | −4.90 pp |
| Ayu Light `foreground` | `#5c6166` → `#4E5256` | −0.00° | −0.28 pp | −5.88 pp |
| Ayu Light `popover.foreground`, `tab.active.foreground` | `#5C6773` → `#4C555F` | +0.27° | ±0.00 pp | −7.06 pp |

Max hue drift 0.29° (claim: ≤ 0.3°), max saturation drift 0.28 pp (claim: ≤ 0.3 pp), every
move darker. Six keys, four distinct colours, three files, no secondary token touched — the
diff (`git diff main...HEAD -- crates/theme/themes/`) contains exactly those six lines.

**MEDIUM 3 — the untouched button foregrounds are not merely unmodelled; two of them are
already below the floor.** The packet leaves Everforest Light's `accent.foreground` and
`secondary.foreground` at the old `#5F6D75` and files it under "unmodelled … worth its own
packet if button labels are ever reported as hard to read" (`US-0127-…md:416-419`). Measured:

| pair | ratio |
| --- | --- |
| Everforest Light `accent.foreground` `#5F6D75` on `accent.background` `#E7E5D4` | **4.21:1** |
| Ayu Light `accent.foreground` `#5C6773` on `accent.background` `#dadadc` | **4.13:1** |
| Everforest Light `secondary.foreground` `#5F6D75` on `secondary.background` `#EEEADA` | **4.43:1** |
| Solarized Light `accent.foreground` `#073642` on `accent.background` `#ebe4ce` | 10.23:1 (fine) |

`accent.background` is a confirmed draw site for `accent.foreground` — a hovered or selected
menu row (kit `menu/menu_item.rs:115-120`), which the script itself cites. So the answer to
"does a button now read lighter than its label" is yes (4.21 against the page's 6.28), but the
sharper point is that those labels were below 4.5:1 before this packet and still are, while
`docs/gui-layout.md:161-162` now states that "every checked token clears 4.5:1" for text in
general. Pre-existing, not a regression, correctly out of scope — but the gaps entry
understates it by a full category.

### Claim 4 — negative tests — CONFIRMED

Both run here, not taken on trust; working tree returned to clean (`git status --short` empty)
after each.

- Hierarchy: Solarized Light's `foreground` put back to `#586E75` → exit 1, with
  `on background foreground is 4.99:1 but muted.foreground is 5.71:1` and
  `on sidebar.background sidebar.foreground is 4.77:1 but muted.foreground is 5.45:1`, and
  `check-theme-contrast: 2 case(s) where muted.foreground reads at least as strongly as the
  primary token beside it`. Matches the packet's transcript verbatim.
- Floor: Zed One Dark's `foreground` (`#efefef`) dimmed to `#4a4a4a` → exit 1 with
  `foreground on list.hover.background is 1.47:1` and `1 token(s) below the 4.5:1 floor`.
  Matches. **TRIVIAL 7:** the same run also prints
  `on popover.background foreground is 1.80:1 but muted.foreground is 6.14:1`, which the
  packet's quoted block omits; the transcript is trimmed, not wrong.

### Claim 5 — the four frames — CONFIRMED (numbers), one sentence mis-attributed

Ratios recomputed from the PNG pixels, independent of the evidence table:

| frame | run | glyph | surface | ratio |
| --- | --- | --- | --- | --- |
| before solarized | `DevServer` (primary) | `#586e75` | `#fdf6e3` | 4.9890 |
| before solarized | `root@192.168.13.128:22` (secondary) | `#576464` | `#fdf6e3` | 5.7055 — louder |
| after solarized | `DevServer` | `#4b5e64` | `#fdf6e3` | **6.3120** |
| after solarized | secondary | `#576464` | `#fdf6e3` | 5.7055 |
| before everforest | `DevServer` | `#5f6d75` | `#fefcee` | 5.1828 |
| before everforest | secondary | `#62676a` | `#fefcee` | 5.5549 — louder |
| after everforest | `DevServer` | `#546067` | `#fefcee` | **6.2764** |
| after everforest | secondary | `#62676a` | `#fefcee` | 5.5549 |

Every figure matches the packet's table to 0.01, and the expected ~6.3 / 5.7 and ~6.3 / 5.55.
The run identities were confirmed by cropping and reading the frames: the session tree is the
right dock, `DevServer` at y113-122 and the host address right-aligned on the same row.

A whole-frame pixel diff is the strongest part of the evidence and it holds: 6 901 pixels
change between the Solarized pair and 7 631 between the Everforest pair, and the only
exact-colour transitions are `#586e75 → #4b5e64` (757 px) and `#5f6d75 → #546067` (809 px)
plus antialiasing shades of the same glyphs. No background, no secondary token and no other
colour moves. Six regions change in each frame: the title-bar channel chips, the panel
headers, the session tree, the status bar, the SFTP header, and (Solarized) the terminal block
cursor — all `foreground` sites, exactly as the token model predicts.

**Part of MAJOR 1 — the "second primary run" is measured against the wrong surface.**
`US-0127-…md:404-406` says "the `Session` panel title in the same frames measures with the
primary token throughout (4.99 → 6.31 and 5.18 → 6.28)". The token is right; the surface is
not. The header sits on `#eee8d5` / `#f4f1e2`, so the measured values are **4.39 → 5.56** and
**4.71 → 5.71**. The before figure for Solarized Light is below the floor, in the frame the
packet committed to prove the floor was never the problem.

### Claim 6 — gaps honest — MOSTLY

Checked and correct: the 70 % opacity sidebar headings (kit `sidebar/group.rs:69` reads
`.text_color(cx.theme().sidebar_foreground.opacity(0.7))`, and the check measures the token at
full opacity); `accent.background` / `muted.background` carrying no primary row, with the
reasons verified at `menu/menu_item.rs:115-120` and the `Kbd` chip; hover and popover rows
computed rather than photographed (true — the frames show no menu, no hover, one tab);
Windows only. "The floor rule caught nothing here" is honest and, as MAJOR 1 shows, also the
consequence of the missing surface: with `tab_bar.background` in the table the floor rule
would have caught Solarized Light.

Not disclosed: the missing `tab_bar.background` surface (Claim 1), and the severity of the
button foregrounds (Claim 3).

### Claim 7 — the gate — CONFIRMED

```
python scripts/check-theme-contrast.py
    check-theme-contrast: 1248 foreground/surface pairings across 273 token/variant rows,
    all >= 4.5:1; primary text out-reads muted.foreground on all 507 shared-surface comparisons
    exit 0
python scripts/check-theme-contrast.py --self-test
    check-theme-contrast: self-test passed          exit 0
python scripts/check-theme-contrast.py --report | tail -4
    273 measurements, 0 below 4.5:1
    507 primary/muted.foreground comparisons, 0 inverted
cargo test -p oneterm-theme
    test result: ok. 2 passed; 0 failed        exit 0
    (both are `crates/theme/src/theme.rs` parse/registration tests, as
     `evidence/US-0111-verify.md:171-173` already noted -- they add no contrast coverage)

pwsh scripts/ci-local.ps1                      exit 0
    ci-local: all checks passed.
```

The gate was run here in full, on this worktree, with `CARGO_BUILD_JOBS=4`, and its
`check-theme-contrast.py` step printed the same 1248 / 273 / 507 line. Its
`check-english.py` and `check-doc-paths.py` steps ran over a tree that already contained this
report, so this file is covered by them too.

Mutation of the hierarchy rule (`:390` `<=` → `<`) makes `--self-test` fail; restored, clean.

## Findings, ranked

1. **MAJOR — `foreground` on `tab_bar.background` is drawn and unmeasured.** The dock panel
   headers ("Session", "SFTP Browser") inherit the root `foreground` through the kit's
   `render_title` (`reference/gpui-kit/crates/component/src/dock/tab_panel.rs:356-424`,
   `title_style` defaulting to `None` at `dock/panel.rs:87-89`) and sit on the tab-bar fill.
   `scripts/check-theme-contrast.py:85-89` excludes that pair on the stated ground that the
   strip draws only `tab.foreground`. Both of this packet's before frames contradict it.
   Before the fix Solarized Light measured **4.39:1** there — below the floor — and inverted
   against `muted.foreground` (5.02) and `tab.foreground` (5.05); Everforest Light 4.71 vs
   5.05; Ayu Light 5.69 vs 6.35. **Remedy:** add `"tab_bar.background"` to
   `SURFACES["foreground"]` with the `render_title` citation. The committed palette already
   passes with it (0 below, 0 inverted over all 39 variants), so no colour changes.
2. **MAJOR — the round's report of record now states a false correction.**
   `evidence/before-after-report.md:236-239` and `US-0127-…md:170-177` assert that the 4.39 /
   4.71 figures are about a surface the themes do not paint primary text on and that "no
   variant's primary text was in fact below 4.5:1". Both sentences are wrong for the reason
   above, and `docs/gui-layout.md:184-185` teaches the same error to the next theme author
   ("`foreground` scored against the tab strip … is a number about nothing"). This turns a
   true §4.2 finding into a false closure, and removes the reason a future reader would have
   to add the surface. **Remedy:** three sentences, plus the corrected panel-title numbers at
   `US-0127-…md:404-406` (4.39 → 5.56 and 4.71 → 5.71).
3. **MEDIUM — two button/menu foregrounds are below the floor, not merely unmodelled.**
   Everforest Light `accent.foreground` 4.21:1 and Ayu Light `accent.foreground` 4.13:1 on
   `accent.background` — a hovered menu row, a draw site the script itself cites
   (`menu/menu_item.rs:115-120`); Everforest Light `secondary.foreground` 4.43:1. Pre-existing
   and correctly out of scope, but `US-0127-…md:416-419` files them as an unmeasured pair
   rather than a known failing one, while `docs/gui-layout.md:161-162` now claims the floor for
   text generally.
4. **MINOR — "comparisons inverted: 10" mixes units.** 10 is the row count
   (`US-0127-…md:234`; `scripts/check-theme-contrast.py:499` prints it beside 507 comparisons).
   The before state has **37 inverted comparisons of 507**.
5. **MINOR — five `FALLBACKS` line citations are off by one** — `tab.background` (schema.rs
   995, cited 994), `tab.active.background` (996, cited 995), `tab_bar.background` (998, cited
   997), `table.hover.background` (1009, cited 1008), `title_bar.background` (1011, cited
   1010). All pre-date this packet; **the three entries it adds — 970, 984, 997 — are correct**,
   as is `tab.foreground` 1000, `table.head.foreground` 1006 and `status_bar.background` 1013.
6. **MINOR — two new draw-site citations point a few lines off.**
   `crates/workspace/src/layout/title_bar.rs:73` is `.size_5()` on the logo `svg`; the app
   menu bar the comment is about is the child at `:77`. `tab_title.rs:245` is `.id(…)`; the
   label element is `tab_title_label()` at `:244`.
7. **MINOR — the two gate scripts still describe a secondary-text-only check.**
   `scripts/ci-local.ps1:145-147` and `scripts/ci-local.sh:112-114` both read "`US-0111`:
   secondary text (`muted.foreground`, `tab.foreground`, `table.head.foreground`) must clear
   WCAG AA on every surface it is drawn on". The packet updated `AGENTS.md:71` and
   `AGENTS.md:135` (both correct now) but not the comments beside the step itself, which is
   where a contributor debugging a gate failure looks first.
8. **TRIVIAL — negative test 2's transcript is trimmed**; the same run also prints a hierarchy
   line for Zed One Dark (`1.80:1` vs `6.14:1`).
9. **TRIVIAL — the floor rule has no end-to-end fixture.** The self-test's below-floor case
   (`:434-437`) asserts on `measure` directly, while the hierarchy case goes through
   `inversions()`. Nothing exercises `rows()` → `failures` on a fixture, so a wiring mistake
   between the two would only be caught by a real theme going bad.

## What is right

- Four-token primary model, `FALLBACKS` wiring for the three overrides, and every kit citation
  that matters (`root.rs:593`, `schema.rs:970/984/997`) — all verified line by line.
- Every arithmetic claim: 39 / 1248 / 273 / 507, 0 below before and after, the ten inverted
  rows with their exact ratios, and 0 after.
- Both rules as specified, including the strictly-greater tie failure, proven by mutation.
- Lightness-only moves inside the stated tolerances, six keys, four colours, no secondary
  token touched.
- Both negative tests, reproduced here and reverted.
- The four frames: ratios exact to 0.01 and a whole-frame pixel diff showing only the intended
  token substitution.
- The scope discipline: not lowering a secondary token to satisfy the hierarchy rule is the
  right call and was actually followed.

## Commands run

```
git reset --hard 357d273c
python scripts/check-theme-contrast.py                       # exit 0
python scripts/check-theme-contrast.py --self-test           # exit 0
python scripts/check-theme-contrast.py --report | tail -4
python <scratch>/indep.py                                    # own WCAG re-derivation, after + before
python <scratch>/indep.py extra                              # same, with tab_bar.background added
python <scratch>/hsl.py                                      # HSL drift + button-fill ratios
python <scratch>/neg.py hier  ; python scripts/check-theme-contrast.py   # exit 1, then git checkout --
python <scratch>/neg.py floor ; python scripts/check-theme-contrast.py   # exit 1, then git checkout --
python <scratch>/neg.py mutate; python scripts/check-theme-contrast.py --self-test  # AssertionError, then git checkout --
PIL: colour histograms, per-token pixel bands with their surrounding fill, whole-frame diffs,
     and crops of the session tree, the title bar, the two panel headers and the status bar
cargo test -p oneterm-theme
pwsh scripts/ci-local.ps1
```

## Gaps in this verification

- The kit reference (`reference/gpui-kit/`) is not checked into the worktree; it was read
  read-only from the main checkout at its current state. If the pinned reference moves, the
  line numbers above move with it.
- No GUI was driven. The panel-header attribution rests on the committed frames plus the kit
  source, not on a live capture; it is a pixel identity (exact token colour on exact token
  fill, changing in lockstep with the token across two themes), which is strong but not a
  breakpoint in `render_title`.
- Hover, selection, popover and multi-tab states are unphotographed here too, for the same
  reason the packet gives.
- The `PARENTS`/`FALLBACKS` model of the kit was audited against `schema.rs` entry by entry but
  the compositing itself was not compared against a rendered pixel for every surface; only the
  six frame regions above were checked against real pixels.
- Windows only.

---

# Re-verification of `045b8f34` — 2026-09-18

Same verifier, same method: own WCAG function, own fallback/parent resolution, frames
re-measured from the PNG pixels with PIL. Target `fix/primary-text-contrast-floor` @
`045b8f34`, three commits on `30eddd5b` (the FAIL above). Scope: the nine findings and the
numbers the rework claims — not a fresh audit of what the first pass already confirmed.

## Verdict

**PASS.** Every finding is fixed, correctly and at the root rather than at the symptom. The
rework did not stop at adding the one surface the FAIL named: measuring `accent.background`
and `secondary.background` too turned up **five more values below the floor** that neither
`US-0111`, `US-0127`'s first revision nor my own first pass had counted, in two themes the
round had never touched. Every number the packet quotes re-derives exactly, the frames were
re-taken and now photograph the pair the argument turns on, and the gate passes.

## Per-finding status

| # | Finding (from the FAIL above) | Status |
|---|---|---|
| 1 | MAJOR — `foreground` on `tab_bar.background` drawn and unmeasured | **Fixed** |
| 2 | MAJOR — the report of record states a false correction | **Fixed** |
| 3 | MEDIUM — button/menu foregrounds below the floor, filed as unmodelled | **Fixed, and wider than I reported** |
| 4 | MINOR — "comparisons inverted" mixes units | **Fixed** |
| 5 | MINOR — five `FALLBACKS` citations off by one | **Fixed** |
| 6 | MINOR — `title_bar.rs:73`, `tab_title.rs:245` off by a few lines | **Fixed** |
| 7 | MINOR — both `ci-local` comments describe a secondary-only check | **Fixed** |
| 8 | TRIVIAL — negative test 2's transcript trimmed | **Fixed** |
| 9 | TRIVIAL — the floor rule has no end-to-end fixture | **Open, disclosed** |

### 1 — `tab_bar.background` — fixed

`scripts/check-theme-contrast.py:104` adds `"tab_bar.background"` to `SURFACES["foreground"]`,
and `:62-71` carries the citation asked for: kit `dock/tab_panel.rs:356-425`, the
`when_some(title_style, …)` at `:380-382`, and `Panel::title_style` defaulting to `None` at
`dock/panel.rs:87-89`. All three re-read and exact — `:380` is the `when_some`, `:381` the
`bg`/`text_color` pair, `:382` its close; `render_title`'s body ends at `:425`; `panel.rs:88`
is the bare `None`. The "deliberately absent" note (`:91-97`) no longer lists the strip, and
the entry it keeps for `accent.background` now says "measured under that token" rather than
implying the surface is unchecked. The comment also states the thing that caused the error —
that one surface can carry two tokens, the strip carrying both a dock header's `foreground`
and an inactive tab's `tab.foreground`.

### 2 — the records — fixed

- `evidence/before-after-report.md:236-246`: the two ratios are now "**confirmed real**",
  attributed to `foreground` on `tab_bar.background`, with the kit citations, the measured
  `#586e75` on `#eee8d5` = 4.39:1 and `#5f6d75` on `#f4f1e2` = 4.71:1, their 5.56 / 5.71
  after, and an explicit note that an earlier revision claimed the opposite and that the
  independent verification caught it. The section heading is again accurate.
- The Ayu-Light-is-a-third-variant and Alduin-is-not-inverted facts survive as *extensions*
  (`:248-258`) rather than as corrections, which is the right classification: both re-derived
  here and both hold.
- `docs/gui-layout.md:169-175` names the tab strip among `foreground`'s surfaces with the
  header explanation; `:188-193` replaces "a number about nothing" with the correct rule and
  adds the line that matters most for the next reader: "Declaring a surface out of scope is
  the one move this table cannot make safely — an omission reads exactly like a pair that was
  never drawn."
- The GUI table (`US-0127-…md:488-502`) gains a **surface** column and the corrected pair:
  `Session` dock header 4.39 → 5.56 (Solarized) and 4.71 → 5.71 (Everforest), with the
  `DevServer` / `root@…` rows still labelled `background`. The on-background pair is intact
  and correctly labelled; the two claims are no longer conflated.
- `AGENTS.md:71` lists all six primary tokens.

### 3 — button and menu foregrounds — fixed, and the fix found more than I did

`accent.foreground` on `accent.background` and `secondary.foreground` on
`secondary.background` are now `SURFACES` rows (`:127-144`), with draw sites: kit
`menu/menu_item.rs:115-120` for the accent pair (verified, exactly those six lines), and for
the secondary pair the fallback chain `button.secondary.background → secondary.background`
(kit `theme/schema.rs:828`) and `button.secondary.foreground → secondary.foreground`
(`:830-832`), plus a secondary `Tag` (`tag.rs:30` fill, `tag.rs:78` text) — all four re-read
and exact. `FALLBACKS` gains `accent.foreground → foreground` (`schema.rs:904`) and
`secondary.foreground → foreground` (`schema.rs:819`), both verified. `accent.foreground` is
in `HIERARCHY` (`:220`) because the shortcut hint on the same row is `muted.foreground`;
`secondary.foreground` is not, and the comment gives the correct reason — `muted.foreground`
has no `secondary.background` surface, so the pair would never fire.

The honest result is that my MEDIUM 3 understated it as much as the packet had. Measured over
`main`, the two new rows are below the floor in **five** places, not the three I found:

| variant | token | before | after |
|---|---|---|---|
| Ayu Light | `accent.foreground` | 4.13 | 5.51 |
| Everforest Light | `accent.foreground` | 4.21 | 5.10 |
| Everforest Light | `secondary.foreground` | 4.43 | 5.37 |
| **Everforest Dark** | `secondary.foreground` | **3.62** | 5.48 |
| **Tokyo Moon** | `secondary.foreground` | 4.26 | 5.05 |

Everforest Dark at 3.62:1 is the worst value in the whole round and neither `US-0111` nor my
first pass measured it. All five are fixed.

### 4-8 — the minors — fixed

- **4.** `inversions()` now returns a per-pair `count` of inverted surfaces (`:400-422`), and
  both the report footer (`:534-537`) and the stderr summary (`:555-560`) print comparisons
  *and* rows in the right units. `--report` prints `585 primary/muted.foreground comparisons,
  0 inverted across 0 token/variant row(s)`. The self-test gained an assertion that the counts
  sum to every shared surface of every pair (`:481-483`).
- **5.** All five citations corrected — `title_bar.background` 1011, `table.hover.background`
  1009, `tab_bar.background` 998, `tab.background` 995, `tab.active.background` 996. Re-read
  against `schema.rs`: all five now right, and the previously-correct ones (970, 984, 997,
  1000, 1006, 1013) untouched.
- **6.** `title_bar.rs:77` is `.child(self.app_menu_bar.clone())` and `tab_title.rs:244` is
  `tab_title_label()`. Both correct now.
- **7.** `scripts/ci-local.ps1:145-151` and `scripts/ci-local.sh:112-118` both name
  `US-0111` + `US-0127`, all six primary tokens and both rules.
- **8.** The negative-2 block (`US-0127-…md:452-460`) now quotes the full transcript including
  the hierarchy line, and says why it changed.

### 9 — the floor fixture — still open, and disclosed

The self-test's below-floor case still asserts on `measure` directly rather than through
`rows()` → `failures`. The packet records it verbatim as a standing gap. Reasonable call:
closing it means a fixture path through `rows()`, which reads the themes directory. Left
as-is.

## Numbers re-derived independently

Own WCAG function; "before" read out of `main` (`b64b70bc`) with `git show`, so nothing was
hand-transcribed.

| | packet | independent |
|---|---|---|
| variants | 39 | 39 |
| pairings | 1365 | 1365 (39 × 35) |
| rows | 351 | 351 (39 × 9) |
| comparisons | 585 | 585 (39 × 15) |
| before: rows below 4.5:1 | 6 | 6 — the five above plus Solarized Light `foreground` on `tab_bar.background` 4.39 |
| before: inverted | 42 comparisons / 12 rows | 42 / 12 |
| after: below / inverted | 0 / 0 | 0 / 0 |

The twelve before-state inversion rows match one for one, including the three
`foreground`-on-`background` rows that each carry 11 inverted surfaces (the tab strip is the
eleventh now) and the two new `accent.foreground` rows (Ayu Light 4.13 vs 5.00, Everforest
Light 4.21 vs 4.51).

**Twelve keys, six colours, five variants, four files** — confirmed against
`git diff b64b70bc 045b8f34 -- crates/theme/themes/`: exactly twelve changed lines, no more.
HSL recomputed for all six moves: **max hue drift 0.29°, max saturation drift 0.28 pp**,
lightness −7.45 to −4.90 pp in the four light variants and +13.73 / +4.90 pp in Everforest
Dark and Tokyo Moon. Lightness-only holds, in both directions.

## Frames

The two after frames were re-taken; the before frames are byte-identical, correctly — nothing
about the before state changed.

| frame | region | glyph | surface | ratio |
|---|---|---|---|---|
| `US-0127-before-solarized-light.png` | `Session` dock header | `#586e75` | `#eee8d5` | **4.39:1** |
| `US-0127-solarized-light.png` | same | `#4b5e64` | `#eee8d5` | **5.56:1** |
| `US-0127-before-everforest-light.png` | same | `#5f6d75` | `#f4f1e2` | **4.71:1** |
| `US-0127-everforest-light.png` | same | `#546067` | `#f4f1e2` | **5.71:1** |

Measured by taking the two modal colours of the header region and running my own contrast
function over them — the claimed figures to the digit. `#5f6d75` is **absent from the
Everforest after frame** (0 pixels, against 160 in the first revision's frame): the
`accent.foreground` / `secondary.foreground` group moved with the rest, so the old value
survives nowhere in the window. Solarized's after frame still holds 308 pixels of `#586e75`,
which is `list.active.background` — a 154 × 2 fill, not text, and unchanged from the first
revision.

## Commands

```
git reset --hard 045b8f34
python scripts/check-theme-contrast.py
    check-theme-contrast: 1365 foreground/surface pairings across 351 token/variant rows,
    all >= 4.5:1; primary text out-reads muted.foreground on all 585 shared-surface
    comparisons                                              exit 0
python scripts/check-theme-contrast.py --self-test           exit 0
python scripts/check-theme-contrast.py --report | tail -3
    351 measurements, 0 below 4.5:1
    585 primary/muted.foreground comparisons, 0 inverted across 0 token/variant row(s)

# Negative 1 -- Solarized Light's old #586E75 put back; now trips BOTH rules
python scripts/check-theme-contrast.py                       exit 1
    solarized.json: Solarized Light (light): foreground on tab_bar.background is 4.39:1
    ... on background foreground is 4.99:1 but muted.foreground is 5.71:1 (11 of its surface(s))
    ... on sidebar.background sidebar.foreground is 4.77:1 but muted.foreground is 5.45:1 (1 ...)
    check-theme-contrast: 1 token(s) below the 4.5:1 floor
    check-theme-contrast: 12 comparison(s) in 2 token/variant row(s) ...
    -- matches the packet's quoted block exactly; reverted, tree clean

<scratch>/indep2.py    own re-derivation, after and before (before via `git show b64b70bc:`)
<scratch>/hsl2.py      HSL drift of all six moves + the after ratios of the five new failures
<scratch>/frames2.py   token histograms and header-region ratios for all four frames

cargo test -p oneterm-theme                                  exit 0, 2 passed
pwsh scripts/ci-local.ps1                                    exit 0
    ci-local: all checks passed.
```

## Gaps in this re-verification

- Scoped to the nine findings and the rework's own numbers. The parts the first pass confirmed
  (the maths, the alpha/gradient handling, the twelve original draw-site citations) were not
  re-audited; only what the rework touched.
- Still no GUI walk here: the header attribution rests on the committed frames plus the kit
  source. The rework's frames strengthen it — the pair is now photographed, in two themes,
  before and after — but a hovered menu row, a `Secondary` button and a tag are still computed
  rather than captured, so the two new `SURFACES` rows are unphotographed.
- `secondary.foreground` has no OneTerm call site, only the kit's fallback chain and `Tag`. The
  packet says so plainly. The chain was confirmed here, but no OneTerm screen that paints a
  `Secondary` button was found, so that row is a contract about the theme rather than an
  observed pair.
- The kit reference is read from the main checkout; line numbers move if the pin moves.
- Windows only.
