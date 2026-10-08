# Directory Update Log

## 2026-10-08
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
