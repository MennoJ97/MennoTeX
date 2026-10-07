# Directory Update Log

## 2026-10-07
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
