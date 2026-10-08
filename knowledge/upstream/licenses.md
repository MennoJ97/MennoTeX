---
type: Reference
title: Licenses of TeX Live, its programs and MiKTeX
description: Verified license terms of kpathsea, XeTeX, pdfTeX, LuaTeX, dvipdfmx and luaotfload, TeX Live's and MiKTeX's redistribution guidelines, and Knuth's renaming condition.
tags: [license, upstream, texlive, miktex, redistribution]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-08T14:00:00Z }
verified:
  - { by: claude-code/claude-opus-5-5, at: 2026-10-08T09:45:00Z }
  - { by: claude-code/claude-opus-5-5, at: 2026-10-08T14:00:00Z }
sources:
  - id: src
    resource: https://github.com/TeX-Live/texlive-source/tree/6a300188053b8f2ded89dbd52293732a706b9c0e
    title: texlive-source @ 6a30018 (the revision in build/texlive-source.rev)
    author: team:tex-live
  - id: tl
    resource: https://tug.org/texlive/LICENSE.TL
    title: LICENSE.TL r70641 (2024-03-14)
    author: team:tex-live
  - id: copying
    resource: https://www.tug.org/texlive/copying.html
    title: TeX Live licensing, copying, and redistribution
    author: team:tex-live
  - id: miktex
    resource: https://miktex.org/copying
    title: Redistributing MiKTeX
    author: human:christian-schenk
  - id: lotf
    resource: https://ctan.org/pkg/luaotfload
    title: CTAN luaotfload package page
---

# Program sources (headers at the pinned revision)

| Code | License | Checked file |
|---|---|---|
| kpathsea | LGPL 2.1 or later | `texk/kpathsea/{tex-file,db,tex-make}.c`; text in `texk/kpathsea/COPYING.LESSERv2`[^src] |
| XeTeX macOS font manager | MIT/X11 (SIL, Jonathan Kew, Jiang Jiang), with a no-endorsement clause | `texk/web2c/xetexdir/XeTeXFontMgr_Mac.mm`[^src] |
| pdfTeX | GPL 2 or later | `texk/web2c/pdftexdir/pdftoepdf.cc`[^src] |
| LuaTeX | GPL 2 or later | `texk/web2c/luatexdir/luatex.h`[^src] |
| dvipdfmx | GPL 2 or later | `texk/dvipdfm-x/dvipdfmx.c`[^src] |
| luaotfload (package) | GPL 2.0 | CTAN catalogue[^lotf] |

Other programs were not checked one by one; the legal statement in each source
directory is authoritative.

# License files in texlive-source

At the pinned revision the tree holds 95 files named `COPYING*`, `LICENSE*` or
`LICENCE*` (and 14 more named `COPYRIGHT*`), one or more per program or library
directory.[^src] The full texts the release archive needs:

- GPL-2: `texk/web2c/pdftexdir/COPYINGv2`; LGPL-2.1: `texk/kpathsea/COPYING.LESSERv2`.
- GPL-3: `texk/dvisvgm/dvisvgm-src/COPYING`, `texk/dvipng/dvipng-src/COPYING`,
  `utils/autosp/autosp-src/COPYING`, `utils/xml2pmx/xml2pmx-src/COPYING` (all four
  programs are in the CI build); LGPL-3: `texk/dvipng/dvipng-src/COPYING.LESSER`.
- XeTeX's MIT/X11 notice: `texk/web2c/xetexdir/COPYING`.
- Bundled libraries, linked statically: `libs/icu/icu-src/LICENSE`,
  `libs/freetype2/freetype-src/LICENSE.TXT`, `libs/libpng/libpng-src/LICENSE`,
  `libs/zlib/zlib-src/LICENSE`, `libs/harfbuzz/harfbuzz-src/COPYING`,
  `libs/graphite2/graphite2-src/{COPYING,LICENSE}`, `libs/luajit/LuaJIT-src/COPYRIGHT`,
  and others under `libs/`.
- TeX Live's trimmed copies of GMP and MPFR (`libs/gmp/gmp-src`, `libs/mpfr/…`) carry
  no `COPYING` file; their LGPL-3 text comes in through dvipng's `COPYING.LESSER`.

The workflow copies all of them into the archive's `licenses/` tree
([decision 0008](/decisions/0008-licensing.md)).

# TeX Live redistribution guidelines

- TeX Live has no single license; you may copy, modify and redistribute a part only
  under that package's own terms.[^tl]
- A modified distribution must prominently say what changed, so users contact the
  modifier and not TeX Live; a top-level README is suggested. A distinct title such as
  "X, based on TeX Live YYYY" is recommended.[^tl]
- You must not place your own copyright on the whole distribution; "all rights
  reserved" is called reprehensible.[^tl]
- Requests (not requirements): acknowledge the TeX user groups and link
  <https://tug.org/usergroups.html>, reference <https://tug.org/texlive/>.[^tl]
- Knuth's TeX, Metafont and CM are public domain, but modified `tex.web`, `mf.web`,
  `plain.tex`, `plain.mf` and `hyphen.tex` should be renamed.[^copying]

# MiKTeX

MiKTeX's redistribution page is an adaptation of TeX Live's: no single license, mark
changes, use a distinct name ("based on MiKTeX x.y"), no copyright on the whole.[^miktex]
It applies only to redistributing MiKTeX itself or its packages; MennoTeX uses neither
(see [MiKTeX](/upstream/miktex.md) for what was studied).

[^src]: texlive-source @ 6a30018 (the revision in build/texlive-source.rev)
[^tl]: LICENSE.TL r70641 (2024-03-14)
[^copying]: TeX Live licensing, copying, and redistribution
[^miktex]: Redistributing MiKTeX
[^lotf]: CTAN luaotfload package page
