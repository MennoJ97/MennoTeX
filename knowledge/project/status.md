---
type: Status
title: Roadmap status
description: Which phases of PLAN.md are done, in progress, or open, with measured results.
tags: [roadmap, status]
status: stable
stale_after: 2026-11-07T00:00:00Z
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-07T13:20:00Z }
verified:
  - { by: process:cargo-test, at: 2026-10-07T09:30:00Z }
---

# Phase 0: spike on unmodified binaries (in progress)

| Item | State |
|---|---|
| tlpdb parser, depend closure | done, tested |
| `files.idx` writer/reader | done, tested ([format](/architecture/files-idx.md)) |
| Signature verification of the tlpdb | done, tested against a real tlnet signature |
| Mirror pinning, freshness TTL, offline marker, bad-mirror avoidance | done |
| Installer (download → lock → unpack → ls-R → config regen) | done |
| tlmgr-compatible `fmtutil.cnf` / `updmap.cfg` / `language.*` | done, tested |
| `mtx bootstrap` against live tlnet | works: 112 packages, 4,096 files, ~36 s |
| `pdflatex` hello world | works: format built on demand, real Type 1 CM fonts embedded |
| `mktextex` on-demand install | works: `atveryend`, `atbegshi` installed during the `pdflatex.fmt` build |
| Mirror failover on network/TLS errors | works live (expired certificate on `mirror.lyrahosting.com`) |
| Exit criterion: article with amsmath, tcolorbox, siunitx from an empty tree | **met** for pdfLaTeX and XeLaTeX (first run); LuaLaTeX needs one rerun |
| Hooks as multi-call symlinks to mtx | done: miss 5.9 ms (was 16.9 ms with a shell wrapper) |

# Measurements

- Bootstrap (2026-10-07): 112 packages / 29.4 MiB planned; root is 197 MiB on disk,
  mostly universal (arm64 + x86_64) engine binaries.
- `pdflatex hello.tex` from a fresh bootstrap: 11.2 s first run (format build + 2 installs), 0.12 s warm.
- The first mirror chosen by `mirror.ctan.org` served a corrupt
  `amsfonts.tar.xz` (4 bytes too long); see [tlnet](/upstream/tlnet.md).

## Test document `tests/documents/article-tcolorbox.tex` (fresh root each, 2026-10-07)

| Engine | First invocation | Packages installed on demand | Warm run |
|---|---|---|---|
| pdflatex | ok, 30 s for two runs incl. format build | 43 (21 requested + 22 declared dependencies) | 0.1–1 s |
| xelatex (after `mtx install xetex`) | ok, 17 s | 39 | 3 s |
| lualatex | **fails once**: `module 'pdftexcmds' not found`; second invocation ok | 9 + 32 | 3–4 s |

The LuaLaTeX failure is the Phase 0 gap predicted in [decision 0002](/decisions/0002-phase0-stock-hooks.md):
a package installed for its `.sty` also ships a `.lua` file, but the running
process's kpathsea only learned the one returned path, and Lua `require`
lookups use `must_exist=false`, so no hook fires.

Hook cost: a miss (file in no package, e.g. `hyperref.cfg`) costs 5.9 ms after warm-up,
close to the 3.5 ms floor of spawning any process; a warm pdfLaTeX run of the
test document still makes 18 such calls.

# Phase 1: kpathsea patch and own arm64 build (in progress)

| Item | State |
|---|---|
| Resolver `mtx-ondemand.c` + patch | written, compiles with `-Wall -Wextra`; not yet run ([decision 0003](/decisions/0003-kpathsea-patch-shape.md)) |
| `mtx ensure --package/--path --siblings` | done |
| `mtx install-binaries`, `mennotex-binaries` protection, kpathsea hook mode | done |
| `build/build-texlive.sh` | first build failed on C23 (libgd); fixed with `ac_cv_prog_cc_c23=no`, rebuilding ([build notes](/upstream/texlive-build.md)) |
| Corpus of 50 documents, first-run success for all three engines | not started |

# Phase 2 items done early

- Install journal and recovery of interrupted installs (tested).
- `mtx update [--dry-run]`: upgrades outdated packages, keeping each package's reason.
- `mtx doctor` (Phase 3 item): detects the MiKTeX symlinks in `/usr/local/bin` that shadow MennoTeX on this Mac.

# Phases 2–4

Otherwise not started. See `PLAN.md` §7.
