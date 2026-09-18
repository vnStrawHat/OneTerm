# Work: Primary text clears the floor and out-reads the secondary text beside it

ID: US-0127
Intake: IN-0042
Created: 2026-09-18

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [x] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

Reopened 2026-09-18 after the independent verification
(`evidence/US-0127-verify.md`) returned **FAIL**: `foreground` on `tab_bar.background` is a
pair the application draws and the first revision excluded it, which turned a true §4.2
finding into a false closure. Reworked and re-proved below.

## Classification

- Change type: existing-contract change
- Risk lane: normal
- Spec Intake, when required: `IN-0042` — `docs/spec-intakes/IN-0042-ux-polish-round-1/IN-0042.md`

## Outcome

Primary text is at least as readable as the secondary text beside it. In every variant of
every theme OneTerm ships, the primary text token drawn on a surface clears **4.5:1** against
that surface **and** measures a *higher* ratio than `muted.foreground` does on the same
surface, and `scripts/check-theme-contrast.py` enforces both rules so the next theme cannot
reintroduce the inversion.

## Findings and proposals covered

§4.2 of the round's before/after report,
`docs/spec-intakes/IN-0042-ux-polish-round-1/evidence/before-after-report.md:217-230`:

> ### 4.2 The two light themes whose primary text is below the floor
>
> `US-0111` raised secondary text above 4.5:1 everywhere but does not touch `foreground`, and
> in two light themes that inverts the hierarchy:
>
> | theme | `foreground` | `muted.foreground` | `tab.foreground` |
> |---|---|---|---|
> | Solarized Light | **4.39:1** | 5.02:1 | 5.05:1 |
> | Everforest Light | **4.71:1** | 5.05:1 | 5.00:1 |
>
> Their secondary text now has more contrast than their primary. The independent verification
> confirmed these are the only two inversions among the 39 variants. `US-0111` records the work
> as the same script with `foreground` added to the token list.

The same item is the first row of §4.6's gap table: "`US-0111` | Primary text in Solarized
Light and Everforest Light (4.2) …". The independent verification report
(`evidence/US-0111-verify.md`) is the other input: its **major** finding was that a foreground
measured against a surface the application does not paint under it proves nothing, which is
the discipline this packet has to follow when it adds the primary tokens.

## Scope

- [x] In scope:
  - `scripts/check-theme-contrast.py` — the primary-text tokens, the surfaces each is drawn
    on (each entry citing the line that draws the pair, as the existing `SURFACES` entries
    do), the two rules, the self-test for both, and the summary line.
  - `crates/theme/themes/*.json` — the primary-text token values the extended check flags,
    moved in **lightness only**, hue and saturation held, exactly as `US-0111` moved the
    secondary ones.
  - `docs/gui-layout.md` — the contrast section names both rules.
  - `evidence/before-after-report.md` §4.2 — closed, in the style of §4.3/§4.4.
  - `AGENTS.md` §3.4 and §4 — the theme-author bullet and the gate's command list still
    described a secondary-text-only check.
- [x] Out of scope:
  - `muted.foreground`, `tab.foreground` and `table.head.foreground`. `US-0111` set those and
    they still clear the floor; lowering one would be a cheaper way to satisfy the hierarchy
    rule and the wrong one — the complaint is that primary text is too quiet, not that
    secondary text is too loud. **No secondary token was touched.**
  - Every non-text token: backgrounds, borders, the ANSI palette. (`accent.foreground` and
    `secondary.foreground` *were* out of scope in the first revision and are now in it — the
    verification showed they were not merely unmeasured but already failing.)
  - Any change to how a component picks a colour. No call site moves; this packet changes
    what the tokens *are*.
  - User-supplied themes, as `US-0111` scoped it. The floor binds what OneTerm ships.

## Acceptance

- [x] `scripts/check-theme-contrast.py` measures the primary-text token drawn on each of the
      surfaces `SURFACES` now lists for it, with a cited draw site per surface.
- [x] Rule 1 (floor): every primary token clears 4.5:1 on every one of its surfaces.
- [x] Rule 2 (hierarchy): on every surface where a primary token and `muted.foreground` are
      both drawn, the primary token's ratio is **strictly greater**. A tie fails.
