---
type: Reference
title: "TeX Live scripts: mktexfmt, fmtutil, updmap, texdoc, latexmk"
description: Behaviour of the TeX Live scripts mtx relies on, including the TEXMFVAR constraint, how texdoc finds its database and documents, and latexmk's rc files and hooks.
tags: [texlive, fmtutil, mktexfmt, updmap, upstream]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-08T02:40:00Z }
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
  - id: latexmk
    resource: latexmk archive (4.88, 9 March 2026), texmf-dist/scripts/latexmk/latexmk.pl
    title: latexmk 4.88 source
    author: john-collins
  - id: texdoc
    resource: texdoc archive (v4.1.2), texmf-dist/scripts/texdoc/texdoclib-search.tlu and texdoclib-config.tlu
    title: texdoc 4.1.2 sources
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
- **Not safe under concurrency.** fmtutil builds in a system temp directory and then
  `TeXLive::TLUtils::copy("-f", …)`s the format over the destination (around
  `fmtutil.pl:950`). When several runs miss the same format, each rebuilds it and
  overwrites the file while others read it: `pdflatex: fatal: Could not undump 1 4-byte
  item(s) from …/pdflatex.fmt` (1 of 8 runs in `tests/run_concurrent.sh`). MennoTeX's
  `mktexfmt` therefore locks per format, re-checks, builds with `--fmtdir` into a
  staging directory and renames.

# updmap

- `updmap-sys --nohash --quiet` regenerates maps without running `mktexlsr`;
  mtx rebuilds `texmf-var/ls-R` itself afterwards.

[^fmtutil]: fmtutil.pl
[^tlutils]: TLUtils.pm determine_config_files / user-vs-sys check

# texdoc (checked 2026-10-08, texdoc 4.1.2)

- Package database: texdoc reads `TEXMFROOT/tlpkg/texlive.tlpdb`, or the
  `texlive_tlpdb` setting; with neither it fails with "No texlive.tlpdb nor shipped
  tlpdb data found".[^texdoc] mtx keeps the tlpdb in `tlpkg/mtx/`, so its overlay
  tree carries `texdoc/texdoc.cnf` with `texlive_tlpdb = …`.
- Settings come from `<tree>/texdoc/texdoc.cnf` (also `texdoc-<platform>.cnf`,
  `texdoc-dist.cnf`) in every `$TEXMF` tree, in order; the first value set wins.
- texdoc reads `texmf-dist/ls-R` **itself** (`init_lsr_db`) and stops at the first
  directory after the `./doc/...` blocks ("we're exiting the ./doc dir, so it's
  over"). It assumes mktexlsr's sorted order: a `./doc/...` block appended at the end
  of ls-R (as kpathsea allows) is never seen. mtx therefore rebuilds ls-R after
  installing documentation instead of appending.
- Without the tlpdb at `TEXMFROOT/tlpkg` it runs "non-vanilla" and resolves files
  through kpathsea (`TeX system documentation`), which works with mtx's ls-R.

[^texdoc]: texdoc 4.1.2 sources

# latexmk (checked 2026-10-08, latexmk 4.88)

- System rc: `$LATEXMKRCSYS` if set (then *only* that file), else the first existing of
  `/etc/LatexMk`, `/opt/local/share/latexmk/LatexMk`, `/usr/local/share/latexmk/LatexMk`,
  `/usr/local/lib/latexmk/LatexMk` (and `latexmkrc` variants); user rc files come after.[^latexmk]
  So without admin rights, a distribution can only add a system rc through
  `$LATEXMKRCSYS`. MennoTeX's rc reads the first of `@UNIX_rc_system_files` itself, so
  nothing is lost.
- Hooks: `add_hook('compile_begin', sub { my %info = @_; … })` runs once per document build
  (not per pass) with `tex_file`, `root_name`, `aux_dir`, …; `$failure_cmd` runs when a
  build fails, in normal and `-pvc` mode (`compile_failure` hooks only in `-pvc`).
- `time()` inside rc code returns fractions (latexmk loads Time::HiRes).
- `latexmk -commands` prints the effective command per program after all rc files,
  which shows overrides such as `$biber` pointing into another installation.
- On this Mac, `~/.latexmkrc` (2026-10-06) pinned `$biber` to MiKTeX's binary
  (`arch -arm64 …/MiKTeX/…/biber`) because MiKTeX's x86 launcher fails on arm64. With
  MennoTeX first on PATH, that mixed MiKTeX's biber with MennoTeX's biblatex; the user
  had it made conditional on `mtx` being next to the first `kpsewhich`.

[^latexmk]: latexmk 4.88 source
