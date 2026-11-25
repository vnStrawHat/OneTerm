# High-Level Design: Remediate Phase 0 and Phase 1 of the 2026-08 refresh review

Intake: IN-0010
Lane: high-risk
Date: 2026-08-17

## Idea

Apply the review's Phase 0/1 checklist as a set of small, independently verifiable changes grouped by
disjoint file ownership so they can be implemented in parallel and merged without conflicts. Each
group carries its own regression tests; the integration branch is gated by the full workspace checks
after each wave.

## Diagram

```text
review checklists (01..08) --> Phase 0 groups (A1..A6) --merge--> gate --> Phase 1 groups (B1..B6) --merge--> gate
```

## Data Flow

1. Each group edits only the crates/files assigned to it and commits on its own branch.
2. The integration branch merges the group branches; conflicts are resolved by file ownership.
3. `cargo fmt/clippy/test/build` + Python checks run on the merged tree before the next wave.
4. Review checklists are ticked and this intake's packets are reconciled at the end.
