# Work: Before/after report of the UX round

ID: US-0126
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

- Change type: maintenance (evidence and reporting; no source change)
- Risk lane: normal
- Spec Intake, when required: `IN-0042` — `docs/spec-intakes/IN-0042-ux-polish-round-1/IN-0042.md`

## Outcome

The round closes with the report the owner asked for: the same 67 scenes captured again on the
finished build, set beside the frames in `research/before/`, with a verdict for every one of
the walkthrough's 34 findings.

## Findings and proposals covered

The owner's ruling of 2026-09-16: *handle all of S, M and L, then make a report with
before/after images.* This packet is the second half of that sentence.

It covers no finding of its own. It covers the **reporting** of F1–F34, including the ones no
packet fixed:

- `F27` (the status bar collapsing to the clock in an empty Space) and `F33` (the single app
  menu) are observations the walkthrough recorded with no proposal behind them. The report
  carries them with that verdict.
- Anything a packet left standing — the regex toggle `US-0115` declined, Duplicate and Move to
  Group that `US-0119` kept out, an upstream limit `US-0116` or `US-0122` recorded — appears as
  partially fixed or not fixed, with the packet's own recorded reason.

## Scope

- [ ] In scope:
  - Re-capturing all 67 scenes on a build of the closed round, into `evidence/after/` under the
    walkthrough's own names.
  - `evidence/before-after-report.md` — one row per finding F1–F34, with the before frame, the
    after frame, the packet that addressed it, and a verdict.
  - Collecting each packet's recorded gaps into the report, so the owner sees what was not
    fixed in the same place as what was.
  - One final `pwsh scripts/ci-local.ps1` over the closed round.
- [ ] Out of scope:
  - **Any source change.** If the walk finds a defect, it is recorded and routed — reopened on
    the owning packet if that packet has not been accepted, or raised as a new `BUG` if it has.
    This packet does not fix things; a report that quietly patches what it is measuring is not
    a report.
  - Re-capturing or editing `research/before/`. It is the frozen baseline.
  - New findings beyond F1–F34. If the walk turns up something new, it is a new packet under
    this intake or a new intake — the report names it and stops there.

## Acceptance

- [ ] `evidence/after/` holds 67 frames, named exactly as the frames in `research/before/`, each
      showing the same scene from the same state.
- [ ] Every scene the walkthrough could not reach is either reached (and said so) or recorded as
      still unreachable with the reason — no Ctrl/Shift chord and no double-click can be
      delivered by posted messages, which is a property of the capture method, not of the
      application.
- [ ] `evidence/before-after-report.md` has a row for every finding F1 through F34. No finding
      is missing and none is silently ticked.
- [ ] Each row carries: the finding id and its one-line symptom, the before frame path, the
      after frame path, the packet id, and a verdict from: fixed / partially fixed / not fixed
      (with the recorded limit) / observation, not packeted.
- [ ] Every "partially fixed" and "not fixed" row names where the reason is recorded — the
      packet's Gaps section, or the upstream follow-up raised.
- [ ] The report states the capture method, the build it was taken from, and the process-id
      discipline used, so a reader can judge the evidence.
