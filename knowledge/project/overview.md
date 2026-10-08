---
type: Project Overview
title: MennoTeX
description: A TeX Live based distribution for Apple Silicon that installs packages on the fly, like MiKTeX.
resource: https://github.com/MennoJ97/MennoTeX
tags: [overview, texlive, macos, arm64, on-demand]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-08T10:00:00Z }
sources:
  - id: plan
    resource: https://github.com/MennoJ97/MennoTeX/blob/main/PLAN.md
    title: PLAN.md design and implementation plan
    author: claude-code/claude-opus-5-5
---

# Goal

A TeX distribution that behaves like MacTeX/TeX Live (same engines, same
packages, same results) but starts small and installs each package the first
time a document needs it, natively on Apple Silicon.[^plan]

# Approach in one paragraph

Engines and packages come from TeX Live. The package manager `mtx` (Rust) keeps a
verified copy of TeX Live's package database (`texlive.tlpdb`) and derives a
memory-mappable file → package index (`files.idx`). When kpathsea cannot find a
file, a hook calls `mtx ensure`, which installs the providing package and returns
the path. Phase 0 uses TeX Live's existing `mktextex`/`mktextfm` hooks with
unmodified binaries; Phase 1 replaces them with a small kpathsea patch. See
[on-demand installation](/architecture/on-demand-install.md).

# Repository layout

| Path | Contents |
|---|---|
| `README.md` | What MennoTeX is, getting started, how it differs from TeX Live. |
| `LICENSING.md`, `LICENSE-*` | Licensing: own code MIT OR Apache-2.0, patches under upstream licenses. See [decision 0008](/decisions/0008-licensing.md). |
| `PLAN.md` | The design study and phased roadmap. |
| `crates/mtx-core/` | Library: tlpdb parser, index, repository client, verification, installer, config generation. See [mtx-core](/architecture/mtx-core.md). |
| `crates/mtx/` | The `mtx` command-line tool. |
| `crates/mtx-core/keys/texlive.asc` | Pinned TeX Live signing key. See [tlnet](/upstream/tlnet.md). |
| `tools/analyze_tlpdb.py` | Reproduces the package-database statistics in PLAN.md. |
| `knowledge/` | This bundle. |

# Related

- [Roadmap status](/project/status.md)
- [Development playbook](/playbooks/development.md)
- [Decision: build on TeX Live](/decisions/0001-texlive-base.md)

[^plan]: PLAN.md design and implementation plan
