# OneTerm desktop scale targets

These targets define the supported desktop operating envelope for the current
architecture. They are capacity targets, not promises that every workload has the
same latency on every machine.

| Resource | Supported target | Benchmark representation |
|---|---:|---|
| Concurrent SSH sessions | 20 | `scripts/benchmark-scale.py` plus the backend test matrix |
| Concurrent local PTYs | 10 | `scripts/benchmark-scale.py` and `local-shell` PTY throughput example |
| Visible terminal panes | 8 | terminal-view renderer integration test and terminal diagnostics |
| Concurrent transfers | 8 | SFTP transfer tests and bounded transport tests |
| SFTP directory entries | 100,000 | immutable-entry snapshot path and SFTP idle-projection benchmark |
| Terminal grid | 240 × 80 | terminal renderer integration test and `terminal-diagnostics` p95/p99 output |
| Scrollback | 100,000 lines | terminal configuration and renderer workload fixtures |
| Shutdown completion | 2 seconds after close request | terminal shutdown integration workload |

## Running the repeatable harness

Run the smoke benchmark from the repository root:

```text
python scripts/benchmark-scale.py --iterations 5
```

The command writes a machine-specific JSON record to
`target/scale-benchmark.json`. It measures the complete deterministic test
workloads, including compilation/cache state, so records should only be compared
on the same machine and with the same build state. Use a checked-in or archived
record as a baseline when investigating a regression:

```text
python scripts/benchmark-scale.py \
  --baseline path/to/scale-benchmark.json \
  --max-regression 0.25
```

The harness currently covers terminal frame/shutdown paths, SFTP idle projection,
and schema migration. For frame-level p95/p99 measurements, build with the
`terminal-diagnostics` feature and collect the rolling diagnostics emitted by
`TerminalElement`; those values are deliberately not converted into a universal
wall-clock threshold because GPU, font, operating-system, and window-size costs
vary substantially.

The target values are intentionally conservative. Raising them requires a new
benchmark record, review of memory/queue/shutdown behavior, and updates to this
matrix rather than an unmeasured capacity change.
