# MennoTeX: design and implementation plan

A TeX distribution for Apple Silicon. It behaves like MacTeX or TeX Live, but installs packages **on the fly**, the way MiKTeX does, instead of shipping 5–8 GB up front.

*Written 2026-10-06. Based on these sources:*
- *[TeX-Live/texlive-source](https://github.com/TeX-Live/texlive-source) @ `7cd76c1`*
- *[MiKTeX/miktex](https://github.com/MiKTeX/miktex) @ `76d1b3b`*
- *the live TeX Live 2026 package database (`texlive.tlpdb`, revision 80505)*
- *CTAN's file index and JSON API*

*File references such as `tex-file.c:1131` point into those trees.*

---

## 0. Summary

| Question | Answer |
|---|---|
| Base it on MiKTeX or TeX Live? | Use **TeX Live** for engines and packages. Copy **MiKTeX's idea**, not its code. See §3. |
| Where do packages come from? | TeX Live's network repository (`tlnet`). It lives on the CTAN mirrors and is updated from CTAN every day. Raw CTAN is an optional extra channel, not the base. See §4. |
| How does "install on the fly" work? | One small patch to **kpathsea**, TeX Live's file-lookup library. On a lookup miss, it checks a memory-mapped index (file → package) built from `texlive.tlpdb`. On a hit, it runs `mtx` to install the package, then returns the path. A miss costs microseconds. See §5.4. |
| Why not use TeX Live's existing `mktextex` hook? | It only fires when `must_exist=true`. `\openin`, LuaTeX's `kpse.find_file`, and every font, map, encoding and CMap lookup pass `false` (§2.2). It also inserts only one file into the in-memory cache, and costs a fork on every miss. |
| What gets written new? | `mtx`, a Rust package manager plus CLI. A ~400-line C patch to kpathsea. Build scripts for texlive-source on arm64. |
| Is the index always current? | Yes. Every network operation first does a 144-byte freshness check, and only re-downloads the 2.7 MB database when it has changed (§4.3). |
| Rough size of the job | About 2–3 months of focused solo work to reach a daily-driver (§7). A useful prototype is possible in the first week. |

---

## 1. Goals and non-goals

**Goals**
1. Native arm64 binaries for every TeX engine and tool. No Rosetta.
   *Context:* your current MiKTeX 22.1 is an `x86_64` build that runs under Rosetta (`file /Applications/MiKTeX Console.app/Contents/bin/miktex-tex`). Apple has said that full Rosetta support ends after macOS 27.
2. A small first install (≈ 40–60 MB). Everything else is installed when a document first needs it.
3. Packages, formats and fonts behave exactly as they do in TeX Live, so documents compile the same way as on MacTeX or Overleaf.
4. Installing needs no `sudo`. It works for `pdflatex`, `xelatex`, `lualatex`, `latexmk`, `biber`, `dvipdfmx`, and editors (TeXShop, VS Code LaTeX Workshop, TeXstudio).
5. Safe under concurrency: `latexmk -pvc`, parallel builds, and two editors at once.
6. Verified downloads: signed package database and SHA-512 per archive.

**Non-goals (for now):** Windows or Linux; a GUI console (CLI first); the X11 tools (xdvi); building packages ourselves from CTAN sources.

---

## 2. What the sources tell us

### 2.1 How MiKTeX does on-the-fly installation

MiKTeX does **not** use kpathsea. It ships emulation layers (`kpsemu`, `w2cemu`) that route every engine's file lookups into its own `Session::FindFile` (C++).

**The core trick is a virtual TEXMF root**, called "MPM" (MiKTeX Package Manager):
- **The index.** MiKTeX builds an ordinary file-name database for an imaginary root (`//MiKTeX/]MPM[`). It lists **every file of every package in the repository**, whether installed or not, and the info field of each record is the package id. See `PackageManagerImpl.cpp:429-526`, `Core/internal.h:150-177`.
- **Search order.** The search-path token `%R` expands to "all real roots, then the MPM root last" (`Core/Session/searchpath.cpp:40-66`). A lookup therefore only reaches the MPM root after the file was not found anywhere real.
- **The trigger.** When the search lands on an MPM path, `CheckCandidate` calls `callback->InstallPackage(packageId)` and rewrites the path into the real install root (`Core/Session/findfile.cpp:48-72`). The callback is `Application::InstallPackage` (`Libraries/MiKTeX/App/app.cpp:640-730`).
- **Formats.** A missing `.fmt` triggers `miktex formats build`. A format is rebuilt when the package database is newer than it (`findfile.cpp:348-379`).
- **Font maps.** After every install, `miktex fontmaps configure` and `fc-cache` run synchronously, in the middle of the TeX run (`PackageInstallerImpl.cpp:1786-1835`).
- **The repository.** A signed `mpm.ini` lists each package with the MD5 of its archive. A signed `package-manifests.ini` lists files and `require[]` dependencies. Archives are `.tar.lzma` files. Mirrors are picked by a REST service at `api2.miktex.org` (`RestRemoteService.cpp`). Signatures are RSA/SHA-256 over a canonicalized INI parse (`Cfg/Cfg.cpp:350-453, 687-720`).
- **Install policy.** `AutoInstall` takes `t`, `f` or `?`. When there is no GUI, "ask" silently means **no** (`UI/include/miktex/UI/UI.h:99-109`). Binaries are never installed by the package manager on macOS; they ship inside the `.app` bundle.

**What to copy:**
- the "virtual root of everything not yet installed" model
- lazy format building
- per-process memory of declined or failed packages
- user-scope installs that need no admin rights

**What to avoid** (all checked in the source):
- **Racy lock.** The lock file is check-then-create with no `O_EXCL` (`Core/LockFile/LockFile.cpp:118-126`). It is held **during downloads** with a fixed 10 s timeout (`PackageManagerImpl.cpp:99-113`), so a second TeX run that needs a package while a slow download is running simply fails.
- **No atomic writes.** INI databases are rewritten in place, with no temp-file-plus-rename and no transaction.
- **Weak link to archives.** MD5 is the only link between the signed manifest and the archives.
- **No tar path sanitization** (`Archive/TarExtractor.cpp:148-161`).
- **Expensive index loading.** Every process copies the whole repository-wide index from its mmap into an `unordered_multimap` (`Core/Fndb/FileNameDatabase.cpp:392-407`).
- **Global, synchronous post-processing** (maps and `fc-cache`) after every single on-the-fly install, even when the package has no fonts.
- **A hard dependency on a central API** for choosing a mirror, re-checked on every install.

### 2.2 How TeX Live finds files, and why its built-in hook falls short

Every TeX Live program finds files through **kpathsea**, and kpathsea is **statically linked** into each binary.

**The main lookup**, `kpathsea_find_file_generic` (`kpathsea/tex-file.c:1015`), works in four steps:
1. Build candidate names (name, name + suffixes).
2. Search the path. Trees marked `!!` are searched **only** through their `ls-R` file database (`texmf.cnf:118`).
3. If `must_exist`, search the disk directly.
4. If still nothing and `must_exist`, call `kpathsea_make_tex` (`tex-file.c:1131-1138`).

**`kpathsea_make_tex`** (`tex-make.c:448`) runs an external `mktex*` script, reads the resulting path from its stdout, and calls `kpathsea_db_insert` (`tex-make.c:431-434`). That function adds the one file to the **in-memory** hash only (`db.c:199-215`).

**Formats that have such a script** (`tex-file.c:529-664`):
- `pk`/`gf`/glyph → `mktexpk`
- `tfm` → `mktextfm`
- `fmt`/`base`/`mem` → `mktexfmt`
- `mf` → `mktexmf`
- `ocp`, `ofm`
- `tex` → `mktextex`, which is **disabled by default** (`MKTEXTEX = 0`, `texmf.cnf:773`; `texmfmp.c:1117`)

**Why simply setting `MKTEXTEX=1` and shipping our own `mktextex` is not enough:**

| Lookup | `must_exist` | Hook fires? |
|---|---|---|
| `\input`, TFM loading | true | yes |
| pdfTeX/XeTeX `\filesize`. Modern LaTeX tests file existence with this, via `\file_full_name:n` → `\tex_filesize:D` in `expl3-code.tex`. It reaches `find_input_file` → `kpse_find_tex` (`texmfmp.c:3440-3483`, `tex-file.h:151`). | true | yes |
| `\openin`, which is how `\IfFileExists` worked in older LaTeX (`openclose.c:299-308`) | **false** | no |
| LuaTeX `kpse.find_file` (`lkpselib.c:644`), including luaotfload and LuaLaTeX existence checks | **false** | no |
| pdfTeX `.pfb`/`.ttf` font files (`pdftexdir/mapfile.c:900-902`) | **false** | no |
| LuaTeX `.enc`/`.map`/`.pfb`/`.otf`/`.vf` (`luatexdir/tex/texfileio.c:159-191`) | **false** | no |
| dvipdfmx maps and CMaps (`dvipdfm-x/dpxfile.c:566-642`) | **false** | no |
| XeTeX OpenType font *by file name* (`xetexdir/XeTeX_ext.c:1186`) | **false** | no |

Three further problems with the stock hook:
- **Font names.** XeTeX font lookup *by family name* goes through CoreText (`XeTeXFontMgr_Mac.mm`), not kpathsea at all.
- **Only one file is cached.** After installing a package, kpathsea inserts only the requested file into its in-memory hash. For `pgf`, with about 600 files, every later sibling lookup misses again.
- **A fork on every miss.** It forks a process even for names that no package provides (LaTeX probes for many optional `.cfg` files).

**Useful facts for the design:**
- kpathsea's `ls-R` parser accepts repeated directory blocks (`db.c:89-160`). That is how `mktexupd` appends, so incremental `ls-R` updates are legal.
- `texmf_casefold_search = 1` (`texmf.cnf:807`): lookups fall back to case-insensitive matching. The index must do the same.
- Program-specific paths such as `TEXINPUTS.pdflatex-dev` (`texmf.cnf:221-222`) mean the same basename can resolve differently per program. The resolver must apply kpathsea's own path matching.

### 2.3 The numbers (from `texlive.tlpdb`, TeX Live 2026, revision 80505)

| | |
|---|---|
| Real packages (excluding collections, schemes and per-arch binary packages) | 4,912 |
| Runtime archives, all packages (xz) | **1.47 GiB**. Median **4 KiB**, p90 96 KiB, p99 6.8 MiB, max 77 MiB (junicode) |
| Documentation archives (xz) | **3.49 GiB**, 70% of everything. Never download these unless asked. |
| Runtime files | 185,783, with 184,862 distinct basenames |
| Basenames provided by more than one package | **375 (0.20%)**. Mostly `*-dev` packages (separated by program paths) and identical generated `.enc` files. |
| File → package index (xz) | **0.40 MiB** by basename; 0.53 MiB with full paths |
| `texlive.tlpdb.xz` | 2.78 MB (20.7 MB unpacked); `.sha512` 144 B; `.sha512.asc` 488 B |
| Post-install directives | `addMap` 339, `addMixedMap` 46, `addKanjiMap` 11, `AddFormat` 56, `AddHyphen` 88 |
| Packages with `depend` lines | 518 of 4,912. Dependencies are mostly implicit, so file-level resolution is the main mechanism. |
| Hyphenation packages | 70, totalling **1.7 MiB**, so install them all up front (see §5.7) |
| `universal-darwin` binary packages | 249, totalling 242 MiB. `biber` alone is 68 MiB. |
| Scheme sizes (runtime, xz) | basic 139 packages / 33 MiB · small 429 / 226 MiB · medium 1,635 / 497 MiB · full 5,203 / 1,752 MiB |

**Consequences:**
1. **Latency is network round-trips, not bandwidth.** The median package is 4 KiB, so what matters is connection reuse, parallel fetching and prefetching.
2. **The whole index is tiny.** Ship it and rebuild it locally; no server is needed.
3. **Ambiguity is a non-problem** once kpathsea's path rules are applied.

### 2.4 MiKTeX on macOS today, and what an arm64 build would take

**How MiKTeX builds its engines** (this is the maintenance cost you would inherit with Option A):
- **kpathsea API emulation.** Every `kpathsea/*.h` header is generated from one template, `KPathSeaEmulation/include/kpathsea/kpathsea.h.in`. Calls become `miktex_kpathsea_*` functions in `kpsemu.cpp` (1,185 lines). For example, `kpse_find_file` → `Session::FindFile` (`kpsemu.cpp:237-258`).
  - There is no `texmf.cnf` at all. Variables are synthesized from session state (`kpsemu.cpp:700-866`).
- **WEB engines (TeX, pdfTeX, XeTeX, BibTeX)** are not built with web2c's `convert`. MiKTeX uses its own Pascal→C++ translator, **C4P** (`BuildUtilities/c4p`). It also layers "adapter" change files so that TeX Live's own `pdftex.ch` applies unmodified (`Programs/TeXAndFriends/pdftex/pdftex/webify.cmake:50-115`).
  - The MiKTeX-specific change files are large: `miktex-tex.ch` alone is 3,375 lines.
- **C engines** (LuaTeX, dvipdfmx, dvips, MetaPost) are vendored and patched in place with `#if defined(MIKTEX)`. For example, LuaTeX has 145 such lines across 48 files.

**macOS bundle and the arm64 situation:**
- **Layout.** MiKTeX is shipped as `MiKTeX Console.app` with `Contents/{bin,libexec,miktex-lib,texmf}`. Per-user data lives in `~/Library/Application Support/MiKTeX/texmfs/{install,config,data}`; the system-wide equivalent is `/Library/…`.
- **Renaming the app breaks it.** Binaries find their prefix by string-stripping the hard-coded `MiKTeX Console.app/Contents/bin` (`Core/Session/config.cpp:94-118`).
- **Intel-only build assumptions:**
  - Homebrew detection checks only `/usr/local/Cellar` (`CMakeLists.txt:154-157`). Apple Silicon Homebrew lives in `/opt/homebrew`.
  - The bundle fixup has hard-coded `/usr/local/opt/{icu4c,poppler-qt5}/lib` paths, marked FIXME (`Programs/MiKTeX/Console/Qt/CMakeLists.txt:259-262`).
  - The deployment target is `10.12` (`CMakeLists.txt:23-28`), below arm64's minimum of 11.0.
  - The in-tree GMP refuses any non-Windows target (`Libraries/3rd/gmp/CMakeLists.txt:17-23`), so a self-contained build needs system GMP.
- **Dependencies.** The Unix build takes most of them from the system: Qt 6, Boost, ICU, curl, OpenSSL, freetype, and so on. `fixup_bundle` copies Homebrew dylibs into the app.
- **No public build CI.** `.github/workflows` contains only `stale.yml`. There is also no notarization anywhere in the repository.
- **XeTeX font lookup.** MiKTeX's XeTeX on macOS uses its own **fontconfig**, not CoreText (`Programs/TeXAndFriends/xetex/CMakeLists.txt:84-85`). Font lookups *by name* never auto-install there either; only bracketed file names do.
- **No CPU-specific blockers.** There is no x86 assembly in MiKTeX's own code, and LuaJIT is built only on Windows.

**Conclusion.** An arm64 MiKTeX is probably reachable with a few days of CMake surgery against arm64 Homebrew dependencies. That is consistent with your installed build being `x86_64`. But you would own a downstream fork of a large C++ code base, with its own translator and roughly 10k lines of change files, held together by Homebrew dylibs. That is a poor base for a distribution you control.

---

## 3. Options considered

| Option | Description | For | Against | Verdict |
|---|---|---|---|---|
| **A. Build MiKTeX for arm64** | Compile the existing MiKTeX for arm64 and use its package repository | On-the-fly already works | Needs fixes for Intel-only Homebrew paths, Cellar detection and the deployment target (§2.4); you own a fork with a custom Pascal→C++ pipeline, roughly 10k lines of change files and Homebrew-copied dylibs; one upstream maintainer; macOS is a secondary platform; you inherit the weaknesses in §2.1 | Worth a weekend experiment if all you want is the result. Not a base for your own distribution. |
| **B. TeX Live engines + new package manager + kpathsea patch** | Build texlive-source natively; write `mtx`; patch kpathsea | Upstream engines with a tiny patch; packages from tlnet (signed, daily, mirrored worldwide); the new code is small and entirely yours | You build and sign the binaries yourself; yearly release transitions | **Chosen** |
| **C. Unmodified TeX Live binaries + `mktex*` hooks only** | `MKTEXTEX=1`, custom `mktextex`/`mktextfm` scripts | No C changes; can use tlnet's own `universal-darwin` binaries, which are already native arm64 | Misses fonts, maps, `\openin` and all LuaTeX lookups (§2.2); a fork per miss | **Phase-0 stepping stone only** |
| **D. Virtual filesystem** | A File Provider or FSKit volume whose `ls-R` lists all 185k files; content arrives on `open()` | No patches at all | `\IfFileExists` is true for everything; font scanners (luaotfload, fontconfig) would materialize gigabytes; needs a signed app extension with entitlements; per-file rather than per-package fetching; fragile | Rejected |

---

## 4. Where packages come from: tlnet versus CTAN

*Answers to "can we couple it to CTAN instead of the TeX Live database? Is that necessary? Can it update when the package manager runs?"*

### 4.1 Coupling to raw CTAN is not necessary, and mostly not possible

- **tlnet is CTAN, already packaged.** It lives on every CTAN mirror (`/systems/texlive/tlnet`). TeX Live imports CTAN uploads every day.
  - *Measured:* of the 36 TeX Live packages that CTAN updated in the last 7 days, **29 were already in tlnet**. The 7 that weren't had been uploaded within about the last 2 days. The lag is roughly 1–2 days.
- **Most of CTAN is not installable as-is.** CTAN lists **7,094** packages, but only **546** ship a ready-to-install TDS zip (`/install/**.tds.zip`), about 8%.
  - The rest are author source bundles. They need `docstrip` runs (`.ins`/`.dtx`), file placement into the TeX directory structure, font-map registration, and so on.
  - That per-package packaging is exactly the curated work TeX Live (`tlpsrc` files) and MiKTeX (its own packaging pipeline) do daily for about 5,000 packages. Redoing it is a full-time job.
- **Raw CTAN adds little coverage.** Of the 546 TDS zips, **467 are for packages already in TeX Live**. The 79 others are mostly support files for commercial or non-free fonts (`adobecaslon`, `bickham`, `lsabon`…), which are useless without the font itself.
- **Trust.** tlnet signs its database with TeX Live's GPG key and gives a SHA-512 for every archive. CTAN's TDS zips have only HTTPS; I found no published signatures or checksums.

### 4.2 Optional CTAN overlay channel (Phase 4)

This is worthwhile for the 1–2 day window, or for one of the few CTAN-only TDS packages:

```
mtx install --from-ctan tcolorbox
```

1. Call `GET https://ctan.org/json/2.0/pkg/<name>`. It returns `version`, `install` (the TDS zip path) and `texlive` (the TeX Live package name).
2. Download the TDS zip into a separate tree, `texmf-ctan/`. It sits ahead of `texmf-dist` in `TEXMF`, so it takes priority.
3. Record it with `source=ctan`. `mtx update` drops the overlay automatically once tlnet's `catalogue-version` catches up.

Overlay packages are never auto-installed on the fly. You choose them explicitly.

### 4.3 Yes: the index refreshes whenever the package manager runs

On any `mtx` operation that touches the network, including every on-the-fly install:

1. **Pin a mirror for the session.** Resolve `mirror.ctan.org` once and stick to that host.
   *Observed:* three consecutive requests were redirected to three different mirrors (lyrahosting, koddos, cicku). Mixing them can pair a database from one sync state with an archive from another.
2. **Freshness check.** If the last check is older than the TTL (default **1 h**, configurable; `mtx update` always checks), conditionally `GET texlive.tlpdb.sha512` (144 B, with ETag).
3. **Refresh if changed.**
   1. Fetch `.sha512.asc` and `texlive.tlpdb.xz` (2.7 MB).
   2. Verify the GPG signature (TeX Live key, shipped with mtx) and then the SHA-512.
   3. Rebuild the local index, which takes about 0.3 s in Rust.
   4. Swap it in atomically with a rename, so running TeX processes keep their old mmap.
4. **Recover from mirror churn.** tlnet archive names are **unversioned** (`archive/<pkg>.tar.xz`). If a downloaded archive's SHA-512 does not match the database, the mirror changed underneath us. Re-fetch the database from the same mirror and retry once; if it still fails, switch mirrors. MiKTeX does the same for MD5 mismatches (`PackageInstallerImpl.cpp:1065-1070`).
5. **Offline.** If the network is down, use the cached index, fail quickly (3 s connect timeout), and write an `offline-until` marker. Further lookups in the same compile then fail instantly instead of each waiting.

**Refreshing the index is not the same as upgrading installed packages.** On-the-fly installs always fetch the current revision, but already-installed packages stay where they are until you run `mtx update`.
- Mixing very new and old packages can break things, because tlnet assumes "update everything together".
- **Policy:**
  - When `mtx` installs a package, it also upgrades the already-installed packages in that package's `depend` closure, plus `l3kernel`, `l3backend` and `latex`, whenever the newly installed package is newer than they are.
  - `mtx update` upgrades everything.
  - An optional weekly `launchd` job runs `mtx update --quiet`.

**Yearly release transition.** `00texlive.config` declares `release/2026`. When tlnet moves to 2027, packages may require 2027 engines.
- `mtx` detects the new release number, stops auto-upgrading, and pins to the frozen `historic/systems/texlive/2026/tlnet-final` repository.
- It then offers `mtx upgrade-release`, which installs 2027 binaries side by side (`~/Library/MennoTeX/2027/`).
- *As built* ([decision 0006](knowledge/decisions/0006-release-transitions.md)): the pin is automatic inside `refresh`; moving up is the next release's MennoTeX build running `mtx bootstrap --from <old root>` side by side, not a command of the old mtx.

---

## 5. Architecture

### 5.1 Components

```
 ┌───────────────────────── TeX programs (our arm64 build of texlive-source) ──────────────────────┐
 │  pdftex  xetex  luahbtex  luatex  dvipdfmx  dvips  bibtex  makeindex  mpost  kpsewhich  …       │
 │  └── libkpathsea (static) + PATCH: ondemand.c                                                   │
 │        miss ─► mmap files.idx ─► candidate(pkg, relpath) ─► path-match ─► on disk? ─► return     │
 │                                                                 └─ no ─► exec `mtx ensure …`    │
 └──────────────────────────────────────────────────────────────────────────────┬──────────────────┘
                                                                                │ stdout: paths
 ┌────────────────────────────────────── mtx (Rust, one binary) ────────────────▼──────────────────┐
 │ repo    : mirror pinning · tlpdb fetch · GPG + SHA-512 verification · HTTP/2 connection pool     │
 │ index   : tlpdb → files.idx (mmap, C-readable) + pkgs.idx                                        │
 │ install : flock'd commit · staging + rename · journal · ls-R append · installed.db              │
 │ actions : maps (updmap) · formats (fmtutil.cnf, lazy build) · language.dat/.def/.lua · luaotfload│
 │ bins    : MennoTeX binary channel (signed manifest) + script shims                              │
 │ cli     : ensure · install · remove · update · search · which · info · prefetch · doctor · gc   │
 └─────────────────────────────────────────────────────────────────────────────────────────────────┘
```

**Language choices:**
- **`mtx` in Rust.** One static binary; mature crates for xz, tar, SHA-2, OpenPGP (rPGP or Sequoia), HTTP with rustls, SQLite, mmap and flock; parses the tlpdb in milliseconds.
- **The in-engine shim in plain C.** That keeps TeX Live's autotools build untouched: no Cargo inside the engine build, and no Rust runtime in every engine.
- Swift and SwiftUI remain an option for a later menu-bar app.

### 5.2 On-disk layout (per user by default; no sudo)

The layout mirrors TeX Live's, so `SELFAUTOPARENT` logic and every stock script (fmtutil, updmap, texdoc, luaotfload-tool) work unmodified.

```
~/Library/MennoTeX/2026/                 ← TEXMFROOT (= bin/<arch>/../..)
  bin/universal-darwin/                  pdftex, xetex, … (ours, arm64) + mtx + shims for scripts
  texmf.cnf                              small overrides (MTX_* variables, TEXMFVAR location)
  texmf-dist/                            packages installed on demand; ls-R maintained by mtx
  texmf-ctan/                            optional CTAN overlay (§4.2)
  texmf-var/                             generated: web2c/<engine>/*.fmt, fonts/map/…, luatex-cache
  texmf-config/                          generated: updmap.cfg, fmtutil.cnf, language.*
  tlpkg/
    texlive.tlpdb.xz, .sha512, .sha512.asc   cached upstream database (verified)
    mtx/files.idx  pkgs.idx                  derived, memory-mapped by every TeX process
    mtx/installed.sqlite                     installed packages, revisions, files, origin
    mtx/journal/                             in-flight transactions (crash recovery)
    mtx/lock                                 flock target
~/Library/Caches/MennoTeX/archives/      downloaded .tar.xz (LRU; also enables offline reinstall)
```

*As built* (details in [knowledge/architecture](knowledge/architecture/)):
- The tlpdb copy, `files.idx` and the archive cache all live in `tlpkg/mtx/` (cache: `tlpkg/mtx/cache`, or `$MTX_CACHE`); there is no `~/Library/Caches` directory and no `pkgs.idx`.
- Generated files go to `texmf-var/` (formats, maps) and `texmf-user-var/`, `texmf-user-config/` (TeX Live's scripts refuse `TEXMFVAR == TEXMFSYSVAR`).
- `texmf-mtx/` is mtx's overlay tree, searched before all others through `TEXMFAUXTREES`; today it holds a `luaotfload-main.lua` that makes fonts selected by name work on LuaLaTeX's first run.

**Naming the binary directory.** The binary directory keeps TeX Live's platform name, `universal-darwin`, even though we only build arm64. Tooling and tlpdb binary package names (`biber.universal-darwin`) assume it. If you prefer an honest name such as `arm64-darwin`, add a mapping in `mtx`.

**Integration with editors and other TeX installs (optional, needs admin once):**
- Register a MacTeX-style `TeXDist` structure (`/Library/TeX/Distributions/MennoTeX-2026.texdist`) and point `/Library/TeX/texbin` at it. GUI editors look there, so they find MennoTeX the same way they find MacTeX.
- `mtx doctor` warns about other TeX installs that shadow ours. On this Mac, MiKTeX's symlinks in `/usr/local/bin` (`tex → MiKTeX Console.app/…`) would do exactly that.

### 5.3 Lookup → install sequence

Example: `\usepackage{tcolorbox}`.

```
pdflatex: \file_full_name:n {tcolorbox.sty} → \filesize → kpse_find_tex("tcolorbox.sty")
 kpathsea: ls-R miss in every TEXMF tree
 ondemand: files.idx["tcolorbox.sty"] → [(pkg=tcolorbox, tex/latex/tcolorbox/tcolorbox.sty)]
           path-match against TEXINPUTS.pdflatex ✓   exists on disk? ✗
           exec: mtx ensure --format=tex --name=tcolorbox.sty --pkg=tcolorbox --progname=pdflatex
 mtx:      policy check (auto / ask / never) → freshness check (TTL) → plan:
             tcolorbox + speculative prefetch of statically detected \RequirePackage targets
           parallel download + verify (outside the lock) → flock → commit → ls-R append → unlock
           post-actions (only if the package declares any) → print paths
 ondemand: kpathsea_db_insert() for every printed path → return …/tcolorbox.sty
pdflatex: continues; later lookups of tcolorbox's siblings hit the in-memory hash
```

### 5.4 The kpathsea patch (the only change to TeX Live's C code)

**Where it goes.** In `kpathsea_find_file_generic` (`tex-file.c`), right **before** the existing mktex block at line 1131:

```c
  /* MennoTeX: install missing files on demand. Unlike mktex*, this runs
     regardless of must_exist: \openin, kpse.find_file, and font/map/enc
     lookups all pass must_exist=false. A miss costs one mmap'd lookup. */
  if (! *ret && !all && kpathsea_ondemand_enabled (kpse, format)) {
    string found = kpathsea_ondemand_find (kpse, format, name);
    if (found) {
      free (ret);
      ret = XTALLOC (2, string);
      ret[0] = found;
      ret[1] = NULL;
    }
  }
```

**What the new file `kpathsea/ondemand.c` does** (~400 lines):

1. **Enable check.** `kpathsea_ondemand_enabled` reads `MTX_AUTOINSTALL`. As a kpathsea variable it can be set per program (`MTX_AUTOINSTALL.kpsewhich = 0`) or overridden from the environment.
   It excludes formats that must never trigger installs: `cnf`, `db` (ls-R), `fmt`/`base`/`mem` (handled by `mktexfmt` → `mtx mkfmt`), `pk`/`gf`/glyph (generated), and pool files. The exclusion also prevents recursion during kpathsea's own start-up.
2. **Load the index.** On first use, mmap `$MTX_INDEX` (default `$TEXMFROOT/tlpkg/mtx/files.idx`) and keep it in the `kpse` struct.
3. **Build candidates** from the name and the format's `suffix`/`alt_suffix` lists, using the same logic as `target_suffixed_names`.
4. **Look up and filter.** For each candidate, binary-search the index (exact match first, then case-folded). Then filter `(pkg, relpath)` hits by matching `$TEXMFDIST/relpath` against the format's expanded path elements, reusing `db.c`'s `match()` (expose it as `kpathsea_path_elt_match`). This one step resolves almost all of the 375 ambiguous names.
5. **Already on disk?** If the file exists (another process installed it, or it is a sibling of a package installed earlier in this run), call `kpathsea_db_insert` and return. No exec.
6. **Otherwise exec** `mtx ensure …`, using the fork/exec pattern of `maketex()` in `tex-make.c:296-406`. Read stdout: line 1 is the requested file and the following lines are the other installed files. `db_insert` all of them.
7. **Per-process negative cache** of names that failed (declined, offline, error), so a run never asks twice (MiKTeX does the same, `app.cpp:642-672`).
8. **Name sanitization.** Reuse `tex-make.c:480-493`: reject names that start with `-` or contain characters outside `[A-Za-z0-9._+-/]`. Font names with spaces never reach `mtx`.

**Why this is robust.** Every engine (pdfTeX, XeTeX, LuaTeX including its Lua `kpse` library, the e-pTeX family, MetaPost, METAFONT) and every tool (dvipdfmx, dvips, bibtex, makeindex, dvisvgm, kpsewhich) reaches files through `kpathsea_find_file_generic`, so one patch covers them all. biber calls `kpsewhich` to find `.bib` files, so it is covered as well.

**Keeping the patch alive.** Maintain it as a quilt-style patch series in this repository, applied to a pinned texlive-source revision. It touches about 15 lines of an existing file plus one new file, so rebasing each year is cheap.

### 5.5 Index format (`files.idx`, written by `mtx`, read by C)

The format is designed for a single mmap, zero parsing, and binary-search lookups (about 18 comparisons for 185k names):

```
Header   magic "MTXIDX\0\1" · u32 version · u64 tlpdb_revision · u32 n_files · u32 n_pkgs · u32 n_dirs
         · offsets of each section · u64 sha256 of the tlpdb it was built from
Keys     n_files × { u64 hash(lowercased basename) , u32 entry_idx }      sorted by hash
Entries  n_files × { u32 basename_off , u32 dir_idx , u16 pkg_idx , u16 flags }
Dirs     n_dirs  × { u32 path_off }                                       "tex/latex/tcolorbox"
Pkgs     n_pkgs  × { u32 name_off , u32 revision , u32 container_size , u32 flags }
                    flags: has_maps · has_formats · has_hyphen · is_binary · is_dev
Strings  NUL-terminated pool
```

*As built:* see [files.idx](knowledge/architecture/files-idx.md). Font *names* (fontspec) are not in it: kpathsea hands names to `mtx ensure --font-name`, which uses a font-name table embedded in mtx ([decision 0005](knowledge/decisions/0005-fonts-by-name.md)).

This comes to about 5–6 MB on disk. It is rebuilt whenever the tlpdb changes and replaced by atomic rename. Generate `.h` constants from the Rust side so the C reader cannot drift.

### 5.6 Installer: transactions and locking

These fix the MiKTeX weaknesses listed in §2.1:

1. **Plan** without any lock: resolve packages, apply the upgrade policy (§4.3), and compute speculative prefetch.
2. **Download** without any lock: parallel requests over one HTTP/2 connection to the pinned mirror; stream SHA-512 checks; write to the cache with temp-file-plus-rename.
3. **Commit** under `flock(tlpkg/mtx/lock)`. Readers never need the lock. The lock is held only for local disk work, so waiting is short. Show "waiting for another mtx…" after 1 s, then keep waiting rather than failing.
   1. Re-check `installed.sqlite`, because another process may have just installed the package.
   2. Write the journal entry `txn-<id>: {packages, files}`.
   3. Extract into `texmf-dist/.staging/<id>/`, with sanitized paths: no absolute paths, no `..`, no symlinks escaping the root. Preserve the executable bit; MiKTeX loses it and has to `chmod` scripts afterwards.
   4. `rename()` each file into place. Files already owned by another package get shared ownership instead of being overwritten.
   5. Append blocks to `ls-R`: write a temp file and rename it, or append with `O_APPEND` plus `fsync`. Compact `ls-R` during `mtx update`.
   6. Update `installed.sqlite` (WAL), then delete the journal entry.
4. **Post-actions** (§5.7), outside the main lock, each under its own lock.
5. **Crash recovery.** At the start of any `mtx` command, replay or roll back leftover journal entries.

### 5.7 Post-install actions (the tlpdb `execute` lines)

| Directive | Count | MennoTeX handling |
|---|---|---|
| `addMap` / `addMixedMap` / `addKanjiMap` | 396 directives | Regenerate `updmap.cfg` from installed packages, then the maps (`pdftex.map`, `psfonts.map`, `kanjix.map`, dvipdfmx). Run it **only** when the transaction contains such a package; MiKTeX runs it after every install. v1 shells out to TeX Live's `updmap.pl` (macOS ships `/usr/bin/perl` 5.34); v2 is a native port. |
| `AddFormat` | 56 | Generate `fmtutil.cnf` from the AddFormat lines of **all** packages in the index, not only installed ones, so `pdflatex` knows how to build `pdflatex.fmt` before `latex-bin` is installed. See the format notes below. |
| `AddHyphen` | 88 (70 packages) | Install **all** hyphenation packages at bootstrap (1.7 MiB), because pdfTeX and XeTeX formats bake patterns in at build time and `babel` cannot add them later. Generate `language.dat`, `language.def` and `language.dat.lua` by porting TeX Live's `create_language_*` logic. |
| fonts for LuaTeX | – | When a transaction adds `.otf`/`.ttf` files, run `luaotfload-tool --update` incrementally, or rely on luaotfload's reload-on-miss. |

**How formats are built:**
- **Building.** A missing `.fmt` makes kpathsea run `mktexfmt` (enabled by default, `MKTEXFMT=1`). Our `mktexfmt` is `mtx mkfmt <name>`. It makes sure the defining package and its `depend` closure are installed, then runs `engine -ini -jobname=… -progname=… <options>` under a per-format lock.
- **No deadlock.** That `-ini` run itself triggers on-the-fly installs (`latex.ltx`, `l3kernel`, …), so **the install lock must not be held while formats build**.
- **Staleness.** Store a stamp next to each format: engine build id, revisions of its `fmttriggers=` packages, and a hash of the hyphenation set. When any of these changes (`mtx update`, a binary update), delete the format and it rebuilds lazily. MiKTeX's equivalent compares timestamps (`findfile.cpp:357-379`).

**Fonts first used after page 1 (known edge case).** pdfTeX and LuaTeX load `pdftex.map` lazily, at the first font they need for output.
- A font package installed while the preamble loads is therefore picked up.
- A font package first installed **after** the first `\shipout` is missing from that run's map. pdfTeX then warns and falls back.
- v1 accepts that the next run is correct (latexmk reruns anyway). v2 adds a ~30-line pdfTeX/LuaTeX patch that reads newly added map files (the internal equivalent of `\pdfmapfile{+x.map}`) when `ondemand.c` reports that maps changed.

### 5.8 Binaries

- **Build from texlive-source natively on arm64** with its `./Build` script, roughly:
  `TL_MAKE_FLAGS=-j12 ./Build --disable-xdvik --without-x`
  - xindy is already off by default (`--enable-xindy` turns it on).
  - asymptote is not part of the default build; take `asymptote.universal-darwin` from tlnet if you need it.
  - Use `MACOSX_DEPLOYMENT_TARGET=13.0`.
  - Every library is vendored in `libs/` (ICU, HarfBuzz, FreeType, cairo, LuaJIT, …), so no Homebrew is needed.
  - Expect 15–30 minutes on this 12-core Mac.
  - The linker ad-hoc signs arm64 output. To distribute to others, sign with a Developer ID and notarize the `.pkg`.
- **CI:** GitHub Actions `macos-15` arm64 runners build on every patch change.
  *As built:* the workflow is manual only (private repository, macOS minutes billed at 10×).
- *As built:* binaries are built from the TeX Live **release branch** matching tlnet (`build/texlive-source.rev`, `tags/texlive-2026.1`), not trunk, which is already next year's development version. `mtx install-binaries` takes a directory or a release archive with `SHA256SUMS`; the signed manifest and `mtx self-update` are not done.
- **Publishing:** a GitHub Release with `bin-<release>-<rev>.tar.xz` and a **minisign/Ed25519-signed** manifest (sizes, SHA-256). This is the "MennoTeX binary channel"; `mtx self-update` and `mtx upgrade-release` consume it.
- **Binaries that are not in texlive-source** (`biber`, and anything else in tlnet's `*.universal-darwin` packages that we don't build) are installed from tlnet like any other package. They are already universal, so native arm64.
  - `mtx` keeps a list of the filenames our own channel provides, and **never** lets a tlnet binary package overwrite them.
- **Script shims.** At bootstrap, `mtx` writes a ~10-line shim for each of the ~500 program names in tlnet's binary packages (`latexmk`, `biber`, `texcount`, …).
  - The first run of a shim installs the owning package, replaces the shim with the real file or symlink, and `exec`s it.
  - This gives `command not found`-free on-demand tools, the CLI counterpart of the kpathsea hook.
  - It matters especially for `biber`, the largest package (68 MiB), which should only download when `latexmk` actually calls it.

### 5.9 Policies and configuration

These go in `texmf.cnf` (readable by C) or `~/Library/MennoTeX/mtx.toml` (CLI-only settings):

| Setting | Values | Default |
|---|---|---|
| `MTX_AUTOINSTALL` | `1` / `0` / `ask` | `1` (install, print one line to stderr, log it) |
| `ask` behavior | TTY → prompt on `/dev/tty`. GUI session with no TTY → `osascript` dialog with a 30 s timeout. Otherwise → the configured fallback. | Fallback is `yes`. MiKTeX's headless "ask" silently means no, which is a known source of confusion. |
| freshness TTL | duration | 1 h |
| `prefetch` | `none` / `depends` / `requires` | `requires` |
| `docs` | `never` / `on-texdoc` / `always` | `on-texdoc`: `texdoc foo` → `mtx` fetches `foo.doc` |

### 5.10 CLI (first version)

```
mtx ensure  --format F --name N [--pkg P] [--progname X]   # called by kpathsea; prints paths
mtx install <pkg|collection|file.sty>…   mtx remove <pkg>…   mtx update [--dry-run]
mtx which <file>        # which package provides it (works before install)
mtx search <text>       mtx info <pkg>       mtx list [--installed|--auto]
mtx prefetch doc.tex    # scan \documentclass/\usepackage/\RequirePackage and install all at once
mtx mkfmt <fmt>         # also installed as `mktexfmt`
mtx doctor              # PATH shadowing, stale formats, ls-R drift, orphan files, journal leftovers
mtx gc                  # drop auto-installed packages unused for N days (needs access tracking)
mtx bootstrap           # first-run: index + core set + hyphenation + shims
mtx self-update · mtx upgrade-release
```

**Bootstrap core set (≈ 40–60 MB xz)**, roughly `scheme-infraonly` plus:
- `kpathsea`, `texlive-scripts`, `latex`, `l3kernel`, `l3backend`, `latex-bin`
- `cm`, `amsfonts`, `lm`, `ec`
- all hyphenation packages
- `graphics-def`, `hyperref`'s required core

Fine-tune this from measurements (§8).

---

## 6. Gotchas checklist

**Lookup and engines**
- [ ] `must_exist=false` lookups must trigger installs (§2.2); the patch ignores the flag on purpose.
- [ ] `\IfFileExists` probes for files that really exist in *optional* packages trigger installs. MiKTeX has the same behavior. Keep a small deny-list in `pkgs.idx` (flag `no_autoinstall`) for known probe-only names, built from corpus measurements.
- [ ] Case-insensitive APFS versus case-sensitive `ls-R`: index lookups use exact-then-casefold, matching `texmf_casefold_search=1`.
- [ ] Recursion: never resolve `cnf`/`ls-R`/`fmt` lookups; set `MTX_IN_ENSURE=1` in `mtx`'s environment so tools it runs (`updmap`, `-ini` builds) only resolve when intended.
- [ ] XeTeX font lookup *by name* bypasses kpathsea (CoreText). See Phase 4.

**Installer and repository**
- [ ] Deadlock: format builds trigger installs, so never hold the install lock across them.
- [ ] Mirror consistency: pin one host per session; handle checksum mismatches by re-fetching the database (§4.3).
- [ ] Partial upgrades: mixing fresh and stale packages. Use the upgrade policy in §4.3.
- [ ] Release year change: pin to `tlnet-final` and offer `upgrade-release`.
- [ ] Never let tlnet's `pdftex.universal-darwin` (unpatched) overwrite our patched binary.
- [ ] Tar safety and file modes (§5.6).

**Formats and fonts**
- [ ] Formats embed hyphenation and `fmttriggers`: invalidate correctly or users see stale behavior.
- [ ] Map files and late font use (§5.7).

**Environment**
- [ ] Offline: fail fast, write the `offline-until` marker, and keep messages clear (`mtx: needs package 'foo' (12 KB); offline`).
- [ ] PATH shadowing: on this Mac, MiKTeX's `/usr/local/bin/tex` symlinks would shadow MennoTeX. `mtx doctor` must flag them.

---

## 7. Roadmap

Effort estimates assume one focused developer and are rough.

### Phase 0: a working spike on unmodified binaries (≈ 1 week)
- Rust workspace: tlpdb parser, `files.idx` writer and reader, mirror pinning, download + SHA-512, xz/tar extraction, `ls-R` writer, `installed.sqlite`.
- `mtx bootstrap` installs a TeX Live layout from tlnet, **including tlnet's own `*.universal-darwin` binaries** (already native arm64). That gives a working "BasicTeX managed by mtx" with no compiling.
- Option C hooks: `MKTEXTEX=1`, plus `mktextex` and `mktextfm` → `mtx ensure`.
- **Exit:** `pdflatex` compiles an article with `amsmath`, `tcolorbox` and `siunitx` from an empty tree. Measure coverage; expect gaps in LuaLaTeX, fonts and maps, which is what proves the need for Phase 1.

### Phase 1: the patched build (≈ 2–3 weeks)
- Scripted texlive-source arm64 build; CI on GitHub Actions; the binary channel with a signed manifest.
- `kpathsea/ondemand.c` plus the patch, with C unit tests against generated index fixtures.
- `mtx ensure` with policies, negative caching and stdout protocol v1.
- **Exit:** pdfLaTeX, XeLaTeX (fonts by file name) and LuaLaTeX compile a 50-document corpus from an empty tree. The second run installs nothing.

### Phase 2: correctness of the system state (≈ 2–3 weeks)
- `updmap.cfg`, `fmtutil.cnf` and `language.*` generation; `mtx mkfmt` with staleness stamps; luaotfload refresh.
- Journal and crash recovery; the concurrency test suite (8 parallel `latexmk` runs on documents that share missing packages); `mtx update` with the upgrade policy.
- Script shims, including biber on demand.
- **Exit:** the corpus compiles identically to a full TeX Live 2026 install (same page count, same `pdftotext` output). A forced crash in the middle of a transaction recovers cleanly.

### Phase 3: daily-driver polish (≈ 2–3 weeks)
- Release-transition handling and `upgrade-release`.
- `mtx prefetch`, `doctor` and `gc`; docs on demand via texdoc.
- The `ask` UI (TTY prompt and `osascript`).
- A `.pkg` installer, optional TeXDist/texbin registration, and an editor smoke test (TeXShop, VS Code LaTeX Workshop, TeXstudio).
- **Exit:** you uninstall MiKTeX and use MennoTeX for a month.

### Phase 4: advanced (open-ended)
- **Font-name index**: a weekly CI job scans the font packages (about 1.3 GB) for family, full and PostScript names and publishes a signed `fontnames.idx`. Hooks:
  - XeTeX: patch `XeTeXFontMgr_Mac.mm` so a miss installs the font and registers it with `CTFontManagerRegisterFontsForURL(…, kCTFontManagerScopeProcess)`
  - LuaTeX: a luaotfload resolver
- The CTAN overlay channel (§4.2).
- The in-run map reload patch (§5.7).
- A server-built `\RequirePackage` dependency graph for better prefetching.
- A menu-bar app; Linux arm64.

---

## 8. Testing and metrics

**Corpora:**
1. About 300 package-documentation `.tex` sources from tlnet's doc archives. They are designed to compile.
2. About 200 arXiv source bundles, kept locally only.
3. Common templates: `beamer`, `moderncv`, `memoir`, university thesis classes, `biblatex` with `biber`.

**Baseline:** the same documents compiled with a full TeX Live 2026 install (scheme-full) in a temporary directory.

**Measure per document:**
- first-run success
- packages installed
- MB downloaded
- first-run versus warm-run wall time
- PDF equivalence (page count, `pdftotext` diff, optionally rasterized diff)

**Targets:**
- ≥ 99% first-run success on the corpus
- median on-the-fly overhead < 300 ms per package
- warm runs indistinguishable from TeX Live

**Other test suites:**
- **Unit tests:** tlpdb parsing, the index round-trip (Rust writer to C reader), path matching against kpathsea's own `match()`, and tar sanitization.
- **Concurrency and fault injection:** kill `mtx` at every step of a transaction; serve corrupted archives; switch mirrors in the middle of a session; simulate offline.

---

## 9. Proposed repository layout

```
MennoTeX/
  PLAN.md
  crates/
    mtx/              CLI (clap)
    mtx-core/         tlpdb, index, repo client, installer, actions
    mtx-index/        files.idx format (+ generated C header)
  kpathsea-ondemand/
    ondemand.c  ondemand.h  patches/0001-kpathsea-ondemand.patch  tests/
  build/
    build-texlive.sh  texlive-source.rev  sign-and-package.sh
  packaging/          pkg installer, TeXDist template, launchd plist
  tests/corpus/       document lists + harness (not the documents themselves)
  .github/workflows/  build-binaries.yml, test-corpus.yml
```

---

## 10. Open questions and risks

- **Licensing.** kpathsea is LGPL-2.1, the engines are mostly GPL/permissive, and tlnet packages are each free-licensed. Redistributing modified binaries means publishing the patch and the source revision (planned anyway).
- **Signing and notarization** need an Apple Developer account if you distribute beyond your own Macs.
- **Mirror etiquette.** The first compile of a large document can fetch about 100 small archives. Use HTTP/2 to one mirror, a cache and prefetching; never hammer the redirector.
- **Load-bearing upstream assumptions.** Upstream could change kpathsea's lookup structure, or LaTeX could change how it probes files. The patch sits at the single choke point, so this risk is low, but CI should build against texlive-source trunk weekly.
- **Option A (build MiKTeX for arm64)** remains a legitimate shortcut if you only want the end result. See §2.4.
