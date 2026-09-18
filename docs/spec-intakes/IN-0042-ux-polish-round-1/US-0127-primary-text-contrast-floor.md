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

Pending the rework's re-proof.

## Handoff

In rework.
