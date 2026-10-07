---
type: Architecture
title: On-demand installation
description: How a missing file becomes an installed package, in Phase 0 (stock hooks) and Phase 1 (kpathsea patch).
tags: [architecture, kpathsea, mtx, hooks]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-07T10:30:00Z }
sources:
  - id: kpse
    resource: /upstream/kpathsea.md
  - id: code
    resource: https://github.com/MennoJ97/MennoTeX/tree/main/crates/mtx-core/src
    title: mtx-core sources
---

# Flow

1. A TeX program asks kpathsea for a file; the file is not in any `ls-R`.
2. **Phase 0:** if the lookup has `must_exist=true`, kpathsea runs `mktextex <name>`
   (or `mktextfm`). Our hook scripts in `bin/universal-darwin/` exec
   `mtx ensure --format tex -- <name>`.[^kpse]
   **Phase 1 (planned):** a patch in `kpathsea_find_file_generic` calls the
   resolver on every miss, regardless of `must_exist`.
3. `mtx ensure` (`ensure.rs`) opens [files.idx](/architecture/files-idx.md) by mmap
   and looks up the candidate names (`name`, `name.tex`, …). A miss exits 1
   immediately: no SQLite, no network. LaTeX probes many files that exist nowhere.
4. On a hit it ranks entries (stable over `-dev` packages, LaTeX/generic over other
   formats, smaller archive first) and, if the file is already on disk, prints it.
5. Otherwise it refreshes the package database if the TTL expired, then runs the
   installer (`install.rs`): download and verify without a lock, then commit under
   `flock(tlpkg/mtx/lock)`, append `ls-R`, regenerate config, run `updmap-sys` if
   maps changed, and delete formats whose `fmttriggers` changed.
6. It prints the absolute path on stdout. kpathsea checks it is readable and
   inserts it into its in-memory hash (`tex-make.c` `kpathsea_db_insert`).[^kpse]

# Protocol rules for `mtx ensure`

- stdout carries only the path; all messages go to stderr and `tlpkg/mtx/mtx.log`.
- Exit 0 means found or installed; exit 1 means not available (TeX then reports the missing file).
- `MTX_AUTOINSTALL=0` disables installation (lookups still succeed for installed files).
- After a network failure an `offline_until` marker makes further misses fail fast for 60 s.

# Files mtx owns

`bin/universal-darwin/{mtx,mktextex,mktextfm}` are protected: the installer never
unpacks tlnet's versions over them (`install::PROTECTED`). The root `texmf.cnf`
sets `MKTEXTEX = 1`, `TEXMFHOME = ~/Library/texmf`, `TEXMFVAR = $TEXMFROOT/texmf-user-var` and
`TEXMFCONFIG = $TEXMFROOT/texmf-user-config`. The user and system trees must differ,
or TeX Live's `mktexfmt` refuses to run (see [TeX Live scripts](/upstream/texlive-scripts.md)).
Formats still land in `texmf-var/web2c/<engine>/` because `mktexfmt` prefers a writable `TEXMFSYSVAR`.

# Related

- [mtx-core modules](/architecture/mtx-core.md)
- [Decision: Phase 0 uses stock hooks](/decisions/0002-phase0-stock-hooks.md)

[^kpse]: kpathsea lookup and mktex hook behaviour
