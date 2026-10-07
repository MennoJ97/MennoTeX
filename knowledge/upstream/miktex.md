---
type: Reference
title: MiKTeX on-the-fly installation
description: How MiKTeX installs packages on demand, what to copy, and what to avoid.
resource: https://github.com/MiKTeX/miktex
tags: [miktex, upstream, comparison]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-07T09:45:00Z }
verified:
  - { by: claude-code/claude-opus-5-5, at: 2026-10-06T19:00:00Z }
sources:
  - id: src
    resource: https://github.com/MiKTeX/miktex
    title: MiKTeX @ 76d1b3b (26.5, 2026-08-02)
    author: human:christian-schenk
---

# Mechanism

- A virtual TEXMF root (`//MiKTeX/]MPM[`) has a file-name database listing every
  file of every repository package, with the package id as info.[^src]
- Search paths expand `%R` to real roots plus the virtual root last
  (`Core/Session/searchpath.cpp:40-66`).
- A hit there calls `callback->InstallPackage(id)` and rewrites the path
  (`Core/Session/findfile.cpp:48-72`); this happens whatever `mustExist` is.
- Engines use a kpathsea **emulation** layer (`kpsemu.cpp`) and a custom
  Pascal→C++ translator (C4P) for WEB engines.

# Weaknesses (avoid)

- Lock file is check-then-create, held during downloads, 10 s timeout (`LockFile.cpp:118-126`, `PackageManagerImpl.cpp:99-113`).
- INI databases rewritten in place; MD5 links manifest to archives; no tar path sanitization.
- Every process loads the whole repository index into a hash map.
- `fontmaps configure` + `fc-cache` run after **every** on-the-fly install.
- Headless "ask" means "no" (`UI.h:99-109`).

# macOS state

The macOS build is Intel-oriented: Homebrew detection only checks
`/usr/local/Cellar`, the bundle fixup hard-codes `/usr/local/opt/...`, and the
deployment target is 10.12. The user's installed MiKTeX 22.1 is x86_64 (Rosetta),
with symlinks in `/usr/local/bin` that can shadow other TeX installations.

[^src]: MiKTeX @ 76d1b3b (26.5, 2026-08-02)
