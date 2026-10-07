---
type: Reference
title: "TeX Live scripts: mktexfmt, fmtutil, updmap"
description: Behaviour of the TeX Live Perl scripts mtx relies on, including the TEXMFVAR constraint.
tags: [texlive, fmtutil, mktexfmt, updmap, upstream]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-07T10:30:00Z }
verified:
  - { by: claude-code/claude-opus-5-5, at: 2026-10-07T09:30:00Z }
sources:
  - id: tlutils
    resource: texlive.infra archive, tlpkg/TeXLive/TLUtils.pm:5710-5740
    title: TLUtils.pm determine_config_files / user-vs-sys check
    author: team:tex-live
  - id: fmtutil
    resource: texlive-scripts archive, texmf-dist/scripts/texlive/fmtutil.pl
    title: fmtutil.pl
    author: team:tex-live
---

# mktexfmt

- `mktexfmt` is `fmtutil.pl` invoked by that name; it always runs in **user mode**
  and prints only the created format's path on stdout (`fmtutil.pl:181-215`).[^fmtutil]
- It writes formats to `TEXMFSYSVAR` when that is writable, else `TEXMFVAR` (`fmtutil.pl:1595`).
- It refuses to run when `TEXMFVAR == TEXMFSYSVAR` **and** `TEXMFCONFIG == TEXMFSYSCONFIG`:
  `-user mode but path setup is -sys type, bailing out` (`TLUtils.pm:5733`).[^tlutils]
  TeX Live's portable-install convention (setting them equal) therefore breaks
  on-demand format building; MennoTeX uses separate `texmf-user-var`/`-config` trees.
- It reads every `fmtutil.cnf` found via kpathsea and writes changes to
  `TEXMFCONFIG/web2c/fmtutil.cnf`.
- A format build runs the engine with `-ini`, which itself triggers `mktextex`
  lookups; with mtx's hook those install packages during the build
  (observed: `atveryend`, `atbegshi` for `pdflatex.fmt`).
- `mktexfmt` guards against recursion with the `mktexfmt_loop` environment variable.

# updmap

- `updmap-sys --nohash --quiet` regenerates maps without running `mktexlsr`;
  mtx rebuilds `texmf-var/ls-R` itself afterwards.

[^fmtutil]: fmtutil.pl
[^tlutils]: TLUtils.pm determine_config_files / user-vs-sys check
