# Harness

Harness keeps implementation and its owning documentation aligned while coding agents make small, safe, verifiable changes.

## Default Flow

```text
Classify -> Document -> Change -> Verify -> Reconcile
```

1. Classify the request as a new capability, existing-contract change, bug, maintenance, acceptance rework, or non-implementation task.
2. Read the owning docs, affected source, nearby decisions, and relevant tests.
3. Before implementation, establish the required Spec Intake, owning contract, and Markdown work packet.
4. Make the smallest change that fits the accepted contract.
5. Run focused proof and relevant regression checks.
6. Reconcile owning docs and packet evidence, then report the outcome and gaps.

A clear or detailed prompt can make the documentation concise; it does not bypass this flow.

## Route the Work

| Situation | Action |
| --- | --- |
| Read-only investigation, discussion, or a documentation-only edit with no implementation change | Work directly in the relevant docs; no implementation packet is required. |
| Any source, test, schema, script, or behavior-affecting configuration change | Before editing implementation, create or update one Markdown work packet from the editable template. |
| A new capability, new spec, spec slice, or initiative | Before implementation, create one Spec Intake, establish or update the owning contract, then create the work packet. |
| A bug or maintenance change, however small | Before implementation, create or locate its owning Intake, then review the owning docs. Update stale docs, or record reviewed paths and a no-change reason in the work packet. |
| Acceptance feedback on work that was just built but not yet accepted ("fix these points" during your own trial/UAT) | Reopen and rework the **owning US/BUG**; do not open a new BUG. Run `harness story reopen --id <US/BUG> --reason "..."`, mark the failed items in that packet's Acceptance checklist, then re-prove. A new BUG is only for defects found in behavior that was already accepted/shipped. |
| Acceptance must survive a session, work has several independent steps, or a handoff is needed | Keep the same owning work packet current; do not create a parallel status record. |
| A choice about architecture, behavior, authorization, data ownership, public contracts, or validation must guide future work | Create or update one decision record. |
| Review, release, benchmark, failure attribution, or a non-reconstructable handoff needs retained evidence | Record one evidence-focused trace. |
| Repeated harness friction is worth follow-up but out of scope | Add a harness backlog item. |

There is no undocumented implementation lane. A work packet is the durable change record, not a duplicate operational status log: `harness.db` is authoritative for status and proof state, and the CLI mirrors those fields into machine-owned task-list blocks in the packet. The packet owns outcome, documentation review, acceptance, verification plan, evidence, and gaps.

## High-Risk Triggers

Treat work as high-risk when failure could materially affect:

- authentication, authorization, tenants, roles, secrets, privacy, or audit;
- data loss, migrations, retention, ownership, or integrity;
- payments, email, queues, webhooks, provider SDKs, or other external effects;
- public APIs, client-visible contracts, or broad established behavior;
- validation or safety checks being removed or weakened.

For high-risk work, make acceptance and proof explicit. Read the relevant project contract, prior decisions, and security or regression tests. Ask before implementing when consequential direction is ambiguous. High risk does not automatically require a large packet or a trace.

## Context Rule

Load the smallest context that makes these four things clear:

- requested outcome;
- owning design seam;
- relevant constraints and invariants;
- verification path.

Start with `docs/PROJECT.md`, the owning contract named by the work packet, affected files, and adjacent patterns. Retrieve decisions, history, or bundled guides only when a task trigger makes them relevant. Record every owning doc reviewed in the packet. Stop retrieving when the four items above are clear; ask the user when the missing information is a decision only they can make.

## Coordination Rule

When work crosses a session or actor boundary, keep one owning packet current with its objective, scope, acceptance, state, evidence, open gaps, and next owner or action. A delegated result is provisional until the integrating actor verifies the parent acceptance criteria and integration.

## Completion Contract

Before reporting a change as complete, reconcile each clause:

1. **Outcome:** the requested result is implemented against the packet acceptance criteria.
2. **Proof:** focused checks support the behavior being claimed.
3. **Owning contract:** changed behavior, schema, architecture, or operator usage is reflected in its owning documentation.
4. **Documentation record:** the packet lists docs reviewed, docs changed, or an explicit no-change reason when the accepted contract was already correct.
5. **Decision:** a consequential choice future work must inherit is recorded once.
6. **Evidence and gaps:** results and anything unavailable, skipped, partial, or failing are retained in the packet or an evidence-focused trace when later review needs it.

Do not mark a work packet implemented while owning docs are stale or documentation review remains blank. If proof is unavailable, skipped, too expensive, or failing, report the behavior as unverified, partial, blocked, or failed rather than claiming completion.

## Source Ownership

| Information | Canonical source |
| --- | --- |
| Harness operating policy | `docs/HARNESS.md` |
| Project purpose, stack, boundaries, stable invariants, and verification commands | `docs/PROJECT.md` and accepted product contracts |
| Consequential rationale | Decision Markdown |
| Task outcome, scope, acceptance, owning-doc review, verification plan, evidence, and gaps | Work packet Markdown |
| Work status, proof result and evidence summary, active guardrails, intake, traces, friction, and harness backlog | `harness.db` via the Harness extension |
| Implemented behavior | Code and executable tests |

Do not maintain the same operational fact manually in both Markdown and the database.

## Editable Templates

Spec Intake, work, and decision document shapes live in:

```text
docs/templates/spec-intake.md
docs/templates/work.md
docs/templates/decision.md
```

Agents must follow the current project templates rather than an extension-internal shape. Humans may edit their sections and instructions at any time. Spec Intake generators replace `{{id}}`, `{{title}}`, `{{date}}`, `{{type}}`, `{{lane}}`, and `{{summary}}`; work and decision generators replace their applicable values including `{{status}}` and `{{intake}}`. All other text is preserved.

Generated Intakes use a gap-free document sequence independent of DB-only classifications and live at `docs/spec-intakes/IN-NNNN-slug/IN-NNNN.md`. `intake create` supports every request type so standalone bugs and maintenance can own a folder; `intake add` remains the DB-only classification path. Every `intake create` also generates the intake's one mandatory High-Level Design at `<intake-folder>/high-level-design.md` and records its path. Detail (Low-Level) Design is optional and split by concern under `<intake-folder>/low-level-design/<concern>.md` via `design lld`; it is **required for the high-risk lane**, where `story create` is blocked until at least one detail design file exists. Work IDs use `US-NNNN` or `BUG-NNNN`; `story create` requires their owning `IN-NNNN` and keeps the packet inside that Intake folder, including when `--doc` is supplied. Decision IDs use `DEC-NNNN` and remain under `docs/decisions/`. The CLI updates only marked Harness status/proof task-list blocks (or appends them to older/custom templates); authored Acceptance and Plan checklists remain human/agent-owned. Keep templates useful and concise.

## Commands

Use the `harness` agent tool or `/harness` slash command. Common operations:

```text
harness intake create|add
harness design hld|lld
harness story create|update|reopen|verify
harness decision create|add|verify
harness trace
harness query matrix|decisions|guardrails|stats
harness guide brownfield|high-risk|trace
```

Create the required documentation records before implementation. Do not use a DB-only record as a substitute for a Markdown work packet.
