---
type: Reference
title: tlnet, the TeX Live network repository
description: Layout, signing chain, archive formats and mirror behaviour of tlnet, as observed and verified.
resource: https://mirror.ctan.org/systems/texlive/tlnet/
tags: [tlnet, texlive, repository, security, mirrors]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-07T10:30:00Z }
verified:
  - { by: process:cargo-test, at: 2026-10-07T09:30:00Z }
sources:
  - id: keyext
    resource: https://www.tug.org/texlive/files/texlive.asc
    title: TeX Live distribution public key
    author: team:tex-live
---

# Layout

| Path | Contents |
|---|---|
| `tlpkg/texlive.tlpdb.xz` | Package database (~2.8 MB; 20.7 MB unpacked) |
| `tlpkg/texlive.tlpdb.sha512` | SHA-512 of the **uncompressed** tlpdb (144 bytes) |
| `tlpkg/texlive.tlpdb.sha512.asc` | Detached OpenPGP signature over the `.sha512` file |
| `archive/<pkg>.tar.xz` | Runtime files; `<pkg>.doc.tar.xz` / `.source.tar.xz` separately; names are **not versioned** |

# Signing chain

Signature → `.sha512` → uncompressed tlpdb → per-package `containerchecksum`
(SHA-512) and `containersize`. Primary key
`C78B82D8C79512F79CC0D7C80D5E5D9106BAB6BC` ("TeX Live Distribution"); signing
subkey `D8F2F860…4CE1877E19438C70`, expiry extended **every year** (currently
2027-07-13), with refreshed key material shipped in `texlive.infra`
(`tlpkg/gpg/pubring.gpg`).[^keyext] The key's user id also carries a
third-party certification, so "verify all bindings" style checks fail; verify
the subkey binding only. Two of the subkey bindings use SHA-1.

# Archive layout

- Relocatable packages (`relocated 1`) store paths **without** `texmf-dist/`.
- Others store root-relative paths (`texmf-dist/…`, `tlpkg/…`, `bin/universal-darwin/…`).
- Every archive contains `tlpkg/tlpobj/<pkg>.tlpobj`.
- Binary packages for `universal-darwin` are mostly symlinks into `texmf-dist/scripts`.

# Database facts used by mtx

- `00texlive.config` has `release/2026`, `minrelease`, `revision`, `frozen`.
- Package names change between releases: in 2026 `l3backend` is part of `l3kernel`.
- `execute` lines: `AddFormat`, `addMap`, `addMixedMap`, `addKanjiMap`, `AddHyphen`.

# Mirror behaviour (observed 2026-10-06/07)

- `mirror.ctan.org` redirects **each request** to a possibly different mirror.
- Mirrors lag differently: in one session `cicku.me` served tlpdb r80534 while `lyrahosting.com` served r80505.
- `nl.mirrors.cicku.me` served a corrupt `archive/amsfonts.tar.xz` (3,626,288 bytes instead of the signed 3,626,284) while other mirrors were correct. mtx now treats size/hash mismatches as integrity failures, retries, then avoids that mirror for 24 h.
- `mirror.lyrahosting.com` intermittently presented an **expired TLS certificate** (not valid after 2026-08-04) on 2026-10-07; requests sometimes succeeded, so it is probably several hosts behind one name. mtx now fails over: on a network/TLS error it re-resolves the redirector; if that answers, the mirror is avoided for 24 h and another is pinned; if not, mtx goes offline for 60 s.

[^keyext]: TeX Live distribution public key
