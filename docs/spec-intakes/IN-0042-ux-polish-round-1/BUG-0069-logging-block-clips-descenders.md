# Work: The Logging block no longer clips descenders

ID: BUG-0069
Intake: IN-0042
Created: 2026-09-17

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
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
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

After implementation, record commands, results, and anything skipped, unavailable, partial, or failing.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
