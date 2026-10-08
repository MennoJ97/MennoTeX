---
type: Reference
title: Package quirks found by the corpus
description: Behaviour of individual LaTeX packages and tools, seen while growing the document corpus, that looks like an on-demand problem but is not (or is), with the cause.
tags: [corpus, packages, luatex, xypic, pstricks, musixtex, upstream]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-08T18:30:00Z }
verified:
  - { by: process:tests/run_documents.sh, at: 2026-10-08T13:00:00Z }
sources:
  - id: xypdf
    resource: tlnet package xypic (Xy-pic 3.8.9), texmf-dist/tex/generic/xypic/xypdf.tex:92-95
    title: xypdf.tex, \xP@testpdfsave
    author: kristoffer-rose
  - id: latexltx
    resource: tlnet package latex (TeX Live 2026), texmf-dist/tex/latex/base/latex.ltx:9827-9856, 20153, 20604
    title: latex.ltx, \IfFileExists and file substitutions
    author: team:latex-project
  - id: l3file
    resource: tlnet package l3kernel (TeX Live 2026), texmf-dist/tex/latex/l3kernel/expl3-code.tex:12708-12730
    title: l3file, \file_full_name:n
    author: team:latex-project
  - id: l3depcheck
    resource: tlnet package l3kernel r80334 (TeX Live 2026), texmf-dist/tex/latex/l3kernel/expl3-code.tex:13235-13275, l3backend-pdftex.def:28-35
    title: \__kernel_dependency_version_check:nn and its use in l3backend
    author: team:latex-project
  - id: ctan-nicematrix
    url: https://ctan.org/json/2.0/pkg/nicematrix
    title: CTAN JSON record of nicematrix (no install field)
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

# How LaTeX checks that a file exists

- `\IfFileExists` (and so `\usepackage`, `\InputIfFileExists`) expands
  `\IfFileExists@@` → `\file_full_name:n`, which tests existence with `\tex_filesize:D`
  (`\pdffilesize`/`\filesize`), not `\openin`.[^latexltx][^l3file] The lookup reaches
  kpathsea (and mtx) but no engine file-open routine; the recorder (`.fls`) still lists
  the file, twice. A missing package therefore stops LaTeX ("File not found", fatal in
  nonstopmode) without any further file being opened, which is why MennoTeX's warning for
  TeX's log is printed from a kpathsea callback ([decision 0014](/decisions/0014-install-warnings-in-tex-log.md)).
- Building the LaTeX format looks up `atveryend.sty` and `atbegshi.sty` (around
  `\declare@file@substitution`, `latex.ltx:20153,20604`[^latexltx]); documents then
  load the kernel's `atveryend-ltx.sty`/`atbegshi-ltx.sty`. With the packages absent and
  installs off, `\RequirePackage{atveryend}` works, so these two installs are only probes
  ([decision 0013](/decisions/0013-no-probe-deny-list.md)).
- Other probes seen in the corpus: KOMA-Script classes check `footmisc.sty`; standalone
  checks `preview.sty`; acmart checks `zi4.sty` and `newtxmath.sty` under every engine
  but loads them only under pdfTeX (`acmart.cls:776-805`).

# Kernel support files check the format's kernel

- `expl3.sty` and every `l3backend-*.def` call `\__kernel_dependency_version_check:nn`
  with the date they need; when the format's expl3 date (`\c__kernel_expl_date_tl`) is
  older, LaTeX stops with "Mismatched LaTeX support files detected. Loading '…' aborted!"
  (`l3backend-pdftex.def` of 2026-09-09 needs 2023-10-10).[^l3depcheck] The backend files
  are read at run time, the kernel comes from the format, so replacing `l3kernel` in the
  middle of a compile can only hurt that run; mtx defers such upgrades
  ([decision 0018](/decisions/0018-upgrades-around-compiles.md)).
- TeX Live 2026 has no `l3backend` package (it is part of `l3kernel`); the first-aid
  package is `firstaid`. Kernel packages (`l3kernel`, `latex`, `amsmath`) carry no
  `catalogue-version`; CTAN versions them by date.

# CTAN versions without a TDS archive

- Many CTAN packages ship only sources (`.dtx`/`.ins`), with no `install` field in the
  JSON record: on 2026-10-08 `nicematrix` was 7.12 on CTAN and 7.11d in TeX Live, without
  a TDS archive.[^ctan-nicematrix] `mtx install --from-ctan` cannot install those
  ([decision 0019](/decisions/0019-ctan-overlay.md)); only tlnet's build of them works.
