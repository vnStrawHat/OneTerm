# Work: The prompt line gets its background

ID: US-0134
Intake: IN-0044
Created: 2026-09-22

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
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

- [x] In scope:
  - `crates/terminal-view/src/render/row_plan.rs` — `build_row_plan` (the function §8 item 6
    calls `layout_row`) pushes the band before the per-cell loop.
  - `crates/terminal-view/src/theme/terminal_theme.rs` — the band's colour becomes
    theme-relative instead of one fixed value shared by every theme, which means
    `class_styles` can no longer be a `&'static` pointing straight at the parsed asset.
  - `crates/terminal-view/assets/highlight/default.json` — `promptLineBg`'s new meaning as an
    explicit override rather than the only source.
  - `docs/terminal-semantic-highlighting.md` §7 (where the band's colour comes from) and §8
    item 6 (the paint order).
- [x] Out of scope:
  - Painting the band under the regex fallback's prompt rows. The default is **not** to: see
    Acceptance. Revisit only if the owner asks.
  - Decorations (underline, box) for other classes — §11 Phase 3.
  - Per-theme `terminal.semantic` override blocks in `crates/theme/themes/*.json`. §7
    describes them; no theme has one, and adding the plumbing for a key nobody sets is
    scope this packet does not need.
  - Extending `scripts/check-theme-contrast.py` to terminal tokens — see Documentation.

## Acceptance

- [x] A prompt row carries a background band across all of `0..cols`.
- [x] Every row of a **wrapped** prompt carries it, including the row that holds only the
      tail of the cwd and the row that holds only the typed command.
- [x] The band is painted **before** any per-cell background, so a selection, an ANSI
      background and an inverse cell all paint on top of it and look exactly as they do on a
      non-prompt row.
- [x] A `LEADING_WIDE_CHAR_SPACER` at a wrap boundary is covered by the band, leaving no
      one-cell hole (`docs/terminal-semantic-highlighting.md` §13 Q4, `BUG-0071` Gaps).
- [x] The band's colour is derived from the active theme's terminal background, so no built-in
      theme shows a dark band under dark text or a light band under light text.
- [x] Every foreground drawn on a prompt row clears 4.5:1 against the resolved band, in every
      variant of every theme in `crates/theme/themes/`: the terminal foreground and the
      semantic classes a prompt row can carry (`PromptSign`, `Command`, `Option`, `Path`, and
      `Success`/`Error` for the `US-0133` tint). Proved by a Rust test, for the reason under
      Documentation.
- [x] **Under the regex fallback (no OSC 133) the band is not painted**, and that is stated in
      §8 item 6 as a rule rather than left as an omission. Rationale: the band is a
      line-level, full-width element, so a regex that changes its mind between frames makes
      the whole row flash.
- [x] `cargo test -p oneterm-terminal-view` passes.
- [x] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed", including
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

**Confirmed at implementation time.** The reasoning above still holds:
`scripts/check-theme-contrast.py` reads `crates/theme/themes/*.json` and measures kit UI
tokens; the semantic palette is in `crates/terminal-view/assets/highlight/default.json`,
which the script never opens, so a `SURFACES` row naming `prompt_line_bg` would have no
foreground to measure against it. The gate is unchanged and the floor is held by two Rust
tests plus the render path itself (see Evidence).

### Reconciliation

Changed: `docs/terminal-semantic-highlighting.md` §7 (a new paragraph: `promptLineBg` is
derived per theme, the asset no longer pins one value, the override rule, and where
readability is enforced) and §8 item 6 (rewritten: the `BgSpan`, the paint order and why it
is the whole mechanism, the spacer, which rows, and the no-band-under-the-fallback rule).

No change needed: §9 (the merge policy already says the band is line-level, always paints
and does not touch fg — which is what was implemented), §13 Q4 (its note that the spacer
"matters when §8 item 6 is implemented" is now satisfied rather than contradicted; §8 item 6
states the resolution and points back).

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

- [x] Resolve the band per theme in `build_terminal_theme()`; settle `class_styles`'
      ownership.
- [x] Push the band in `build_row_plan` before the cell loop, for `Prompt` and `Command`
      rows only.
- [x] Contrast test over every embedded theme variant and every foreground a prompt row can
      carry.
- [x] Frame tests: the band's column range; its order relative to a selection and to an ANSI
      background; every row of a wrapped prompt; no band without a mark.
- [x] Reconcile §7 and §8 item 6.

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
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### What was built

- `crates/terminal-view/src/theme/terminal_theme.rs` — `TerminalTheme::prompt_line_bg()`.
  The band is the terminal background moved `PROMPT_BAND_MIX` (11%) of the room it has
  toward the side the foreground is on, lightness only, with `ClassStyles::prompt_line_bg`
  as an explicit override. `DECSCNM` swaps the pair it is derived from.
- `crates/terminal-view/assets/highlight/default.json` — the fixed `"promptLineBg":
  "#1a1a1a"` is gone; the key is now an override nothing ships.
- `crates/terminal-view/src/render/row_plan.rs` — `push_prompt_band()` pushes one
  `BgSpan { col: 0, cols }` before the per-cell loop for a row whose role is `Prompt` or
  `Command`, and returns it as the row's effective background; `resolve_style` takes that as
  the contrast reference for a cell that paints no background of its own.
- `crates/theme/src/theme.rs` — `embedded_theme_files()`, so a test in another crate can
  register every variant without `init()` and the app globals it needs.

### The band's derivation, and why the direction is taken but not the distance

"The background, slightly toward the text" cannot invert a theme: the band is always on the
background's side of the pair, so a light theme gets a light band and a dark theme a dark
one. Taking the **distance** from the foreground as well — a fraction of the gap between the
two — turned out to be fragile: the first GUI walk showed a band one sRGB unit from the
background on a light theme, because the `TerminalTheme::fg` in the running app right after
a theme switch was not the value the theme file names (the unit test over the theme files
resolved the same theme's band correctly). Taking only the **direction** from the foreground
and a fixed fraction of the room in that direction makes the band the same strength on every
theme and robust to that: it depends on one bit of the foreground, not on its value. The
underlying theme-refresh observation is a gap below, not something this packet changed.

Measured in the walk: dark theme `#0a0a0a` background, `#252525` band; light theme `#fafafa`
background, `#dedede` band.

### Commands

- `cargo test -p oneterm-terminal-view` — 389 passed, 3 ignored.
- `cargo test -p oneterm-highlight -p oneterm-terminal-view -p oneterm-theme` — green.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `pwsh scripts/ci-local.ps1` — see the final entry below, including
  `python scripts/check-theme-contrast.py`, which is unchanged and unaffected.

### Tests

`crates/terminal-view/src/theme/tests.rs`: `the_band_never_crosses_its_theme`,
`the_shipped_asset_leaves_the_band_to_the_theme`,
`every_prompt_row_foreground_clears_the_band` (every embedded theme variant, every
prompt-row foreground: the terminal foreground plus `PromptSign`, `Command`, `Option`,
`Path`, `Success`, `Error`).

`crates/terminal-view/src/render/row_plan.rs`:
`a_marked_prompt_row_carries_a_full_width_band`,
`every_row_of_a_wrapped_prompt_carries_the_band`,
`the_band_is_painted_under_the_per_cell_backgrounds`,
`the_band_covers_a_leading_wide_char_spacer`, `an_unmarked_prompt_row_gets_no_band`,
`a_marked_output_row_gets_no_band`, `every_glyph_on_a_prompt_row_clears_the_band` (the
floor through `build_row_plan`, so the render path really does measure against the band).

`crates/terminal-view/src/highlight/bridge.rs`: `default_styles_loaded` (the asset pins no
band), `prompt_line_bg_is_parsed_when_a_theme_sets_it`.

### GUI walk (Windows, `fast-dev`)

- `evidence/us-0134-band-dark-theme.png` — Default Dark. The band covers `0..cols` on every
  prompt row, including **both** rows of the wrapped first prompt; output rows have none.
- `evidence/us-0134-band-light-theme.png` — Zed One Light, the same screen. The band is
  light, not the dark stripe the old fixed `#1a1a1a` would have painted under dark text.

### Gaps

- **`TerminalTheme::fg` right after a theme switch.** The walk showed the running theme's
  foreground lightness close to its background's on a light theme, which the same theme's
  colours in a unit test do not reproduce. `refresh_theme` layers
  `apply_dynamic_colors(base, dynamic)` with a `dynamic` read earlier in the same frame, so
  a stale value is plausible; it was not investigated here because it is neither caused nor
  worsened by this packet, and the band no longer depends on it. Worth its own `BUG` if the
  owner sees washed-out terminal text after switching themes.
- **No per-theme `terminal.semantic` block.** §7 describes one and no theme has one; the
  override path is exercised by `parse_semantic_json` only. Out of scope, as stated.
- **The band is not painted under the regex fallback**, by decision rather than by omission.
  A shell with no OSC 133 therefore sees no change at all from this packet. Revisit only if
  the owner asks.
- The contrast floor is proved in Rust, not by `scripts/check-theme-contrast.py`; extending
  `SURFACES` to terminal tokens remains the owner question recorded in `IN-0044`.

## Handoff

Implemented. `US-0135` and `BUG-0073` are the remaining `IN-0044` packets and are
independent of this one.
