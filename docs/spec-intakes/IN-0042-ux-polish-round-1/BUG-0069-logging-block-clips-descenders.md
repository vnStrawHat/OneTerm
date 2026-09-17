# Work: The Logging block no longer clips descenders

ID: BUG-0069
Intake: IN-0042
Created: 2026-09-17

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

## Classification

- Change type: bug
- Risk lane: normal
- Spec Intake, when required: `IN-0042` — `docs/spec-intakes/IN-0042-ux-polish-round-1/IN-0042.md`

## Outcome

Text in the session dialog's Logging block renders whole. "Logging" keeps the tail of its `g`
and "Use global" keeps both of its descenders, like every other label in the same dialog.

## Findings and proposals covered

`P11` (descender half) — *"Fix the clipped descenders in the Logging block…"*. The
key-binding half of `P11` belongs to `US-0121`.

Addresses `F19` (medium), quoted from `research/ux-walkthrough-2026-09-16.md`:

> | F19 | New/Edit SSH Session dialog | In the **Logging** block — the last row — descenders
> are clipped flat: "Loggin**g**" and "Use glo**b**a**l**" lose the tails of their `g`s.
> Adjacent labels ("Jump host", "Group") render their `p`s intact, so it is specific to that
> block. | Legibility / polish, in a dialog the user meets on every session edit.
> `crates/session-ui/src/session_dialog.rs:396-419`. | medium | 11, 12 |

## Scope

- [ ] In scope:
  - `crates/session-ui/src/session_dialog.rs:396-419` — the Logging `labelled_field` and its
    three radio rows.
  - Whatever constrains their height: the row's own height, the radio group's, or the dialog
    body's bottom edge. The root cause decides where the fix goes.
  - Any sibling that shares the same cause, if the cause turns out not to be local to this
    block.
- [ ] Out of scope:
  - The dialog's width, its field order, and the Advanced disclosure (`US-0120`).
  - `FormDialog`'s scroll (`US-0120`), unless the investigation shows the clipping is caused by
    the body's fixed height — in which case say so here and let `US-0120` own the fix, because
    two packets must not change the same constraint in opposite directions.
  - Font selection, font fallback and ligatures (`IN-0027`, shipped).
  - The terminal's own glyph rendering, which is a different engine entirely.

## Acceptance

- [ ] In the New SSH Session dialog, "Logging" and "Use global" render with their descenders
      intact, at the default UI font size.
- [ ] The same holds at the largest UI font size the Appearance page offers — the fix must be
      a layout fix, not a value tuned for one font size.
- [ ] No other label in the dialog regresses, and the dialog's overall height does not grow
      noticeably.
- [ ] The root cause is named in Evidence: which constraint clipped the text, and why it
      applied to this block and not to "Jump host" or "Group" two rows above.
- [ ] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

## Documentation

### Owning Docs Reviewed

- `docs/ssh-client-connect.md` §4 — the session dialog's fields and layout. Read to confirm the
  Logging block is described and that nothing there specifies a height. **No change expected**
  — this is a rendering defect, not a contract change. Record the confirmation.
- `docs/terminal-logging.md` and
  `docs/decisions/DEC-0003-define-terminal-logging-capture-and-override-semantics.md` — the
  Logging control's *meaning*, read to confirm this packet changes only how it is drawn.
  **No change.**
- `docs/spec-intakes/IN-0004-terminal-tab-title-descender-clipping/` — a previous descender
  clipping bug in this repository, on the tab title. Read it first: it may name the same cause
  and the same fix, in which case this packet is ten minutes rather than an investigation.
  **No change**, but cite what it said in Context.
- `crates/state/src/form_dialog.rs` — `labelled_field`, the shared row builder every crate's
  forms go through. If the cause is there, the fix is there and it is not local to this dialog.
- `docs/PROJECT.md` — read for standing invariants. **No change.**

### Documentation Action

No contract change expected: the reviewed documents describe what the Logging control means
and what the dialog contains, and neither is wrong. The defect is in layout.

