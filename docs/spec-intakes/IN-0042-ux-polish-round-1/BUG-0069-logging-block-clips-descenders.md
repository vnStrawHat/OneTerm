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
- [ ] Reopened (acceptance rework)
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
