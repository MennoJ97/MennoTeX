---
type: Decision
title: Shape of the kpathsea on-demand patch
description: The resolver is one C file included by tex-make.c, called from kpathsea_find_file_generic on every miss, and hands installation to mtx.
tags: [decision, phase-1, kpathsea, c]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-07T12:30:00Z }
sources:
  - id: kpse
    resource: /upstream/kpathsea.md
---

# Decision

- The resolver lives in `kpathsea-ondemand/mtx-ondemand.c`. The build script
  copies it into `texk/kpathsea/`, and the patch makes `tex-make.c` `#include` it.
  Adding a real source file would mean changing `Makefile.am` and regenerating
  `Makefile.in` with automake, which the build machine does not need otherwise.
- `kpathsea-ondemand/patches/0001-kpathsea-ondemand.patch` touches five existing
  files (31 changed lines):
  - `tex-file.c`: call `kpathsea_ondemand_find` when nothing was found and `all` is false, **before** the mktex block and regardless of `must_exist`[^kpse]
  - `db.c`/`db.h`: expose the static `match()` as `kpathsea_path_elt_match`
  - `tex-make.h`/`tex-make.c`: declaration and `#include "mtx-ondemand.c"`
- The resolver picks the file with kpathsea's own search path (`FMT_INFO.path`,
  `kpathsea_path_element`, `match`), ranks by path-element order, then stable over
  `-dev`, then smaller archive. It then runs
  `$SELFAUTOLOC/mtx ensure --package P --path REL --siblings -- NAME` and inserts
  every printed path into the in-memory `ls-R` hash.
- Formats that never trigger installs: `cnf`, `ls-R`, `fmt`/`base`/`mem`, `gf`/`pk`/glyph, pool files.
- `MTX_AUTOINSTALL=0` (environment or `texmf.cnf`, per program possible) disables
  installs; mtx sets it for tools it runs while holding its lock.

# Reasons

- One choke point covers every engine and tool, including Lua's `kpse` library.
- Misses cost a binary search in an mmap'd file; no process is spawned.
- Printing sibling files fixes the Phase 0 LuaLaTeX failure (Lua modules of a just-installed package).

[^kpse]: kpathsea lookup and mktex hook behaviour