Reason: nothing documented changes; a rendering bug is fixed. If the investigation shows the
cause is in `labelled_field` and the fix changes every form row in the application, revisit
this — that would be a shared-component change worth a sentence somewhere.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

## Context

- The finding's own diagnosis is the most useful clue: *"Adjacent labels ("Jump host",
  "Group") render their `p`s intact, so it is specific to that block."* Two rows above, in the
  same dialog, with the same `labelled_field` helper, the descenders are fine. So the cause is
  something the Logging row has and the others do not — its position as the **last** row, its
  radio children, or a height constraint applied to the `h_flex` that holds the three radios
  (`session_dialog.rs:398-418`).
- Being the last row makes the body's bottom edge a strong suspect, and that is the same edge
  `US-0120` moves when it makes the body scroll. Check `US-0120`'s status: if the scroll lands
  first, re-capture before investigating — the bug may be gone, and if it is, this packet
  closes as "fixed by `US-0120`" with the frame to prove it rather than with a redundant fix.
  If this packet lands first, note the interaction so `US-0120` re-checks.
- `IN-0004` already fixed a descender clip on the tab title in this repository. Whatever it
  found — a line height, a `text_sm` interacting with a fixed row height, an `overflow_hidden`
  — is the first hypothesis here.
- Ladder: this is a two-line fix once the cause is known, and an afternoon of guessing if it is
  not. The reading order above is the whole plan.
- `research/before/11-session-property-dialog.png` and `12-session-dialog-privatekey.png` are
  the before pictures; both show the Logging block at the bottom of the dialog.

## Plan

- [ ] Read `IN-0004`'s finding. Try its cause first.
- [ ] Check `US-0120`'s status and re-capture if it has landed.
- [ ] Find the constraint; fix it at its source, not by padding the label.
- [ ] Capture at the default and the largest UI font size.

## Decisions

None.

## Verification Plan

1. **Focused:** none available at the unit level — glyph clipping is a rendered-pixel property
   and the gpui element tree is not queryable in tests. Say this plainly in Evidence rather
   than listing a test that does not prove the behaviour. The proof is the captured frame at
   two font sizes.
2. **Unit:** `cargo test -p oneterm-session-ui` — a regression check that the dialog still
   builds and its existing form tests pass.
3. **Integration:** `cargo test --workspace`.
4. **Platform:** `pwsh scripts/ci-local.ps1`.
5. **E2E (GUI walk, re-capture these scenes):**
   - `11-session-property-dialog.png` — the dialog opened from a saved session's Properties.
   - `12-session-dialog-privatekey.png` — the taller Private Key variant, where the block sits
     lowest.
   - `12b-session-dialog-agent.png` — the agent-auth variant, for completeness.
   Capture each at the default UI font size and once at the largest the Appearance page offers.
   Zoom the Logging block in the after frames — a descender is a few pixels, and a full-window
   screenshot does not prove it either way. The `US-0110` walk used a 3x zoomed crop for
   exactly this reason; do the same.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Risks

- **Fixing the symptom.** Adding bottom padding to the Logging label hides the clip at one font
  size and leaves it at another, and leaves every sibling with the same latent constraint. The
  acceptance requires the cause to be named.
- **Fixing it in the shared helper without checking the callers.** If the cause is in
  `labelled_field`, the fix changes every form in the application. That may be correct — but
  it needs the other dialogs captured too, not just this one.
- **Colliding with `US-0120`.** Both packets touch the bottom edge of the same dialog body.
  Sequence them, and re-capture whichever lands second.
- **Claiming it fixed from a full-window screenshot.** A few clipped pixels are invisible at
  window scale. Zoomed crops, or the claim is unverified.

## Evidence and Gaps

### The root cause, named

`Checkbox` and `Radio` in `gpui-component` wrap a `.label(...)` in
`div().line_height(relative(1.))` — `checkbox.rs:329` and `radio.rs:245` in
`gpui-component-0.6.0`. A line box exactly as tall as the font is shorter than the
font's ascent plus descent (about 1.35 em for the UI fonts here), so the tail of a
`g`, a `p` or a `y` falls outside its own text line and is never drawn. That line
height is hard-coded inside the control and cannot be overridden from the outside —
`Radio::label` takes a `Text`, not an element.

