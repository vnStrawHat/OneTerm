# Project Context

Keep this file short and factual. It is project memory, not a speculative architecture plan. Add only facts discovered from accepted requirements, code, configuration, tests, and explicit decisions.

## Mode

`brownfield`

For brownfield projects, existing code is authoritative where a contract has not yet been documented. Missing documentation means “inspect and preserve current behavior,” not “design freely.”

## Purpose

Describe what this repository does, for whom, and which outcomes it owns.

## Stack and Surfaces

Record confirmed languages, frameworks, storage, providers, deployment targets, and product surfaces. Do not prescribe layers or folders before the project needs them.

## Important Boundaries

List only load-bearing boundaries such as public APIs, user input, identity, data ownership, external systems, jobs, files, environment configuration, or platform shells.

## Invariants

List short project-specific rules future changes must preserve. Framework-level Harness policy belongs in `docs/HARNESS.md`.

## Verification

Record commands that actually exist:

```text
Focused:
Unit:
Integration:
End-to-end:
Release:
```

Do not invent unavailable commands. Report missing proof explicitly.

## Open Questions

List only unresolved questions that can materially change implementation direction, risk, or proof.