- [ ] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed" on the closed round.
- [ ] `python scripts/check-english.py` and `python scripts/check-doc-paths.py` pass with the
      report in place.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0042-ux-polish-round-1/IN-0042.md` — the intake's packet list, which is
  the checklist this report closes. **Update required:** every candidate work packet ticked or
  explained.
- `docs/spec-intakes/IN-0042-ux-polish-round-1/high-level-design.md` §Data Flow 5 — the
  evidence plan this packet executes: `research/before/` frozen, per-packet evidence during the
  round, `evidence/after/` at the end, one report keyed by finding. **No change** unless the
  plan had to be departed from, in which case record why.
- `docs/spec-intakes/IN-0042-ux-polish-round-1/research/ux-walkthrough-2026-09-16.md` — the
  finding list. **Never edited** — it is the research record, and it stays as it was written
  even where the behaviour it describes has changed.
- Every packet in this intake — their Gaps sections are the source of the "partially" and "not
  fixed" rows. **No change** to them from here; if one is wrong, reopen it rather than
  correcting it in the report.
- `docs/HARNESS.md` §Completion Contract — clause 6, evidence and gaps. This packet is that
  clause for the round.

### Documentation Action

Update required: `IN-0042.md`'s Candidate Work Packets list, and any owning contract a late walk
shows is still stale.

Reason: the intake's checklist is the round's completion record, and it is not complete until
the report exists.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

## Context

- **The baseline is already in place.** `research/before/` holds the 67 frames from the walk of
  `main @2f12628a`, copied with this intake at its creation and never regenerated. That is what
  makes a before/after comparison possible at all — recapturing a "before" from a changed build
  would be worthless.
- **Scene numbers are the index.** The walkthrough's `NN-*.png` names are how the report,
  the packets and the owner all refer to the same view. An after frame named `24b` must show
  what `24b` showed, reached the same way.
- **Discipline the walk inherits** (from `IN-0042` and the original walkthrough's method):
  launch the packet's own build; drive **only that process id**; never enumerate, focus or
  close a window by name or title, because the owner runs their own OneTerm; capture with
  `PrintWindow(hwnd, dc, PW_RENDERFULLCONTENT)` because the desktop is locked; back up
  `target/{docks,ssh_session,terminal,ui_config,update_config}.json` before and restore after.
- **Some scenes need a server.** The SFTP and connect scenes were walked against the
  repository's loopback `sftp-dev-server`; the failure path used `10.10.10.10`. Use the same,
  and stop the server afterwards.
- **Some scenes need seeded state.** The saved-session scenes need an `ssh_session.json` with
  grouped and ungrouped entries, as the `US-0110` walk seeded. The key-binding migration scenes
  (`US-0123`) need seeded `ui_config.json` profiles. Reuse each packet's own seeding recipe
  from its Evidence rather than inventing new fixtures — a frame taken from different data is
  not comparable with the before frame.
- **The verdict column is the point.** A folder of 67 pictures is not a report. The owner asked
  for a comparison, and the value is in a reader being able to see, finding by finding, what
  changed and what did not.
- **Do not fix things during the walk.** The strongest temptation in this packet is to correct a
  small thing noticed while capturing. That produces a report describing a build nobody
  reviewed. Record, route, re-walk if needed.

## Plan

- [ ] Confirm every implementation packet is implemented and its evidence recorded.
- [ ] Build once; walk all 67 scenes from that build; nothing rebuilt mid-walk.
- [ ] Write the report from the frames and the packets' Gaps sections.
- [ ] Tick `IN-0042.md`'s packet list.
- [ ] Run `pwsh scripts/ci-local.ps1`, `check-english.py`, `check-doc-paths.py`.

## Decisions

None.

## Verification Plan

1. **Focused:** a mechanical check that `evidence/after/` and `research/before/` hold the same
   67 names, and that the report references every finding id F1–F34. Both are trivially
   scriptable and both are exactly the kind of completeness a human reviewer misses. Run it and
   record the output.
2. **Unit / Integration:** none of this packet's own — it changes no source. The round's own
   test results come from the implementation packets and are cited, not re-run per packet.
3. **Platform:** `pwsh scripts/ci-local.ps1` once, on the closed round, from the merge result
   rather than from any single packet's branch.
4. **E2E:** the walk itself is the E2E proof, and it is the whole packet. All 67 scenes, one
   build, one process id.
5. **Documentation gates:** `python scripts/check-english.py` and
   `python scripts/check-doc-paths.py`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Risks

- **Capturing from several builds.** Frames taken over a week from different branches make a
  report that describes no build that ever existed. One build, one walk.
- **Different state, incomparable frame.** A scene captured against a different session store,
  a different theme or a different window size is not a comparison. Reuse each packet's seeding
  recipe and record the window sizes.
- **Silent ticking.** The easiest way to finish this packet is to mark everything fixed. The
  acceptance requires a row per finding and a named location for every reason, precisely
  because the round's value to the owner is an honest verdict.
- **Fixing during the walk.** Covered in Context; it is out of scope and it invalidates the
  report.
- **Touching the owner's application.** The owner runs Claude inside OneTerm. A walk that
  enumerates windows by name or closes a process by image name can kill the owner's session.
  Only the walk's own pid, every time.
- **The unreachable scenes staying invisible.** The walkthrough could not exercise Ctrl/Shift
  chords or double-clicks. If the report does not say so, a reader will take those rows as
  verified. Mark them.

## Evidence and Gaps

After implementation, record commands, results, and anything skipped, unavailable, partial, or failing.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.

Blocked until: every other packet in `IN-0042` is implemented and its evidence recorded.
