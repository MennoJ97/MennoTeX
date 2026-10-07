---
type: Decision
title: Embedded font-map index
description: mtx ships a TeX-font → map-package table so installing TFMs also installs the packages whose font maps cover them (e.g. ec → cm-super).
tags: [decision, fonts, pdftex, updmap]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-07T11:30:00Z }
verified:
  - { by: process:tests/run_documents.sh, at: 2026-10-07T11:25:00Z }
---

# Problem

With `\usepackage[T1]{fontenc}` and the default fonts, pdfTeX loads `ecrm1000.tfm`
(package `ec`), then needs a map entry `ecrm1000 → sfrm1000.pfb`, which only
`cm-super`'s map provides. That lookup is internal to pdfTeX (its loaded
`pdftex.map`); no kpathsea file lookup ever names cm-super. pdfTeX then falls back
to a bitmap font and fails (`Font ecrm1000 at 600 not found`). Seen in the corpus
documents `babel-multilingual` and `moderncv-cv`.

# Decision

- `tools/build_fontmap_index.py` downloads every tlnet package with
  `addMap`/`addMixedMap` (303 packages, 831 MiB in 2026-10), verifies them against
  the tlpdb, reads the named map files, and writes
  `crates/mtx-core/data/fontmaps.tsv.xz` (45,974 fonts, 88 KiB). Every font maps to
  exactly one package.
- The table is embedded in mtx (`fontmaps.rs`). Every install transaction adds, as
  dependencies, the packages whose maps cover TFMs being installed, unless the
  package maps its own fonts or a provider is installed already.
- Maps are therefore regenerated (updmap) in the same transaction, before pdfTeX
  reads `pdftex.map` at its first shipout, so the first run works.

# Consequences

- `ec` pulls in `cm-super` (62 MiB), as MiKTeX does in this situation.
- The table must be regenerated when font packages change; for now by hand, later
  by CI next to the binary channel.
- Roots created before this rule may have `ec` without `cm-super`; a repair step is
  still missing (the `mktexpk` fallback could install the map package for the next run).
