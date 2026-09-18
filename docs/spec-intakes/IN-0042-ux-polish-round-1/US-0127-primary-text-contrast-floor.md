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
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

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
  - Every non-text token: backgrounds, borders, the ANSI palette, `accent.foreground` and
    `secondary.foreground` (text on a button fill, a surface pair this check does not model).
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
- `evidence/before-after-report.md` §4.2 — closure line, plus a correction of the two ratios
  the section quoted (see Evidence: they were measured on a surface the themes do not paint
  primary text on).
- `docs/spec-intakes/IN-0042-ux-polish-round-1/IN-0042.md` — `US-0127` added to the packet
  list.
- `evidence/US-0127-solarized-light.png`, `evidence/US-0127-everforest-light.png` and their
  two `US-0127-before-*` counterparts — the GUI proof.

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
- **The tab strip is not a primary-text surface.** An inactive tab label is
  `tab.foreground`, which `US-0111` already measures on `tab_bar.background`; the active one
  is `tab.active.foreground` on `tab.active.background`. §4.2's headline numbers (Solarized
  Light 4.39, Everforest Light 4.71) are `foreground` scored against the strip, where those
  themes draw `tab.foreground` (`#52646b`, `#5b6971`) instead. Attributed correctly, **no
  variant's primary text was actually below 4.5:1** — Solarized Light bottomed out at 4.53:1
  on a hovered row. The defect §4.2 describes is real and is entirely the *hierarchy*: the
  floor rule alone would never have caught it, which is why rule 2 exists.
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

### What the extended check found before any theme was edited

| | before | after |
|---|---|---|
| foreground/surface pairings | 1248 (546 of them new: the four primary tokens) | 1248 |
| token/variant rows | 273 (156 new) | 273 |
| rows below 4.5:1 | **0** | 0 |
| primary/secondary comparisons | 507 (all new) | 507 |
| comparisons inverted | **10**, across 3 variants | 0 |

The ten, as the check printed them (`ayu.json` x4, `everforest.json` x4, `solarized.json` x2):

```
ayu.json: Ayu Light (light): on background foreground is 6.10:1 but muted.foreground is 6.80:1
ayu.json: Ayu Light (light): on popover.background popover.foreground is 4.88:1 but muted.foreground is 5.91:1
ayu.json: Ayu Light (light): on sidebar.background sidebar.foreground is 5.76:1 but muted.foreground is 6.43:1
ayu.json: Ayu Light (light): on tab.active.background tab.active.foreground is 5.62:1 but muted.foreground is 6.80:1
everforest.json: Everforest Light (light): on background foreground is 5.18:1 but muted.foreground is 5.55:1
everforest.json: Everforest Light (light): on popover.background popover.foreground is 5.18:1 but muted.foreground is 5.55:1
everforest.json: Everforest Light (light): on sidebar.background sidebar.foreground is 5.03:1 but muted.foreground is 5.39:1
everforest.json: Everforest Light (light): on tab.active.background tab.active.foreground is 5.18:1 but muted.foreground is 5.55:1
solarized.json: Solarized Light (light): on background foreground is 4.99:1 but muted.foreground is 5.71:1
solarized.json: Solarized Light (light): on sidebar.background sidebar.foreground is 4.77:1 but muted.foreground is 5.45:1
```

`sidebar.foreground` is unset in all three, so it follows `foreground`; Solarized Light's
`popover.foreground` and `tab.active.foreground` are `#073642`, far above the floor, and were
not touched. **Alduin was not flagged** — the report listed it as "close" at 5.65, but that is
`foreground` on the title and status strips against `muted.foreground`'s 5.05 there: correct
order, 0.6 of headroom. **Ayu Light was flagged**, at 5.01 on an alternating row and inverted
on all four of its surfaces; the report had listed it as "close" rather than failing.

### Changed values — six keys, four distinct colours

Every move is lightness-only. Max hue drift **0.3°**, max saturation drift **0.3 pp**, both the
8-bit rounding of a pure lightness step. **No secondary token was touched.**

| theme | token(s) | before | after | HSL before → after |
|---|---|---|---|---|
| Solarized Light | `foreground` | `#586E75` | `#4B5E64` | h194.5 s14.1% l40.2% → h194.4 s14.3% l34.3% |
| Everforest Light | `foreground`, `popover.foreground`, `tab.active.foreground` | `#5F6D75` | `#546067` | h201.8 s10.4% l41.6% → h202.1 s10.2% l36.7% |
| Ayu Light | `foreground` | `#5c6166` | `#4E5256` | h210.0 s5.2% l38.0% → h210.0 s4.9% l32.2% |
| Ayu Light | `popover.foreground`, `tab.active.foreground` | `#5C6773` | `#4C555F` | h211.3 s11.1% l40.6% → h211.6 s11.1% l33.5% |

