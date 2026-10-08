---
type: Reference
title: Package quirks found by the corpus
description: Behaviour of individual LaTeX packages and tools, seen while growing the document corpus, that looks like an on-demand problem but is not (or is), with the cause.
tags: [corpus, packages, luatex, xypic, pstricks, musixtex, upstream]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-08T16:30:00Z }
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

# PSTricks under XeLaTeX needs Ghostscript

- With `xetex-pstricks` installed (as in any full TeX Live), XeLaTeX reads its
  `tex/xelatex/xetex-pstricks/pstricks.con`, which loads PSTricks's
  `config/xdvipdfmx.cfg`: the drawing goes into `pst:` specials, which xdvipdfmx
  collects into a temporary PostScript file and converts through Ghostscript
  (`rungs`). Without `gs` on PATH that file comes back empty: `xdvipdfmx:fatal: File
  ended prematurely` (checked 2026-10-08 with a full TeX Live 2026 and with MennoTeX).
  MacTeX bundles Ghostscript; elsewhere `brew install ghostscript`.
- **Correction:** this note first said PSTricks works on XeLaTeX without Ghostscript.
  It did in MennoTeX only because `xetex-pstricks` was never installed, so the generic
  `pstricks.con` (plain `ps:` specials, handled by xdvipdfmx itself) was used: a
  silent difference from TeX Live, fixed by [decision 0010](/decisions/0010-shadowing-packages.md).
  Corpus `pstricks-xelatex` is marked `% requires: gs`.

# XeTeX on macOS does not find TeX-tree fonts by name

- A full TeX Live's XeLaTeX on macOS fails on `\setmainfont{FreeSerif}`, `Amiri`, `EB
  Garamond`, `CMU Serif`, `TeX Gyre Heros` (fonts in the TeX tree, not registered with
  CoreText), and metropolis falls back from Fira Sans to Latin Modern Sans. MennoTeX's
  XeTeX patch finds them ([fonts by name](/upstream/fonts-by-name.md)); these are the
  only XeLaTeX differences from TeX Live in the corpus comparison.

# Tools between TeX runs

- `musixflx` (MusiXTeX's spacing pass) and `mpost` run through MennoTeX's command
  shims and binaries between TeX runs; corpus `music-musixtex` and `metapost-mpost`.
