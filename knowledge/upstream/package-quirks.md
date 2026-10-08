---
type: Reference
title: Package quirks found by the corpus
description: Behaviour of individual LaTeX packages and tools, seen while growing the document corpus, that looks like an on-demand problem but is not (or is), with the cause.
tags: [corpus, packages, luatex, xypic, pstricks, musixtex, upstream]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-08T13:30:00Z }
verified:
  - { by: process:tests/run_documents.sh, at: 2026-10-08T13:00:00Z }
sources:
  - id: xypdf
    resource: tlnet package xypic (Xy-pic 3.8.9), texmf-dist/tex/generic/xypic/xypdf.tex:92-95
    title: xypdf.tex, \xP@testpdfsave
    author: kristoffer-rose
---

Facts from `tests/documents/` (51 documents since 2026-10-08), checked with the
MennoTeX binaries of CI run 37750229150.

# Xy-pic under LuaLaTeX

- `\usepackage[all]{xy}` fails under LuaLaTeX with "pdfTeX version 1.40.0 or higher is
  needed for the xypdf package with PDF output". The PDF driver tests whether
  `\pdfsave` is defined (`xypdf.tex:92-95`);[^xypdf] LuaTeX has no `\pdfsave` (it is
  `\pdfextension save`), so the test fails in any TeX Live, not because a package is
  missing. Loading `luatex85` first defines it (`\protected macro:->\pdfextension
  save\relax`) and the diagram compiles. Corpus `xypic-diagrams` does that on LuaTeX.

# PSTricks under XeLaTeX

- `pstricks`, `pst-plot` and `pst-node` compile with XeLaTeX on a Mac without
  Ghostscript (`gs` absent from PATH): xdvipdfmx handles the PostScript specials of
  these packages itself. Corpus `pstricks-xelatex`.

# Tools between TeX runs

- `musixflx` (MusiXTeX's spacing pass) and `mpost` run through MennoTeX's command
  shims and binaries between TeX runs; corpus `music-musixtex` and `metapost-mpost`.