- [x] `--self-test` covers both rules: a fixture below the floor, an inverted fixture, an
      upright one that passes, a tie that fails, and a fixture where only an overriding
      primary token is lowered.
- [x] The check exits 1 with Solarized Light's old `foreground` (`#586E75`) put back.
- [x] Every changed value keeps its hue and saturation; only lightness moves.
- [x] The summary line reports both rules in the same style as before.
- [x] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed.".
- [x] A GUI frame of the main window in Solarized Light and in Everforest Light, with
      pixel-measured before/after ratios of one primary and one secondary text run on each.

Added by the acceptance rework:

- [x] `foreground` on `tab_bar.background` is measured, with the `render_title` citation, and
      the record says the two §4.2 ratios were real.
- [x] `accent.foreground` on `accent.background` and `secondary.foreground` on
      `secondary.background` are measured, with a draw site each, and every value the check
      flags there is raised by lightness only.
- [x] The inverted count is reported in comparisons as well as rows -- the two are different
      units and the first revision printed one beside the other.

## Documentation

### Owning Docs Reviewed

- `docs/gui-layout.md` § "Secondary text contrast floor" (`:155-173`) — the owning contract
  for this rule. It states the 4.5:1 floor, the three secondary tokens, the `SURFACES`-is-the-
  contract discipline and the "aim near 5:1" band. It says nothing about primary text.
  **Update required:** the section names both rules and the primary tokens.
- `AGENTS.md` §3.4 (theme bullet) and §4 (gate command list) — both describe the check as
  guarding `muted.foreground`, `tab.foreground` and `table.head.foreground`.
  **Update required:** one clause each.
- `docs/spec-intakes/IN-0042-ux-polish-round-1/US-0111-secondary-text-contrast-floor.md` — the
  predecessor. Its Scope explicitly puts every other token, `foreground` included, out of
  scope ("Primary text already measures 13:1"), which is true of the dark themes and false of
  three light ones. **No change:** `US-0111` is accepted and its record is accurate about what
  it did; this packet is the follow-up its own gap row names, not a correction of it.
- `evidence/US-0111-verify.md` — the independent verification. Its major finding (a token
  measured against a surface it is not drawn on is not evidence) is the constraint on how the
  new surfaces were chosen. **No change:** it is a dated verification record.
- `evidence/before-after-report.md` §4.2 / §4.6 — **update required:** §4.2 gets its closure
  line; §4.6's `US-0111` row keeps its text, which remains a true record of what stood then.
- `docs/PROJECT.md` — read for standing invariants; nothing there constrains theme token
  values. **No change.**
- `reference/gpui-kit/crates/component/src/theme/schema.rs` — the token fallback chain
  (`apply_color!` lines 970, 984, 997, 1000). Reference material, not an owning doc.
  **No change.**

### Documentation Action

Update required: `docs/gui-layout.md` § "Secondary text contrast floor" becomes a section
about both rules; `AGENTS.md` §3.4/§4 follow it; the report's §4.2 is closed.

Reason: the floor alone is what let this through. A contributor who reads only "clears 4.5:1"
can raise a secondary token and ship an inverted page, which is what happened. The rule that
prevents it has to be written where a theme author reads, not only where a script fails.

### Reconciliation

Docs changed:

- `docs/gui-layout.md` — the section is renamed **Text contrast floor and hierarchy**, names
  the primary tokens and both rules, and keeps the `SURFACES`-is-the-contract paragraph.
- `AGENTS.md` §3.4 — the theme bullet names both rules and the primary tokens; §4's gate list
  comment now reads "text >= 4.5:1 in every built-in theme, primary above secondary".
- `evidence/before-after-report.md` §4.2 — closure line, confirming the two ratios the section
  quoted as real measurements of `foreground` on `tab_bar.background` and adding the two
  extensions (a third inverted variant, and five button/menu values below the floor).
- `docs/spec-intakes/IN-0042-ux-polish-round-1/IN-0042.md` — `US-0127` added to the packet
  list.
