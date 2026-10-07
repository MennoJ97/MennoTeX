---
type: Decision
title: Phase 0 uses tlnet binaries and stock mktex hooks
description: Develop and test mtx against unmodified universal-darwin binaries from tlnet, hooking in through MKTEXTEX/mktextfm, before building patched binaries.
tags: [decision, phase-0]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-07T09:45:00Z }
---

# Decision

Phase 0 installs TeX Live's own `*.universal-darwin` binary packages (native
arm64 and x86_64) and enables on-demand installation through kpathsea's existing
`mktextex` (with `MKTEXTEX = 1`) and `mktextfm` hooks, which call `mtx ensure`.

# Reasons

- The whole package manager can be built and tested without compiling TeX Live.
- pdfLaTeX/XeLaTeX package loading goes through `\filesize` with `must_exist=true`, so it is covered (see [kpathsea](/upstream/kpathsea.md)).

# Confirmed in testing (2026-10-07)

pdfLaTeX and XeLaTeX compile the test document on the first run. LuaLaTeX fails
once with `module 'pdftexcmds' not found` and succeeds on the next run (see
[status](/project/status.md)).

# Known gaps (accepted for Phase 0)

- Font files, maps and `\openin` lookups cannot trigger installs; the core set therefore includes `amsfonts` (Type 1 Computer Modern + maps).
- LuaTeX lookups cannot trigger installs.
- Each hook call forks `mtx` (about 6 ms per miss).
- Files of a just-installed package other than the requested one are invisible to the running process until its next start.

These are fixed in Phase 1 by the kpathsea patch.