`IN-0004`'s cause (a label-level `overflow_hidden` content mask) was tried first and
is **not** this: no element in the dialog's chain sets an overflow, and a rectangular
content mask would clip only the bottom-most line, not one line in the middle of the
form.

### The finding's diagnosis was wrong, and it matters

`F19` says *"Adjacent labels ('Jump host', 'Group') render their `p`s intact, so it is
specific to that block."* They do not differ, and it is not specific to that block —
the difference is the **control**, not the position. Measured on
`research/before/11-session-property-dialog.png` as the number of pixel rows a glyph
occupies below the baseline (a full descender at this size is 3 rows):

| Text | Control | Rows below baseline, before |
|---|---|---|
| "Jump host" | `labelled_field` label | 3 — whole |
| "Group" | `labelled_field` label | 3 — whole |
| "Logging" | `labelled_field` label | 3 — **whole; it was never clipped** |
| "Select or type group…" | combobox placeholder | 3 — whole |
| status bar path, session subtitles | plain labels | 3 — whole |
| **"Use global"** | `Radio::label` | **1 — clipped** |
| **"Forward the SSH agent…"** | `Checkbox::label` | **1 — clipped** |

So the block the finding named contains exactly one broken label, and the label the
finding thought was broken ("Logging") never was. Reading it as "the Logging block"
would have produced a fix in the wrong place; reading it as "the kit's checkbox and
radio labels" also fixes the `y` of "Private Key" in the Authentication row, which the
walkthrough never noticed, and every other checkbox and radio in these dialogs.

### The fix

`oneterm_state::form_dialog::control_label` — the label text as a **child** of the
control, in a line box of `CONTROL_LABEL_LINE_HEIGHT` (1.5) times the font size, with
`accessibility_label` carrying the screen-reader name that `.label(...)` used to
provide. It is relative, not an absolute pixel value, so it holds at any UI font size.

Applied at every checkbox and radio in `crates/session-ui`:
`session_dialog.rs` (the three Logging radios, the agent-forwarding checkbox),
`auth_form.rs` (the Password / Private Key / SSH Agent radio group, shared by all
three dialogs), `connect_dialog.rs` ("Save username to session" — a clipped `y`) and
`quick_connect_dialog.rs` ("Save to SSH Sessions" — no descender today, latent
tomorrow).

### Commands and measurements

Same measurement on the captured after frames:

| Text | Rows below baseline, after, 16 px UI font | after, 20 px UI font |
|---|---|---|
| "Use global" | 4 | 5 |
| "Forward the SSH agent…" | 4 | 5 |
| "Private Key" | 4 | — |
| "Logging" (unchanged control) | 3 | — |

- `cargo clippy -p oneterm-session-ui -p oneterm-state --all-targets -- -D warnings` — clean.
- `cargo test --workspace` — passed.
- `pwsh scripts/ci-local.ps1` — "ci-local: all checks passed".

### Evidence frames

- `evidence/BUG-0069-11-session-property-dialog.png` — the dialog, after.
- `evidence/BUG-0069-11b-logging-block-3x.png` — the Logging block at 3x.
- `evidence/BUG-0069-11c-before-above-after.png` — before above after, 3x, the same
  crop. The flat `g` of "agent" and of "Use global" in the top half have their tails in
  the bottom half.
- `evidence/BUG-0069-12-session-dialog-privatekey.png` — the Private Key variant.
- `evidence/BUG-0069-11d-largest-ui-font.png` — the same dialog at a 20 px UI font.

### Documentation

No contract change, as planned. `docs/ssh-client-connect.md` §4 describes the dialog's
fields and specifies no height; `docs/terminal-logging.md` and `DEC-0003` describe what
the Logging control means, which is untouched; `docs/PROJECT.md` carries no invariant
about text rendering. The one sentence worth keeping lives in the rustdoc on
`control_label`, next to the code that must not be undone.

