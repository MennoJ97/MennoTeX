---
type: File Format
title: files.idx
description: The mmap-able, C-readable file-name → package index that mtx derives from texlive.tlpdb.
resource: https://github.com/MennoJ97/MennoTeX/blob/main/crates/mtx-core/src/index.rs
tags: [format, index, kpathsea]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-07T09:40:00Z }
verified:
  - { by: process:cargo-test, at: 2026-10-07T09:30:00Z }
---

# Purpose

Answer "which package ships `foo.sty`?" in microseconds, from any process,
including C code inside kpathsea in Phase 1. Stored at `tlpkg/mtx/files.idx`;
rebuilt from the tlpdb on every database refresh and replaced by atomic rename,
so running processes keep a consistent mapping.

# Schema

All integers little-endian. Offsets are absolute file offsets, except string
offsets, which are relative to the strings section.

| Section | Layout |
|---|---|
| header (64 bytes) | magic `MTXIDX\0\x01`, u32 version=1, u32 n_entries, u32 n_dirs, u32 n_pkgs, u64 tlpdb revision, u32 offsets of hashes/entries/dirs/pkgs/strings, u32 strings length, zero padding |
| hashes | n_entries × u64: FNV-1a 64 of the ASCII-lowercased basename, sorted ascending |
| entries | n_entries × {u32 basename_off, u32 dir_idx, u32 pkg_idx}, parallel to hashes |
| dirs | n_dirs × u32 string offset of a root-relative directory (`texmf-dist/tex/latex/amsmath`) |
| pkgs | n_pkgs × {u32 name_off, u32 revision, u32 container_size, u32 flags}, sorted by name |
| strings | NUL-terminated UTF-8 |

Package flags: `1` has font maps, `2` has formats, `4` has hyphenation, `8` is a `-dev` package.

# Lookup

Binary-search the hash, scan equal hashes, return exact basename matches; if
none, return case-insensitive matches (mirrors kpathsea's
`texmf_casefold_search = 1`). Only content packages are indexed: no
collections, schemes or per-platform binary packages.

# Examples

```
$ mtx which l3backend-pdftex.def
l3kernel	texmf-dist/tex/latex/l3kernel/l3backend-pdftex.def	available
```

Real-database size: about 185k entries; see PLAN.md §2.3.
