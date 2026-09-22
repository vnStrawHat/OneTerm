# Work: The prompt line gets its background

ID: US-0134
Intake: IN-0044
Created: 2026-09-22

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [ ] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability
- Risk lane: normal
- Spec Intake, when required: `IN-0044` — `docs/spec-intakes/IN-0044-semantic-highlighting-phase-2/IN-0044.md`

Depends on `US-0133`. The band is painted under the rows the OSC 133 roles name, and the
exit-code tint and the band are two readings of the same role data. Painting a band from a
regex guess is the flicker `BUG-0071` was reported for; a wrong foreground on one word is
not.

## Outcome

`ClassStyles::prompt_line_bg` is painted: a background band across the full width of every
row of a prompt line, wrapped or not, in a colour that is correct on a light theme and on a
dark one, under any ANSI or selection background rather than over it.

## Scope

- [ ] In scope:
  - `crates/terminal-view/src/render/row_plan.rs` — `build_row_plan` (the function §8 item 6
    calls `layout_row`) pushes the band before the per-cell loop.
  - `crates/terminal-view/src/theme/terminal_theme.rs` — the band's colour becomes
    theme-relative instead of one fixed value shared by every theme, which means
    `class_styles` can no longer be a `&'static` pointing straight at the parsed asset.
  - `crates/terminal-view/assets/highlight/default.json` — `promptLineBg`'s new meaning as an
    explicit override rather than the only source.
  - `docs/terminal-semantic-highlighting.md` §7 (where the band's colour comes from) and §8
    item 6 (the paint order).
- [ ] Out of scope:
  - Painting the band under the regex fallback's prompt rows. The default is **not** to: see
    Acceptance. Revisit only if the owner asks.
  - Decorations (underline, box) for other classes — §11 Phase 3.
  - Per-theme `terminal.semantic` override blocks in `crates/theme/themes/*.json`. §7
    describes them; no theme has one, and adding the plumbing for a key nobody sets is
    scope this packet does not need.
  - Extending `scripts/check-theme-contrast.py` to terminal tokens — see Documentation.

## Acceptance

- [ ] A prompt row carries a background band across all of `0..cols`.
- [ ] Every row of a **wrapped** prompt carries it, including the row that holds only the
      tail of the cwd and the row that holds only the typed command.
- [ ] The band is painted **before** any per-cell background, so a selection, an ANSI
      background and an inverse cell all paint on top of it and look exactly as they do on a
      non-prompt row.
- [ ] A `LEADING_WIDE_CHAR_SPACER` at a wrap boundary is covered by the band, leaving no
      one-cell hole (`docs/terminal-semantic-highlighting.md` §13 Q4, `BUG-0071` Gaps).
- [ ] The band's colour is derived from the active theme's terminal background, so no built-in
      theme shows a dark band under dark text or a light band under light text.
- [ ] Every foreground drawn on a prompt row clears 4.5:1 against the resolved band, in every
      variant of every theme in `crates/theme/themes/`: the terminal foreground and the
      semantic classes a prompt row can carry (`PromptSign`, `Command`, `Option`, `Path`, and
      `Success`/`Error` for the `US-0133` tint). Proved by a Rust test, for the reason under
      Documentation.
- [ ] **Under the regex fallback (no OSC 133) the band is not painted**, and that is stated in
      §8 item 6 as a rule rather than left as an omission. Rationale: the band is a
      line-level, full-width element, so a regex that changes its mind between frames makes
      the whole row flash.
- [ ] `cargo test -p oneterm-terminal-view` passes.
- [ ] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed", including
      `python scripts/check-theme-contrast.py`.

## Documentation

### Owning Docs Reviewed

- `docs/terminal-semantic-highlighting.md` §7 (`ClassStyles`, the theme JSON block and the
  shipped default asset), §8 item 6 (the band, "inserted before per-cell backgrounds,
  emitted as one rect"), §9 (the merge policy: the band "always paints, line-level, not
  per-cell, and does not touch fg"), §13 Q4 (the wide-char spacer note that says explicitly
  it "matters when §8 item 6 is implemented").
- `AGENTS.md` §3.4 — the theme and contrast rules, including that `SURFACES` is the
  contract and a token drawn on an unlisted surface must add that surface.
- `scripts/check-theme-contrast.py` — the `SURFACES`, `PARENTS`, `FALLBACKS` and `HIERARCHY`
  tables and, above all, what the script reads: `crates/theme/themes/*.json`.
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/low-level-design/render-pipeline.md`
  — `RowPlan`'s elements and their paint order, which is what makes "before" meaningful.

### Documentation Action

Update required:

- `docs/terminal-semantic-highlighting.md` §7 — the band's colour is resolved per theme in
  `build_terminal_theme()`, with the asset value as an explicit override. §7 today implies a
  single asset value applies to every theme, which is what makes it wrong on a light one.
- `docs/terminal-semantic-highlighting.md` §8 item 6 — the paint order as implemented, which
  rows receive it, that it spans the full row width (and therefore covers a spacer), and
  that it is not painted under the regex fallback.

**The contrast gate, and why the proof is a Rust test.** `AGENTS.md` says text drawn on a
surface `SURFACES` does not list is not checked, and that the fix is to add the surface.
That rule cannot be applied literally here, and the packet must not pretend otherwise:

- the script measures **kit UI tokens** read out of `crates/theme/themes/*.json`. The text
  drawn on this band is terminal grid text — an ANSI palette entry or a semantic `Class`
  foreground — which is not one of those tokens;
- the semantic palette is not in a theme file at all. It lives in
  `crates/terminal-view/assets/highlight/default.json`, which the script never opens, so a
  `SURFACES` row naming `prompt_line_bg` would have no foreground to measure against it.

So this packet proves the same property where the colours actually are: a
`crates/terminal-view` test that resolves the band for every embedded theme variant and
applies the same WCAG floor. Extending `SURFACES` to terminal tokens is a change to that
gate's scope and is an owner question recorded in `IN-0044`'s Open Decisions; if the owner
wants it, it is its own packet. Confirm this reasoning still holds at implementation time
and record the confirmation here.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

## Context

- `ClassStyles::prompt_line_bg` is parsed at
  `crates/terminal-view/src/highlight/bridge.rs:51` and read by nothing.
- `TerminalTheme::class_styles` is `&'static ClassStyles`
  (`crates/terminal-view/src/theme/terminal_theme.rs:49`), always the process-wide parsed
  asset (`:105`). A theme-relative band means a per-theme value, so that field's lifetime
  is part of this packet's diff.
- `build_row_plan` (`crates/terminal-view/src/render/row_plan.rs:575`) pushes per-cell
  backgrounds through `push_bg`, which merges with `plan.bg.last_mut()` only. A full-width
  band pushed first neither merges with, nor blocks merging of, the per-cell spans that
  follow.
- `resolve_style` sets `paint_bg = inverse || bg_color != Color::Background`, so a
  default-background cell emits no span of its own and the band shows through. Nothing in
  that function needs to change.

## Plan

- [ ] Resolve the band per theme in `build_terminal_theme()`; settle `class_styles`'
      ownership.
- [ ] Push the band in `build_row_plan` before the cell loop, for `Prompt` and `Command`
      rows only.
- [ ] Contrast test over every embedded theme variant and every foreground a prompt row can
      carry.
- [ ] Frame tests: the band's column range; its order relative to a selection and to an ANSI
      background; every row of a wrapped prompt; no band without a mark.
- [ ] Reconcile §7 and §8 item 6.

## Decisions

If the owner rules on extending `scripts/check-theme-contrast.py` to terminal surfaces,
that is a decision record of its own (`DEC-NNNN`), not a paragraph here.

## Verification Plan

- Unit: the resolved band per theme; the contrast floor per theme variant and per
  prompt-row foreground.
- Integration: `RowPlan` tests for the band's span, its paint order against per-cell
  backgrounds and selection, the wrapped-prompt row set, the spacer column, and its absence
  without a mark.
- E2E: `fast-dev` frames of a wrapped prompt with the band, on a dark theme and a light one,
  under `evidence/`.
- Platform: `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

After implementation, record commands, results, and anything skipped, unavailable, partial, or failing.

## Handoff

Blocked on `US-0133` until the roles exist.
