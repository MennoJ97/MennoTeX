#!/usr/bin/env python3
"""On-the-fly install overhead from a root's mtx.log (PLAN.md §8 target:
median < 300 ms per package).

Usage: tests/measure_overhead.py <root> [<root> ...]

Counts only installs made during a compile (`mtx ensure`, which logs
"<file>: done after N ms"), not prefetch or explicit installs. For each
such call it pairs that line with the same process's "installed K
package(s) in T ms (D ms downloading B bytes)" line. "done after" runs from
the start of the ensure call (database check, consent, download, unpack,
ls-R, configuration) to the file being there; process start-up and
kpathsea's own work are not in it.
"""
import re
import statistics
import sys
from collections import defaultdict

INSTALLED = re.compile(r"installed (\d+) package\(s\) in (\d+) ms \((\d+) ms downloading (\d+) bytes\)")
DONE = re.compile(r": done after (\d+) ms$")
LINE = re.compile(r"^\d+ \[(\d+)\] (.*)$")


def calls(log_path):
    pending = defaultdict(list)  # pid -> installed tuples not yet matched
    out = []
    for line in open(log_path, encoding="utf-8", errors="replace"):
        m = LINE.match(line.rstrip("\n"))
        if not m:
            continue
        pid, msg = m.groups()
        if i := INSTALLED.search(msg):
            pending[pid].append(tuple(int(x) for x in i.groups()))
        elif d := DONE.search(msg):
            got = pending.pop(pid, [])
            if got:
                pkgs = sum(g[0] for g in got)
                out.append({
                    "ms": int(d.group(1)),
                    "packages": pkgs,
                    "download_ms": sum(g[2] for g in got),
                    "bytes": sum(g[3] for g in got),
                })
    return out


def pct(values, p):
    values = sorted(values)
    return values[min(len(values) - 1, int(round(p / 100 * (len(values) - 1))))]


def main():
    rows = []
    for root in sys.argv[1:]:
        rows += calls(f"{root}/tlpkg/mtx/mtx.log")
    if not rows:
        sys.exit("no on-demand installs in the logs")
    per_pkg = [r["ms"] / r["packages"] for r in rows]
    total_ms = sum(r["ms"] for r in rows)
    total_pkgs = sum(r["packages"] for r in rows)
    print(f"on-demand installs: {len(rows)} calls, {total_pkgs} packages, "
          f"{sum(r['bytes'] for r in rows) / 1048576:.1f} MiB downloaded")
    print(f"per call:    median {statistics.median(r['ms'] for r in rows):.0f} ms, "
          f"p90 {pct([r['ms'] for r in rows], 90):.0f} ms, max {max(r['ms'] for r in rows)} ms")
    print(f"per package: median {statistics.median(per_pkg):.0f} ms, p90 {pct(per_pkg, 90):.0f} ms, "
          f"mean {total_ms / total_pkgs:.0f} ms")
    print(f"downloading: {100 * sum(r['download_ms'] for r in rows) / total_ms:.0f}% of the time")
    print(f"total:       {total_ms / 1000:.1f} s")


if __name__ == "__main__":
    main()
