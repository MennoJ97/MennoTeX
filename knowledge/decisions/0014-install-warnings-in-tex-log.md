---
type: Decision
title: Install problems as warnings in TeX's own log
description: When a package that has a missing file is declined or fails to install, kpathsea's resolver queues a "Package mtx Warning" line and the engines print it to the terminal and the .log, where editors look; mtx ensure reports the reason by exit status.
tags: [decision, logging, ux, kpathsea, patch, editors]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-08T14:00:00Z }
verified:
  - { by: process:cargo-test, at: 2026-10-08T13:50:00Z }
  - { by: process:tests/run_c_tests.sh, at: 2026-10-08T13:40:00Z }
sources:
  - id: texmfmp
    resource: texlive-source @ 6a3001880, texk/web2c/lib/texmfmp.c (open_in_or_pipe, maininit)
    title: web2c TeX engine support code
    author: team:tex-live
  - id: luatex
    resource: texlive-source @ 6a3001880, texk/web2c/luatexdir/tex/texfileio.c, lua/luainit.c
    title: LuaTeX file opening and kpathsea setup
    author: team:tex-live
---

# Context

[Decision 0007](0007-install-consent.md) logs declined and failed installs to `mtx.log`
and stderr. Editors (LaTeX Workshop) parse TeX's `.log` for their problem list, and that
log only said "File `foo.sty' not found", with no hint that a package has the file.

# Decision

- **Reason by exit status:** `mtx ensure` exits 0 (found), 1 (no package has it),
  2 (error), 3 (declined: policy, `$MTX_AUTOINSTALL`, the user), 4 (the install failed:
  offline, no working mirror). Stdout stays the path protocol. `ensure.rs` returns a typed
  `NotInstalled` error only when no complete copy of the file exists.
- **Queue in kpathsea** (`mtx-ondemand.c`, patch 0001): on a failed install, or when
  `MTX_AUTOINSTALL=0` keeps mtx from running, queue
  `Package mtx Warning: package P (for F) was not installed: declined; see `mtx log'.`
  (or `could not be installed`, or `MTX_AUTOINSTALL is off`). Once per name per run, like
  the miss cache. API: `kpathsea_ondemand_problem()` pops one;
  `kpathsea_ondemand_set_printer()` registers a callback run when one is queued.
- **Print in the engines** (patch `0004-print-install-warnings.patch`): pdfTeX, XeTeX and
  the other web2c TeX engines register `mtx_print_problems` in `maininit` and also flush
  after `open_in_or_pipe`[^texmfmp]; LuaTeX registers in `init_kpse` and flushes after
  `lua_a_open_in`[^luatex]. Printing uses TeX's own `print`/`println` (LuaTeX `tprint_nl`),
  only while the selector is terminal and/or log; otherwise the message waits.
- The format `Package mtx Warning:` is what LaTeX Workshop and latexmk list as a
  package warning.

# Why printing at queue time

LaTeX checks existence with `\pdffilesize` before `\openin`, so a failed `\usepackage`
never opens a file: it stops with "File not found" (fatal in nonstopmode) before
another file open could flush the queue. The first build of this patch only flushed
after file opens and printed nothing for `\usepackage{fancyvrb}`; the callback prints
the warning right before LaTeX's error.

# Consequences

- Seen 2026-10-08 on pdfLaTeX, XeLaTeX and LuaLaTeX: the warning sits right above
  `! LaTeX Error: File `fancyvrb.sty' not found.`, for all three outcomes.
- TeX wraps the line at 79 columns like its own warnings.
- Probes of optional files whose package is declined also get a warning; that is the
  useful hint for `autoinstall no` users (which package to install).