- `evidence/US-0127-solarized-light.png`, `evidence/US-0127-everforest-light.png` and their
  two `US-0127-before-*` counterparts — the GUI proof.
- `scripts/ci-local.ps1` / `scripts/ci-local.sh` — the comment beside the step still described
  a secondary-text-only check; it now names both rules and every checked token.
- `evidence/US-0127-verify.md` — the independent verification that reopened this packet, kept
  as written. It is a dated record of the first revision, not of this one.

No-change reasons confirmed still valid: `US-0111` and `evidence/US-0111-verify.md` (accepted,
dated records of work that was correct for its scope), `docs/PROJECT.md`, and the kit schema.

## Context

- **Which token is primary text on which surface is not one answer.** The kit sets the
  window's default text colour once (`reference/gpui-kit/crates/component/src/root.rs:593`)
  and most labels inherit it, but three surfaces route primary text through their own token,
  each falling back to `foreground` when a theme omits it:
  `popover.foreground` (`schema.rs:970`), `sidebar.foreground` (`schema.rs:984`) and
  `tab.active.foreground` (`schema.rs:997`). Twenty of the 39 variants set
  `popover.foreground`, 29 set `tab.active.foreground`, 3 set `sidebar.foreground`.
  Measuring `foreground` on those surfaces would have scored a colour the kit does not paint
  there — the exact flaw `evidence/US-0111-verify.md` raised as its major finding — so each
  gets its own `SURFACES` row and the existing `FALLBACKS` machinery resolves it to
  `foreground` for the variants that leave it unset. It changed the answer: **Ayu Light's
  `popover.foreground` is a fourth failure** the naive model would have missed, and Ayu Light
  itself is a third failing variant the report did not list.
- **The tab strip carries two text tokens, not one, and §4.2's numbers are real.** An
  inactive tab label is `tab.foreground`, which `US-0111` already measures on
  `tab_bar.background` — but the **dock panel headers** ("Session", "SFTP Browser") are drawn
  on the same strip in the plain `foreground`: the kit's `render_title` sets a text colour
  only `when_some(title_style, …)` (kit `dock/tab_panel.rs:356-425`, the `when_some` at
  `:380-382`) and `Panel::title_style` defaults to `None` (kit `dock/panel.rs:87-89`), which
  neither `SessionPanel` nor `SftpPanel` overrides. So §4.2's headline ratios (Solarized Light
  4.39, Everforest Light 4.71) are `foreground` on `tab_bar.background`, a pair the
  application really paints, and **Solarized Light's primary text was below the floor**.
  A first revision of this packet excluded that surface on the reasoning that the strip draws
  only `tab.foreground`, and wrote the opposite into the report; the independent verification
  (`evidence/US-0127-verify.md`) caught it from these frames. One surface can carry more than
  one token, and declaring a pair out of scope is the one move the `SURFACES` table cannot
  make safely: an omission is indistinguishable from a pair that is never drawn.
- **Buttons and menu rows have their own text pair, and three variants were below the floor
  there.** `accent.foreground` on `accent.background` is the label of a hovered or selected
  menu row (kit `menu/menu_item.rs:115-120` — a site the script already cited for the *other*
  side of the swap), and `secondary.foreground` on `secondary.background` is a `Secondary`
  button or tag (kit `theme/schema.rs:828`/`:830-832`, `tag.rs:30`/`:78`). Both fall back to
  `foreground` (kit `theme/schema.rs:904`, `:819`). A first revision filed these as
  "unmodelled"; measured, five values across four variants were already below 4.5:1, two of
  them also inverted against `muted.foreground` on a hovered menu row.
- Ladder: no new file, no new dependency, no new machinery. The primary tokens are four more
  `SURFACES` rows; the hierarchy rule is a tuple of `(primary, secondary)` pairs and one
  function that walks the intersection of their surface lists. `FALLBACKS`, `PARENTS`,
  `measure` and the gradient/alpha handling are reused unchanged.
- The fix is the same lightness-only move `US-0111` made, in the opposite direction: the
  themes are light, so primary text darkens.

## Plan

