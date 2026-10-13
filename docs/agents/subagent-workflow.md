# Subagent roster and workflow for one intake

How a coordinator session turns an owner request into merged, verified packets, and which
subagent (model + requested effort) does each step. Written after IN-0050 (master password
vault, four high-risk packets, 2026-10-09..13), whose numbers calibrate the roster.

## 1. What IN-0050 taught

| Packet | Implement | First verify | Rework | Re-verify |
| --- | --- | --- | --- | --- |
| US-0156 vault core | opus, 30 min | opus, 16 min, **FAIL** (4 defects, 1 blocking) | 8 min | 6 min PASS |
| US-0157 lifecycle UI | opus, 50 min | opus, 17 min, PASS + 2 latent Medium | 7 min | 4 min PASS |
| US-0158 SSH passwords | opus, 48 min | opus, 22 min, PASS + 4 Low | 8 min | 5 min PASS |
| US-0159 idle lock | opus, 53 min | opus, 32 min, PASS + 1 Medium | 7 min | 10 min PASS |
| US-0159 lock-at-startup (owner re-ruling) | opus, 97 min (OOM waits) | pending | | |

Facts that drive the design below:

1. **Every first verification found real defects** (format contract, lifecycle/concurrency,
   init-order dependence, a security-boundary gap, a UX rule that deleted a secret). The
   adversarial verifier is the highest-value role and must stay at the strongest model and a
   high effort; it must never be the implementer.
2. **Rework is cheap when the implementer is resumed, not respawned**: 7-8 minutes each time,
   because the context is still loaded. Targeted re-verification by the resumed verifier costs
   4-10 minutes. Spawning fresh agents for either would have cost 20-50 minutes.
3. **Records quality was high but not self-consistent**: the verifier of US-0157 found an LLD
   formula that contradicted its own listed sequence, and two LLD sections still described a
   design choice the packet had dropped. A cheap consistency pass before implementation would
   have caught both.
4. **One owner re-ruling after shipping (Q1, lock at startup) cost a fifth implement/verify
   cycle.** The HLD had the right question, but the owner answered it from a text list. A
   scenario walkthrough in the owner's language ("you open the app: what do you see?") before
   the first packet is the fix.
5. **Machine limits shaped the workflow more than the design did**: 32 GB RAM with no page
   file (rustc crashes `0xc0000409` above 2-3 jobs), and disk that fell from 58 GB to 12 GB
   across four packets. Verifiers shared the implementer's finished worktree when disk was
   short; only one builder ran at a time; the full gate on `main` ran at three jobs.
6. **Independent repro beats reading**: the US-0156 verifier rewrote the envelope in Python
   from the LLD text and matched it byte for byte; the US-0159 verifier walked the native file
   picker under the lock and found the Medium defect that code reading had missed.

## 2. Roster

Five project subagents, defined in `.claude/agents/<name>.md` (owner simplification,
2026-10-13). "Effort" is the thinking budget in the definition's frontmatter. The coordinator
still names the agent, its model and its effort in the report of every dispatch (owner rule,
2026-09-28).

