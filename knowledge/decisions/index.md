# Decisions

* [0001: Build on TeX Live, copy MiKTeX's idea](0001-texlive-base.md) - TeX Live engines and tlnet with a new Rust package manager.
* [0005: On-demand fonts selected by name](0005-fonts-by-name.md) - Font-name index, XeTeX CoreText registration, prefetch for LuaLaTeX.
* [0004: Embedded font-map index](0004-font-map-index.md) - Installing TFMs also installs the packages whose maps cover them (ec → cm-super).
* [0003: Shape of the kpathsea patch](0003-kpathsea-patch-shape.md) - One included C file, called on every miss, hands installation to mtx.
* [0002: Phase 0 uses stock hooks](0002-phase0-stock-hooks.md) - Develop against unmodified tlnet binaries via MKTEXTEX/mktextfm first.