- [x] Extend the check first; run it over the current themes and record the inventory.
- [x] Move the flagged values in lightness only, smallest step that clears both rules with
      about half a ratio point of headroom.
- [x] Re-run the check, the self-test and a negative test.
- [x] Update `docs/gui-layout.md`, `AGENTS.md`, the report's §4.2 and `IN-0042.md`.
- [x] `pwsh scripts/ci-local.ps1`.
- [x] GUI frames in both fixed themes, with pixel-measured ratios.

## Decisions

None. WCAG AA is not a choice this project makes, and "primary text reads more strongly than
secondary text" is a restatement of what "primary" means, not a new position.

## Verification Plan

1. **Focused:** `python scripts/check-theme-contrast.py --self-test` — the maths and both
   rules, against fixtures with independently known ratios.
2. **Negative:** put Solarized Light's old `foreground` back; the check must exit 1. Repeat
   for the floor rule with a deliberately dimmed `foreground`.
3. **Unit/Integration:** `cargo test --workspace` — the theme crate still loads and registers
   every edited JSON.
4. **Platform:** `pwsh scripts/ci-local.ps1`.
5. **E2E:** the application's own window in Solarized Light and in Everforest Light, captured
   from this packet's build, driving only its own process id, with `PrintWindow`. Sample the
   glyph core of one primary and one secondary text run in each frame and recompute the ratio
   from the pixels, against the same runs in a build whose `crates/theme/themes/` is the
   pre-fix state (the themes are `include_str!`-embedded, so a before frame needs its own
   build).

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

All numbers below are the **reworked** check (the one on this branch). Where the first
revision of this packet reported something different, the difference is named.

### What the extended check finds on `main`, before any theme was edited

| | before (`main`) | after | first revision |
|---|---|---|---|
| foreground/surface pairings | 1365 | 1365 | 1248 |
| token/variant rows | 351 | 351 | 273 |
| primary/secondary comparisons | 585 | 585 | 507 |
| rows below 4.5:1 | **6** | 0 | 0 |
| inverted comparisons | **42** (in 12 rows) | 0 | 37, reported as "10" |

The three additions the rework makes are +117 pairings (39 variants x 3: `foreground` on
`tab_bar.background`, plus the two new tokens), +78 rows and +78 comparisons. They are what
turns 0 rows below the floor into 6, and 10 reported rows into 12.

The six below the floor:

```
ayu.json:         Ayu Light (light):        accent.foreground    on accent.background    4.13:1
everforest.json:  Everforest Light (light): accent.foreground    on accent.background    4.21:1
everforest.json:  Everforest Light (light): secondary.foreground on secondary.background 4.43:1
everforest.json:  Everforest Dark (dark):   secondary.foreground on secondary.background 3.62:1
solarized.json:   Solarized Light (light):  foreground           on tab_bar.background   4.39:1
tokyonight.json:  Tokyo Moon (dark):        secondary.foreground on secondary.background 4.26:1
```

The twelve inverted rows, worst surface first, with how many of that pair's shared surfaces
are inverted:

```
Ayu Light         foreground            background            6.10 <= 6.80  (11 surfaces)
Ayu Light         popover.foreground    popover.background    4.88 <= 5.91  (1)
Ayu Light         sidebar.foreground    sidebar.background    5.76 <= 6.43  (1)
Ayu Light         tab.active.foreground tab.active.background 5.62 <= 6.80  (1)
Ayu Light         accent.foreground     accent.background     4.13 <= 5.00  (1)
Everforest Light  foreground            background            5.18 <= 5.55  (11 surfaces)
Everforest Light  popover.foreground    popover.background    5.18 <= 5.55  (1)
Everforest Light  sidebar.foreground    sidebar.background    5.03 <= 5.39  (1)
Everforest Light  tab.active.foreground tab.active.background 5.18 <= 5.55  (1)
Everforest Light  accent.foreground     accent.background     4.21 <= 4.51  (1)
Solarized Light   foreground            background            4.99 <= 5.71  (11 surfaces)
Solarized Light   sidebar.foreground    sidebar.background    4.77 <= 5.45  (1)
```

