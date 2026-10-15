# DEC-0022 OSC 7501 replaces OSC 20308 and the Agent Panel

Date: 2026-10-15

## Status

Accepted (owner ruling, 2026-10-15; intake `IN-0049`)

## Context

OneTerm shipped its own agent channel, OSC 20308 (`docs/archive/osc-agent-status.md`), and a
right-dock Agent Panel fed by it. No third-party agent ever emitted 20308. The cross-vendor
Program Status Protocol, OSC 7501, has real emitters (Claude Code 2.1.295, pi 1.1.0) and covers
the one thing users looked at the panel for: which terminal is working, waiting on them, done
or failed.

## Decision

- OneTerm consumes program status through **OSC 7501 only**, in the adapter
  (`oneterm-terminal`), with no `oneterm-vt` API for it.
- OSC 20308 and its `OSC 9;7` alias are withdrawn: no route, no parser, no reply. The Agent
  Panel, `AgentRegistry` and the right dock's "Agent" mode are removed.
- Program status is shown **in the Terminal Tab title** (an icon per state) and nowhere else;
  remote text (`msg`, `title`) is validated and never displayed.
- A saved `right_dock_mode: "agent"` reads as `none`. New right-dock modes must not reuse the
  string `agent`.

## Alternatives

- [x] Selected approach described above.
- [ ] Keep 20308 and the panel, add 7501 cards beside them (the intake's original
  recommendation): two channels and a dashboard nobody fed; rejected by the owner.
- [ ] Parse 7501 in the engine as a built-in event: public API for a three-day-old rev 0.3
  spec; revisit when the spec is stable or an engine embedder asks.

## Consequences

- [ ] Benefit to confirm: Claude Code and pi report status in OneTerm with no setup, locally
  and over SSH (`US-0192` E2E).
- [ ] Tradeoff: per-agent detail (model, tools, files, approvals) is gone. A future need for it
  should extend 7501 upstream, not revive 20308.
- [ ] Tradeoff: `RIS` does not clear 7501 records (deviation from rev 0.3, `IN-0049` HLD).
