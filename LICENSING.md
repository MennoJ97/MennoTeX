# Licensing

MennoTeX is two things: original code (the `mtx` package manager and its build
tooling), and changes to TeX Live programs. They are licensed differently. TeX Live
itself has no single license: each package keeps its own, and those terms apply to
the parts of it that MennoTeX builds or installs.

## MennoTeX's own work

Unless a file says otherwise, everything in this repository is
Copyright (c) 2026 Menno Jansen and licensed under either of

- the Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE)), or
- the MIT license ([LICENSE-MIT](LICENSE-MIT)),

at your option. This covers `crates/`, `tools/`, `build/`, `tests/`, `.github/`,
`knowledge/`, `PLAN.md` and `crates/mtx-core/data/luaotfload-main.lua`.

Contributions are licensed the same way: unless you say otherwise, anything you
submit for inclusion is dual licensed as above, with no additional terms or
conditions.

The MIT option keeps this code compatible with GPL-2.0-only software. That matters
for `luaotfload-main.lua`, which runs inside luaotfload (GPL-2.0).

## Changes to TeX Live programs (`kpathsea-ondemand/`)

| File | Changes | License |
|---|---|---|
| `patches/0001-kpathsea-ondemand.patch` | kpathsea (`texk/kpathsea/`) | GNU LGPL, version 2.1 or later, same as kpathsea |
| `patches/0002-xetex-tex-tree-fonts-by-name.patch` | XeTeX (`texk/web2c/xetexdir/XeTeXFontMgr_Mac.mm`) | The XeTeX MIT/X11 license, same as the file it changes |
| `mtx-ondemand.c` | added to kpathsea, included by the patched `tex-make.c` | Public domain (as its header states) |

A patched kpathsea is still kpathsea, under the LGPL. The full license texts come
with the TeX Live sources these patches apply to, for example
`texk/kpathsea/COPYING.LESSERv2` in
[texlive-source](https://github.com/TeX-Live/texlive-source).

## Built binaries

`build/build-texlive.sh` and the `build-binaries` workflow build TeX Live's programs
from [texlive-source](https://github.com/TeX-Live/texlive-source) at the revision in
`build/texlive-source.rev`, with the patches above. The binaries keep their upstream
licenses. Most are GNU GPL version 2 or later (pdfTeX, LuaTeX, dvipdfmx and others),
kpathsea is LGPL-2.1-or-later, and XeTeX is MIT/X11. The legal statements in each
program's source directory are the final word.

If you redistribute these binaries, the GPL requires you to make the corresponding
source available: the texlive-source revision plus this repository's
`kpathsea-ondemand/` directory.

## TeX Live packages

`mtx` downloads packages unmodified from TeX Live's network repository (tlnet) and
verifies them against TeX Live's signature. Each package keeps its own license (LPPL,
GPL, OFL, public domain, …); the [TeX Catalogue](https://ctan.org/pkg) lists them.
MennoTeX does not ship these packages and does not change their files.

Two generated files under `crates/mtx-core/data/` are indexes built from those
packages by `tools/build_fontmap_index.py` and `tools/build_fontname_index.py`:
`fontmaps.tsv.xz` (font file → map package) and `fontnames.tsv.xz` (font name →
package). They hold only names, no font data. `crates/mtx-core/keys/texlive.asc` is
TeX Live's public signing key, included so downloads can be verified.

Knuth asks that modified versions of `tex.web`, `mf.web`, `plain.tex`, `plain.mf` and
`hyphen.tex` be renamed. MennoTeX changes none of them.

## Relationship to TeX Live and MiKTeX

MennoTeX is based on TeX Live 2026. Following TeX Live's
[redistribution guidelines](https://tug.org/texlive/LICENSE.TL):

- It is **not** TeX Live and is not made by the TeX Live team. The changes are listed
  in the [README](README.md#how-mennotex-differs-from-tex-live). Report problems
  with MennoTeX here, not to TeX Live.
- No copyright is claimed on TeX Live or on its packages, only on the work described
  in [MennoTeX's own work](#mennotexs-own-work).
- TeX Live is a joint effort of the TeX user groups; please consider
  [joining one](https://tug.org/usergroups.html). TeX Live's home page is
  <https://tug.org/texlive/>.

MiKTeX inspired the idea of installing packages on the fly. MennoTeX contains no
MiKTeX code and does not use MiKTeX's packages or repositories; it only took design
ideas from it. MennoTeX is not affiliated with MiKTeX.

TeX is a trademark of the American Mathematical Society. MiKTeX and other names are
used only to refer to those projects.
