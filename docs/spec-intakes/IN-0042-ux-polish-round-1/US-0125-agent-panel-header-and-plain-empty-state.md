# Work: The Agent panel has a header and plain-language empty state

ID: US-0125
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

- Change type: existing-contract change
- Risk lane: normal
- Spec Intake, when required: `IN-0042` — `docs/spec-intakes/IN-0042-ux-polish-round-1/IN-0042.md`

## Outcome

The Agent mode of the right dock looks like the SSH Client mode: it has a header naming what
it shows, and its empty state explains itself in words a user who has never heard of OSC 20308
can act on.

## Findings and proposals covered

`P25` (effort S–M) — *"Give the Agent panel a header consistent with the SSH Client sections
and plain-language empty-state copy."*

Addresses `F28` (low), quoted from `research/ux-walkthrough-2026-09-16.md`:

> | F28 | Agent panel | The Agent panel has **no header**, while SSH Client mode shows "Session"
> and "SFTP Browser" headers. Its empty-state hint names the raw protocol ("Agents that emit
> OSC 20308 appear here") with no link. | Consistency + jargon. The empty state is otherwise
> well done. | low | 22 |

The walkthrough also lists the Agent panel's empty state under "what already works well":
*"icon, headline, and a one-line explanation of what would appear — better than most empty
states in the app."* So this packet changes the **words** and adds a header; it does not
redesign a state the walkthrough praised.

## Scope

- [ ] In scope:
  - `crates/agent-ui/src/view.rs` — the panel's header and its empty-state copy.
  - `crates/workspace/src/layout/workspace/dock_skin.rs` — `OneTermDockSkin`, which currently
    suppresses the outer tab bar for the single `agent_panel` leaf, and is where the header
    treatment for the two modes is decided.
  - Whether the copy points somewhere (the OSC proposal, or a shorter in-app explanation).
- [ ] Out of scope:
  - The agent list itself, its rows, its staleness threshold and its focus behaviour.
  - The OSC 20308 protocol, its parsing and its routing (`docs/osc-agent-status.md`,
    `DEC-0017`).
  - The right dock's mode toggle (`BUG-0067`) and its width (`US-0113`).
  - The SSH Client mode's headers, which are the model being matched, not the thing changed.

## Acceptance

- [ ] In Agent mode the right dock shows a header naming the panel, styled like the "Session"
      and "SFTP Browser" headers in SSH Client mode.
- [ ] The header does not reintroduce the outer tab bar that `OneTermDockSkin` suppresses for
      the single-leaf case — the two modes must look like siblings, not like one panel and one
      tab group.
- [ ] The empty state names no raw protocol identifier. A reader who has never seen "OSC 20308"
      understands what would appear here and roughly what makes it appear.
- [ ] The empty state keeps its icon and its headline — the parts the walkthrough praised.
- [ ] With an agent actually reporting, the panel renders its list exactly as before, and the
      header does not steal a row from it.
- [ ] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

## Documentation

### Owning Docs Reviewed

- `docs/agent-panel-display.md` §1.1 — the panel's composition and its states. `P25` names it
  as the owning section. **Update required:** the header, and the empty-state copy if the
  document quotes it.
- `docs/agent-panel-display.md` §1–3 — read for what the panel is for and what its states mean,
  so the new copy is accurate rather than merely friendlier.
- `docs/gui-layout.md` §Dock composition — *"`SshClientPanel` owns a vertical `v_resizable`
  split containing `SessionPanel` and `SftpPanel`, each with its own header… The right dock
  still has a tab-group node internally, but `OneTermDockSkin` suppresses that outer tab bar for
  the single `ssh_client_panel` or `agent_panel` leaf."* This is the exact seam: the SSH Client
  panel supplies its own section headers, and the Agent panel supplies none.
  **Update required:** that the Agent panel now has one too.
- `docs/osc-agent-status.md` — the OSC 20308 proposal, read so the plain-language copy does not
  misdescribe what triggers an entry. **No change** — the protocol is not this packet's
  business.
- `docs/PROJECT.md` — read for standing invariants. **No change.**

### Documentation Action

Update required: `docs/agent-panel-display.md` §1.1, and `docs/gui-layout.md` §Dock composition.

Reason: the panel's composition is documented in both, and one of them explicitly contrasts the
two right-dock modes' headers.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

## Context

- The asymmetry is structural, not cosmetic: `SshClientPanel` is a composite that draws its own
  section headers for the two panels inside it, while `AgentListView` is a single leaf with
  nothing above it. So "add a header" means deciding where it lives — in `agent-ui`'s own view,
  or in the dock skin that already treats the two leaves alike. Prefer the panel's own view if
  it can produce a header that matches; touching `dock_skin.rs` affects both modes.
- The copy: *"Agents that emit OSC 20308 appear here"* is accurate and useless to the user who
  needs it. The replacement has to say what an agent is in this context (a coding agent running
  in one of your terminals) and what makes it show up, without promising a mechanism the user
  cannot act on. `docs/osc-agent-status.md` is the source of truth for the second half — read it
  before writing the sentence, so the copy is not merely vaguer.
- `P25` mentions "no link". A link out of the application to a protocol proposal is not
  obviously useful to the person reading the empty state; a sentence they can act on is. If a
  link is added, it should go somewhere that helps a user, not a specification. Decide and
  record.
- Ladder: a header and a sentence. No new component, no new state, no change to the list.
- `research/before/22-agent-panel.png` is the before picture. `01-first-launch.png` shows the
  SSH Client mode's headers, which are the model.

## Plan

- [ ] Read `docs/osc-agent-status.md` enough to write an accurate sentence.
- [ ] Decide where the header lives; prefer `agent-ui`'s own view.
- [ ] Write the copy; check it against what actually makes an entry appear.
- [ ] Update the two docs.
- [ ] Re-capture the scene, empty and with an agent present.

## Decisions

None.

## Verification Plan

1. **Focused:** `cargo test -p oneterm-agent-ui` — the crate's existing tests must still pass;
   the header and the copy are element properties and are not queryable, so there is no
   meaningful new unit test here. Say that in Evidence rather than listing a test that proves
   nothing.
2. **Unit:** `cargo test -p oneterm-agent-ui`, `cargo test -p oneterm-workspace`.
3. **Integration:** `cargo test --workspace`.
4. **Platform:** `pwsh scripts/ci-local.ps1`.
5. **E2E (GUI walk, re-capture these scenes):**
   - `22-agent-panel.png` — Agent mode, empty, with the header and the new copy. This is the
     acceptance frame.
   - `01-first-launch.png` — SSH Client mode in the same session, so the two headers can be
     compared side by side in the report.
   Plus, not in the walkthrough: the Agent panel with at least one agent reporting, to prove
   the header did not displace a row. Producing one needs a process emitting the OSC sequence;
   the repository's own tooling or a hand-emitted escape from a local shell is enough. If no
   agent can be produced, capture the empty state only and record the gap — do not claim the
   populated case works.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Risks

- **Breaking the suppressed tab bar.** `OneTermDockSkin` deliberately hides the outer tab bar
  for both single-leaf modes. A header added at the wrong level could bring it back, or produce
  a header plus a tab bar. Compare the two modes in one session.
- **Vaguer, not clearer.** Removing "OSC 20308" and replacing it with "agents appear here" is a
  regression disguised as plain language. The copy must still tell the user what causes an
  entry.
- **Copy that is wrong.** The empty state describes a mechanism; if the new sentence
  misdescribes what triggers an entry, it is worse than jargon. Check it against
  `docs/osc-agent-status.md`.
- **Stealing a row.** The panel is in a narrow dock. A header that pushes the list down costs a
  visible agent. Check with a populated list, or record that it was not checked.

## Evidence and Gaps

After implementation, record commands, results, and anything skipped, unavailable, partial, or failing.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
