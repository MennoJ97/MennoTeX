---
type: Decision
title: Installing a missing map package during the compile
description: On a font-map miss, pdfTeX and LuaTeX ask kpathsea's resolver, which binary-searches mtx's font-map table (tlpkg/mtx/fontmaps.tsv) and runs mtx ensure --font-map only for fonts a package's map covers; the map is then re-read in the same run.
tags: [decision, fonts, maps, pdftex, luatex, patch]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-08T14:00:00Z }
verified:
  - { by: process:tests/run_c_tests.sh, at: 2026-10-08T13:40:00Z }
  - { by: process:cargo-test, at: 2026-10-08T13:50:00Z }
---

# Context

[Decision 0004](0004-font-map-index.md) installs the packages whose maps cover a
package's TFMs (`ec` → `cm-super`) in the same transaction, and `mtx doctor`/`repair`
fix roots made before that rule. A root that still lacks a map package (made before the
rule and not repaired, or a package removed with `--force`) made pdfTeX fall back to
bitmap fonts: `mktexpk` runs METAFONT, or the run fails without its sources.

# Decision

- `mtx` writes its embedded font-map table to `tlpkg/mtx/fontmaps.tsv` (lines
  `FONT<TAB>PACKAGES`, sorted bytewise, 46 k lines, 1.5 MB) whenever it installs its
  hooks (bootstrap, repair, self-update).
- `kpathsea_ondemand_font_map(kpse, font)` (patch 0001, `mtx-ondemand.c`) memory-maps the
  table on first use and binary-searches it. Only for a listed font does it run
  `mtx ensure --font-map --siblings FONT`, once per font per run; virtual and METAFONT
  fonts, which no map covers, cost no process.
- `mtx ensure --font-map` (`ensure::ensure_font_map`) installs the covering package that
  is not installed (subject to `autoinstall`, prompt "FONT (font map) needs package P"),
  which regenerates `pdftex.map`, and prints the package's files.
- Patch 0003's `mtx_reread_default_map(tfm)`: on a map miss with no install since the map
  was read, it asks `kpathsea_ondemand_font_map` first; if that installed, the default
  map is re-read as in [decision 0012](0012-map-reread.md).
- Declined or failed: a `Package mtx Warning: the font map for FONT …` line
  ([decision 0014](0014-install-warnings-in-tex-log.md)).

# Consequences

- Tested 2026-10-08 in a scratch root with `cm-super` removed: pdfLaTeX (T1 text in
  Computer Modern) and LuaLaTeX (`\font\x=ecrm1000`) installed `cm-super` (61.6 MiB)
  mid-run and embedded `sfrm1000.pfb`; no `mktexpk`.
- LuaLaTeX's T1 default is Latin Modern, so `ec` fonts reach LuaTeX's map only when
  named directly.
- `mtx doctor` reports missing map packages as a warning now, not a problem.
