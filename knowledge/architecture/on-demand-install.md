---
type: Architecture
title: On-demand installation
description: How a missing file becomes an installed package, in Phase 0 (stock hooks) and Phase 1 (kpathsea patch).
tags: [architecture, kpathsea, mtx, hooks]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-09T10:00:00Z }
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
   (or `mktextfm`). In `bin/universal-darwin/` these are symlinks to `mtx`, which
   dispatches on its program name (no shell wrapper: that cost ~10 ms per call).
   `mktextfm` falls back to TeX Live's METAFONT-based script when no package
   ships the TFM.[^kpse]
   **Phase 1 (planned):** a patch in `kpathsea_find_file_generic` calls the
   resolver on every miss, regardless of `must_exist`.
3. `mtx ensure` (`ensure.rs`) opens [files.idx](/architecture/files-idx.md) by mmap
   and looks up the candidate names (`name`, `name.tex`, …). A miss exits 1
   immediately: no SQLite, no network. LaTeX probes many files that exist nowhere.
4. On a hit it ranks entries (stable over `-dev` packages, LaTeX/generic over other
   formats, smaller archive first) and, if the file is already on disk, prints it.
5. Otherwise it refreshes the package database if the TTL expired, then runs the
   installer (`install.rs`): under `autoinstall ask`, take `tlpkg/mtx/ask.lock`, re-plan and
   ask ([decision 0007](/decisions/0007-install-consent.md)); download and verify without the install lock, then commit under
   `flock(tlpkg/mtx/lock)`, append `ls-R`, regenerate config, run `updmap-sys` if
   maps changed, and delete formats whose stamp no longer matches
   ([decision 0016](/decisions/0016-format-stamps.md)). During a compile it installs
   only missing packages: installed ones that are out of date, and the LaTeX kernel,
   are upgraded by the next `mtx prefetch` (latexmk runs it before TeX), install or
   update ([decision 0018](/decisions/0018-upgrades-around-compiles.md)).
6. It prints the absolute path on stdout. kpathsea checks it is readable and
   inserts it into its in-memory hash (`tex-make.c` `kpathsea_db_insert`).[^kpse]

# Programs

kpathsea hooks only see files TeX looks up. Commands (`biber`, `latexmk`,
`makeglossaries`, …) are covered by [command shims](/architecture/mtx-core.md): bootstrap
creates about 470 (2026-10), and the first `biber --version` installs `biber` (67.8 MiB, a
universal arm64/x86_64 binary) and runs it.

# Formats

`mktexfmt` is also a link to `mtx` (both hook modes). TeX Live's own mktexfmt copies
the finished format over the old file, which broke concurrent runs (see
[TeX Live scripts](/upstream/texlive-scripts.md)); mtx serializes builds per format
and renames the result into place.

# Protocol rules for `mtx ensure`

- stdout carries only the path; all messages go to stderr and `tlpkg/mtx/mtx.log`.
- Exit 0 means found or installed; 1 that no package has the file; 2 an error; 3 that
  the install was declined; 4 that it failed (offline, no working mirror). For 2–4
  kpathsea queues a `Package mtx Warning` line that the engines print to the terminal
  and TeX's `.log` ([decision 0014](/decisions/0014-install-warnings-in-tex-log.md)).
- `mtx ensure --font-map FONT`: pdfTeX and LuaTeX found no map entry for a font that
  mtx's table (`tlpkg/mtx/fontmaps.tsv`) says a package's map covers; install that
  package, and the engine re-reads its map ([decision 0015](/decisions/0015-font-map-on-miss.md)).
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