| Agent | Model | Effort | Does | Resumed for |
| --- | --- | --- | --- | --- |
| **advisor** | opus | high | research, assessment of issues and proposals, records consistency review; writes reports and intake Source sections only; no cargo | follow-up questions |
| **architect** | opus | high | Spec Intake, HLD, LLDs for the high-risk lane, DEC candidates, packet list, numbered owner questions, scenario walkthrough; no cargo | reconciling records after an owner ruling |
| **coder** | opus | high | one packet end to end: records-first commit, code, tests, in-worktree gate, GUI walk | **rework** (same worktree, cherry-picks the verifier's tests) |
| **coder-lite** | sonnet | medium | tiny-lane packets, doc reconciliation, mechanical scripted fixes; stops when a design choice appears | — |
| **ui-verifier** | opus | xhigh | independent adversarial verification of code and UI: gate rerun, attacks, independent reproduction, mutation checks, pinned tests, evidence file, PASS/FAIL | **targeted re-verify** |

Not a subagent: the coordinator (this session: classification, briefs, merges, `harness.db`,
the gate on `main`, memory) and the owner (rulings, pushes, the manual walks no agent can do:
UAC elevation, real SSH hosts, physical keys, sleep/resume, taskbar).

### Why these models

- **opus/high for advisor, architect, coder**: every brief in IN-0050 needed 10-20 documents
  plus code and judgement calls the LLD left open; a wrong design choice in a secrets feature
  costs more than the model saving.
- **opus/xhigh for ui-verifier**: its job is to find what the architect and the coder both
  missed; extra budget here is the cheapest insurance in the pipeline (a found Medium costs an
  8-minute rework, a shipped one costs a BUG packet).
- **sonnet/medium for coder-lite**: the shape of the change is already decided; it needs
  care, not depth.

## 3. Workflow

```
owner request
  │
  ▼
Coordinator: classify (tiny / normal / high-risk), check main vs origin, disk, RAM
  │
  ├─ unknown subject or an issue to assess? ──► advisor ──► (merged docs-only) ──┐
  │                                                                               ▼
  └───────────────────────────────────────────────────────────────────────► architect
                                                              │ intake + HLD (+ LLDs if high-risk)
                                                              ▼
                                                     advisor (records consistency pass)
                                                              │ findings → architect fixes (resume)
                                                              ▼
Coordinator: merge records (docs-only, no cargo gate), harness `intake` row,
             report to the owner: numbered questions + a SCENARIO WALKTHROUGH
             ("app starts → ...", "you save a password → ...") in the owner's language
  │
  ▼
Owner ruling ("as recommended" or per question)
  │
  ▼  per packet, smallest viable first, serial unless disk ≥ 20 GB per builder and RAM allows two
coder ── records-first commit → code → tests → in-worktree gate → GUI walk → report
  │
  ▼
ui-verifier (never the coder) ── own branch verify/<id> on the implementer's head
  │
  ├─ FAIL or PASS-with-defects ──► resume coder (rework on its branch, cherry-picks the
  │                                 verifier's tests) ──► resume ui-verifier (targeted re-verify)
  │                                 (repeat until PASS; two cycles were enough every time)
  ▼
Coordinator: `git merge --no-ff <verify head> -F <msg file>` into main
             → full gate on main (background, CARGO_BUILD_JOBS=3)
             → harness `story` row (+ `decision` row when a DEC was accepted)
             → delete target/debug/incremental and target/fast-dev
             → remove the finished worktrees
             → memory file update
  │
  ▼
next packet … → intake close → owner manual walks → platform proof from CI
  │
  └─ owner re-rules after shipping ──► reopen the owning US (harness `reopened`), same loop;
                                        never a new BUG for acceptance rework
```

### Rules the workflow depends on

- **Records before code**, always as a separate first commit by the implementer.
- **The verifier commits evidence on top of the implementer's head**; the implementer then
  cherry-picks or rebases onto that commit so the pinned tests travel with the fix. Flipped
  "current behaviour" tests become the proof of the fix.
- **Resume, do not respawn**: an agent can be resumed only while its worktree exists. Keep
  implementer and verifier worktrees until the packet merges.
- **Shared worktree when disk is short**: the verifier works in the implementer's finished
  worktree on its own branch; the two never run at the same time. Never point two agents at
  one `target` from two different paths (build scripts bake the path).
- **One builder per 20 GB free** with `CARGO_INCREMENTAL=0`; the coordinator checks `df`
  before every dispatch and tells the agent the stop threshold.
- **`CARGO_BUILD_JOBS`**: 3 on `main`, 2 in worktrees, 1 as a fallback; a rustc
  `0xc0000409` is memory, not code — wait and retry at fewer jobs.
- **Every brief carries the safety rules verbatim**: never kill, close or enumerate
  `oneterm.exe` by name or title (the owner works inside it); GUI walks only on the agent's
  own pid with a private `USERPROFILE`; scoped searches; no `rm -rf` without a guard path;
  no push; unique scratchpad prefixes because the scratchpad is shared.
- **The coordinator alone writes `harness.db`** (sqlite via python; no harness binary) and
  runs the gate on `main`.
- **Commit trailers** exactly as the session's attribution reminder says at the time.

## 4. Brief templates (what every brief of that role must contain)

- **advisor (research)**: the question; where to look first in the repo; which web sources count;
  "if nothing defines it, say so and list the nearest matches"; the records to write; the
  15-line executive summary format.
- **architect**: the request verbatim; the facts already verified by the coordinator; the
  mandatory reading list; the file set to write (intake, HLD, LLD per concern for high-risk);
  packet ids to use (check reservations by other intakes); the DEC candidates to propose;
  "numbered owner questions with a recommendation each".
- **advisor (records review)**: the file set; the checklist (template sections, formula vs list,
  dropped choices still described elsewhere, every Q numbered, security section, the two
  python checks); "findings only, no rewrites".
- **coder**: the ruling ("all Q as recommended" or per item); the carry-forwards from
  earlier packets' evidence; the exact gate commands for the worktree; the GUI walk steps;
  "record every choice the LLD leaves open in the packet Plan"; "what the verifier should
  attack first".
- **ui-verifier**: the coder's head and worktree; the implementer's own weakest-points
  list first; attack list by concern; "independent reproduction where possible" (a second
  implementation of a format, a GUI walk of a boundary); "add tests that pin defects; do not
  fix code"; the evidence file path and commit message; verdict format.
- **Rework (resume)**: the verifier's head to cherry-pick; each defect with file:line and the
  required fix shape; "flip the pinned tests"; the gate; the commit message.
- **Re-verify (resume)**: the rework head; "attack each fix" with the specific adversarial
  cases; the gate; the evidence section title.

## 5. Changes adopted from the IN-0050 review

1. Add a **records consistency pass** (advisor) between design and the owner ruling.
2. Add the **scenario walkthrough** to the owner report before the first packet of any
   UX-visible intake, so re-rulings happen on paper.
3. The **ui-verifier** runs at xhigh effort.
4. Keep the resume-based rework loop; it is the main reason four high-risk packets shipped
   in two working days.
5. Collapse the roster to five project subagents (`.claude/agents/`): advisor, architect,
   coder, coder-lite, ui-verifier.