### Gaps

- **No unit-level proof, as the packet predicted.** Glyph paint is not queryable from
  the gpui element tree. The proof is the pixel-row measurement above, taken from the
  captured frames; it is quantitative rather than a visual claim.
- **The Appearance page offers no UI font size.** It offers theme mode and the theme
  list only (`crates/settings-ui/src/appearance.rs`), so the acceptance's "largest UI
  font size the Appearance page offers" has no control behind it. The check was made
  instead by setting `ui_font_size` to 20 in `target/ui_config.json` — the same value
  the page would persist — and re-capturing.
- **Two crates, not three, still have a `.label(...)` checkbox or radio.** The original entry
  named settings-ui, sftp-ui and terminal-view; `crates/settings-ui` and `crates/terminal-view`
  contain no `Checkbox`, `Radio`, `RadioGroup` or `Switch` at all. The only remaining sites are
  `crates/sftp-ui/src/render.rs:276` and `crates/sftp-ui/src/edit.rs:588`. `control_label` is in
  the shared `form_dialog` module and is ready for them; fixing them belongs to `US-0124` and to
  `US-0126`'s sweep.
- **`Button::label` needs nothing.** The claim that Browse, Add and Cancel were "latent" clips
  was wrong in the same way `B69-MAJOR-1` was: a button label has no vertical clip around it.
  Nothing to do, now or later.
- **`CONTROL_LABEL_LINE_HEIGHT = 1.5` is pinned by nothing.** No test and no check catches a
  change back to 1.0; the value's justification lives only in its rustdoc.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.

## Rework after independent verification

Verdict **PASS with findings**. The outcome held and the root cause was proven, but the
verifier found the same cause unfixed on a sibling control and two wrong crate names in Gaps.

