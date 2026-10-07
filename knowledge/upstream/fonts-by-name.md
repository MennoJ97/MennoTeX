---
type: Reference
title: How fonts are found by name
description: How XeTeX (macOS), luaotfload and kpathsea resolve fontspec font names, and where on-demand installation can hook in.
tags: [fonts, xetex, luaotfload, fontspec, kpathsea, coretext]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-07T12:20:00Z }
verified:
  - { by: claude-code/claude-opus-5-5, at: 2026-10-07T12:15:00Z }
sources:
  - id: xetex
    resource: https://github.com/TeX-Live/texlive-source/blob/trunk/texk/web2c/xetexdir/XeTeXFontMgr_Mac.mm
    title: XeTeXFontMgr_Mac.mm (searchForHostPlatformFonts)
    author: team:tex-live
  - id: lotf
    resource: tlnet package luaotfload v3.29 (luaotfload-resolvers.lua, luaotfload-database.lua)
    title: luaotfload 2024-12-03 v3.29
---

# XeTeX on macOS

- Font names go through CoreText only (`searchForHostPlatformFonts`: display name,
  family-style, PostScript name, family).[^xetex] Fonts inside the TeX tree are not
  registered with CoreText, so `\setmainfont{TeX Gyre Pagella}` fails **even with
  TeX Live's own XeTeX and the font installed** (checked with tlnet's
  universal-darwin binary). Only `[file.otf]` / `Extension=` requests use kpathsea.
- MennoTeX patch `0002-xetex-tex-tree-fonts-by-name.patch`: when CoreText finds
  nothing, ask kpathsea for the name (opentype, then truetype), register every font
  in that file's directory with `CTFontManagerRegisterFontsForURLs(…,
  kCTFontManagerScopeProcess)`, and search again (once per name).

# luaotfload (LuaLaTeX)

- Request kinds: `name:`/`file:`/`[path]`/anonymous. Anonymous requests run the
  sequence `tex, path, name` (`luaotfload-resolvers.lua`); `tex` probes
  `kpse.find_file(name..".tfm", "tfm")` first.[^lotf]
- **fontspec sends `name:` requests** (`name:TeX Gyre Pagella:mode=node;…`), which go
  straight to the names database: miss → one `reload_db` per run → miss →
  `resolve_file` → `kpse.find_file(name, "tfm")`. That kpathsea probe comes
  **after** the only reload, so a font installed by it is found only on the next run.
- Installed fonts are found by name: the reload rescans the font directories.
- The names database lives in the TeX tree's var directory and is generated on the
  first font request of a run if missing.

# kpathsea

- `//` path expansions are cached per process (`elt-dirs.c`, `kpse->the_cache`).
  Directories created after the first expansion (e.g. by an on-demand install) stay
  invisible to later expansions, such as luaotfload's rescans. The MennoTeX resolver
  clears this cache after every install.

[^xetex]: XeTeXFontMgr_Mac.mm (searchForHostPlatformFonts)
[^lotf]: luaotfload 2024-12-03 v3.29