Everforest Light spelled the same colour in three keys, so all three move together; its
`accent.foreground` and `secondary.foreground` keep the old `#5F6D75` and are left alone — they
are text on a button fill, a pair this check does not model.

#### Per-surface ratios, before → after (`muted.foreground` unchanged, shown for the rule)

**Solarized Light** — `foreground`, and `sidebar.background` through the `sidebar.foreground`
fallback:

| surface | before | after | `muted.foreground` |
|---|---|---|---|
| `background` | 4.99 | **6.31** | 5.71 |
| `popover.background` | 4.99 | **6.31** | 5.71 |
| `title_bar.background` | 4.57 | **5.78** | 5.22 |
| `status_bar.background` | 4.57 | **5.78** | 5.22 |
| `list.background` | 4.99 | **6.31** | 5.71 |
| `list.even.background` | 4.63 | **5.85** | 5.29 |
| `list.hover.background` | 4.53 | **5.73** | 5.18 |
| `table.background` | 4.99 | **6.31** | 5.71 |
| `table.even.background` | 4.63 | **5.85** | 5.29 |
| `table.hover.background` | 4.53 | **5.73** | 5.18 |
| `table.head.background` | 4.99 | **6.31** | — |
| `sidebar.background` | 4.77 | **6.03** | 5.45 |

**Everforest Light** — `foreground`, plus `popover.foreground` and `tab.active.foreground`,
which carried the same value:

| surface | before | after | `muted.foreground` |
|---|---|---|---|
| `background` | 5.18 | **6.28** | 5.55 |
| `popover.background` | 5.18 | **6.28** | 5.55 |
| `title_bar.background` | 4.89 | **5.92** | 5.24 |
| `status_bar.background` | 4.89 | **5.92** | 5.24 |
| `list.background` | 5.18 | **6.28** | 5.55 |
| `list.even.background` | 4.71 | **5.71** | 5.05 |
| `list.hover.background` | 4.59 | **5.55** | 4.91 |
| `table.background` | 5.18 | **6.28** | 5.55 |
| `table.even.background` | 4.71 | **5.71** | 5.05 |
| `table.hover.background` | 4.59 | **5.55** | 4.91 |
| `table.head.background` | 4.71 | **5.71** | — |
| `sidebar.background` | 5.03 | **6.09** | 5.39 |
| `tab.active.background` | 5.18 | **6.28** | 5.55 |

**Ayu Light** — `foreground`:

| surface | before | after | `muted.foreground` |
|---|---|---|---|
| `background` | 6.10 | **7.68** | 6.80 |
| `popover.background` | 5.30 | **6.67** | 5.91 |
| `title_bar.background` | 5.30 | **6.67** | 5.91 |
| `status_bar.background` | 5.30 | **6.67** | 5.91 |
| `list.background` | 6.10 | **7.68** | 6.80 |
| `list.even.background` | 5.01 | **6.31** | 5.59 |
| `list.hover.background` | 5.09 | **6.41** | 5.68 |
| `table.background` | 6.10 | **7.68** | 6.80 |
| `table.even.background` | 5.01 | **6.31** | 5.59 |
| `table.hover.background` | 5.09 | **6.41** | 5.68 |
| `table.head.background` | 5.85 | **7.37** | — |
| `sidebar.background` | 5.76 | **7.26** | 6.43 |

**Ayu Light** — `popover.foreground` / `tab.active.foreground`, which override `foreground` and
so are measured on their own:

| surface | before | after | `muted.foreground` |
|---|---|---|---|
| `popover.background` | 4.88 | **6.42** | 5.91 |
| `tab.active.background` | 5.62 | **7.38** | 6.80 |

Nothing that already cleared both rules moved: the other 36 variants are untouched, and the row
count is 273 before and after, so no token was dropped from the check to make it pass.

### Commands

```
python scripts/check-theme-contrast.py --self-test
    check-theme-contrast: self-test passed

python scripts/check-theme-contrast.py
    check-theme-contrast: 1248 foreground/surface pairings across 273 token/variant rows,
    all >= 4.5:1; primary text out-reads muted.foreground on all 507 shared-surface comparisons
    (exit 0)

python scripts/check-theme-contrast.py --report | tail -4
    273 measurements, 0 below 4.5:1
    507 primary/muted.foreground comparisons, 0 inverted

# Negative 1 (hierarchy) -- Solarized Light's old foreground #586E75 put back
python scripts/check-theme-contrast.py            # exit 1
    solarized.json: Solarized Light (light): on background foreground is 4.99:1 but
      muted.foreground is 5.71:1 -- secondary text out-reads primary
    solarized.json: Solarized Light (light): on sidebar.background sidebar.foreground is
      4.77:1 but muted.foreground is 5.45:1 -- secondary text out-reads primary
    check-theme-contrast: 2 case(s) where muted.foreground reads at least as strongly as the
      primary token beside it

# Negative 2 (floor) -- Zed One Dark foreground dimmed to #4a4a4a
python scripts/check-theme-contrast.py            # exit 1
    zed-one-dark.json: Zed One Dark (dark): foreground on list.hover.background is 1.47:1
    check-theme-contrast: 1 token(s) below the 4.5:1 floor

cargo test -p oneterm-theme                       # exit 0, every edited JSON still loads
pwsh scripts/ci-local.ps1
    ci-local: all checks passed.
```

