---
type: Code Module
title: mtx-core
description: Map of the mtx-core library modules, their responsibilities and invariants.
resource: https://github.com/MennoJ97/MennoTeX/tree/main/crates/mtx-core/src
tags: [rust, code-map]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-07T10:30:00Z }
verified:
  - { by: process:cargo-test, at: 2026-10-07T09:30:00Z }
---

# Modules

| Module | Responsibility | Key invariants |
|---|---|---|
| `tlpdb` | Parse `texlive.tlpdb`; `RELOC/` → `texmf-dist/`; `depend` closure with `.ARCH` → `universal-darwin` | Collections/schemes are only followed when they are roots |
| `index` | Build/read [files.idx](/architecture/files-idx.md) | Layout is shared with C; bump `VERSION` on any change |
| `verify` | OpenPGP check of `texlive.tlpdb.sha512.asc` | Pins the primary fingerprint; accepts the newest valid subkey binding (see [tlnet](/upstream/tlnet.md)) |
| `repo` | HTTP/`file://` repository access, verified streaming downloads | Size or SHA-512 mismatch → `ChecksumMismatch` (triggers retry) |
| `ctx` | Root + DB + pinned mirror + logging + `refresh` | Logs never go to stdout; TTL 1 h; mirror pin 24 h; bad mirrors avoided 24 h; `failover` blames the mirror only if the redirector answers |
| `db` | SQLite `installed.sqlite`: packages, files, kv state | Reinstall as `auto` never downgrades an `explicit`/`bootstrap` reason |
| `extract` | Unpack `.tar.xz` via a staging dir | No absolute paths, no `..`, symlinks must stay inside the root; skips `tlpkg/tlpobj/` |
| `lsr` | Rebuild/append kpathsea `ls-R` | Repeated directory blocks are legal |
| `configfiles` | `fmtutil.cnf`, `updmap.cfg`, `language.*` | Output matches tlmgr byte for byte apart from the generated-by line (see [tlmgr config generation](/upstream/tlmgr-config.md)) |
| `install` | Plan → download → lock → commit → regen | Never holds the lock while downloading; never unpacks over `PROTECTED` files |
| `ensure` | Hook entry point | Miss path touches only the index |
| `bootstrap` | Core set + hyphenation + hooks + root `texmf.cnf` | Hooks and `texmf.cnf` are written before `updmap` runs |
| `root` | Installation layout (TeX Live compatible) | `tool_path()` = our bin + system dirs only |

# Tests

`cargo test` runs unit tests for every module above (20 as of 2026-10-07),
including a real tlnet signature fixture in `crates/mtx-core/testdata/`.
