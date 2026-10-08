---
type: Playbook
title: Development and testing
description: How to build mtx, run the tests, bootstrap a throw-away installation and compile documents with it.
tags: [playbook, development, testing]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-08T16:40:00Z }
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
run follows), `% prefetch: <engines>` (run `mtx prefetch` first for those engines),
`% requires: <programs>` (skipped unless on PATH; `pstricks-xelatex` needs `gs`).
TeX runs use `-recorder`, so each document's `.fls` lists the files it read. Bibliographies are embedded with `filecontents*` so each document is
self-contained. The corpus covers beamer, biblatex/biber, natbib/bibtex,
fontspec/unicode-math, babel, KOMA-Script, memoir + index, pgfplots/tikz-cd, tables,
theorems, listings/algorithm2e, mhchem/chemfig, Libertinus + microtype,
glossaries, standalone TikZ and moderncv, and since 2026-10-08 (51 documents) also
Japanese (LuaTeX-ja), Arabic and Hebrew (polyglossia), Russian and Greek (babel, legacy
encodings and fonts by name), journal classes (REVTeX, IEEEtran, acmart, elsarticle),
classicthesis, lecture notes with newtx, MusiXTeX with `musixflx`, MetaPost (`mpost` and
luamplib), METAFONT-only fonts (`bbm`, bitmaps via mktexpk), Xy-pic, CircuiTikZ,
TikZ-Feynman (Lua graph drawing), PSTricks on XeLaTeX, Lua packages, OpenType fonts by
name, beamer metropolis, PDF/A (pdfx), CSV data and small puzzles/QR codes. Logs stay in the printed temp
directory. Use a fresh root to measure first-run behaviour; set `MTX_CACHE` to
an existing root's `tlpkg/mtx/cache` to skip re-downloading archives.

# Concurrency test

```bash
tests/run_concurrent.sh /tmp/fresh-root pdflatex 8
```

Starts 8 compiles at once against one fresh installation; passes when all succeed and
`mtx doctor` reports no problem.

# Comparison with a full TeX Live (Phase 2 exit)

Install a full TeX Live 2026 into a scratch directory (not on PATH, `TEXMFHOME`,
`TEXMFVAR`, `TEXMFCONFIG` in the profile pointing into it; `scheme-full` without docs
and sources is 5.0 GiB, about 45 minutes with `install-tl -profile`), then build the
corpus with both and compare:

```bash
DOCS_OUT=/tmp/out-mtx tests/run_documents.sh /tmp/fresh-root xelatex
DOCS_OUT=/tmp/out-tl  tests/run_documents.sh /tmp/texlive/2026 xelatex
tests/compare_texlive.py /tmp/out-mtx /tmp/out-tl xelatex
```

`run_documents.sh` treats a root without `mtx` as a plain TeX Live. The comparison
checks pages, `pdftotext` text, embedded fonts (`pdffonts`), font warnings in the log and
the `texmf-dist` files read (`.fls`); it needs poppler and pypdf. Expected differences
on macOS: XeLaTeX documents that select TeX-tree fonts by name fail or fall back in a
stock TeX Live ([package quirks](/upstream/package-quirks.md)).

# Crash-recovery test

```bash
tests/run_crash.sh /tmp/fresh-root
```

Needs MennoTeX's binaries in the root and the network. Kills mtx during five installs
of font packages: at the crash points `unpacked`, `recorded`, `listed`, `regenerated`
(`MTX_CRASH_AT`, mtx sends itself SIGKILL) and with a real `kill -9` while `cm-super`
unpacks. After each, a lookup of the package's file and if needed another on-demand
install must finish the install; then `mtx doctor` must be clean, the font map in
`pdftex.map`, and pdfLaTeX must compile with the font. The report says what finished
each install ([decision 0009](/decisions/0009-crash-recovery.md)). With binaries
built before the `mtx_interrupted` check, the first three cases are finished by the
next install instead of the lookup.

# Building the TeX Live binaries

Locally (about 20 minutes on 12 cores):

```bash
git clone --depth 1 --branch tags/texlive-2026.1 https://github.com/TeX-Live/texlive-source.git /tmp/tl2026
build/build-texlive.sh /tmp/tl2026
mtx --root /tmp/mtxroot install-binaries /tmp/tl2026/inst/bin/aarch64-apple-darwin*/
```

Or let mtx fetch the newest successful CI build itself (needs `gh` with access to the
private repository; checks the archive's release and `SHA256SUMS`, skips a build that is
installed already):

```bash
mtx --root /tmp/mtxroot install-binaries --github            # or --run <id>, --release <tag|latest>
```

