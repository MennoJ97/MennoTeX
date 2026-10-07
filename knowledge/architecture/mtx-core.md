---
type: Code Module
title: mtx-core
description: Map of the mtx-core library modules, their responsibilities and invariants.
resource: https://github.com/MennoJ97/MennoTeX/tree/main/crates/mtx-core/src
tags: [rust, code-map]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-07T13:20:00Z }
verified:
  - { by: process:cargo-test, at: 2026-10-07T13:00:00Z }
---

# Modules

| Module | Responsibility | Key invariants |
|---|---|---|
| `tlpdb` | Parse `texlive.tlpdb`; `RELOC/` → `texmf-dist/`; `depend` closure with `.ARCH` → `universal-darwin` | Collections/schemes are only followed when they are roots |
| `index` | Build/read [files.idx](/architecture/files-idx.md) | Layout is shared with C; bump `VERSION` on any change |
| `verify` | OpenPGP check of `texlive.tlpdb.sha512.asc` | Pins the primary fingerprint; accepts the newest valid subkey binding (see [tlnet](/upstream/tlnet.md)) |
| `repo` | HTTP/`file://` repository access, verified streaming downloads | Size or SHA-512 mismatch → `ChecksumMismatch` (triggers retry) |
| `ctx` | Root + DB + pinned mirror + logging + `refresh` | Logs never go to stdout; TTL 1 h; mirror pin 24 h; bad mirrors avoided 24 h; `failover` blames the mirror only if the redirector answers |
| `db` | SQLite `installed.sqlite`: packages, files, kv state | Reinstall as `auto` never downgrades an `explicit`/`bootstrap` reason; `Reason::Upgrade` keeps the recorded reason |
| `binaries` | Install MennoTeX-built programs as `mennotex-binaries`; switch to kpathsea hook mode | Only Mach-O files are taken |
| `extract` | Unpack `.tar.xz` via a staging dir | No absolute paths, no `..`, symlinks must stay inside the root; skips `tlpkg/tlpobj/` |
| `lsr` | Rebuild/append kpathsea `ls-R` | Repeated directory blocks are legal |
| `configfiles` | `fmtutil.cnf`, `updmap.cfg`, `language.*` | Output matches tlmgr byte for byte apart from the generated-by line (see [tlmgr config generation](/upstream/tlmgr-config.md)) |
| `install` | Plan → download → lock → commit → regen; `outdated` for `mtx update` | Never holds the lock while downloading; never unpacks over `PROTECTED` or `mennotex-binaries` files; a package is journaled (`tlpkg/mtx/journal/<pkg>`) from unpack until recorded, and journaled packages count as not installed, so an interrupted install is redone on next use |
| `ensure` | Hook entry point | Miss path touches only the index |
| `bootstrap` | Core set + hyphenation + hooks + root `texmf.cnf` | Hooks and `texmf.cnf` are written before `updmap` runs |
| `prefetch` | `mtx prefetch doc.tex`: statically scan a document (and local `\input`s) for classes, packages, TikZ/pgfplots libraries and `.bst`, then iterate over `\RequirePackage` in installed files; install in batched transactions | Heuristic: may over-fetch conditional packages; on-demand hooks cover the rest |
| `doctor` | `mtx doctor`: PATH shadowing, hook-mode consistency, index/db agreement, journal and staging leftovers, missing files, avoided mirrors | Exit 1 only for problems, not warnings |
| `root` | Installation layout (TeX Live compatible) | `tool_path()` = our bin + system dirs only |

# Tests

`cargo test` runs 25 tests (2026-10-07):

- unit tests for every module above, including a real tlnet signature fixture
  (`testdata/texlive.tlpdb.sha512{,.asc}`);
- `local_repo_tests.rs`: offline end-to-end tests against `testdata/tlnet`, a tiny
  repository (`foo` depends on `bar`; `fonts-x`) signed with a throw-away key that has
  TeX Live's structure (certification-only primary + signing subkey). They cover
  refresh → `ensure_path` with siblings, Phase 0 name resolution, failover from an
  unreachable pinned mirror, rejection of a corrupted archive, and redoing an
  interrupted (journaled) install.
  `Ctx::test_key` / `Verifier::with_key` exist only under `cfg(test)`.
  Regenerate the repository with `tools/make_test_repo.sh` (needs `gpg`).
