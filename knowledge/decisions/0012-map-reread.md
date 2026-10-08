---
type: Decision
title: Re-reading the font map after an install
description: pdfTeX and LuaTeX read pdftex.map once; a MennoTeX patch re-reads the default map on a lookup miss when kpathsea's install counter moved, so a font package installed after page 1 works in the same run.
tags: [decision, pdftex, luatex, fonts, maps, patch]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-08T19:00:00Z }
verified:
  - { by: process:tests/run_documents.sh, at: 2026-10-08T18:50:00Z }
  - { by: process:tests/run_c_tests.sh, at: 2026-10-08T18:50:00Z }
sources:
  - id: pdftex
    resource: texlive-source @ 6a3001880, texk/web2c/pdftexdir/mapfile.c
    title: pdfTeX map file handling
    author: team:tex-live
  - id: luatex
    resource: texlive-source @ 6a3001880, texk/web2c/luatexdir/font/mapfile.c
    title: LuaTeX map file handling
    author: team:tex-live
---

# Context

pdfTeX reads its default map file (`pdftex.map`) once, at the first font lookup
(`fmlookup` → `fm_read_info`, `mapfile.c:591,629`), into an AVL tree; LuaTeX does the
same for Type 1 fonts (`getfontmap`, `mapfile.c:570`).[^pdftex][^luatex] mtx installs a
font package when its TFM is first looked up and regenerates `pdftex.map` inside that
install, but a font first used after page 1 is looked up after the map was read.
PLAN.md §5.7 expected a warning and a fallback; reproduced on 2026-10-08 the run is
**fatal**: with no map entry the font counts as a bitmap font, `mktexpk` fails
(`gsftopk` needs Ghostscript) and pdfTeX stops with `Font ec-qplr at 600 not found`,
no PDF (corpus `late-font-map`).

# Decision

- kpathsea's resolver counts successful mtx runs per process
  (`kpathsea_ondemand_generation()`, declared in `tex-make.h` by patch 0001).
- Patch `0003-pdftex-luatex-map-reread.patch` (both engines' `mapfile.c`): remember the
  default map's name and the count when it is read; on a map lookup miss, if the count
  changed and the default map was actually read (not replaced by `\pdfmapfile` without
  `+`), read it again with `FM_DUPIGNORE` and look up again. Known entries stay; the
  "duplicates ignored" warning (`mapfile.c:196`, LuaTeX `:160`) is suppressed during
  the re-read.
- No re-read happens without an install, so documents that never install pay nothing.

# Consequences

- Corpus `late-font-map` passes on the first run on all engines (pdfTeX and LuaTeX read
  `pdftex.map` twice, no duplicate warnings); the full corpus is unchanged
  (2026-10-08, local build: pdfLaTeX 41/41, XeLaTeX 43/43, LuaLaTeX 47/47).
- `lookup_fontmap` (PostScript names, for embedded PDF files) does not re-read.
- XeTeX is unaffected: xdvipdfmx reads the maps after TeX has finished.