11 + 1 + 1 + 1 + 1 = 15 comparisons per variant; 42 of the 585 were inverted. The first
revision printed the *row* count (10) on a line that named comparisons (507) -- two different
units side by side. `inversions()` now returns the per-row count and every caller reports both.

**Alduin was not flagged.** The report listed it as "close" at 5.65; that is `foreground` on
the title and status strips against `muted.foreground`'s 5.05 there -- correct order, 0.6 of
headroom. **Ayu Light was flagged**, inverted on all five of its pairs; the report listed it as
"close" rather than failing. Both stand from the first revision.

**Solarized Light's `foreground` really was below the floor**, at 4.39:1 on
`tab_bar.background` -- the surface the first revision excluded. See Context; the correction is
carried into `evidence/before-after-report.md` §4.2.

### Changed values -- twelve keys, six distinct colours, five variants, four files

Every move is lightness-only: the light variants darken their text, the two dark variants
lighten it. Max hue drift **0.29 deg**, max saturation drift **0.28 pp**, both the 8-bit
rounding of a pure lightness step. **No secondary token was touched.**

| variant | key(s) | before | after | HSL before -> after |
|---|---|---|---|---|
| Solarized Light | `foreground` | `#586E75` | `#4B5E64` | h194.5 s14.1% l40.2% -> h194.4 s14.3% l34.3% |
| Everforest Light | `foreground`, `popover.foreground`, `tab.active.foreground`, `accent.foreground`, `secondary.foreground` | `#5F6D75` | `#546067` | h201.8 s10.4% l41.6% -> h202.1 s10.2% l36.7% |
| Ayu Light | `foreground` | `#5c6166` | `#4E5256` | h210.0 s5.2% l38.0% -> h210.0 s4.9% l32.2% |
| Ayu Light | `popover.foreground`, `tab.active.foreground`, `accent.foreground` | `#5C6773` | `#4B545E` | h211.3 s11.1% l40.6% -> h211.6 s11.2% l33.1% |
| Everforest Dark | `secondary.foreground` | `#849087` | `#A9B1AB` | h135.0 s5.1% l54.1% -> h135.0 s4.9% l67.8% |
| Tokyo Moon | `secondary.foreground` | `#8c94b5` | `#9BA2BF` | h228.3 s21.7% l62.9% -> h228.3 s22.0% l67.8% |

Where a variant spelled one colour in several keys, all of them move together, so no theme
gains a near-duplicate: Everforest Light's five keys were all `#5F6D75`, and Ayu Light's three
were all `#5C6773`. Ayu Light's `#5C6773` group is one 8-bit step darker than the first
revision's `#4C555F`, which is what its `accent.foreground` needed to keep half a ratio point
over `muted.foreground` on a hovered menu row.

Everforest Light's `accent.foreground` and `secondary.foreground` were the two values the first
revision left at `#5F6D75` and filed as "unmodelled"; they are now part of the same group.

#### Per-surface ratios, before -> after

`muted.foreground` is unchanged throughout and is shown wherever the hierarchy rule applies.
Only the tokens that moved are listed; every other token in every other variant is untouched.

**Solarized Light**

| token | surface | before | after | `muted.foreground` |
|---|---|---|---|---|
| `foreground` | `background` | 4.99 | **6.31** | 5.71 |
| `foreground` | `popover.background` | 4.99 | **6.31** | 5.71 |
| `foreground` | `title_bar.background` | 4.57 | **5.78** | 5.22 |
| `foreground` | `status_bar.background` | 4.57 | **5.78** | 5.22 |
| `foreground` | `tab_bar.background` | 4.39 | **5.56** | 5.02 |
| `foreground` | `list.background` | 4.99 | **6.31** | 5.71 |
| `foreground` | `list.even.background` | 4.63 | **5.85** | 5.29 |
| `foreground` | `list.hover.background` | 4.53 | **5.73** | 5.18 |
| `foreground` | `table.background` | 4.99 | **6.31** | 5.71 |
| `foreground` | `table.even.background` | 4.63 | **5.85** | 5.29 |
| `foreground` | `table.hover.background` | 4.53 | **5.73** | 5.18 |
| `foreground` | `table.head.background` | 4.99 | **6.31** | -- |
| `sidebar.foreground` | `sidebar.background` | 4.77 | **6.03** | 5.45 |

