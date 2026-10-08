# Directory Update Log

## 2026-10-08
* **Update**: Releases `mennotex-2026-cf6183267` and `mennotex-2026-675989434` (run 37794123819, 2½ min, programs reused, new mtx) signed and published by the user; the real installation runs mtx `675989434` with programs `9c5675bd`, has `fontmaps.tsv`, and its unstamped formats were removed to rebuild on use ([handoff](/project/handoff.md)).
* **Update**: GitHub lists releases oldest first (seen 2026-10-08), so `mtx self-update` chose the release already installed; it now sorts by `published_at`. Installed versions up to `cf6183267` need `--release TAG` once ([decision 0011](/decisions/0011-releases-and-self-update.md)).
* **Update**: Release `mennotex-2026-cf6183267` drafted by CI (run 37791928013) with everything above; the incremental CI build from the cached tree works (5½ min) ([handoff](/project/handoff.md)).
* **Creation**: [Decision 0017](/decisions/0017-mirror-faults.md): fault-injection tests over HTTP (PLAN.md §8) found that error statuses, broken-off transfers and unverifiable databases did not switch mirrors, and that a good mirror one revision ahead got avoided after a failover; all fixed ([playbook](/playbooks/development.md), [status](/project/status.md)).
* **Update**: The first CI build from a partially matching build-tree cache failed: after the restore every file has a new inode, `git checkout -- .` then rewrote all tracked files (new mtimes, full rebuild, ICU "config.status has become stale"); the workflow refreshes git's index first ([playbook](/playbooks/development.md)).
* **Creation**: [Decision 0016](/decisions/0016-format-stamps.md): formats carry a stamp (engine, trigger revisions, hyphenation hash); every transaction deletes those whose stamp differs, `mtx doctor` reports them, and `mtx repair` no longer rebuilds every format ([mtx-core](/architecture/mtx-core.md), [on-demand install](/architecture/on-demand-install.md)).
* **Creation**: [Decision 0015](/decisions/0015-font-map-on-miss.md): a font-map miss in pdfTeX or LuaTeX installs the package whose map covers the font (`tlpkg/mtx/fontmaps.tsv`, `mtx ensure --font-map`) and re-reads the map in the same run.
* **Creation**: [Decision 0014](/decisions/0014-install-warnings-in-tex-log.md): declined or failed installs put a `Package mtx Warning` line in TeX's own log (exit statuses 3/4, a queue and printer callback in kpathsea, patch 0004 for the engines). LaTeX checks existence with `\pdffilesize`, so the first version, which only printed after file opens, printed nothing for `\usepackage` ([package quirks](/upstream/package-quirks.md)).
* **Update**: The install prompt lists every package with the dependencies it brings: terminal text, and a Cocoa alert with an expandable, scrolling outline (JavaScript for Automation), at the user's request; shown to the user, who clicked Install All ([decision 0007](/decisions/0007-install-consent.md), [playbook](/playbooks/development.md)).
* **Update**: `mtx search TEXT` and `mtx search --file TEXT` (PLAN.md §5.10) ([mtx-core](/architecture/mtx-core.md)).
* **Creation**: [Decision 0013](/decisions/0013-no-probe-deny-list.md): probe-only installs measured with `tests/probe_installs.py` (6 of 418, all loaded for real elsewhere); no deny list. Facts about LaTeX's existence checks and kernel substitutions in [package quirks](/upstream/package-quirks.md).
* **Update**: First signed release `mennotex-2026-f06b1506f` published by the user; the real installation self-updated to it and PATH now goes through `…/MennoTeX/current` ([handoff](/project/handoff.md)).
* **Update**: The `ask` dialog lost every click (AppleScript's `result` was read after an `if` had replaced it, error -2753); found through the new diagnostics in a VS Code build, fixed with a variable and a unit test ([decision 0007](/decisions/0007-install-consent.md), [handoff](/project/handoff.md)).
* **Update**: `mtx self-update` decides whether to replace mtx by comparing with the installed program, not the running one ([decision 0011](/decisions/0011-releases-and-self-update.md)).
* **Update**: `.gitignore`'s `*.tlpdb` also hid `kpathsea-ondemand/tests/fixture.tlpdb`, so the first release run failed in `cargo test` (the second such case after `texlive.tlpdb.xz`); now excepted, and checked with `cargo test` in a clean copy of the commit.
* **Creation**: [Decision 0012](/decisions/0012-map-reread.md): pdfTeX and LuaTeX re-read `pdftex.map` after an install (patch 0003, install counter in kpathsea); corpus `late-font-map` ([status](/project/status.md), [handoff](/project/handoff.md)).
* **Update**: C unit tests of the kpathsea patch (`tests/run_c_tests.sh`, fixture written by the Rust index code); CI caches the TeX Live build tree for incremental builds and Rust's dependencies, runs the C tests, scopes `GH_TOKEN` to `gh` steps and pins the third-party action ([playbook](/playbooks/development.md)).
* **Creation**: [Decision 0011](/decisions/0011-releases-and-self-update.md): signed releases (minisign key on the user's Mac), `mtx self-update`, `mtx upgrade-release`, the `current` link; the workflow builds mtx, reuses unchanged TeX Live programs and drafts releases ([mtx-core](/architecture/mtx-core.md), [playbook](/playbooks/development.md), [handoff](/project/handoff.md)).
* **Update**: The repository is public (the user); docs that said private are corrected. Roots are resolved to their real directory, so `current` can move under a running mtx.
* **Update**: Phase 2 exit met: the corpus matches a full TeX Live 2026 (`tests/compare_texlive.py`; [status](/project/status.md), [playbook](/playbooks/development.md)).
* **Creation**: [Decision 0010](/decisions/0010-shadowing-packages.md): installs add packages whose same-named `tex/` files a LaTeX format finds first (`xetex-pstricks`); [mtx-core](/architecture/mtx-core.md).
* **Update**: `texfonts.map` is read by a path search that bypasses the hook, so `fontname` joined the bootstrap core ([kpathsea](/upstream/kpathsea.md)).
* **Update**: Corrected [package quirks](/upstream/package-quirks.md): PSTricks on XeLaTeX needs Ghostscript in any TeX Live; stock XeTeX on macOS misses TeX-tree fonts by name.
* **Creation**: [Decision 0009](/decisions/0009-crash-recovery.md): installs stay journaled until ls-R, configuration and font maps are done; lookups (also in the kpathsea patch) and later installs finish interrupted ones; `tests/run_crash.sh` passes all five crash cases ([mtx-core](/architecture/mtx-core.md), [playbook](/playbooks/development.md)).
* **Update**: Corpus grown from 26 to 51 documents; all pass on fresh roots and the second run installs nothing, meeting Phase 1's exit ([status](/project/status.md), [playbook](/playbooks/development.md), [handoff](/project/handoff.md)).
* **Update**: `mktexfmt` builds METAFONT's `mf.base` (installing `metafont`) so mktexpk works; fmtutil only acts as mktexfmt under that name ([TeX Live scripts](/upstream/texlive-scripts.md), [mtx-core](/architecture/mtx-core.md)).
* **Update**: The luaotfload overlay rescans after an install, so a second font installed in one LuaLaTeX run is found ([decision 0005](/decisions/0005-fonts-by-name.md), [fonts by name](/upstream/fonts-by-name.md)).
* **Creation**: [Package quirks](/upstream/package-quirks.md): Xy-pic needs `luatex85` under LuaLaTeX; PSTricks works on XeLaTeX without Ghostscript.
* **Update**: The real installation runs the mtx with `ask` diagnostics (user ran `mtx repair`; auto mode blocks agents from changing it) ([handoff](/project/handoff.md)).
* **Update**: First GitHub Release published by CI run 37750229150 (user-approved): binary and source archives verified against `SHA256SUMS` and installed with `--github --release latest` ([status](/project/status.md), [handoff](/project/handoff.md), [decision 0008](/decisions/0008-licensing.md), [playbook](/playbooks/development.md)).
* **Update**: Every `ask` outcome is logged with why a prompt could not be shown (osascript exit status, stderr, run time) and the process chain the request came from; `mtx doctor` warns about unshown prompts ([decision 0007](/decisions/0007-install-consent.md), [mtx-core](/architecture/mtx-core.md), [handoff](/project/handoff.md)).
* **Update**: Real installation is on the user's PATH and used from VS Code ([handoff](/project/handoff.md), [status](/project/status.md)); recorded a build where the `ask` dialog failed fast and fell back to yes.
* **Update**: "Install All" now covers a whole latexmk build, not just one process ([decision 0007](/decisions/0007-install-consent.md)); reported by the user from the VS Code dialog.
* **Update**: README states the use of AI (Claude through Claude Code) in writing MennoTeX ([decision 0008](/decisions/0008-licensing.md)).
* **Update**: A network blip failed a VS Code build: connect timeout 5 → 10 s, builds start with a fresh network check, failure report covers prefetch errors and skipped installs, and latexmk reruns after prefetch installs ([TeX Live scripts](/upstream/texlive-scripts.md), [tlnet](/upstream/tlnet.md)).
* **Update**: Release archives carry license texts (`licenses/` tree of every texlive-source license file, GPL-2/LGPL-2.1 texts, `LICENSING.md`) and `kpathsea-ondemand/`; a Release also gets the corresponding source archive ([decision 0008](/decisions/0008-licensing.md), [licenses](/upstream/licenses.md), [playbook](/playbooks/development.md), [handoff](/project/handoff.md)).
* **Update**: latexmk integration (MennoTeX system rc via `$LATEXMKRCSYS`, prefetch hook, failure report, doctor check); latexmk's rc order and hooks recorded in [TeX Live scripts](/upstream/texlive-scripts.md).
* **Creation**: `README.md`, `LICENSE-MIT`, `LICENSE-APACHE`, `LICENSING.md`; [decision 0008](/decisions/0008-licensing.md) and [licenses](/upstream/licenses.md): upstream license terms checked at the pinned texlive-source revision; release archives still lack license texts and source ([handoff](/project/handoff.md)).
* **Update**: Documentation on demand: `texdoc` is an mtx multi-call that installs `<pkg>.doc` containers; texdoc's own ls-R reader and tlpdb lookup recorded in [TeX Live scripts](/upstream/texlive-scripts.md) ([mtx-core](/architecture/mtx-core.md)).
* **Update**: `mtx gc` removes on-demand packages unused for N days, by file access time ([mtx-core](/architecture/mtx-core.md), [status](/project/status.md)).
* **Update**: `mtx install-binaries --github` fetches CI builds through `gh` ([mtx-core](/architecture/mtx-core.md), [playbook](/playbooks/development.md), [status](/project/status.md)).
* **Creation**: [Decision 0007](/decisions/0007-install-consent.md): `autoinstall yes|no|ask`, `mtx config`, `mtx log`, failures in `mtx.log` ([mtx-core](/architecture/mtx-core.md), [playbook](/playbooks/development.md)).
* **Update**: [Handoff](/project/handoff.md): real installation done (no PATH change), CI step done, LuaLaTeX font-name gap closed, corpus at 26.
* **Update**: LuaLaTeX fonts by name work on the first run through mtx's `luaotfload-main.lua` overlay ([decision 0005](/decisions/0005-fonts-by-name.md), [fonts by name](/upstream/fonts-by-name.md)); `TEXMFDBS` excludes aux trees, so the overlay is searched on disk.
* **Update**: First CI binary build: 14 min, artifact installed and used for the corpus ([playbook](/playbooks/development.md), [status](/project/status.md)).
* **Update**: [Handoff](/project/handoff.md): release transitions item now reflects [decision 0006](/decisions/0006-release-transitions.md) and lists what is still open.
* **Update**: [Handoff](/project/handoff.md): next step for an `ask` install setting and logging install failures to `mtx.log`, as requested by the user; gotcha about stale test binaries from another clone sharing `target/`.
* **Creation**: [Decision 0006](/decisions/0006-release-transitions.md): release transitions pin the frozen historic repository; `mtx bootstrap --from` ([mtx-core](/architecture/mtx-core.md)).
* **Update**: `.gitignore` excluded the test repository's `texlive.tlpdb.xz` (`*.tlpdb.xz` was meant for downloaded data), so offline tests only passed in the original working copy; now tracked and checked in a clean worktree.
* **Update**: Historic archive layout, mirrors, freeze timing and texlive.info's bot wall recorded in [tlnet](/upstream/tlnet.md).
* **Creation**: [Handoff](/project/handoff.md) for the next session; `CLAUDE.md` now points every session at it.

## 2026-10-07
* **Update**: Format-build race found by the concurrency test and fixed with mtx as `mktexfmt` ([TeX Live scripts](/upstream/texlive-scripts.md), [mtx-core](/architecture/mtx-core.md)).
* **Update**: Concurrency test passes ([status](/project/status.md), [playbook](/playbooks/development.md)).
* **Creation**: [Decision 0005](/decisions/0005-fonts-by-name.md) and [fonts by name](/upstream/fonts-by-name.md); corpus at 18 documents ([status](/project/status.md)).
* **Update**: Manual-only binary build workflow and archive installs ([playbook](/playbooks/development.md), [status](/project/status.md)).
* **Update**: `mtx repair`, doctor check for missing font-map packages, format invalidation on binary install ([mtx-core](/architecture/mtx-core.md)).
* **Update**: Phase 1 exit criterion met with the 2026.1 release build: corpus passes on all three engines ([status](/project/status.md)).
* **Creation**: [Decision 0004](/decisions/0004-font-map-index.md) (font-map index); corpus results in [status](/project/status.md).
* **Update**: Command shims ([on-demand installation](/architecture/on-demand-install.md)); C++17 requirement of the 2026 release recorded in [build notes](/upstream/texlive-build.md).
* **Update**: Phase 1 resolver works for all three engines on the first run ([status](/project/status.md)); build pinned to the 2026.1 release branch ([build notes](/upstream/texlive-build.md)).
* **Update**: Added `mtx remove` ([mtx-core](/architecture/mtx-core.md)).
* **Update**: Added `mtx prefetch` with measured effect ([status](/project/status.md)).
* **Update**: Added `mtx doctor` ([mtx-core](/architecture/mtx-core.md), [playbook](/playbooks/development.md)).
* **Update**: Install journal and `mtx update` ([mtx-core](/architecture/mtx-core.md), [status](/project/status.md)).
* **Update**: Offline end-to-end tests against a signed fake repository ([mtx-core](/architecture/mtx-core.md), [playbook](/playbooks/development.md)).
* **Creation**: [Decision 0003](/decisions/0003-kpathsea-patch-shape.md) and [texlive-source build notes](/upstream/texlive-build.md); Phase 1 started in [status](/project/status.md).
* **Update**: Corrected the LuaTeX gap description in [decision 0002](/decisions/0002-phase0-stock-hooks.md).
* **Update**: Phase 0 exit criterion met for pdfLaTeX/XeLaTeX; LuaLaTeX gap measured ([status](/project/status.md), [decision 0002](/decisions/0002-phase0-stock-hooks.md)).
* **Update**: First on-demand installs from a real `pdflatex` run; recorded in [status](/project/status.md).
* **Creation**: Added [TeX Live scripts](/upstream/texlive-scripts.md) (mktexfmt refuses TEXMFVAR == TEXMFSYSVAR).
* **Update**: Recorded the expired-TLS-certificate mirror and the failover rule in [tlnet](/upstream/tlnet.md).
* **Creation**: Established the bundle: [overview](/project/overview.md), [status](/project/status.md), architecture, upstream reference, decisions and the [development playbook](/playbooks/development.md).
* **Update**: Recorded the corrupt `amsfonts.tar.xz` on one mirror and the resulting bad-mirror avoidance in [tlnet](/upstream/tlnet.md).

## 2026-10-06
* **Initialization**: Design study of texlive-source and MiKTeX (see PLAN.md); Phase 0 coding started.
