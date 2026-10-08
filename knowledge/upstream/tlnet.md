---
type: Reference
title: tlnet, the TeX Live network repository
description: Layout, signing chain, archive formats and mirror behaviour of tlnet, as observed and verified.
resource: https://mirror.ctan.org/systems/texlive/tlnet/
tags: [tlnet, texlive, repository, security, mirrors]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-08T07:40:00Z }
verified:
  - { by: process:cargo-test, at: 2026-10-07T09:30:00Z }
sources:
  - id: keyext
    resource: https://www.tug.org/texlive/files/texlive.asc
    title: TeX Live distribution public key
    author: team:tex-live
  - id: historic
    resource: https://tug.org/historic/
    title: TeX Live historic archive and its mirrors
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

# Font maps

Which package's map covers a TeX font is not in the tlpdb; it is only in the map
files. See [decision 0004](/decisions/0004-font-map-index.md) for the extracted table.
`cleveref` is still 0.21.4 (2018); with the 2025+ LaTeX kernel, thmtools'
`\declaretheorem[sibling=…]` plus cleveref fails with `\c@<name> already defined`.

# Mirror behaviour (observed 2026-10-06/07)

- `mirror.ctan.org` redirects **each request** to a possibly different mirror.
- Mirrors lag differently: in one session `cicku.me` served tlpdb r80534 while `lyrahosting.com` served r80505.
- `nl.mirrors.cicku.me` served a corrupt `archive/amsfonts.tar.xz` (3,626,288 bytes instead of the signed 3,626,284) while other mirrors were correct. mtx now treats size/hash mismatches as integrity failures, retries, then avoids that mirror for 24 h.
- 2026-10-08, on the user's network: TCP connects to `mirror.koddos.net` and
  `mirror.ctan.org` took about 2 s, and a moment earlier both exceeded mtx's 5 s connect
  timeout, so mtx went offline for 60 s in the middle of a VS Code build. The connect
  timeout is now 10 s, and each latexmk build starts with a fresh network check
  (prefetch ignores an older offline marker).
- 2026-10-08, during three parallel corpus runs: `mirror.lyrahosting.com` again presented
  its expired certificate, and `nl.mirrors.cicku.me` served `pst-node.tar.xz` and
  `translations.tar.xz` with a SHA-512 that did not match the signed database. Both were
  avoided automatically; every document still compiled on its first run.
- `mirror.lyrahosting.com` intermittently presented an **expired TLS certificate** (not valid after 2026-08-04) on 2026-10-07; requests sometimes succeeded, so it is probably several hosts behind one name. mtx now fails over: on a network/TLS error it re-resolves the redirector; if that answers, the mirror is avoided for 24 h and another is pinned; if not, mtx goes offline for 60 s.

# Release transitions and the historic archive (checked 2026-10-08)

- Each release's last tlnet state is kept as `systems/texlive/<release>/tlnet-final/`
  in TeX Live's **historic archive**, same layout as tlnet, with a signed
  `tlpkg/texlive.tlpdb.sha512.asc`.[^historic]
- 2025's `tlnet-final` declares `frozen/1`, `release/2025`, `minrelease/2016`,
  revision 78234. Its signature was made on **2026-03-01** with the same subkey
  `D8F2F860…`, and verifies against the 2026 keyring. So freezing happens about a
  month before tlnet switches to the next release (April), and the previous
  release's frozen repository exists by the time tlnet moves on.
- `mirror.ctan.org` does **not** carry the historic archive: it redirects
  `systems/texlive/historic/…` to a CTAN mirror, which answers 404.
- Historic mirrors listed by tug.org[^historic], all serving 2025's `tlnet-final`
  (database and archives) on 2026-10-08: `ftp.math.utah.edu/pub/tex/historic/`,
  `ftp.tu-chemnitz.de/pub/tug/historic/`, `texlive.info/historic/`,
  `mirrors.tuna.tsinghua.edu.cn/tex-historic-archive/`, `mirror.nju.edu.cn/tex-historic/`.
  `ftp.tug.org` presents a TLS certificate for another host name.
- **texlive.info** is behind an Anubis bot challenge: with mtx's User-Agent, every
  path (also missing ones) returns **HTTP 200** with an HTML challenge page;
  curl's default User-Agent gets the real 200/404. mtx leaves it out of its
  default list and checks that the probe file is a checksum, not just its status.

[^keyext]: TeX Live distribution public key
[^historic]: TeX Live historic archive and its mirrors
