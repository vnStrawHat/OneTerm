# Work: Implicit OSC 8 links are re-interned on every repaint

ID: BUG-0079
Intake: IN-0045
Created: 2026-09-25

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

- Change type: bug (shipped behavior since the `IN-0029` engine, `US-0076`)
- Risk lane: normal, with a public-contract edge: `oneterm-vt` is an external contract, and
  `HyperlinkId` identity and `Interner::hyperlinks` are visible to embedders. Start with an
  LLD note (below) before code.
- Spec Intake, when required: `IN-0045` (phase 4 measured it). The table and its ladder come
  from `IN-0029` (`low-level-design/dispatch-and-modes.md` § "The hyperlink table needs a
  ladder", `low-level-design/cell-and-style.md` § "Extras").

## Reported by

`IN-0045` phase 4, 2026-09-25 ([`research/agent-load-phase-4.md`](research/agent-load-phase-4.md)
§ 5). Measured, not estimated:

- **Headless** (`oneterm-vt` release, one terminal, a 3-line Ink-style repaint whose middle
  line holds one link without `id=`, the same URI every time):
  - every repaint adds one `HyperlinkTable` entry **and** one `ExtrasTable` entry;
  - both tables are full at 65,535 repaints; the Rust heap has grown by **21.0 MB** for that
    one terminal;
  - after that, **nothing that needs an extras entry works in that terminal**: the repainted
    link, a new link with an explicit `id=`, and a Sixel image all render with no link and no
    image (0 linked cells, 0 image cells; the same Sixel in a fresh terminal gets its cells).
    Only `RIS` clears the hyperlink table, and nothing clears the extras table.
- **Live** (probe build, two tabs, 30 repaints a second each, 5 minutes, maximized
  1920x1032): 4 small allocations per repaint, the allocator's live bytes +7.4 MB and commit
  +6.5 MB in 5 minutes. At that rate each tab fills both tables in 36 minutes.

Who hits it: any program that repaints a line holding an OSC 8 link without `id=`. `claude`
does this when it believes the terminal supports links (with an inherited `WT_SESSION`; see
`research/agent-load-phase-3.md` § 7). Any SSH peer can do it on purpose.

## Outcome

Repainting the same link does not consume table entries without bound: after 70,000 repaints
of one implicit link the hyperlink and extras tables hold a small, constant number of entries,
the link still resolves, and a later explicit link or image in the same terminal still works.

## Scope

- [x] In scope: `oneterm-vt` `HyperlinkTable` / `ExtrasTable` behaviour for links without
  `id=`; its rustdoc; the IN-0029 LLD rows that describe the ladder; `docs/terminal-backend.md`
  if it gains a hyperlink section. Added by the coordinator: `RIS` releases the extras table,
  and a full extras table does not silently break Sixel placement.
- [x] Out of scope: terminal-view hover and Ctrl+click behaviour; the style table; the
  grapheme arena (already swept).

## Acceptance

- [x] A failing `oneterm-vt` test first: repaint one implicit link 70,000 times; assert
  `hyperlinks.len()` and `extras.entries()` stay below a small constant, the last repaint's
  cells resolve to the link, and a following explicit `id=` link and a Sixel image still get
  their cells. `terminal::tests::repainting_an_implicit_link_does_not_grow_the_tables`; with
  the implicit lookup disabled it fails at `hyperlinks.len()`: `left: 65535, right: 1`.
- [x] Two different implicit links with the same URI in separate runs that are adjacent on
  screen stay distinguishable where the spec needs it (hover underlines one run, not both), or
  the LLD note records why merging them is acceptable. Recorded in
  [`low-level-design/osc8-interning.md`](low-level-design/osc8-interning.md): hover groups a
  contiguous same-id run within one row, so only touching occurrences merge, and they open the
  same URI; `id=` keeps them apart.
- [x] `vt-public-api.py --check` is unchanged, or the change is recorded in
  `crates/vt/CHANGELOG.md` with its semver level. Surface unchanged; the behaviour change is a
  "Fixed" entry (no signature, patch level).
- [x] Headless re-measure: live-byte growth for 70,000 repaints is under 1 MB (0.00 MB).

## Documentation

### Owning Docs Reviewed

- `docs/terminal-backend.md` — names OSC 8 hyperlinks (Ctrl+click) only; no table bound or
  identity rule is described there.
- `crates/vt/src/intern.rs` rustdoc (`HYPERLINK_TABLE_LIMIT`, `HyperlinkTable`, `Interner`) —
  states that an implicit link gets a fresh id on every occurrence and that the table is
  bounded at 65,535; silent on the extras table filling in step.
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/dispatch-and-modes.md` § "The
  hyperlink table needs a ladder" and `cell-and-style.md` § "Extras" — the ladder, and
  "one extras entry per hyperlink", which is the collateral path measured here.

### Documentation Action

Update required: the `intern.rs` rustdoc (the identity rule for implicit links and what bounds
the tables), the IN-0029 LLD ladder rows, and a short hyperlink paragraph in
`docs/terminal-backend.md` (the bound, what a full table does, how it recovers). Write
`low-level-design/osc8-interning.md` under `IN-0045` first, choosing between:

1. **Reuse by URI while open.** An implicit link reopened with the same URI while the
   previous implicit entry for that URI is the most recent one reuses it. Cheapest; merges two
   genuinely separate occurrences of one URI into one hover group.
2. **Sweep unreferenced ids**, the way the grapheme arena is swept (remap live cells, drop the
   rest). Keeps per-occurrence identity; costs a scan of the grid and history on the sweep, and
   `HyperlinkId` / `ExtrasId` values then change across a sweep (an embedder-visible change).

Reason: the current docs describe the per-occurrence rule as intended and do not mention that
repainting exhausts both tables, or that a full extras table also disables images.

### Reconciliation

Done:

- [`low-level-design/osc8-interning.md`](low-level-design/osc8-interning.md) (new): identity
  key, rejected option B, `RIS`, full extras table and images.
- `crates/vt/src/intern.rs` rustdoc: `HYPERLINK_TABLE_LIMIT`, `HyperlinkTable::intern` (the
  identity rule), `InternTable` (the extras table is emptied by `RIS`).
- IN-0029 `dispatch-and-modes.md` § "The hyperlink table needs a ladder": step 1 row and an
  amendment paragraph.
- `docs/terminal-backend.md` § 13 Risks: one row on the per-terminal table bounds.
- `crates/vt/docs/guide/10-limits.md`: the identity rule and `RIS`.
- `crates/vt/CHANGELOG.md` `[Unreleased]` "Fixed": three entries.
- Parity gate: new deviation `D17` (IN-0029 `dispatch-and-modes.md` deviations table,
  `corpus::KNOWN_DEVIATIONS`; first declared as `C16`, renumbered after verification F4
  because the reference is not wrong and `C16` was already the grapheme cap) and `crates/tools/corpus/alacritty-ref/hyperlinks/expected-diffs.json`
  declaring the one measured difference: the second implicit link to `https://example.com`
  (row 29, cols 0-2) now shares the first one's id (`#0`, reference `#1`).
- Reviewed, no change: IN-0029 `high-level-design.md` line "one entry per image plus one per
  hyperlink" stays true (one per distinct link); `cell-and-style.md` § "Extras" (R-21 is
  unchanged); `crates/vt/README.md` (does not describe the tables).