- **B69-MAJOR-1 was a false finding, and the correction matters more than the finding did.**
  It reported that `Button::label` clips descenders like `Checkbox` and `Radio` do, and that
  the Connect button's "Connectin**g**" had lost its tail. A second verifier **measured** the
  frames rather than reading them, and the button has the same 4 descender rows in all three
  captures — `research/before/15-connect-inflight.png` (pre-`IN-0042`),
  `evidence/US-0118-15-connect-inflight.png` (`f4ea1765`, still `.label(...)`) and
  `evidence/BUG-0069-rw-15-connecting-button.png` (with the child label). The first verifier's
  own 6x crop shows 4 rows too. **`Button::label` never clipped anything.**

  I took the finding on trust and repeated its "zero rows before" in this packet without
  measuring the before-frame myself, which is the same mistake `BUG-0069` exists to correct:
  `F19` was also a confident diagnosis that the pixels did not support.

  **The mechanism, corrected.** The line height alone is not the cause. `Checkbox` and `Radio`
  put their label inside `v_flex().flex_1().overflow_hidden()` whose height is that one-em line
  box (`checkbox.rs:315-318`, `radio.rs:236-245`) — it is the **clip** that removes the tail.
  `Button` sets the same `line_height(relative(1.))` on its label
  (`button.rs:679-687`) but hangs it in an `h_flex().size_full()` sized to the whole button with
  nothing clipping it vertically, so the glyph paints outside its own box unharmed. The
  rustdoc on `control_label` now says this.

  **The remedy is reverted.** The three button labels converted to `control_label` children
  (`common.rs`'s Connect, `group_combo.rs`'s `Create "<typed text>"`, `FormDialog`'s confirm)
  are back on `.label(...)`. The conversion fixed nothing and cost something: `.label(...)`
  wraps the text in `min_w_0 / whitespace_nowrap / text_ellipsis`, so an over-long label ends
  in an ellipsis; a plain child in the button's `overflow_hidden` content row is hard-clipped
  at the button edge instead (`R-m4`). The footer's label is arbitrary user text, so that is
  the site where it would have shown.

  The original outcome is untouched: the checkbox and radio labels in these dialogs still
  render their descenders whole, re-measured by both verifiers on
  `evidence/BUG-0069-11c-before-above-after.png`.
- **B69-m2.** Gaps corrected: see above.
- **B69-m3.** Recorded in Gaps rather than fixed — a test that pins a line height needs the
  laid-out glyph box, which is the same thing the packet already records as unavailable.

## Acceptance rework 2026-09-17 — the label sits level with its indicator

The owner tried the built round and reported: in the SSH Session dialogs the radio buttons'
selected/unselected icon and their text are **not vertically aligned — the icon sits higher than
the text**. The descenders are whole, which is this packet's outcome, but the fix that made them
whole moved the text down.

This is acceptance rework of this packet, not a new `BUG`: the misalignment is a property of
`control_label`, which this packet introduced, and it was reported while trying the same round's
build.

### Root cause: the extra leading is what saves the glyph, and what drops the text

The mechanism this packet's first rework corrected was still one layer short. **gpui clips every
text line to its own line box**, whatever the containing elements do:
`gpui-pre-0.3.3/src/text_system/line.rs:344-364`, where `paint_line` opens
`window.paint_layer(line_bounds, …)` with `line_bounds` exactly `line_height` tall, and positions
the baseline at `padding_top + ascent` with `padding_top = (line_height - ascent - descent) / 2`;
`paint_layer` pushes that as a scene layer at `gpui-pre-0.3.3/src/window.rs:4134-4143`. (The
workspace's `gpui` is the **`gpui-pre`** package — `Cargo.toml:40`,
`gpui = { package = "gpui-pre", version = "0.3" }` — resolved to 0.3.3; there is no `gpui`
package in `Cargo.lock`. The first draft of this section cited `gpui-0.2.2`, which the project
does not build: right conclusion, wrong pointer, corrected here and in the rustdoc.)
A `relative(1.)` line box on a font
whose ascent plus descent is about 1.2 em therefore has a **negative** `padding_top`: the glyph
box hangs out of the layer by about 0.1 em at each end and the descender is cut. That is the clip
— the `overflow_hidden` on `Checkbox`'s label slot (`checkbox.rs:315-318`) sits on top of it but
is not what removed the tail, which is why the `Radio` clipped too although its own slot
(`radio.rs:235-254`) has no `overflow_hidden` at all.

So `CONTROL_LABEL_LINE_HEIGHT = 1.5` was the right lever and still is — it is the only lever;
bottom padding cannot help, because the clip is the line box and not the element's box.

The same formula is the defect. Raising the line box from 1.0 to 1.5 em adds 0.25 em of leading
**above** the text as well as below, and the controls lay their row out with
`h_flex().items_start()` (`radio.rs:195-199`, `checkbox.rs:255-257`), pinning the top of that
taller box to the top of the 1 rem indicator. The text therefore starts a quarter of an em lower
than the kit intends — 4 px at a 16 px UI font — and the indicator reads as sitting high.

### The fix, and why this one

`.items_center()` on the `Radio` and the `Checkbox`. Both implement `Styled`
(`radio.rs:136-140`, `checkbox.rs:119-123`) and both apply `.refine_style(&self.style)` **after**
their own `.items_start()` — in `radio.rs` the `h_flex()` chain runs `.items_start()` at `:198`
and `.refine_style(&self.style)` at `:211`; in `checkbox.rs`, `:257` and `:271` — so a call-site
refinement wins with no fork and no patch. The row
then centres the 1.5 em label box against the indicator, which restores exactly the optical
relationship the kit's own `.label(…)` path has — with `items_start` and a 1 em line box the
label box and the indicator are both 1 em tall, so the kit is already centring them; it just
never had to say so.

`control_label` itself is unchanged. The line box stays at 1.5 em, so the descender proof of the
original packet still holds byte for byte, and the change cannot regress it.

Rejected, with the reason:

- **Bottom padding on the label instead of the leading.** Cannot work: `paint_layer` clips at the
  line box, so padding grows the element and not the clip. This was the first candidate and the
  gpui source ruled it out before it was written.
- **A negative top margin on the label**, cancelling the added leading so the line box lands where
  a 1 em box would. Single-point in `control_label` and needs no call-site change, but it puts the
  label's box above its parent's — which for `Checkbox` is the slot that *does* clip — to buy an
  alignment `items_center` gives with no overhang.
- **Wrapping the whole control in a `form_dialog` helper** (`control_radio` / `control_checkbox`)
  so the invariant cannot be forgotten. Two new public functions and a rewrite of five call sites
  to remove a one-word repetition; recorded in Gaps as the upgrade path instead.

### What changed

| Where | Change |
|---|---|
| `crates/session-ui/src/auth_form.rs` | The Authentication radio group's three radios (Password / Private Key / SSH Agent). |
| `crates/session-ui/src/session_dialog.rs` | `logging_radio` (Use global / Enabled / Disabled) and the Agent-forwarding checkbox. |
| `crates/session-ui/src/connect_dialog.rs` | "Save username to session". |
| `crates/session-ui/src/quick_connect_dialog.rs` | "Save to SSH Sessions". |
| `crates/state/src/form_dialog.rs` | `control_label`'s rustdoc: the corrected mechanism (the clip is gpui's line box, not the slot's `overflow_hidden`) and the rule that the control's row must be `items_center` when it carries this child. |

The `Advanced` disclosure (`session_dialog.rs`) also passes `control_label`, into a `Button`
whose content row is already centred; it needs nothing and got nothing.

### Measurements

Pixel rows read off the captured frames at a 16 px UI font (the walk's default), by scanning each
row of a narrow column band for ink against the dialog background. "Indicator centre" is the
middle of the control's own 16 px box. **"Cap centre" is halfway between the top of the capitals
and the baseline — the text's optical centre, and the basis the acceptance is read on.**

**The cap top is taken from an unambiguous all-caps run**, not from the whole label. The first
draft of this table scanned each label's full width and so caught an ascender or a `t`-bar one
row above the capitals — "Private" tops out on its `t`/`i`-dot, "Use global" on its `l`/`b`. That
made every number 0.5 px optimistic and the acceptance line read "within 1 px", which does not
hold literally. The independent verification (`evidence/acceptance-rework-2-verify.md`, B69-m2)
re-measured against the all-caps runs and got 1.5 px; its bands are pixel-identical to the
committed frames, so this is a reading correction, not a different result. The table below is the
corrected reading.

| Row | Control | Indicator rows (centre) | Cap top (run) | Baseline | Cap centre | Indicator low by | Descender rows |
|---|---|---|---|---|---|---|---|
| Authentication: Private Key | `Radio` | 520-535 (527.5) | 524 (`P`, `SSH`) | 535 | 529.0 | **1.5 px** | 4 (`y`) |
| Logging: Use global | `Radio` | 690-705 (697.5) | 694 (`U`) | 705 | 699.0 | **1.5 px** | 4 (`g`) |
| Forward the SSH agent… | `Checkbox` | 663-678 (670.5) | 667 (`SSH`) | 678 | 672.0 | **1.5 px** | 4 (`g`) |
| **Before**, `evidence/after/13-new-ssh-session-dialog.png` | `Radio` | 463-478 (470.5) | 471 (`SSH`) | 482 | 476.0 | **5.5 px** | — |

Password and SSH Agent share the Authentication row, so they share its numbers; the crop shows
all three.

**The improvement is exactly 4.0 px — 5.5 px low becomes 1.5 px low.** 4.0 px is 0.25 em at a
16 px font, which is precisely the half of the extra leading that `items_start` was putting above
the text: the fix takes back what the explanation says it should, with nothing left over.

**The residual 1.5 px is inherent to centring a line box, not drift.** The box's own centre sits
`(ascent - descent) / 2` above the baseline while the cap centre sits `cap / 2` above it, and at
this font and size the difference is 1.5 px. Closing it would mean offsetting the label against
every other label in the application. The acceptance below is therefore stated as the 4.0 px
improvement and a 1.5 px residual, not as "within 1 px".

**The descender is whole in every row** — 4 rows below the baseline, the same count this packet
recorded after its first fix, so the original outcome is unchanged by the rework.

**The x-height centre is further down still** (the x-height here is 7 px against an 11 px cap),
and deliberately so — see Gaps.

### Frames

- `evidence/BUG-0069-rw2-before-above-after-4x.png` — **the rework in one image.** The same
  Authentication row at 4x, before above after. In the top half the indicators sit visibly above
  the text; in the bottom half they are level, and the `y` of "Key" keeps its tail in both.
- `evidence/BUG-0069-rw2-session-dialog.png` — the New SSH Session dialog, with the
  Authentication and Logging rows.
- `evidence/BUG-0069-rw2-session-dialog-advanced.png` — the same dialog with Advanced expanded,
  for the agent-forwarding checkbox.
- `evidence/BUG-0069-rw2-auth-radios-4x.png` — the Authentication row at 4x, all three radios.
- `evidence/BUG-0069-rw2-radio-row-4x.png` — the Private Key radio alone at 4x, the crop the
  first measurement row is taken from.
- `evidence/BUG-0069-rw2-logging-4x.png` — Logging "Use global" at 4x.
- `evidence/BUG-0069-rw2-agent-forwarding-4x.png` — the agent-forwarding checkbox at 4x.

### Acceptance, reworked

- [x] The descender stays whole: 4 rows below the baseline on every radio and checkbox in these
      dialogs, unchanged from this packet's first fix.
- [x] The indicator and the text read as level. Measured on the cap-height (optical) centre:
      **4.0 px of improvement, from 5.5 px low to 1.5 px low**, on all three kinds of control.
- [ ] ~~The icon's vertical centre is within 1 px of the text's centre.~~ **Not met as written,
      and not achievable by centring**: 1.5 px on the cap-height centre, more on the x-height
      centre. Both residuals are what line-box centring gives and what every other label in the
      application shows; the reasoning and the alternative are in Gaps. Stated here rather than
      graded green, because the first reading of these frames made it look met.
- [x] `control_label`'s line box is unchanged, so the original descender proof still holds.
- [x] `cargo test -p oneterm-session-ui -p oneterm-state` green; `pwsh scripts/ci-local.ps1` ends
      with "ci-local: all checks passed".

### Gaps carried

- **The rule rests on five call sites.** `control_label` cannot centre the row it is a grandchild
  of, so `items_center` has to be written at each control. No test catches a sixth control added
  without it — the same class of gap as `CONTROL_LABEL_LINE_HEIGHT` being pinned by nothing, and
  with the same cause: laid-out glyph boxes are not queryable from the element tree. The upgrade
  path is a `control_radio` / `control_checkbox` pair in `form_dialog` that owns both the child
  and the alignment.
- **The alignment is not exact, and cannot be made exact by centring: 1.5 px of cap-centre
  residual remains.** Centring a line box puts the box's own centre — `(ascent - descent) / 2`
  above the baseline — on the indicator's centre, and the text's optical centre is `cap / 2`
  above the baseline, 1.5 px lower at this font and size. The x-height centre is lower again,
  because the x-height here is 7 px against an 11 px cap. Both are where the kit's own labels
  sit and what every other checkbox in the application looks like; pulling the text up to close
  either would raise its capitals above the indicator and make these five controls the odd ones
  out. What the rework buys is the 4.0 px the tall line box had added on top of that.
- **The first reading of the measurements was 0.5 px optimistic** (B69-m2): scanning a whole
  label for its topmost ink catches an ascender or a `t`-bar, not the capitals. The numbers are
  corrected above and stated against an all-caps run. The lesson is the same one this packet
  exists for — measure the thing the claim is about.
- **`crates/sftp-ui`'s two `.label(…)` sites are still unfixed**, as the original Gaps record.
  They clip, and they are also not centred; nothing here changes them.