Both negatives were reverted immediately; the working tree holds only the intended values.

### GUI walk

Two builds of `cargo build -p oneterm-app --profile fast-dev` from this worktree: the after
state at `f7232072`, and a before state with only `crates/theme/themes/` reverted to the
pre-fix commit. Each run launched one process, found its own window by `EnumWindows` +
`GetWindowThreadProcessId` filtered on its own pid, sized it to 1400x900, captured it with
`PrintWindow(hwnd, dc, PW_RENDERFULLCONTENT)` — the desktop is locked, as it was for the whole
round — and closed that pid. No window was addressed by name or title and no process was closed
by image name. The theme was selected by writing `theme_name` into the walk's own
`target/ui_config.json` before launch; the rest of the state is the round's `cfgbak/`, so the
session tree holds the same `DevServer` and `PAM` entries.

Ratios below are recomputed from the PNG pixels: the script locates every pixel equal to the
token colour inside the named region and reads the panel colour beside the glyphs, so the
numbers are measured, not assumed.

| frame | text run | surface px | glyph px | ratio |
|---|---|---|---|---|
| `US-0127-before-solarized-light.png` | `DevServer` session name (primary) | `#fdf6e3` | `#586e75` | 4.99:1 |
| `US-0127-before-solarized-light.png` | `root@192.168.13.128:22` (secondary) | `#fdf6e3` | `#576464` | **5.71:1 — louder** |
| `US-0127-solarized-light.png` | `DevServer` session name (primary) | `#fdf6e3` | `#4b5e64` | **6.31:1** |
| `US-0127-solarized-light.png` | `root@192.168.13.128:22` (secondary) | `#fdf6e3` | `#576464` | 5.71:1 |
| `US-0127-before-everforest-light.png` | `DevServer` session name (primary) | `#fefcee` | `#5f6d75` | 5.18:1 |
| `US-0127-before-everforest-light.png` | `root@192.168.13.128:22` (secondary) | `#fefcee` | `#62676a` | **5.55:1 — louder** |
| `US-0127-everforest-light.png` | `DevServer` session name (primary) | `#fefcee` | `#546067` | **6.28:1** |
| `US-0127-everforest-light.png` | `root@192.168.13.128:22` (secondary) | `#fefcee` | `#62676a` | 5.55:1 |

The `Session` panel title in the same frames measures with the primary token throughout
(4.99 → 6.31 and 5.18 → 6.28), which is the second primary run asked for. The inversion is
visible in the before frames and gone in the after ones.

### Gaps

- **`accent.background` and `muted.background` carry no primary-text row.** Reading the kit
  says primary text is never drawn on them — a hovered or selected menu row swaps to
  `accent.foreground` (`menu/menu_item.rs:115-120`), and the key-binding chip's text is the
  secondary token — and the check records that in a comment rather than a measurement. If a
  component starts drawing a primary token on either, nothing fails until someone adds the
  surface.
- **`accent.foreground` / `secondary.foreground` / the `button.*` foregrounds are unmodelled.**
  Text on a button or badge fill has its own foreground/background pair that neither `US-0111`
  nor this packet measures; Everforest Light's `accent.foreground` still carries the old
  `#5F6D75`. Worth its own packet if button labels are ever reported as hard to read.
- **`sidebar.foreground`'s group headings are drawn at 70 % opacity** (kit
  `sidebar/group.rs:69`); the check measures the token at full opacity, so a heading's real
  ratio is lower than the row reports. Modelling per-call-site opacity would need a fourth
  mechanism beside `PARENTS`, `FALLBACKS` and gradient stops.
- **No hover or selection state was driven in the walk**, so the `list.hover` / `table.hover`
  rows above are computed, not photographed — posted messages cannot deliver a real hover
  (report §4.7). The same limit applies to the popover and active-tab rows: the frames show
  neither a menu nor a second tab.
- **Windows only**, as the whole round was.
- **The floor rule caught nothing here.** Every failure was the hierarchy rule. That is the
  honest result, and it means §4.2's "below the floor" framing was a surface-attribution
  artefact, which the report now records.

## Handoff

None. The work is complete on `fix/primary-text-contrast-floor`; nothing is left in flight.
