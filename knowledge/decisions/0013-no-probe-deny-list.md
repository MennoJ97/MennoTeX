---
type: Decision
title: No deny list for probe-only file lookups
description: Measured on the corpus, 6 of 418 on-demand installs were for files that were only probed (about 6.4 MB, mostly newtx); every one of those files is loaded for real elsewhere, so PLAN.md's per-name deny list is not built.
tags: [decision, probes, iffileexists, measurement]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-08T14:00:00Z }
verified:
  - { by: process:tests/probe_installs.py, at: 2026-10-08T13:20:00Z }
sources:
  - id: latex
    resource: texlive 2026 tlnet, tex/latex/base/latex.ltx (lines 20153, 20604)
    title: LaTeX kernel file substitutions
    author: team:latex-project
  - id: acmart
    resource: texlive 2026 tlnet, tex/latex/acmart/acmart.cls (lines 776-805)
    title: acmart font checks
    author: person:boris-veytsman
---

# Context

PLAN.md §6 expected `\IfFileExists` probes for files of *optional* packages to install
packages a document never uses, and proposed a deny list (a `no_autoinstall` flag) for
"known probe-only names, built from corpus measurements". kpathsea cannot tell a probe
from a load: LaTeX checks existence before every `\input`/`\usepackage` (`\pdffilesize`
or `\openin`), and the recorder (`.fls`) lists probed files too.

# Measurement (2026-10-08)

`tests/probe_installs.py` compares each on-demand install's trigger file (`mtx.log`,
`X → package P`) with the files TeX's `.log` shows as read (`(…/X`). On the 52-document
corpus (pdfLaTeX, XeLaTeX, LuaLaTeX, fresh roots: 418 installs) six triggers were never
read:

| File | Package | Engines | Who probes it |
|---|---|---|---|
| `atveryend.sty`, `atbegshi.sty` | atveryend, atbegshi (tiny) | all | the format build: `\declare@file@substitution` maps them to the kernel's `-ltx` versions[^latex]; documents then load those |
| `footmisc.sty` | footmisc (8 KB) | all | KOMA-Script classes |
| `preview.sty` | preview (7 KB) | all | standalone |
| `newtxmath.sty`, `zi4.sty` | newtx (6 MB), inconsolata (0.3 MB) | XeTeX, LuaTeX | acmart's "please upgrade" checks; it loads them only under pdfTeX[^acmart] |

# Decision

No deny list. Every probed file is loaded for real in other documents: `newtxmath` under
LuaLaTeX, `atbegshi.sty` in plain TeX (it is a generic package), footmisc and preview
whenever a document asks for them. Blocking a name breaks those documents ("File not
found") to save at most ~6 MB once per installation. With the packages removed and
installs off, `\RequirePackage{atveryend}` still works (the kernel substitutes), so the
only cost of these probes is the install itself.

# Consequences

- Rerun `tests/probe_installs.py` when the corpus grows (PLAN.md §8's package-doc and
  arXiv corpora); a file that is *never* loaded anywhere would justify a list.