On GitHub: run the **Build TeX Live binaries** workflow by hand (Actions tab,
`workflow_dispatch`; optionally publish a release). It is manual on purpose: macOS
minutes are billed at 10× on a private repo. A run's artifact is on the run's
**Summary** page (Actions → Build TeX Live binaries → the run → "Artifacts" below the
job graph), not in the job log; with `release: true` the files are also under the
repository's Releases. The second run (2026-10-08, run 37750229150, `release: true`)
took 15 min 49 s and published the first Release. The first run (2026-10-07, run
37694133534) took 14 min 16 s (build 13 min), about 150 billed minutes, and produced a
39 MB artifact `mennotex-bin-2026-6a3001880-arm64-darwin`. Fetch and install it:

```bash
gh run download <run-id> -R MennoJ97/MennoTeX -D /tmp/ci
(cd /tmp/ci/mennotex-bin-* && shasum -a 256 -c SHA256SUMS)
mtx --root /tmp/mtxroot install-binaries /tmp/ci/mennotex-bin-*/*.tar.xz --sums /tmp/ci/mennotex-bin-*/SHA256SUMS
```

## Release archives and licenses

The Package step puts the license texts, `kpathsea-ondemand/` and a `licenses/` tree
(every `COPYING*`/`LICENSE*`/`COPYRIGHT*` file of texlive-source, via `git archive` with
`:(icase,glob)` pathspecs) next to the programs; with `release: true` the Publish step
adds `mennotex-src-<release>-<rev>.tar.xz` and appends it to `SHA256SUMS`
([decision 0008](/decisions/0008-licensing.md)). To try that shell without a CI run,
take a shallow clone at the pinned revision and a directory of fake binaries (Mach-O
magic `cf fa ed fe` is enough), then run the two steps' commands by hand with
`TL_SRC`, `BIN_DIR` and `ARCHIVE` set and `GITHUB_ENV=/dev/null`, from a checkout of
this repository. Check with `tar -tJf`, then install the result into a scratch root
with `mtx --root <scratch> install-binaries <archive> --sums SHA256SUMS`.
Measured 2026-10-08 on this Mac: the binary archive (3 real programs) was 8 MB, the
source archive 100 MB, built in 2 min 22 s, single-threaded: Apple's `bsdtar` ignores
`--options xz:threads=0`. `git archive` of a blob-less partial clone fetches blobs
lazily and is far slower than a plain shallow clone. A shallow `git fetch` of
texlive-source (~150 MB) can be dropped by GitHub mid-transfer (`curl 92 HTTP/2 stream
… CANCEL`) and cannot resume; the codeload tarball of the revision can
(`curl -C -`), but its tree is not identical (export-ignore'd files, CRLF in `.bat`
files), so commit it locally only to test the shell, not as the release's source.

# Useful commands

- `mtx doctor`: run it first when something behaves oddly. On this Mac it flags MiKTeX in `/usr/local/bin` shadowing `pdflatex`, `lualatex`, `kpsewhich`, … unless PATH is reduced.

- `mtx log [--problems] [-n N]`: recent installs, and why an install failed or was declined. TeX's log never shows this.
- `mtx config [KEY [VALUE]]`, `mtx config --unset KEY`: settings such as `autoinstall yes|no|ask`. Test `ask` without a terminal with `ask_dialog no` set, so no dialog appears on the user's screen; drive the terminal prompt with `expect`.
- `latexmk` is mtx too: it runs TeX Live's latexmk with `LATEXMKRCSYS=<root>/texmf-mtx/latexmk/LatexMk` (prefetch before each build, failed/declined installs printed when a build fails). `latexmk -commands` shows which program each step really runs; `mtx doctor` warns about ones from another TeX installation. `mtx config auto_prefetch no` turns the prefetch off.
- `mtx gc [--days N] [--dry-run]`: drop packages installed on demand that have not been read for N days (default 90). To test it, age a package by hand: `touch -a -t 202501010000` its files and set its `installed_at` in `installed.sqlite`.
- `mtx which <file> [--format tfm]`: which package provides a file and whether it is installed.
- `mtx info <pkg>`, `mtx list`, `mtx refresh`, `mtx regen`.
- `tlpkg/mtx/mtx.log` in the root records every install and verification.
- `MTX_REPOSITORY=/path/to/local/tlnet` or `file://…` uses a local repository; `MTX_CACHE` moves the archive cache.

# Debugging kpathsea

```bash
kpsewhich -var-value=TEXMFVAR
kpsewhich -debug=-1 foo.sty
```
