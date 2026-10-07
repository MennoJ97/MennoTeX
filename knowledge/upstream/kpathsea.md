---
type: Reference
title: kpathsea file lookup and mktex hooks
description: Verified facts about how kpathsea finds files and when it runs mktex* scripts, which decide what on-demand installation can hook into.
resource: https://github.com/TeX-Live/texlive-source/tree/trunk/texk/kpathsea
tags: [kpathsea, texlive, upstream, hooks]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-07T09:45:00Z }
verified:
  - { by: claude-code/claude-opus-5-5, at: 2026-10-06T18:00:00Z }
sources:
  - id: src
    resource: https://github.com/TeX-Live/texlive-source
    title: texlive-source @ 7cd76c1 (2026-10-06)
    author: team:tex-live
---

# Lookup

- `kpathsea_find_file_generic` (`texk/kpathsea/tex-file.c:1015`): candidate names →
  path search (trees marked `!!` only via `ls-R`, `texmf.cnf:118`) → disk search if
  `must_exist` → `kpathsea_make_tex` if still missing **and** `must_exist`
  (`tex-file.c:1131-1138`).[^src]
- `maketex` (`tex-make.c:167-437`) fork/execs the script found via `PATH`, reads the
  path from stdout, checks it is readable, then `kpathsea_db_insert` adds that one
  file to the in-memory hash only (`db.c:199-215`).
- Names are sanitized before running a script: no leading `-`; only
  `[A-Za-z0-9_.+-/]` (`tex-make.c:480-493`).
- kpathsea does **not** add its own directory to `PATH` (`progname.c:354` only reads it), so hook scripts are found only if the installation's `bin` is on `PATH`.

# Which formats have a mktex script

`pk`/`gf` → mktexpk, `tfm` → mktextfm, `fmt`/`base`/`mem` → mktexfmt, `mf` →
mktexmf, `ocp`, `ofm`, `tex` → mktextex (off by default: `MKTEXTEX = 0`,
`texmf.cnf:773`; `texmfmp.c:1117`). (`tex-file.c:529-664`)

# must_exist by call site

| Lookup | must_exist |
|---|---|
| `\input`, TFM loading | true |
| pdfTeX/XeTeX `\filesize` → `find_input_file` → `kpse_find_tex` (`texmfmp.c:3440`, `tex-file.h:151`); modern LaTeX tests existence this way (`expl3` `\file_full_name:n` → `\tex_filesize:D`) | true |
| `\openin` (`openclose.c:299-308`) | false |
| LuaTeX `kpse.find_file` default (`lkpselib.c:644`) | false |
| pdfTeX `.pfb`/`.ttf` (`pdftexdir/mapfile.c:900-902`) | false |
| LuaTeX enc/map/type1/otf/vf (`luatexdir/tex/texfileio.c:159-191`) | false |
| dvipdfmx maps, CMaps (`dvipdfm-x/dpxfile.c:566-642`) | false |

Consequence: stock hooks cover pdfLaTeX/XeLaTeX package loading and TFMs but not
fonts, maps, `\openin` or anything in LuaTeX. Hence the Phase 1 patch.

# Other facts

- `ls-R` accepts repeated directory blocks, so appending is legal (`db.c:89-160`); no magic header is required.
- `texmf_casefold_search = 1` by default (`texmf.cnf:807`).
- Program-specific paths: `TEXINPUTS.pdflatex-dev` adds `tex/latex-dev` (`texmf.cnf:221-222`).

[^src]: texlive-source @ 7cd76c1 (2026-10-06)