**Everforest Light**

| token | surface | before | after | `muted.foreground` |
|---|---|---|---|---|
| `foreground` | `background` | 5.18 | **6.28** | 5.55 |
| `foreground` | `popover.background` | 5.18 | **6.28** | 5.55 |
| `foreground` | `title_bar.background` | 4.89 | **5.92** | 5.24 |
| `foreground` | `status_bar.background` | 4.89 | **5.92** | 5.24 |
| `foreground` | `tab_bar.background` | 4.71 | **5.71** | 5.05 |
| `foreground` | `list.background` | 5.18 | **6.28** | 5.55 |
| `foreground` | `list.even.background` | 4.71 | **5.71** | 5.05 |
| `foreground` | `list.hover.background` | 4.59 | **5.55** | 4.91 |
| `foreground` | `table.background` | 5.18 | **6.28** | 5.55 |
| `foreground` | `table.even.background` | 4.71 | **5.71** | 5.05 |
| `foreground` | `table.hover.background` | 4.59 | **5.55** | 4.91 |
| `foreground` | `table.head.background` | 4.71 | **5.71** | -- |
| `sidebar.foreground` | `sidebar.background` | 5.03 | **6.09** | 5.39 |
| `popover.foreground` | `popover.background` | 5.18 | **6.28** | 5.55 |
| `tab.active.foreground` | `tab.active.background` | 5.18 | **6.28** | 5.55 |
| `accent.foreground` | `accent.background` | 4.21 | **5.10** | 4.51 |
| `secondary.foreground` | `secondary.background` | 4.43 | **5.37** | -- |

**Ayu Light**

| token | surface | before | after | `muted.foreground` |
|---|---|---|---|---|
| `foreground` | `background` | 6.10 | **7.68** | 6.80 |
| `foreground` | `popover.background` | 5.30 | **6.67** | 5.91 |
| `foreground` | `title_bar.background` | 5.30 | **6.67** | 5.91 |
| `foreground` | `status_bar.background` | 5.30 | **6.67** | 5.91 |
| `foreground` | `tab_bar.background` | 5.69 | **7.17** | 6.35 |
| `foreground` | `list.background` | 6.10 | **7.68** | 6.80 |
| `foreground` | `list.even.background` | 5.01 | **6.31** | 5.59 |
| `foreground` | `list.hover.background` | 5.09 | **6.41** | 5.68 |
| `foreground` | `table.background` | 6.10 | **7.68** | 6.80 |
| `foreground` | `table.even.background` | 5.01 | **6.31** | 5.59 |
| `foreground` | `table.hover.background` | 5.09 | **6.41** | 5.68 |
| `foreground` | `table.head.background` | 5.85 | **7.37** | -- |
| `sidebar.foreground` | `sidebar.background` | 5.76 | **7.26** | 6.43 |
| `popover.foreground` | `popover.background` | 4.88 | **6.52** | 5.91 |
| `tab.active.foreground` | `tab.active.background` | 5.62 | **7.50** | 6.80 |
| `accent.foreground` | `accent.background` | 4.13 | **5.51** | 5.00 |

**Everforest Dark**

| token | surface | before | after | `muted.foreground` |
|---|---|---|---|---|
| `secondary.foreground` | `secondary.background` | 3.62 | **5.48** | -- |

**Tokyo Moon**

| token | surface | before | after | `muted.foreground` |
|---|---|---|---|---|
| `secondary.foreground` | `secondary.background` | 4.26 | **5.05** | -- |

Nothing that already cleared both rules moved: the other 34 variants are untouched, and the
row count is 351 before and after, so no token was dropped from the check to make it pass.

### Commands