## Context

- `crates/vt/src/terminal/dispatch.rs` `set_hyperlink` interns the link, then interns an
  `Extras { hyperlink, graphic }` value; a new `HyperlinkId` is always a new extras value.
- The extras table fills one entry before the hyperlink table (id 0 is the default), so from
  the 65,535th repaint on even a successfully interned link is dropped at the extras step.

## Plan

- [x] LLD note with the choice above; owner review, because `HyperlinkId` identity is
  embedder-visible. The coordinator ruled option A.
- [x] Failing test, then the fix, then the headless re-measure. The live re-measure was not
  run (see Evidence and Gaps).

## Decisions

Option A (coordinator ruling, 2026-09-25): an implicit link is keyed by its URI; ids are never
renumbered, so `HyperlinkId` stability is unchanged and no DEC record is needed. Option B
(sweep) is rejected in the LLD note.

## Verification Plan

- Focused: the new `oneterm-vt` test above.
- Unit: `cargo test -p oneterm-vt`, `--features vt-paranoid`, `--no-default-features`.
- Integration: `measure.ps1 -Mode S7 -MimicArgs --link-repaint -AllocLog` on a probe build for
  5 minutes (as in phase 4 § 5): live-byte growth near zero.
- Public API: `python scripts/vt-public-api.py --check --no-doc`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Independent verification: **PASS**, findings F1-F9, in
[`evidence/BUG-0079-verify.md`](evidence/BUG-0079-verify.md). F3 (empty `id=` counts as no
id), F4 (the corpus difference renumbered `C16` to deviation `D17`), F8 (LLD wording) and F9
(this section) are addressed in the follow-up commit.

Headless, release profile, counting allocator, one 158 x 40 terminal fed the
`tui-mimic.py --link-repaint` frame (from the verify doc; the implementer's own run matched):

| Repaints | Before: hyperlink / extras entries | Before: live MB | After: hyperlink / extras entries | After: live MB |
| --- | --- | --- | --- | --- |
| 5,000 | 5,000 / 5,001 | 1.63 | 1 / 2 | 0.01 |
| 30,000 | 30,000 / 30,001 | 9.99 | 1 / 2 | 0.01 |
| 65,534 | 65,534 / 65,535 | 20.56 | 1 / 2 | 0.01 |
| 70,000 | 65,535 / 65,535 | 20.56 | 1 / 2 | 0.01 |

After the 70,000 repaints:

| Check | Before | After |
| --- | --- | --- |
| Linked cells on the repainted row | 0 | 4 |
| An explicit `id=x` link printed next | 0 cells | 2 cells |
| A Sixel image placed next | 0 cells | 1 cell |

Gate: `pwsh scripts/ci-local.ps1` at `d993d89d`, final line `ci-local: all checks passed.`
(run by both the implementer and the verifier).

Gaps:

- F7: every Sixel image still takes a permanent extras entry until `RIS`, so a program that
  re-sends an image on every redraw fills the extras table after 65,534 images, and new links
  and images then lose their cells. Follow-up `BUG-0081`, to be opened by the coordinator.
- No live 5-minute two-tab `measure.ps1 -AllocLog` re-measure on the app, and no run of the
  real `claude` CLI; the headless figure covers the mechanism.
- A hostile stream can still fill the hyperlink table with distinct URIs or distinct
  explicit ids; the unchanged ladder drops further links.
- Hover merges two touching same-URI runs on one row (F1), which the OSC 8 specification
  allows and which differs from alacritty and VTE.

## Handoff

Implemented and verified. Next: merge, and the coordinator opens `BUG-0081` for F7.
