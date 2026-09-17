# Work: The Agent panel has a header and plain-language empty state

ID: US-0125
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

- [x] In Agent mode the right dock shows a header naming the panel, styled like the "Session"
      and "SFTP Browser" headers in SSH Client mode.
- [x] The header does not reintroduce the outer tab bar that `OneTermDockSkin` suppresses for
      the single-leaf case — the two modes must look like siblings, not like one panel and one
      tab group.
- [x] The empty state does not require the reader to know a raw protocol identifier: the
      headline and the sentence below it name none, and a reader who has never seen "OSC 20308"
      understands what would appear here and roughly what makes it appear. **Amended during
      implementation** (owner instruction, 2026-09-17): one short mention of the OSC 20308
      proposal is kept as a dimmed footnote, for the curious. A unit test holds the identifier
      to that footnote.
- [x] The empty state keeps its icon and its headline — the parts the walkthrough praised.
- [x] With an agent actually reporting, the panel renders its list exactly as before, and the
      header does not steal a row from it.
- [x] `pwsh scripts/ci-local.ps1` ends with "ci-local: all checks passed".

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

Changed:

- `docs/agent-panel-display.md` §1.1 — the header is drawn in every state, in the same shape as
  `SshClientPanel::render_header`, and the dock skin still suppresses the outer tab bar.
- `docs/agent-panel-display.md` §4 — the empty state's current composition and its exact copy.
- `docs/agent-panel-display.md` §8 — the header title is plain text; the bot icon is now only in
  the empty state.
- `docs/gui-layout.md` §Dock composition — the sentence contrasting the two right-dock modes now
  says the Agent panel supplies its own header too.

Unchanged, and why: `docs/osc-agent-status.md` (the protocol is untouched — the copy was checked
against §1 and §4.2.1 before it was written), `docs/PROJECT.md` (no invariant moves),
`crates/workspace/src/layout/workspace/dock_skin.rs` (`owns_header` already covers
`panel_names::AGENT`; nothing to change).

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

Three, all local to this packet — none of them sets a rule future work inherits, so no `DEC`:

1. **The header lives in `agent-ui`'s own view**, as the packet preferred.
   `dock_skin.rs` needs no change at all: `OneTermDockSkin::owns_header` already treats
   `panel_names::AGENT` like `SSH_CLIENT` and suppresses the outer tab bar for both, and
   `AgentListView` already drew a header — for the populated state only. The whole fix is that
   the empty state draws it too.
2. **The title bar matches `SshClientPanel::render_header` exactly**: `h_8`, `tab_bar`
   background, one bottom border, plain `text_sm` title, and a framed trailing control group for
   `Clear ended (n)`. The bot icon and the bold weight the title used to carry are gone — they
   were what made the two modes look unrelated. The large bot icon stays in the empty state.
3. **No link out of the application.** `P25` noted the empty state has "no link"; a link to a
   protocol proposal helps nobody who is looking at an empty panel. The footnote names the
   sequence so a curious reader can search for it, and that is all.

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
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
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

### What changed

`crates/agent-ui/src/view.rs` only — 1 file, +106/-61 including the test. `render_header` was
split into `render_title_bar` (the section header) and the filter-chip row; the empty state now
renders the title bar above its centred block. `HEADER_TITLE` and an `EMPTY_STATE` constant hold
the copy.

### Focused

- `cargo test -p oneterm-agent-ui` — 6 passed, including the new
  `view::tests::empty_state_copy_keeps_the_protocol_in_the_footnote`: the headline and the body
  contain neither `OSC` nor `20308`, the footnote contains `OSC 20308` exactly once, the body
  names the terminal and says the agent shows up by itself, and the header title is `Agents`.
  The packet's Verification Plan predicted no meaningful unit test here; there is one, because
  the copy was made a constant. The header's *styling* is still element properties and is proven
  by the frames below, not by a test.
- `cargo test --workspace` — green (the full list is in the US-0124 evidence; both packets were
  verified in the same run).

### E2E (GUI walk)

Built `cargo build -p oneterm-app --profile fast-dev`, driven by the walkthrough's `gui.ps1`
(PrintWindow + posted `WM_*`, own pid only), 1600x1000 unless stated:

| Frame | Shows |
|---|---|
| `evidence/US-0125-22-agent-panel.png` | Agent mode, empty. The `Agents` header, then the icon, `No agents are running`, the plain-language sentence and the one dimmed footnote naming OSC 20308. |
| `evidence/US-0125-01-first-launch-ssh-client.png` | SSH Client mode in the same session: the `Session` and `SFTP Browser` headers the new one matches. |
| `evidence/US-0125-22b-agent-panel-900.png` | The same empty state in a 900 px window (a ~317 px dock): the copy wraps, nothing clips. |
| `evidence/US-0125-22c-agent-panel-populated.png` | Two agents reporting (OSC 20308 emitted from a local shell): header, filter chips, the tab group and **both** cards visible — the header took no row from the list. |

Producing the populated case needed the payload to be base64-encoded (spec §3); the first
attempt sent raw JSON and the app logged `parse_agent_status returned None`, which is the parser
behaving correctly.

### Platform

`pwsh scripts/ci-local.ps1` — `ci-local: all checks passed`.

### Gaps

- The header's appearance is proven by screenshots, not by an assertion: gpui element properties
  are not queryable from a test. Anyone changing `render_title_bar` has to re-take
  `US-0125-22-agent-panel.png` beside `US-0125-01-first-launch-ssh-client.png`.
- One acceptance clause was amended during implementation (see Acceptance): the empty state keeps
  a single mention of OSC 20308 in a dimmed footnote, on the owner's instruction, rather than
  naming no protocol identifier at all.
- The populated frame was produced by hand-emitting the sequence, not by a real agent.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
