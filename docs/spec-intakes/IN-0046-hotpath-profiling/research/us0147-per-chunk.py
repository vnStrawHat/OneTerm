"""US-0147: the PTY loop's cost per ConPTY chunk from hotpath reports.

    python us0147-per-chunk.py <timing.json> [<alloc-count.json>]

A chunk is one `Pump::advance` call (one feed). For every loop, pump and conout site it
prints calls per chunk, the average call, the time per chunk (total / chunks) and, with
an allocation-count report, allocations per chunk (exclusive of nested sites). Then the
PTY owner and conout threads' CPU per chunk (hotpath thread table: average % x elapsed).
"""
import json
import sys

sys.dont_write_bytecode = True  # importing hotpath-tables.py must not leave a __pycache__

from importlib.machinery import SourceFileLoader
from pathlib import Path

value = SourceFileLoader("tables", str(Path(__file__).with_name("hotpath-tables.py"))).load_module().value

SITES = [
    "loop::poll_wait", "loop::commands", "loop::drain_pty", "pty_read", "loop::lock",
    "Pump::advance", "Terminal::feed", "Parser::advance", "loop::unlock",
    "Pump::finish_batch_blocking", "SessionEventSink::post_repaint",
    "read_pipe", "push", "Ring::wake",
]


def rows(report):
    for section, body in report.items():
        if section.startswith("functions_") and isinstance(body, dict):
            if section == "functions_timing" and "functions_alloc" in report:
                continue
            return body, {r["name"]: r for r in body["data"]}
    raise SystemExit("no functions section")


def find(table, site):
    for name, row in table.items():
        if name == site or name.endswith("::" + site) or name.endswith(site):
            return row
    return None


def main():
    timing = json.load(open(sys.argv[1], encoding="utf-8"))
    body, table = rows(timing)
    alloc = rows(json.load(open(sys.argv[2], encoding="utf-8")))[1] if len(sys.argv) > 2 else {}
    chunks = find(table, "Pump::advance")["calls"]
    elapsed_s = body["total_elapsed_ns"] / 1e9
    print(f"run {elapsed_s:.1f} s, chunks (Pump::advance calls) {chunks:,}\n")
    print("| Site | Calls | Calls/chunk | Avg | µs/chunk | Allocs/chunk |")
    print("| --- | ---: | ---: | ---: | ---: | ---: |")
    for site in SITES:
        row = find(table, site)
        if row is None:
            continue
        per_chunk = value(row["total"]) / 1e3 / chunks
        allocs = ""
        arow = find(alloc, site)
        if arow is not None and value(arow["total"]) is not None:
            allocs = f"{value(arow['total']) / chunks:.3f}"
        print(f"| `{site}` | {row['calls']:,} | {row['calls'] / chunks:.2f} | {row['avg']} "
              f"| {per_chunk:.2f} | {allocs} |")
    print()
    for thread in timing["threads"]["data"]:
        if thread["name"] in ("PTY owner", "oneterm-vt-pty-conout", "oneterm-vt-pty-conin"):
            pct = float(thread["cpu_percent_avg"].rstrip("%"))
            cpu_us = pct / 100 * elapsed_s * 1e6
            print(f"- `{thread['name']}` (tid {thread['os_tid']}): {pct:.1f} % of a core avg "
                  f"= {cpu_us / 1e6:.2f} s = {cpu_us / chunks:.2f} µs per chunk")


if __name__ == "__main__":
    main()
