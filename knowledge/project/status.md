---
type: Status
title: Roadmap status
description: Which phases of PLAN.md are done, in progress, or open, with measured results.
tags: [roadmap, status]
status: stable
stale_after: 2026-11-07T00:00:00Z
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-07T13:00:00Z }
verified:
  - { by: process:cargo-test, at: 2026-10-07T09:30:00Z }
---

# Phase 0: spike on unmodified binaries (in progress)

| Item | State |
|---|---|
| tlpdb parser, depend closure | done, tested |
| `files.idx` writer/reader | done, tested ([format](/architecture/files-idx.md)) |
| Signature verification of the tlpdb | done, tested against a real tlnet signature |
| Mirror pinning, freshness TTL, offline marker, bad-mirror avoidance | done |
| Installer (download → lock → unpack → ls-R → config regen) | done |
| tlmgr-compatible `fmtutil.cnf` / `updmap.cfg` / `language.*` | done, tested |
| `mtx bootstrap` against live tlnet | works: 112 packages, 4,096 files, ~36 s |
| `pdflatex` hello world | works: format built on demand, real Type 1 CM fonts embedded |
| `mktextex` on-demand install | works: `atveryend`, `atbegshi` installed during the `pdflatex.fmt` build |
| Mirror failover on network/TLS errors | works live (expired certificate on `mirror.lyrahosting.com`) |
| Exit criterion: article with amsmath, tcolorbox, siunitx from an empty tree | **met** for pdfLaTeX and XeLaTeX (first run); LuaLaTeX needs one rerun |
| Hooks as multi-call symlinks to mtx | done: miss 5.9 ms (was 16.9 ms with a shell wrapper) |

# Measurements

- Bootstrap (2026-10-07): 112 packages / 29.4 MiB planned; root is 197 MiB on disk,
  mostly universal (arm64 + x86_64) engine binaries.
- `pdflatex hello.tex` from a fresh bootstrap: 11.2 s first run (format build + 2 installs), 0.12 s warm.
- The first mirror chosen by `mirror.ctan.org` served a corrupt
  `amsfonts.tar.xz` (4 bytes too long); see [tlnet](/upstream/tlnet.md).

## Test document `tests/documents/article-tcolorbox.tex` (fresh root each, 2026-10-07)

| Engine | First invocation | Packages installed on demand | Warm run |
|---|---|---|---|
| pdflatex | ok, 30 s for two runs incl. format build | 43 (21 requested + 22 declared dependencies) | 0.1–1 s |
| xelatex (after `mtx install xetex`) | ok, 17 s | 39 | 3 s |
| lualatex | **fails once**: `module 'pdftexcmds' not found`; second invocation ok | 9 + 32 | 3–4 s |

The LuaLaTeX failure is the Phase 0 gap predicted in [decision 0002](/decisions/0002-phase0-stock-hooks.md):
a package installed for its `.sty` also ships a `.lua` file, but the running
process's kpathsea only learned the one returned path, and Lua `require`
lookups use `must_exist=false`, so no hook fires.

Hook cost: a miss (file in no package, e.g. `hyperref.cfg`) costs 5.9 ms after warm-up,
close to the 3.5 ms floor of spawning any process; a warm pdfLaTeX run of the
test document still makes 18 such calls.

# Phase 1: kpathsea patch and own arm64 build (exit criterion met; CI and binary channel open)

| Item | State |
|---|---|
| Resolver `mtx-ondemand.c` + patch | **works**: all three engines compile the test article on the first run ([decision 0003](/decisions/0003-kpathsea-patch-shape.md)) |
| `mtx ensure --package/--path --siblings` | done |
| `mtx install-binaries`, `mennotex-binaries` protection, kpathsea hook mode | done |
| `build/build-texlive.sh` | release build of `tags/texlive-2026.1` with the patch: exit 0 in 20 min, 153 arm64 Mach-O programs (stripped), "TeX Live 2026" banners, minimum macOS 13.0, no non-system dylibs; kpathsea's own tests 10/10 ([build notes](/upstream/texlive-build.md)) |
| Document corpus (17 documents, [playbook](/playbooks/development.md)) | **all pass on the first run from a fresh root with the 2026.1 release binaries**: pdfLaTeX 16/16 (42 s for the corpus), XeLaTeX 17/17 (47 s, starting from the `xelatex` shim), LuaLaTeX 17/17 (74 s); archives from a warm cache |
| CI build on GitHub Actions | `.github/workflows/build-binaries.yml`, **manual trigger only** (decided by the user: macOS minutes on a private repo are billed at 10×); not run yet. Packaging step dry-run locally: 39 MB archive of 153 programs + `SHA256SUMS` |
| Binary channel | `mtx install-binaries` takes a directory or a release archive (`--sums SHA256SUMS`); the workflow can publish a GitHub Release. No signature yet: the repository is private, so release downloads need GitHub auth anyway |
| Real installation on this Mac | deferred by the user; all testing in scratch roots |

## Phase 1 measurements (2026-10-07, trunk build + patch, fresh root per engine, cached archives)

| Engine | Phase 0 first run | Phase 1 first run | Install calls (run 1 / run 2) |
|---|---|---|---|
| lualatex | failed, needed a rerun | ok, 9 s, 41 packages | 16 / 0 |
| pdflatex | ok, 30 s, 43 packages | ok, 12 s, 43 packages | 20 / 0 |
| xelatex | ok, 17 s, 39 packages | ok, 12 s, 39 packages | 15 / 0 |

- Only `mktexfmt` (format builds) still spawns a process; lookups of files that no
  package has are answered in-process (whole `kpsewhich` run: 6 ms installed, 7 ms
  no-package miss, vs 2 ms for `/usr/bin/true`).
- `kpsewhich tcolorbox.sty` (a `must_exist=false` lookup) installs tcolorbox and 7
  dependencies, 0.26 s with cached archives.

# Phase 4 items done early

- Fonts by name ([decision 0005](/decisions/0005-fonts-by-name.md)): corpus document
  `fontspec-by-name` passes on XeLaTeX (first run) and LuaLaTeX (with prefetch).
  Corpus now 18 documents: pdfLaTeX 16/16, XeLaTeX 18/18, LuaLaTeX 18/18 on fresh roots.

# Phase 2 items done early

- Install journal and recovery of interrupted installs (tested).
- Concurrency (`tests/run_concurrent.sh`): 8 simultaneous pdfLaTeX runs on a fresh root. The first committed run exposed a format race (TeX Live's mktexfmt overwrites `pdflatex.fmt` in place; 1 of 8 runs failed to undump it); fixed by MennoTeX's own `mktexfmt`. Since then 2/2 runs pass (14–15 s, every run waited for the install lock, `mtx doctor` clean), and the corpus still passes on all engines, in both hook modes.
- `mtx update [--dry-run]`: upgrades outdated packages, keeping each package's reason.
- `mtx prefetch doc.tex` (Phase 3 item): on the test article, 39 packages in one batch (2.3 s, cached archives); the following pdfLaTeX run needed 6 on-demand installs instead of 43 (`tcolorbox` libraries load `listings`, `tikzfill`, `pdfcol` indirectly; `ec` comes from font loading).
- `mtx doctor` (Phase 3 item): detects the MiKTeX symlinks in `/usr/local/bin` that shadow MennoTeX on this Mac.

# Phases 2–4

Otherwise not started. See `PLAN.md` §7.
