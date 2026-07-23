#!/usr/bin/env python3
"""Run repeatable OneTerm scale smoke benchmarks and emit percentile records."""

from __future__ import annotations

import argparse
import json
import platform
import statistics
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WORKLOADS = {
    "terminal_frame": [
        "cargo",
        "test",
        "-p",
        "oneterm-terminal-view",
        "phase0_renderer_baseline_counts_dirty_and_idle_frames",
        "--",
        "--nocapture",
    ],
    "terminal_shutdown": [
        "cargo",
        "test",
        "-p",
        "oneterm-terminal-view",
        "phase1_shutdown_cancels_tasks_and_closes_session",
        "--",
        "--nocapture",
    ],
    "sftp_idle_projection": [
        "cargo",
        "test",
        "-p",
        "oneterm-sftp-ui",
        "idle_ticks_do_not_request_repeated_snapshots",
        "--",
        "--nocapture",
    ],
    "schema_migration": [
        "cargo",
        "test",
        "-p",
        "oneterm-core",
        "absent_version_migrates_sequentially_and_is_idempotent",
        "--",
        "--nocapture",
    ],
}


def percentile(samples: list[float], fraction: float) -> float:
    """Return a nearest-rank percentile for non-empty millisecond samples."""
    ordered = sorted(samples)
    rank = max(0, min(len(ordered) - 1, int(fraction * len(ordered) + 0.999999) - 1))
    return ordered[rank]


def run_workload(command: list[str], iterations: int) -> dict[str, object]:
    samples: list[float] = []
    for _ in range(iterations):
        started = time.perf_counter()
        result = subprocess.run(
            command,
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        elapsed_ms = (time.perf_counter() - started) * 1000.0
        if result.returncode != 0:
            sys.stderr.write(result.stdout)
            sys.stderr.write(result.stderr)
            raise RuntimeError(f"benchmark command failed: {' '.join(command)}")
        samples.append(round(elapsed_ms, 3))
    return {
        "command": command,
        "samples_ms": samples,
        "median_ms": round(statistics.median(samples), 3),
        "p95_ms": round(percentile(samples, 0.95), 3),
        "p99_ms": round(percentile(samples, 0.99), 3),
    }


def compare_baseline(current: dict[str, object], baseline_path: Path, max_regression: float) -> None:
    baseline = json.loads(baseline_path.read_text(encoding="utf-8"))
    failures: list[str] = []
    current_workloads = current["workloads"]
    for name, current_result in current_workloads.items():
        previous = baseline.get("workloads", {}).get(name)
        if not previous:
            continue
        previous_p95 = float(previous["p95_ms"])
        current_p95 = float(current_result["p95_ms"])
        if previous_p95 > 0 and current_p95 > previous_p95 * (1.0 + max_regression):
            failures.append(
                f"{name}: p95 {current_p95:.3f} ms exceeds baseline "
                f"{previous_p95:.3f} ms by more than {max_regression:.0%}"
            )
    if failures:
        raise RuntimeError("scale benchmark regression:\n" + "\n".join(failures))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--iterations", type=int, default=5)
    parser.add_argument("--output", type=Path, default=ROOT / "target" / "scale-benchmark.json")
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--max-regression", type=float, default=0.25)
    parser.add_argument("--list", action="store_true", help="print workload commands without running them")
    args = parser.parse_args()
    if args.iterations < 1:
        parser.error("--iterations must be at least 1")
    if args.list:
        for name, command in WORKLOADS.items():
            print(f"{name}: {' '.join(command)}")
        return 0

    record: dict[str, object] = {
        "format_version": 1,
        "environment": {
            "platform": platform.platform(),
            "python": platform.python_version(),
        },
        "iterations": args.iterations,
        "workloads": {},
    }
    for name, command in WORKLOADS.items():
        print(f"benchmarking {name} ({args.iterations} iterations)", flush=True)
        record["workloads"][name] = run_workload(command, args.iterations)

    if args.baseline:
        compare_baseline(record, args.baseline, args.max_regression)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {args.output}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except RuntimeError as error:
        print(error, file=sys.stderr)
        raise SystemExit(1) from error
