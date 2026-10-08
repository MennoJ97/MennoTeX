---
type: Decision
title: Installing packages that shadow installed files
description: Installing a package also installs packages with a same-named tex/ file in a directory a LaTeX format searches first (pstricks → xetex-pstricks), because kpathsea never misses such a file and so never asks mtx.
tags: [decision, install, kpathsea, search-order, pstricks]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-08T16:30:00Z }
verified:
  - { by: process:cargo-test, at: 2026-10-08T16:00:00Z }
  - { by: process:tests/compare_texlive.py, at: 2026-10-08T16:20:00Z }
---

# Context

MennoTeX installs a package when kpathsea **misses** a file. When two packages ship
a file of the same name in different `tex/` directories, a full TeX Live takes the copy
in the directory the format searches first (`TEXINPUTS.xelatex =
tex/{xelatex,latex,xetex,generic,}//`), but MennoTeX takes whichever copy is installed:
the lookup succeeds, nothing is missing, mtx is never asked. Found by
`tests/compare_texlive.py` on 2026-10-08: `pstricks.con` is in `pstricks`
(`tex/generic/pstricks/`) and `xetex-pstricks` (`tex/xelatex/xetex-pstricks/`).
Under XeLaTeX, full TeX Live reads the latter (which selects `config/xdvipdfmx.cfg`,
PostScript specials converted by Ghostscript); MennoTeX read the generic one and
produced different `.xdv` output, silently (corpus `pstricks-xelatex`, and
`chess-xskak` whose PDF happened to be identical).

A scan of the tlpdb for the three LaTeX formats found this to be the only real pair
in TeX Live 2026 (the others were `README` files, never input).

# Decision

Like the font-map rule ([decision 0004](/decisions/0004-font-map-index.md)): when a
package is installed, `shadows::shadow_packages_for` looks up the basename of each of
its `tex/` files (with an extension) in the index; a package with the same name in a
`tex/` directory that `latex`/`pdflatex`, `xelatex` or `lualatex` searches earlier is
added to the transaction. `mtx repair` applies the same rule to installed packages.
The plain formats are left out: they search `generic` before `latex`, so including
them would install packages for plain TeX whenever a LaTeX file shares a generic
name. The cost is a few index lookups per installed file.

# Consequences

- XeLaTeX now reads the same files as a full TeX Live for every corpus document
  (`.fls` comparison), including `pstricks-xelatex`, which therefore needs Ghostscript
  like in any TeX Live ([package quirks](/upstream/package-quirks.md)).
- Shadowing in other search paths (fonts, Lua, BibTeX styles) is not covered; the
  corpus comparison found none.
