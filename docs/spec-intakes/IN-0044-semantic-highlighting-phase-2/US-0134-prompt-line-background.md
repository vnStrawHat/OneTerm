# Work: The prompt line gets its background — WITHDRAWN

ID: US-0134
Intake: IN-0044
Created: 2026-09-22

> **Owner ruling, 2026-09-22 (acceptance rework #2): remove the prompt-line background
> entirely.** The band was built, verified twice and accepted on paper; the owner then ran
> the build and decided he does not want a band under the prompt line at all. This packet is
> **Reopened** and its outcome is now the removal. Everything below the line is the original
> record, kept as history — read it as "what was built and then taken out", not as current
> behaviour. The current state is in **Acceptance rework #2** near the end of this file.

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
- [ ] Changed
- [x] Reopened (acceptance rework)
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

**Current (after the owner's 2026-09-22 ruling).** There is no prompt-line background. The
band and everything that existed only to serve it are removed from the code, the tests and
the design docs; the row roles, the regex fallback and the exit-code tint — which are
`US-0133` and do not depend on the band — stay exactly as they are.

**Original (built, then withdrawn).** `ClassStyles::prompt_line_bg` is painted: a background
band across the full width of every row of a prompt line, wrapped or not, in a colour that is
correct on a light theme and on a dark one, under any ANSI or selection background rather
than over it.

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

**Superseded 2026-09-22 by the owner's ruling.** The criteria below were all met by the band
that was built; they are void because the band is gone. The criteria that now decide this
packet are:

- [x] No prompt row carries a background band, on any theme, marked or not.
- [x] `push_prompt_band`, `TerminalTheme::prompt_line_bg`, `PROMPT_BAND_MIX`, the
      `promptLineBg` parsing, the `ClassStyles::prompt_line_bg` field and
      `oneterm_theme::embedded_theme_files()` are gone; no dead code and no dead key is left
      behind.
- [x] `resolve_style` is back to measuring contrast against the cell's own background: with
      no band, `row_bg` and that background are the same colour, so the extra reference had
      nothing left to say.
- [x] What is kept still works: row roles and the transition rule, the regex fallback, the
      exit-code tint. Their tests are untouched and green.
- [x] `docs/terminal-semantic-highlighting.md` §7, §8 item 6, §9, §11 and §13 Q4 say the band
      is **withdrawn by owner decision**, not deferred to a later phase; §13 Q4 records that
      the `LEADING_WIDE_CHAR_SPACER` hole returns to its pre-`US-0134` state.
- [x] `cargo test -p oneterm-terminal-view -p oneterm-theme -p oneterm-highlight`,
      `cargo clippy --workspace --all-targets -- -D warnings` and `pwsh scripts/ci-local.ps1`
      all pass.

<details><summary>Original acceptance (void — the band it describes no longer exists)</summary>

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

</details>

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

> Superseded by **Acceptance rework #2** below: those sections were rewritten again when the
> owner withdrew the band. The paragraph that follows records the band's own doc pass.

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

`crates/terminal-view/src/theme/tests.rs`: `the_band_sits_between_the_background_and_the_text`
(and `assert_band_between`, reused by the per-theme sweep),
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

### Acceptance rework after independent verification (2026-09-22)

**ACCEPT WITH CHANGES.** The band itself was found correct and well tested; two defects:

- **MAJ-3 — the central design choice was unasserted.** Inverting the band's direction
  (`toward` flipped) left all 389 tests green. `the_band_never_crosses_its_theme` asserted
  "nearer the background than the foreground", which an *inverted* band satisfies more
  comfortably than the intended one. Replaced by
  `the_band_sits_between_the_background_and_the_text`, which asserts three separate
  properties — the band moved **toward** the text (`signum` of the move equals `signum` of
  the gap), it did not reach or pass it, and it cleared a floor — over synthetic pairs and,
  through `assert_band_between`, over every embedded theme variant. The inversion now fails
  two tests.
- **MED-2 — "never crosses its theme" was false on a low-contrast pair.** With `bg.l = 0.5`
  and `fg.l = 0.55` the band landed at `0.555`, past the text. The move is now capped at
  half the gap to the foreground, so the band is always strictly between the two; the
  probes from the verification (`0.5/0.55`, `0.5/0.45`, `0.5/0.52`) are in the test.

The band also inherited `US-0133`'s MAJ-1 — it was painted on every output row of an A-only
session. That is fixed in `US-0133` by the transition rule and asserted here by
`an_a_only_shell_paints_no_band`.

`NIT-2` (the explicit `promptLineBg` override ignores `reverse_video`) is left as it is:
nothing ships an override, so it is unreachable.

### Re-verification (2026-09-22) — ACCEPT

Both of this packet's findings were confirmed closed and pinned by mutation (inverting the
band's direction now fails `the_band_sits_between_the_background_and_the_text` and
`every_prompt_row_foreground_clears_the_band`). Two follow-ups landed here:

- The band it inherited from `US-0133`'s `RV-MAJ-1` — one prompt row of a back-to-back pair
  losing its band, which is the flashing row this packet's own fallback rule exists to
  avoid — is fixed in `US-0133` and shown in `evidence/us-0133-back-to-back-prompts.png`,
  where all three adjacent prompt rows are banded.
- **RV-NIT-3**: `assert_band_between` returned early on a zero fg/bg gap, the one pair it
  most needed to judge. It now asserts `band == bg` there — the cap really did hold — and
  `0.50/0.50` is in the fixture list.

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
  A shell with no OSC 133 therefore sees no change at all from this packet — and after the
  `US-0133` rework, neither does an `A`-only one (bash, SSH). Revisit only if the owner asks.
- **A prompt at the viewport's top edge has no band**, because it is not a marked prompt
  there (`US-0133`'s transition rule and its stated limit).
- The contrast floor is proved in Rust, not by `scripts/check-theme-contrast.py`; extending
  `SURFACES` to terminal tokens remains the owner question recorded in `IN-0044`.

## Acceptance rework #2 (2026-09-22) — the owner removed the band

**Ruling.** The owner ran the build and decided against the feature itself: **remove the
prompt-line background entirely.** Not a defect in the band — both verification rounds found
it correct — a product decision that a full-width band under the prompt line is not the look
OneTerm wants. It is a **withdrawal, not a deferral**: no later phase ships it, and
`docs/terminal-semantic-highlighting.md` §11 now says so in those words, because "deferred"
would leave the next reader hunting for the packet that finishes it.

### Removed

| Thing | Where |
| --- | --- |
| `push_prompt_band()` and the `BgSpan` it pushed | `crates/terminal-view/src/render/row_plan.rs` |
| the `role: Option<RowRole>` parameter of `build_row_plan` and its caller's argument | `row_plan.rs`, `plan_cache.rs` |
| `TerminalTheme::prompt_line_bg()`, `PROMPT_BAND_MIX` | `crates/terminal-view/src/theme/terminal_theme.rs` |
| `promptLineBg` parsing, and the `ClassStyles::prompt_line_bg` field it wrote | `crates/terminal-view/src/highlight/bridge.rs`, `crates/highlight/src/theme.rs` |
| `embedded_theme_files()` and its re-export — added for the band's per-theme sweep, no other caller | `crates/theme/src/theme.rs`, `crates/theme/src/lib.rs` |
| `#[cfg(test)] pub(crate) use contrast::contrast_ratio` — same, no other caller | `crates/terminal-view/src/theme/mod.rs` |
| the 11 band tests: `the_band_sits_between_the_background_and_the_text`, `the_shipped_asset_leaves_the_band_to_the_theme`, `every_prompt_row_foreground_clears_the_band` and its `assert_band_between` / `PROMPT_ROW_CLASSES` / `class_name` helpers, `a_marked_prompt_row_carries_a_full_width_band`, `every_row_of_a_wrapped_prompt_carries_the_band`, `the_band_is_painted_under_the_per_cell_backgrounds`, `the_band_covers_a_leading_wide_char_spacer`, `an_unmarked_prompt_row_gets_no_band`, `a_marked_output_row_gets_no_band`, `every_glyph_on_a_prompt_row_clears_the_band`, `an_a_only_shell_paints_no_band`, `prompt_line_bg_is_parsed_when_a_theme_sets_it`, and the band assertion inside `default_styles_loaded` | `row_plan.rs`, `theme/tests.rs`, `highlight/bridge.rs` |

The asset key needed nothing: `crates/terminal-view/assets/highlight/default.json` has not
carried `promptLineBg` since the band was made theme-relative. With the parsing gone the key
is simply an unknown key, which `parse_semantic_json` has always ignored — the simpler of the
two options the ruling offered (ignore vs. reject), and it is recorded in §7, in the
function's own doc comment, and pinned by a one-line test (`a_prompt_line_bg_key_is_ignored`).

### Kept, and why

- **Row roles and the transition rule** (`US-0133`): they decide which rows the scanner reads
  in prompt mode and where the prompt/command boundary is. Nothing about them was the band.
- **The regex fallback**: unchanged. The "no band under the fallback" rule it motivated is
  moot, and §8 item 6 says so rather than leaving a rule about a thing that does not exist.
- **The exit-code tint on the prompt sign** (`US-0133`): a foreground class substitution, no
  background involved.
- **`resolve_style`'s contrast pass**, but **not** the `row_bg` reference the band added. The
  ruling's own test applies: `row_bg` was only ever consulted when `paint_bg` was false, and a
  cell that paints no background of its own now sits on exactly `theme.color(Color::Background)`
  — which is the `bg` local already in scope. (Under `DECSCNM` a default-background cell has
  `bg_color == Color::Foreground` after `swap_default`, so `paint_bg` is true and the branch
  is not reached at all.) The two were the same colour in every reachable case, so the
  parameter was pure ceremony and `resolve_style` is byte-for-byte its pre-`US-0134` self.
- **`RowRole` itself and `PlanCache::roles`**: still used by the scanner and by `US-0133`'s
  tests. Only the paint-time reader is gone.

### The `LEADING_WIDE_CHAR_SPACER` hole

Recorded in `docs/terminal-semantic-highlighting.md` §13 Q4, as the ruling requires. The
full-width rect did close it for prompt rows; with the band gone the note returns to exactly
its original state — a spacer carries no class, no class carries a background today, so the
hole is **invisible rather than fixed**. It becomes real the first time a class-level
background or line decoration ships (§11 phase 3), and that packet owns it. The HLD says the
same (item 7).

### Docs reconciled

- `docs/terminal-semantic-highlighting.md` §7 (the `ClassStyles` sketch, the theme JSON
  sample and the `promptLineBg` paragraph), §8 item 6 (rewritten as the withdrawal, with what
  is kept and why the contrast reference went with it), §9 (the merge-policy row and the
  "prompt-line bg always paints" rule, which were false the moment the band left), §11 (a
  paragraph under the phase table: withdrawn, not deferred, and phase 3's decorations are not
  this), §13 Q4.
- `docs/spec-intakes/IN-0044-semantic-highlighting-phase-2/high-level-design.md` — the data
  flow box, the UI Wireframe (the "after" sketch no longer shows a band) and design items 7
  and 8, both marked WITHDRAWN with the removed/kept split.
- `docs/spec-intakes/IN-0044-semantic-highlighting-phase-2/IN-0044.md` — the Themes and Users
  impact bullets, and the contrast-gate Open Decision, which is still open but no longer
  blocks anything.
- No `US-0126`-style before/after report mentions this packet: the only one in the repository
  is `IN-0042`'s, and it predates `IN-0044`. Nothing to update there.
- `DEC-NNNN`: none. The ruling is a product preference about one visual element, recorded
  here and in the two design docs; there is no rule future work has to inherit beyond "there
  is no band", which §8 item 6 states.

### Verification

- `cargo test -p oneterm-terminal-view -p oneterm-theme -p oneterm-highlight` — 388 + 5 + 96
  passed, 0 failed (3 ignored, pre-existing).
- `cargo clippy --workspace --all-targets -- -D warnings` — clean. Nothing was left dangling:
  the unused `to_gpui_hsla` import in `terminal_theme.rs` and the test-only `contrast_ratio`
  re-export went with the code that used them.
- `pwsh scripts/ci-local.ps1` — see the final line quoted below. `python
  scripts/check-theme-contrast.py` is unchanged and still passes; it never saw the band.
- GUI (Windows, `fast-dev`): `evidence/us-0134-band-removed.png` — a `cmd` tab with several
  marked prompts and no band on any of them.

### Gaps after the removal

- The two band frames from the original walk
  (`evidence/us-0134-band-dark-theme.png`, `evidence/us-0134-band-light-theme.png`) are kept
  as the record of what was built and rejected. They no longer describe the product.
- The `TerminalTheme::fg`-after-a-theme-switch observation in Gaps above stands. It was never
  caused by this packet and nothing now depends on it, so it is even less urgent; still worth
  a `BUG` if the owner sees washed-out terminal text after switching themes.
- The contrast floor for terminal grid text is now proved by nothing in particular — the band
  was the only thing that measured it. That is the pre-`US-0134` state: `ensure_contrast`
  still runs per cell against the cell's own background at render time, which is what it did
  before, and the open `SURFACES` question in `IN-0044` is where the gap is recorded.

## Handoff

**Withdrawn.** `US-0135` and `BUG-0073` are the remaining `IN-0044` packets and are
independent of this one.
