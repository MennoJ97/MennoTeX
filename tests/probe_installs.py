#!/usr/bin/env python3
"""List on-demand installs whose trigger file no document read (probes).

Usage: tests/probe_installs.py ROOT LOGDIR [ROOT LOGDIR ...]

ROOT is a MennoTeX root a corpus ran in (tests/run_documents.sh), LOGDIR the
directory with that run's TeX logs ($DOCS_OUT). For every `FILE → package P`
line in ROOT's mtx.log, FILE counts as read if some TeX log shows it being
input (`(…/FILE`). kpathsea cannot tell an existence check from a load, so
files installed but never read were only probed (\\IfFileExists and friends).
See knowledge/decisions/0013-no-probe-deny-list.md.
"""
import collections
import glob
import re
import sys

# Files TeX reads with \input; their reads show in the log. Fonts and Lua
# files do not, so they are left out.
TEXT = (".sty", ".cls", ".cfg", ".def", ".clo", ".fd", ".ldf", ".tex", ".ltx", ".dfu")


def main(args):
    if len(args) < 2 or len(args) % 2:
        sys.exit(__doc__)
    found = collections.defaultdict(set)
    total = 0
    for root, logdir in zip(args[::2], args[1::2]):
        # TeX wraps log lines at 79 columns; joining them restores paths.
        logs = "".join(open(f, errors="replace").read().replace("\n", "") for f in glob.glob(f"{logdir}/*.log"))
        for line in open(f"{root}/tlpkg/mtx/mtx.log", errors="replace"):
            m = re.match(r"\d+ \[\d+\] (\S+) → package (\S+)", line)
            if not m:
                continue
            total += 1
            name, pkg = m.groups()
            if name.endswith(TEXT) and "/" + name not in logs:
                found[(name, pkg)].add(root.rstrip("/").rsplit("/", 1)[-1])
    print(f"{total} on-demand installs, {len(found)} trigger files never read:")
    for (name, pkg), roots in sorted(found.items()):
        print(f"  {name:32} {pkg:20} {', '.join(sorted(roots))}")


if __name__ == "__main__":
    main(sys.argv[1:])
