---
type: Playbook
title: Development and testing
description: How to build mtx, run the tests, bootstrap a throw-away installation and compile documents with it.
tags: [playbook, development, testing]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-07T09:45:00Z }
verified:
  - { by: process:cargo-test, at: 2026-10-07T09:30:00Z }
---

# Build and unit tests

```bash
cargo build --release
cargo test
```

Rust comes from Homebrew (`brew install rust`); there is no rustup toolchain file yet.

Offline tests use the signed fake repository in `crates/mtx-core/testdata/tlnet`;
regenerate it with `tools/make_test_repo.sh` (needs Homebrew `gpg`) after changing its
contents, and commit the result.

# Throw-away installation

Use a scratch root so nothing touches `~/Library/MennoTeX`:

```bash
./target/release/mtx --root /tmp/mtxroot bootstrap
export PATH=/tmp/mtxroot/bin/universal-darwin:/usr/bin:/bin
pdflatex hello.tex
```

Keep `PATH` minimal: this Mac has MiKTeX symlinks in `/usr/local/bin` (`tex`,
`pdflatex`, `kpsewhich`, …) that would otherwise be picked up, and kpathsea finds
the `mktex*` hooks through `PATH`.

# Test documents

```bash
tests/run_documents.sh /tmp/mtxroot pdflatex   # or xelatex / lualatex
```

Compiles every `tests/documents/*.tex` and prints, per document: result, pages,
packages installed on demand and seconds. Header comments control a run:
`% engines: pdflatex xelatex lualatex` (others are skipped) and
`% tools: biber|bibtex|makeindex|makeglossaries` (run between TeX runs; a third TeX
run follows). Bibliographies are embedded with `filecontents*` so each document is
self-contained. The corpus covers beamer, biblatex/biber, natbib/bibtex,
fontspec/unicode-math, babel, KOMA-Script, memoir + index, pgfplots/tikz-cd, tables,
theorems, listings/algorithm2e, mhchem/chemfig, Libertinus + microtype,
glossaries, standalone TikZ and moderncv. Logs stay in the printed temp
directory. Use a fresh root to measure first-run behaviour; set `MTX_CACHE` to
an existing root's `tlpkg/mtx/cache` to skip re-downloading archives.

# Useful commands

- `mtx doctor`: run it first when something behaves oddly. On this Mac it flags MiKTeX in `/usr/local/bin` shadowing `pdflatex`, `lualatex`, `kpsewhich`, … unless PATH is reduced.

- `mtx which <file> [--format tfm]`: which package provides a file and whether it is installed.
- `mtx info <pkg>`, `mtx list`, `mtx refresh`, `mtx regen`.
- `tlpkg/mtx/mtx.log` in the root records every install and verification.
- `MTX_REPOSITORY=/path/to/local/tlnet` or `file://…` uses a local repository; `MTX_CACHE` moves the archive cache.

# Debugging kpathsea

```bash
kpsewhich -var-value=TEXMFVAR
kpsewhich -debug=-1 foo.sty
```
