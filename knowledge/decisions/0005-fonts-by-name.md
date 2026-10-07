---
type: Decision
title: On-demand fonts selected by name
description: An embedded font-name index plus small XeTeX and resolver changes make fontspec names install their packages; LuaLaTeX relies on mtx prefetch for the first run.
tags: [decision, fonts, fontspec, xetex, luaotfload]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-07T12:20:00Z }
verified:
  - { by: process:tests/run_documents.sh, at: 2026-10-07T12:15:00Z }
---

# Decision

- `tools/build_fontname_index.py` reads the `name` tables of every OpenType/TrueType
  font in tlnet (243 packages, 849 MiB; 3,183 faces, 7,926 names) into
  `crates/mtx-core/data/fontnames.tsv.xz` (41 KiB), embedded in mtx (`fontnames.rs`).
  Names are normalized (lowercase alphanumerics); full and PostScript names select a
  face, family names the regular face.
- `mtx ensure --font-name [--siblings] NAME` installs the package and prints the
  font file (and the package's other font files).
- The kpathsea resolver sends names that are not file names (spaces etc.), and
  one-word names that miss the file index, to `mtx ensure --font-name` for the
  opentype/truetype/tfm/ofm formats. It returns the file for OpenType/TrueType;
  for TFM/OFM probes it installs but returns "not found", so luaotfload rescans.
- XeTeX patch registers the found font's directory with CoreText for the process
  (see [fonts by name](/upstream/fonts-by-name.md)).
- `mtx prefetch` reads fontspec commands (`\setmainfont`, `\newfontfamily`,
  `\babelfont`, …) and installs those fonts before the run.

# Results (fresh roots, 2026.1 build)

- XeLaTeX: `fontspec-by-name.tex` (Pagella, Heros, Cursor by name) compiles on the
  first run; regular, bold and italic faces are embedded. Better than stock MacTeX,
  where this fails even with the fonts installed.
- LuaLaTeX: first run without prefetch fails (the font is installed during that run;
  the next run works); with `mtx prefetch` it works on the first run.

# Not done

Patching luaotfload to probe kpathsea before its reload would remove the need for
prefetch, but luaotfload is a frequently updated tlnet package; not worth it yet.