```
python scripts/check-theme-contrast.py --self-test
    check-theme-contrast: self-test passed

python scripts/check-theme-contrast.py
    check-theme-contrast: 1365 foreground/surface pairings across 351 token/variant rows,
    all >= 4.5:1; primary text out-reads muted.foreground on all 585 shared-surface comparisons
    (exit 0)

python scripts/check-theme-contrast.py --report | tail -3
    351 measurements, 0 below 4.5:1
    585 primary/muted.foreground comparisons, 0 inverted across 0 token/variant row(s)

# Negative 1 (hierarchy AND floor) -- Solarized Light's old foreground #586E75 put back.
# With `tab_bar.background` measured, this now trips both rules, which is the whole point
# of the rework: the first revision's model let it trip only the hierarchy rule.
python scripts/check-theme-contrast.py            # exit 1
    solarized.json: Solarized Light (light): foreground on tab_bar.background is 4.39:1
    solarized.json: Solarized Light (light): on background foreground is 4.99:1 but
      muted.foreground is 5.71:1 -- secondary text out-reads primary (11 of its surface(s))
    solarized.json: Solarized Light (light): on sidebar.background sidebar.foreground is
      4.77:1 but muted.foreground is 5.45:1 -- secondary text out-reads primary (1 of its
      surface(s))
    check-theme-contrast: 1 token(s) below the 4.5:1 floor
    check-theme-contrast: 12 comparison(s) in 2 token/variant row(s) where muted.foreground
      reads at least as strongly as the primary token beside it

# Negative 2 (floor) -- Zed One Dark foreground dimmed to #4a4a4a. Full transcript this
# time: the first revision's quoted block omitted the hierarchy line the same run prints.
python scripts/check-theme-contrast.py            # exit 1
    zed-one-dark.json: Zed One Dark (dark): foreground on list.hover.background is 1.47:1
    zed-one-dark.json: Zed One Dark (dark): on popover.background foreground is 1.80:1 but
      muted.foreground is 6.14:1 -- secondary text out-reads primary (11 of its surface(s))
    check-theme-contrast: 1 token(s) below the 4.5:1 floor
    check-theme-contrast: 11 comparison(s) in 1 token/variant row(s) where muted.foreground
      reads at least as strongly as the primary token beside it

cargo test -p oneterm-theme                       # exit 0, every edited JSON still loads
pwsh scripts/ci-local.ps1                         # CARGO_BUILD_JOBS=4
    ci-local: all checks passed.
```

Both negatives were reverted immediately; the working tree holds only the intended values.

### GUI walk

Two builds of `cargo build -p oneterm-app --profile fast-dev` from this worktree: the after
state, and a before state with only `crates/theme/themes/` reverted to the pre-fix commit.
Each run launched one process, found its own window by `EnumWindows` +
`GetWindowThreadProcessId` filtered on its own pid, sized it to 1400x900, captured it with
`PrintWindow(hwnd, dc, PW_RENDERFULLCONTENT)` -- the desktop is locked, as it was for the
whole round -- and closed that pid. No window was addressed by name or title and no process
was closed by image name. The theme was selected by writing `theme_name` into the walk's own
`target/ui_config.json` before launch; the rest of the state is the round's `cfgbak/`, so the
session tree holds the same `DevServer` and `PAM` entries. Both after frames were re-taken on
the reworked palette.

Ratios are recomputed from the PNG pixels: the script locates every pixel equal to the token
colour inside the named region and reads the fill beside the glyphs, so both the colour and
**the surface** are measured rather than assumed -- which is exactly what the first revision
got wrong when it reported the dock header against the window body.

