---
type: Decision
title: Licensing
description: Own code is MIT OR Apache-2.0; patches keep the license of the TeX Live file they change; binaries and packages keep theirs; README follows TeX Live's redistribution guidelines.
tags: [decision, license, legal, redistribution]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-08T14:00:00Z }
sources:
  - id: tl
    resource: https://tug.org/texlive/LICENSE.TL
    title: LICENSE.TL, copying conditions for TeX Live
    author: team:tex-live
---

# Context

On 2026-10-08 the user asked for a README and a license, "taking the licenses of TeX
Live and MiKTeX into account where relevant". `Cargo.toml` already declared
`MIT OR Apache-2.0`, but the repository had no license files. The upstream facts are
in [licenses](/upstream/licenses.md).

# Decision

- **Own code:** `LICENSE-MIT` + `LICENSE-APACHE`, Copyright (c) 2026 Menno Jansen,
  matching `Cargo.toml`. MIT keeps the code GPL-2.0-only compatible (luaotfload, which
  the `luaotfload-main.lua` overlay runs inside, is GPL-2.0).
- **Patches** (`kpathsea-ondemand/patches/`) keep the license of the file they change:
  0001 LGPL-2.1-or-later (kpathsea), 0002 MIT/X11 (XeTeX). `mtx-ondemand.c` stays
  public domain, as its header already says.
- **Binaries and packages** keep their upstream licenses; we claim no copyright on
  TeX Live or its packages ([LICENSE.TL](/upstream/licenses.md)[^tl]).
- **README** carries what TeX Live's guidelines ask of a modified distribution: a
  different name ("MennoTeX, based on TeX Live 2026"), a list of changes, "report
  problems here", and links to the TeX user groups and the TeX Live home page.
- **MiKTeX:** no MiKTeX code, packages or repositories are used, so its terms do not
  apply; the README and `LICENSING.md` say so and credit the idea.
- `LICENSING.md` (not `LICENSE`) holds the overview, so GitHub's license detection
  still reports MIT and Apache-2.0.

# Consequences

- Publishing binaries (a GitHub Release) must make the corresponding source available
  (GPL): the texlive-source revision plus `kpathsea-ondemand/`. Since 2026-10-08 the
  workflow (`.github/workflows/build-binaries.yml`) does this. The binary archive holds,
  next to the programs, `LICENSING.md`, `LICENSE-MIT`, `LICENSE-APACHE`,
  `texlive-source.rev`, `kpathsea-ondemand/`, `COPYINGv2` (GPL-2, from pdfTeX),
  `COPYING.LESSERv2` (LGPL-2.1, from kpathsea) and `licenses/`: every `COPYING*`,
  `LICENSE*` and `COPYRIGHT*` file of texlive-source at its path there. Two license
  texts are not enough: several programs are GPL-3 (dvisvgm, dvipng, autosp, xml2pmx),
  and the statically linked libraries (ICU, FreeType, libpng, zlib, HarfBuzz, …) ask
  for their notices to go with binaries ([licenses](/upstream/licenses.md)).
  With `release: true` the Release also gets `mennotex-src-<release>-<rev>.tar.xz`:
  `git archive` of texlive-source at the pinned revision (the committed tree, so
  unpatched) plus `kpathsea-ondemand/`, `build/`, the license files and the workflow,
  listed in the same `SHA256SUMS`. `mtx install-binaries` ignores the extra files (it
  installs top-level Mach-O files only; a `cargo test` covers it), and
  `--github --release` downloads only `mennotex-bin-*.tar.xz` and `SHA256SUMS`. Not
  yet exercised in CI: the next run needs the user's OK.
- New code files default to MIT OR Apache-2.0; changes to TeX Live sources take that
  source's license.

[^tl]: LICENSE.TL, copying conditions for TeX Live
