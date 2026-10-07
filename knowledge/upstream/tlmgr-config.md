---
type: Reference
title: How tlmgr generates configuration files
description: Exact rules tlmgr uses for fmtutil.cnf, updmap.cfg and language.dat/.def/.dat.lua, which mtx reproduces.
resource: https://github.com/MennoJ97/MennoTeX/blob/main/crates/mtx-core/src/configfiles.rs
tags: [tlmgr, fmtutil, updmap, hyphenation, texlive]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-07T09:45:00Z }
verified:
  - { by: process:cargo-test, at: 2026-10-07T09:30:00Z }
sources:
  - id: tlutils
    resource: texlive.infra archive, tlpkg/TeXLive/TLUtils.pm (create_*, parse_AddFormat_line, parse_AddHyphen_line)
    title: TeX Live TLUtils.pm
    author: team:tex-live
  - id: tlpobj
    resource: texlive.infra archive, tlpkg/TeXLive/TLPOBJ.pm (fmtutil_cnf_lines, updmap_cfg_lines, language_*_lines)
    title: TeX Live TLPOBJ.pm
    author: team:tex-live
---

# Files

| File | Header file | Destination | Comment char | Footer |
|---|---|---|---|---|
| `fmtutil.cnf` | `texmf-dist/web2c/fmtutil-hdr.cnf` | `texmf-dist/web2c/fmtutil.cnf` | `#` | none |
| `updmap.cfg` | `texmf-dist/web2c/updmap-hdr.cfg` | `texmf-dist/web2c/updmap.cfg` | `#` | none |
| `language.dat` | `…/tex/generic/config/language.us` | `TEXMFSYSVAR/tex/generic/config/` | `%` | none |
| `language.def` | `language.us.def` | same | `%` (no generated-by line) | `%%% No changes…`, blank, `\uselanguage {USenglish} …` |
| `language.dat.lua` | `language.us.lua` | same | `--` | `}` |

Packages are processed in name order; only installed packages contribute.[^tlutils]
The header files ship in `texlive.infra` (fmtutil/updmap) and `hyphen-base` (language).

# Line formats

- fmtutil: `#\n# from <pkg>:\n` before the first format, then `[#! ]<name> <engine> <patterns|-> <options>`; `mode=disabled` → `#! `.[^tlpobj]
- updmap: `Map x` / `MixedMap x` / `KanjiMap x`, sorted by map name, no per-package comment.
- language.dat: `% from <pkg>:`, optional `% <comment>`, `<name> <file>`, `=<synonym>` lines.
- language.def: `\addlanguage{<name>}{<file>}{}{<lhm>}{<rhm>}` for the name and each synonym.
- language.dat.lua: tab-indented `['name'] = {`, `loader`, `lefthyphenmin`, `righthyphenmin`, `synonyms = { 'a', 'b' },` (empty renders as `{  }`), optional `patterns`/`hyphenation`/`special`, `},`.
- `AddHyphen` `databases=` restricts which files get the entry (default dat,def,lua). Empty `file_patterns=` counts as absent.
- AddFormat values are split like Perl `quotewords`: quotes group words and are dropped.

[^tlutils]: TeX Live TLUtils.pm
[^tlpobj]: TeX Live TLPOBJ.pm
