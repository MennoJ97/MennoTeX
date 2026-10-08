---
type: Decision
title: CTAN overlay channel
description: mtx install --from-ctan PKG installs TeX Live's package from tlnet, then CTAN's current TDS archive into texmf-ctan, which TeX searches before texmf-dist; only explicit, only TDS input directories, format stamps include it, and mtx update drops it once tlnet has caught up.
tags: [decision, ctan, overlay, phase-4]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-08T18:30:00Z }
verified:
  - { by: process:cargo-test, at: 2026-10-08T18:20:00Z }
sources:
  - id: ctan-api
    title: CTAN JSON API, package record
    url: https://ctan.org/json/2.0/pkg/tcolorbox
---

# Context

PLAN.md §4.2 (Phase 4): tlnet lags CTAN by a day or two, and a few TDS packages are on
CTAN only. CTAN's JSON API names a package's version, its TeX Live name and the path of
its TDS zip below CTAN's `install/` tree[^ctan-api], served through `mirrors.ctan.org`
(a 307 redirect to a mirror, seen 2026-10-08).

Facts checked 2026-10-08:

- Many CTAN packages have no TDS archive, only sources: `nicematrix` was 7.12 on CTAN
  and 7.11d in TeX Live, with no `install` field. The overlay cannot help those.
- Kernel packages carry no `catalogue-version` in TeX Live (`l3kernel`, `amsmath`); CTAN
  versions them by date.
- CTAN publishes no signatures or checksums for its archives.

# Decision

- `mtx install --from-ctan PKG…` (module `ctan`): install TeX Live's package first
  (verified, with dependencies, font maps and formats), then fetch CTAN's TDS zip over
  HTTPS and unpack into `<root>/texmf-ctan`, the second entry of `TEXMFAUXTREES` after
  `texmf-mtx`. If TeX Live already has CTAN's version, only TeX Live's is installed.
- Only TDS input directories are taken (`tex`, `fonts`, `bibtex`, `makeindex`,
  `metapost`, `metafont`, `mft`, `dvips`, `tex4ht`, `context`, `scripts`, `doc`); not
  `source/`, not `web2c/` or other configuration. Unsafe paths fail the whole archive;
  symlinks are skipped. Files are staged and renamed in under the install lock.
- Recorded under the TeX Live name as the pseudo-package `<pkg>.ctan` (its files) and the
  setting `ctan:<pkg>` (CTAN version, CTAN name, tlnet's version at the time). Never used
  for on-demand installs.
- Format stamps add `ctan@<install time>` to a trigger package that has an overlay
  ([decision 0016](/decisions/0016-format-stamps.md)), so formats are rebuilt with the
  overlay and again without it; font maps are regenerated.
- `mtx update` drops an overlay once tlnet's version equals CTAN's or has changed since
  the overlay was installed (catalogue version, else the revision). `mtx remove --from-ctan`
  drops it by hand; `mtx remove PKG` removes it with the package; `bootstrap --from` does
  not carry it; `mtx doctor` lists it.

# Consequences

- The overlay is the one place where mtx installs files nobody signed. It is explicit per
  package and per command, and is said so in `mtx install --help`; verification of tlnet
  is unchanged.
- Checked on a scratch root: CTAN's `l3kernel` (2026-09-09) over TeX Live's r80334:
  `kpsewhich expl3.sty` found the overlay, the stamp named it, pdfLaTeX rebuilt its
  format and compiled an expl3 document; `remove --from-ctan` restored TeX Live's files
  and the format was rebuilt again. Tests: `ctan_overlay_until_tlnet_catches_up`,
  `ctan_refuses_bad_archives`, `parses_ctan_package_records`.
