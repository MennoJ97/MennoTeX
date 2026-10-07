---
type: Playbook
title: Development and testing
description: How to build mtx, run the tests, bootstrap a throw-away installation and compile documents with it.
tags: [playbook, development, testing]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-07T23:00:00Z }
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
run follows), `% prefetch: <engines>` (run `mtx prefetch` first for those engines). Bibliographies are embedded with `filecontents*` so each document is
self-contained. The corpus covers beamer, biblatex/biber, natbib/bibtex,
fontspec/unicode-math, babel, KOMA-Script, memoir + index, pgfplots/tikz-cd, tables,
theorems, listings/algorithm2e, mhchem/chemfig, Libertinus + microtype,
glossaries, standalone TikZ and moderncv. Logs stay in the printed temp
directory. Use a fresh root to measure first-run behaviour; set `MTX_CACHE` to
an existing root's `tlpkg/mtx/cache` to skip re-downloading archives.

# Concurrency test

```bash
tests/run_concurrent.sh /tmp/fresh-root pdflatex 8
```

Starts 8 compiles at once against one fresh installation; passes when all succeed and
`mtx doctor` reports no problem.

# Building the TeX Live binaries

Locally (about 20 minutes on 12 cores):

```bash
git clone --depth 1 --branch tags/texlive-2026.1 https://github.com/TeX-Live/texlive-source.git /tmp/tl2026
build/build-texlive.sh /tmp/tl2026
mtx --root /tmp/mtxroot install-binaries /tmp/tl2026/inst/bin/aarch64-apple-darwin*/
```

On GitHub: run the **Build TeX Live binaries** workflow by hand (Actions tab,
`workflow_dispatch`; optionally publish a release). It is manual on purpose: macOS
minutes are billed at 10× on a private repo. The first run (2026-10-07, run
37694133534) took 14 min 16 s (build 13 min), about 150 billed minutes, and produced a
39 MB artifact `mennotex-bin-2026-6a3001880-arm64-darwin`. Fetch and install it:

```bash
gh run download <run-id> -R MennoJ97/MennoTeX -D /tmp/ci
(cd /tmp/ci/mennotex-bin-* && shasum -a 256 -c SHA256SUMS)
mtx --root /tmp/mtxroot install-binaries /tmp/ci/mennotex-bin-*/*.tar.xz --sums /tmp/ci/mennotex-bin-*/SHA256SUMS
```

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
