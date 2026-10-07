---
type: Status
title: Roadmap status
description: Which phases of PLAN.md are done, in progress, or open, with measured results.
tags: [roadmap, status]
status: stable
stale_after: 2026-11-07T00:00:00Z
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-07T09:40:00Z }
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
| `pdflatex` hello world | blocked: `mktexfmt` refuses user mode with our path setup (being fixed) |
| `mktextex` on-demand install from a real document | not yet tested |

# Measurements

- Bootstrap (2026-10-07): 112 packages / 29.4 MiB planned; root is 197 MiB on disk,
  mostly universal (arm64 + x86_64) engine binaries.
- The first mirror chosen by `mirror.ctan.org` served a corrupt
  `amsfonts.tar.xz` (4 bytes too long); see [tlnet](/upstream/tlnet.md).

# Phases 1–4

Not started. See `PLAN.md` §7.