| frame | text run | glyph px | surface px | surface is | ratio |
|---|---|---|---|---|---|
| `US-0127-before-solarized-light.png` | `DevServer` session name (primary) | `#586e75` | `#fdf6e3` | `background` | 4.99:1 |
| `US-0127-before-solarized-light.png` | `root@192.168.13.128:22` (secondary) | `#576464` | `#fdf6e3` | `background` | **5.71:1 -- louder** |
| `US-0127-before-solarized-light.png` | `Session` dock header (primary) | `#586e75` | `#eee8d5` | `tab_bar.background` | **4.39:1 -- below the floor** |
| `US-0127-solarized-light.png` | `DevServer` session name (primary) | `#4b5e64` | `#fdf6e3` | `background` | **6.31:1** |
| `US-0127-solarized-light.png` | `root@192.168.13.128:22` (secondary) | `#576464` | `#fdf6e3` | `background` | 5.71:1 |
| `US-0127-solarized-light.png` | `Session` dock header (primary) | `#4b5e64` | `#eee8d5` | `tab_bar.background` | **5.56:1** |
| `US-0127-before-everforest-light.png` | `DevServer` session name (primary) | `#5f6d75` | `#fefcee` | `background` | 5.18:1 |
| `US-0127-before-everforest-light.png` | `root@192.168.13.128:22` (secondary) | `#62676a` | `#fefcee` | `background` | **5.55:1 -- louder** |
| `US-0127-before-everforest-light.png` | `Session` dock header (primary) | `#5f6d75` | `#f4f1e2` | `tab_bar.background` | 4.71:1 |
| `US-0127-everforest-light.png` | `DevServer` session name (primary) | `#546067` | `#fefcee` | `background` | **6.28:1** |
| `US-0127-everforest-light.png` | `root@192.168.13.128:22` (secondary) | `#62676a` | `#fefcee` | `background` | 5.55:1 |
| `US-0127-everforest-light.png` | `Session` dock header (primary) | `#546067` | `#f4f1e2` | `tab_bar.background` | **5.71:1** |

`#eee8d5` and `#f4f1e2` are those themes' `tab_bar.background` values, read from the JSON, so
the dock header is `foreground` on the tab strip and nothing else. Every figure matches the
per-surface tables above to 0.01.

The Everforest after frame also shows the two newly raised button tokens: `#5f6d75` (the old
`accent.foreground` / `secondary.foreground`) occurs 980 times in the before frame and **0**
times in the after frame, where `#546067` occurs 973 times.

### Gaps

- **`muted.background` carries no primary-text row.** The key-binding chip's text is the
  secondary token; the check records that in a comment rather than a measurement. Every other
  surface `muted.foreground` is drawn on now has a primary token measured against it too.
- **`button.*` tokens are only reached through their fallbacks.** `button.secondary.background`
  and `button.secondary.foreground` fall back to the `secondary.*` pair this packet measures,
  and 38 of the 39 variants take that fallback -- but the one variant that sets them
  (`macos-classic.json`) has its button colours unmeasured, as do `button.primary.*`,
  `button.danger.*` and the rest. A theme that overrides them can still ship an illegible
  button.
- **`secondary.foreground` has no OneTerm call site of its own.** Its draw sites are the kit's
  `Secondary` button and tag; the surface is listed because the values are the theme's, not
  because a OneTerm component was found painting that exact pair. If the kit stops using it,
  the row becomes decoration.
- **`sidebar.foreground`'s group headings are drawn at 70 % opacity** (kit
  `sidebar/group.rs:69`); the check measures the token at full opacity, so a heading's real
  ratio is lower than the row reports. Modelling per-call-site opacity would need a fourth
  mechanism beside `PARENTS`, `FALLBACKS` and gradient stops.
- **The floor rule has no end-to-end fixture.** The self-test's below-floor case asserts on
  `measure` directly while the hierarchy cases go through `inversions()`; nothing exercises
  `rows()` -> `failures` on a fixture, so a wiring mistake between the two would only be caught
  by a real theme going bad. Raised by the independent verification as TRIVIAL 9 and left
  standing.
- **No hover, selection, menu or multi-tab state was driven in the walk** -- posted messages
  cannot deliver a real hover -- so the `list.hover`, `table.hover`, `popover`, `accent`,
  `secondary` and `tab.active` rows are computed, not photographed. The frames do photograph
  the `background` and `tab_bar.background` rows.
- **Windows only**, as the whole round was.
- **`evidence/US-0127-verify.md` describes the first revision**, not this one. It is kept as
  written; its MAJOR 1, MAJOR 2, MEDIUM 3, MINOR 4, MINOR 5, MINOR 6 and MINOR 7 are all acted
  on here, TRIVIAL 7 is folded into the negative-test transcript above, and TRIVIAL 9 is the
  gap immediately above. No second independent verification of the rework has been run.

## Handoff

None. The work is complete on `fix/primary-text-contrast-floor`; nothing is left in flight.
